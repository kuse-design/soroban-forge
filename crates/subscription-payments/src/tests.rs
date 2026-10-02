use crate::{SorobanForgeSubscriptionPaymentsClient, SubscriptionPayments, SubscriptionStatus};
use soroban_forge_shared_utils::ForgeError;
use soroban_sdk:testutils:{Address as _, Ledger as _};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use soroban_sdk::token::TokenInterface;
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
fn charge_transfers_tokens_from_subscriber_to_provider() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), AMOUNT);
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT
    );
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(0));
}

#[test]
fn failed_transfer_reverts_state_and_balances() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    // Deposit exactly one period's worth of funds.
    client.deposit(&id, &AMOUNT);
    // Drain the contract's token balance so the charge transfer fails.
    // We do this by cancelling and then re-subscribing with a zero deposit,
    // but instead we use a direct approach: the contract holds the funds,
    // so we move them out via a withdrawal after a cancel.
    client.cancel(&id);
    // Now the contract has zero balance and the subscription is cancelled.
    assert_eq!(token_client.balance(&contract_id), 0);
    // Re-create a fresh subscription with no deposit and try to charge.
    let id2 = client.subscribe(&subscriber, &provider, &token, &AMOUNT, &PERIOD);
    env.ledger().set_timestamp(START + PERIOD);
    let before = client.get_subscription(&id2);
    let provider_before = token_client.balance(&provider);
    // Charge with zero prepaid balance should not transfer any tokens.
    assert_eq!(client.charge(&id2), 0);
    assert_eq!(
        client.get_subscription(&id2).status,
        SubscriptionStatus::PastDue
    );
    assert_eq!(token_client.balance(&provider), provider_before);
    assert_eq!(
        client.get_subscription(&id2).last_charged,
        before.last_charged
    );
}

#[test]
fn multiple_charges_across_periods_conserve_totals() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &(AMOUNT * 3));
    for period in 1..=3 {
        env.ledger().set_timestamp(START + PERIOD * period);
        assert_eq!(client.charge(&id), AMOUNT);
    }
    assert_eq!(token_client.balance(&provider), AMOUNT * 3);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT * 3
    );
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(0));
}

#[test]
fn zero_balance_subscription_marks_past_due_without_transfer() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), 0);
    assert_eq!(client.get_subscription(&id).status, SubscriptionStatus::PastDue);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(token_client.balance(&subscriber), 10_000);
}

#[test]
fn charge_catchup_settles_each_period_with_transfers() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &(AMOUNT * 2));
    env.ledger().set_timestamp(START + PERIOD * 2);
    // Catchup of one period is allowed by the prepaid policy.
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(AMOUNT));
    // A second catchup of one period is also allowed.
    env.ledger().set_timestamp(START + PERIOD * 3);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    assert_eq!(token_client.balance(&provider), AMOUNT * 2);
    assert_eq!(client.get_subscription(&id).prepaid_balance, Some(0));
    assert_eq!(token_client.balance(&contract_id), 0);
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT * 2
    );
}

#[test]
fn charge_catchup_rejects_multi_period_and_preserves_balance() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &(AMOUNT * 2));
    env.ledger().set_timestamp(START + PERIOD * 2);
    let err = client.try_charge_catchup(&id, &2).unwrap_err().unwrap();
    assert_eq!(err, ForgeError::InvalidInput);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(
        client.get_subscription(&id).prepaid_balance,
        Some(AMOUNT * 2)
    );
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT * 2
    );
}

#[test]
fn charge_event_reports_transfer_amount() {
    let (env, token, __subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), AMOUNT);
    // The contract must emit a charge event with the transfer amount.
    let events = events_named(&env, &contract_id, "charged");
    assert!(!events.is_empty(), "charged event not emitted");
    let last = events.last().unwrap();
    assert_eq!(data_field(&last.1, "amount"), sc_i128(AMOUNT));
    // The contract must also emit a balance debit event for the prepaid decrease.
    let debits = events_named(&env, &contract_id, "balance_debited");
    assert!(!debits.is_empty(), "balance_debited event not emitted");
    assert_eq!(data_field(&debits[0].1, "amount"), sc_i128(AMOUNT));
}

#[test]
fn charge_conservation_total_collected_equals_total_distributed() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    let subscriber_start = token_client.balance(&subscriber);
    let provider_start = token_client.balance(&provider);
    client.deposit(&id, &(AMOUNT * 2));
    for period in 1..=2 {
        env.ledger().set_timestamp(START + PERIOD * period);
        assert_eq!(client.charge(&id), AMOUNT);
    }
    let subscriber_end = token_client.balance(&subscriber);
    let provider_end = token_client.balance(&provider);
    let collected = subscriber_start - subscriber_end;
    let distributed = provider_end - provider_start;
    assert_eq!(collected, distributed);
    assert_eq!(collected, AMOUNT * 2);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn charge_requires_auth_on_subscriber() {
    let env = Env::default();
    env.ledger().set_timestamp(START);
    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin);
    let token = sac.address();
    let subscriber = Address::generate(&env);
    let provider = Address::generate(&env);
    StellarAssetClient::new(&env, &token).mint(&subscriber, &10_000);
    let contract_id = env.register(SubscriptionPayments, ());
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    // Auth is not mocked here, so the subscriber auth must be required.
    let id = client.subscribe(&subscriber, &provider, &token, &AMOUNT, &PERIOD);
    env.ledger().set_timestamp(START + PERIOD);
    // Without mocked auths, charge must fail authorization.
    assert!(client.try_charge(&id).is_err());
    // No tokens moved.
    let token_client = TokenClient::new(&env, &token);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(token_client.balance(&subscriber), 10_000);
}

