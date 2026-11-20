use crate::{SorobanForgeSubscriptionPaymentsClient, SubscriptionPayments, SubscriptionStatus};
use soroban_forge_shared_utils::ForgeError;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use soroban_sdk::xdr::{ContractEventBody, Int128Parts, ScSymbol, ScVal};
use soroban_sdk::{Address, Env};

const START: u64 = 1_000_000;
const PERIOD: u64 = 1_000;
const AMOUNT: i128 = 250;

fn setup() -> (Env, Address, Address, Address, Address, u64) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().set_timestamp(START);
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin);
    let token = sac.address();
    let subscriber = Address::generate(&env);
    let provider = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&subscriber, &10_000);
    let contract_id = env.register(SubscriptionPayments, ());
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let id = client.subscribe(&subscriber, &provider, &token, &AMOUNT, &PERIOD);
    (env, token, subscriber, provider, contract_id, id)
}

#[test]
fn deposit_failure_leaves_balance_and_tokens_unchanged() {
    let (env, token, subscriber, _provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    let before = client.get_subscription(&id);
    let user_before = token_client.balance(&subscriber);
    let result = client.try_deposit(&id, &(user_before + 1));
    assert_eq!(
        result.unwrap_err().unwrap(),
        ForgeError::TokenTransferFailed
    );
    assert_eq!(client.get_subscription(&id), before);
    assert_eq!(token_client.balance(&subscriber), user_before);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn exact_period_balance_debits_then_lapses_and_top_up_recovers() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &(AMOUNT * 2));
    assert_eq!(
        client.get_subscription(&id).prepaid_balance,
        Some(AMOUNT * 2)
    );

    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), AMOUNT);
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(AMOUNT));

    env.ledger().set_timestamp(START + PERIOD * 2);
    assert_eq!(client.charge(&id), AMOUNT);
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(0));
    assert_eq!(token_client.balance(&provider), AMOUNT * 2);

    env.ledger().set_timestamp(START + PERIOD * 3);
    assert_eq!(client.charge(&id), 0);
    assert_eq!(
        client.get_subscription(&id).status,
        SubscriptionStatus::PastDue
    );
    assert_eq!(
        client.get_subscription(&id).last_charged,
        START + PERIOD * 2
    );

    client.deposit(&id, &AMOUNT);
    assert_eq!(client.charge(&id), AMOUNT);
    let recovered = client.get_subscription(&id);
    assert_eq!(recovered.status, SubscriptionStatus::Active);
    assert_eq!(recovered.last_charged, START + PERIOD * 3);
    assert_eq!(recovered.prepaid_balance, Some(0));
    assert_eq!(token_client.balance(&provider), AMOUNT * 3);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(token_client.balance(&subscriber), 10_000 - AMOUNT * 3);
}

#[test]
fn cancellation_refunds_exact_remainder_and_withdrawal_preserves_mode() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &(AMOUNT + 83));
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), AMOUNT);
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(83));
    client.cancel(&id);
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(0));
    assert_eq!(token_client.balance(&subscriber), 10_000 - AMOUNT);
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(token_client.balance(&contract_id), 0);

    let second = client.subscribe(&subscriber, &provider, &token, &AMOUNT, &PERIOD);
    client.deposit(&second, &500);
    assert_eq!(client.withdraw_balance(&second, &200), 300);
    assert_eq!(client.get_subscription(&second).prepaid_balance, Some(300));
}

#[test]
fn retry_limit_auto_cancellation_refunds_the_exact_remainder() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &(AMOUNT + 37));
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), AMOUNT);
    for attempt in 2..=4 {
        env.ledger().set_timestamp(START + PERIOD * attempt);
        assert_eq!(client.charge(&id), 0);
    }
    let sub = client.get_subscription(&id);
    assert_eq!(sub.status, SubscriptionStatus::Cancelled);
    assert_eq!(sub.prepaid_balance, Some(0));
    assert_eq!(token_client.balance(&subscriber), 10_000 - AMOUNT);
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn catchup_cannot_bypass_prepaid_one_period_lapse_policy() {
    let (env, _token, _subscriber, _provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &(AMOUNT * 2));
    env.ledger().set_timestamp(START + PERIOD * 2);
    let err = client.try_charge_catchup(&id, &2).unwrap_err().unwrap();
    assert_eq!(err, ForgeError::InvalidInput);
    assert_eq!(
        client.get_subscription(&id).prepaid_balance,
        Some(AMOUNT * 2)
    );
}

#[test]
fn prepaid_events_report_exact_amounts_and_resulting_balances() {
    let (env, _token, _subscriber, _provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &300);
    let deposits = events_named(&env, &contract_id, "deposited");
    assert_eq!(deposits.len(), 1);
    assert_eq!(data_field(&deposits[0].1, "amount"), sc_i128(300));
    assert_eq!(data_field(&deposits[0].1, "balance_after"), sc_i128(300));

    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), AMOUNT);
    let debits = events_named(&env, &contract_id, "balance_debited");
    assert_eq!(debits.len(), 1);
    assert_eq!(data_field(&debits[0].1, "amount"), sc_i128(AMOUNT));
    assert_eq!(data_field(&debits[0].1, "balance_after"), sc_i128(50));

    client.cancel(&id);
    let refunds = events_named(&env, &contract_id, "balance_refunded");
    assert_eq!(refunds.len(), 1);
    assert_eq!(data_field(&refunds[0].1, "amount"), sc_i128(50));
    assert_eq!(data_field(&refunds[0].1, "balance_after"), sc_i128(0));
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(0));
}

fn events_named(
    env: &Env,
    contract: &Address,
    name: &str,
) -> std::vec::Vec<(std::vec::Vec<ScVal>, ScVal)> {
    let name = ScVal::Symbol(ScSymbol::try_from(name).unwrap());
    env.events()
        .all()
        .filter_by_contract(contract)
        .events()
        .iter()
        .filter_map(|event| {
            let ContractEventBody::V0(body) = &event.body;
            if body.topics.first() != Some(&name) {
                return None;
            }
            Some((body.topics[1..].to_vec(), body.data.clone()))
        })
        .collect()
}

fn data_field(data: &ScVal, field: &str) -> ScVal {
    let ScVal::Map(Some(entries)) = data else {
        panic!("event data is not a map: {data:?}");
    };
    let key = ScVal::Symbol(ScSymbol::try_from(field).unwrap());
    entries
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| entry.val.clone())
        .unwrap_or_else(|| panic!("event data has no `{field}` field: {data:?}"))
}

fn sc_i128(value: i128) -> ScVal {
    ScVal::I128(Int128Parts {
        hi: (value >> 64) as i64,
        lo: value as u64,
    })
}

#[test]
fn prepaid_pause_and_resume_shift_due_date_without_debit() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    client.pause(&id);
    env.ledger().set_timestamp(START + PERIOD * 5);
    let paused = client.try_charge(&id).unwrap_err().unwrap();
    assert_eq!(paused, ForgeError::InvalidInput);
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(AMOUNT));
    assert_eq!(token_client.balance(&provider), 0);

    client.resume(&id);
    let due = client.get_subscription(&id).last_charged + PERIOD;
    env.ledger().set_timestamp(due);
    assert_eq!(client.charge(&id), AMOUNT);
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(0));
    assert_eq!(token_client.balance(&subscriber), 10_000 - AMOUNT);
}