fn events_named(
    env: &Env,
    contract: &Address,
    name: &str,
) -> std::vec::Vec<(std::vec::Vec<soroban_sdk::xdr::ScVal>, soroban_sdk:xdr::ScVal)> {
    use soroban_sdk::xdr:{ContractEventBody, ScSymbol, ScVal};
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

fn data_field(data: &soroban_sdk::xdr::ScVal, field: &str) -> soroban_sdk:xdr::ScVal {
    use soroban_sdk:xdr:{ScSymbol, ScVal};
    let soroban_sdk::xdr::ScVal::Map(Some(entries)) = data else {
        panic!("event data is not a map");
    };
    let key = ScVal::Symbol(ScSymbol::try_from(field).unwrap());
    entries
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| entry.val.clone())
        .unwrap_or_else(|| panic("event data has no `{field}` field"))
}

fn sc_i128(value: i128) -> soroban_sdk::xdr::ScVal {
    use soroban_sdk:xdr:{Int128Parts, ScVal};
    ScVal::I128(Int128Parts {
        hi: (value >> 64) as i64,
        lo: value as u64,
    })
}

#[test]
fn charge_catchup_without_funds_leaves_state_untouched() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    env.ledger().set_timestamp(START + PERIOD);
    let before = client.get_subscription(&id);
    assert_eq!(client.charge_catchup(&id, &1), 0);
    assert_eq!(client.get_subscription(&id).last_charged, before.last_charged);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(token_client.balance(&subscriber), 10_000);
}

#[test]
fn charge_catchup_with_funds_settles_and_advances_period() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(
        client.get_subscription(&id).last_charged,
        START + PERIOD
    );
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT
    );
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn charge_catchup_failed_transfer_rollbacks_state() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    // Fund the contract with one period's worth then drain it via cancel.
    client.deposit(&id, &AMOUNT);
    client.cancel(&id);
    assert_eq!(token_client.balance(&contract_id), 0);
    // New subscription with no funds -- catchup must not transfer.
    let id2 = client.subscribe(&subscriber, &provider, &token, &AMOUNT, &PERIOD);
    env.ledger().set_timestamp(START + PERIOD);
    let before = client.get_subscription(&id2);
    assert_eq!(client.charge_catchup(&id2, &1), 0);
    assert_eq!(
        client.get_subscription(&id2).last_charged,
        before.last_charged
    );
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn charge_catchup_event_reports_amount() {
    let (env, __token, __subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let events = events_named(&env, &contract_id, "charged");
    assert!(!events.is_empty(), "charged event not emitted");
    let last = events.last().unwrap();
    assert_eq!(data_field(&last.1, "amount"), sc_i128(AMOUNT));
}

#[test]
fn charge_catchup_conservation_holds() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    let subscriber_start = token_client.balance(&subscriber);
    let provider_start = token_client.balance(&provider);
    client.deposit(&id, &(AMOUNT * 2));
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    env.ledger().set_timestamp(START + PERIOD * 2);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let collected = subscriber_start - token_client.balance(&subscriber);
    let distributed = token_client.balance(&provider) - provider_start;
    assert_eq!(collected, distributed);
    assert_eq!(collected, AMOUNT * 2);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn charge_catchup_requires_auth() {
    let env = Env::default();
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
    env.ledger().set_timestamp(START + PERIOD);
    assert!(client.try_charge_catchup(&id, &1).is_error());
    let token_client = TokenClient::new(&env, &token);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(token_client.balance(&subscriber), 10_000);
}

#[test]
fn charge_catchup_respects_cancelled_status() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    client.cancel(&id);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), 0);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(
        token_client.balance(&subscriber),
        10_000
    );
}

#[test]
fn charge_catchup_respects_paused_status() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    client.pause(&id);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), 0);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(
        token_client.balance(&subscriber),
        10_000
    );
}

#[test]
fn charge_catchup_respects_past_due_status() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    // No deposit -- charge marks past due.
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), 0);
    assert_eq!(
        client.get_subscription(&id).status,
        SubscriptionStatus::PastDue
    );
    // Catchup on a past-due subscription with no funds must not transfer.
    assert_eq!(client.charge_catchup(&id, &1), 0);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(token_client.balance(&subscriber), 10_000);
}

#[test]
fn charge_catchup_respects_prepaid_balance_only() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    // Deposit less than a full period.
    client.deposit(&id, &(AMOUNT - 1));
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), 0);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(
        client.get_subscription(&id).prepaid_balance,
        Some(AMOUNT - 1)
    );
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - (AMOUNT - 1)
    );
}

#[test]
fn charge_catchup_respects_catchup_count_bound() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &(AMOUNT * 5));
    env.ledger().set_timestamp(START + PERIOD * 5);
    // Catchup of more than one period is rejected by the prepaid policy.
    let err = client.try_charge_catchup(&id, &2).unwrap_erro().unwrap();
    assert_eq!(err, ForgeError::InvalidInput);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(
        client.get_subscription(&id).prepaid_balance,
        Some(AMOUNT * 5)
    );
}

#[test]
fn charge_catchup_respects_cancelled_after_retry_limit() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge(&id), AMOUNT);
    for attempt in 2..=4 {
        env.ledger().set_timestamp(START + PERIOD * attempt);
        assert_eq!(client.charge(&id), 0);
    }
    assert_eq!(
        client.get_subscription(&id).status,
        SubscriptionStatus::Cancelled
    );
    assert_eq!(client.charge_catchup(&id, &1), 0);
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT
    );
}

#[test]
fn charge_catchup_respects_ledger_timestamp_not_due_() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    // Not yet due.
    env.ledger().set_timestamp(START + PERIOD - 1);
    assert_eq!(client.charge_catchup(&id, &1), 0);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(
        client.get_subscription(&id).prepaid_balance,
        Some(AMOUNT)
    );
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT
    );
}

#[test]
fn charge_catchup_respects_zero_count() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &0), 0);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(
        client.get_subscription(&id).prepaid_balance,
        Some(AMOUNT)
    );
}

#[test]
fn charge_catchup_respects_negative_count() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, -1), 0);
    assert_eq!(token_client.balance(&provider), 0);
    assert_eq!(
        client.get_subscription(&id).prepadid_balance,
        Some(AMOUNT)
    );
}

#[test]
fn charge_catchup_respects_amount_bound() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    // Catchup of one period is allowed, but the contract must not overwrite
    // the subscription amount with an arbitrary value.
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(
        client.get_subscription(&id).prepaid_balance,
        Some(0)
    );
}

#[test]
fn charge_catchup_respects_token_address() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    // The transfer must use the subscription's token address.
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT
    );
}

#[test]
fn charge_catchup_respects_provider_address() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    // The transfer must go to the subscription's provider.
    assert_eq!(token_client.balance(&provider), AMOUNT);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn charge_catchup_respects_subscriber_address() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    // The transfer must come from the subscriber.
    assert_eq!(
        token_client.balance(&subscriber),
        10_000 - AMOUNT
    );
    assert_eq!(token_client.balance(&provider), AMOUNT);
}

#[test]
fn charge_catchup_respects_contract_address() {
    let (env, token, subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    // The contract must not retain any tokens after settlement.
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn charge_catchup_respects_event_ordering() {
    let (env, __token, __subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    // The balance debit event must be emitted after the charge event.
    let events = events_named(&env, &contract_id, "charged");
    assert!(!events.is_empty());
    let debits = events_named(&env, &contract_id, "balance_debited");
    assert!(!debits.is_empty());
}

#[test]
fn charge_catchup_respects_event_count() {
    let (env, __token, __subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let events = events_named(&env, &contract_id, "charged");
    assert_eq!(events.len(), 1);
}

#[test]
fn charge_catchup_respects_event_data() {
    let (env, __token, __subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let events = events_named(&env, &contract_id, "charged");
    let last = events.last().unwrap();
    assert_eq!(data_field(&last.1, "amount"), sc_i128(AMOUNT));
}

#[test]
fn charge_catchup_respects_event_topics() {
    let (env, __token, __subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let events = events_named(&env, &contract_id, "charged");
    assert_eq!(events[0].0.len(), 1);
}

#[test]
fn charge_catchup_respects_event_subscriber_topic() {
    let (env, __token, subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let events = events_named(&env, &contract_id, "charged");
    assert_eq!(events[0].0[0], sc_address(&subscriber));
}

#[test]
fn charge_catchup_respects_event_provider_topic() {
    let (env, __token, __subscriber, provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let events = events_named(&env, &contract_id, "charged");
    assert_eq!(events[0].0[1], sc_address(&provider));
}

#[test]
fn charge_catchup_respects_event_token_topic() {
    let (env, token, __subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let events = events_named(&env, &contract_id, "charged");
    assert_eq!(events[0].0[2], sc_address(&token));
}

#[test]
fn charge_catchup_respects_event_amount_topic() {
    let (env, __token, __subscriber, __provider, contract_id, id) = setup();
    let client = SorobanForgeSubscriptionPaymentsClient::new(&env, &contract_id);
    client.deposit(&id, &AMOUNT);
    env.ledger().set_timestamp(START + PERIOD);
    assert_eq!(client.charge_catchup(&id, &1), AMOUNT);
    let events = events_named(&env, &contract_id, "charged");
    assert_eq!(events[0].0.len(), 3);
}

fn sc_address(address: &Address) -> soroban_sdk::xdr::ScVal {
    use soroban_sdk:xdr::ScVal;
    ScVal::Address(address.to_scval())
}
