use soroban_forge_dao_governance::{
    BondState, DaoGovernance, ProposalState, SorobanForgeDaoGovernanceClient,
};
use soroban_forge_shared_utils::ForgeError;
use soroban_forge_test_utils::{MockTarget, MockTargetClient};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{Address, Bytes, Env};
use std::string::String;
use std::vec::Vec;

const START: u64 = 1_000_000;
const VOTING_DURATION: u64 = 100;
const PROPOSAL_BOND: i128 = 100;

/// Test scenario: DAO dispatches an approved action to a downstream target.
///
/// Flow:
/// 1. A proposer posts a token bond and a voter approves an action.
/// 2. The DAO finalises the proposal after the deterministic voting period.
/// 3. The target fails on its configured first call.
/// 4. Verify proposal state, target state, bond custody, and the independent
///    token-balance mirror are unchanged by the failed downstream call.
/// 5. Clear the failure point and retry, checking the execution event and
///    conservation again.
#[test]
fn dao_dispatch_failure_rolls_back_and_retry_composes_successfully() {
    let env = soroban_forge_test_utils::new_env();
    env.ledger().set_timestamp(START);
    let accounts = soroban_forge_test_utils::TestAccounts::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_admin = StellarAssetClient::new(&env, &token);
    let token_client = TokenClient::new(&env, &token);
    let dao_id = env.register(DaoGovernance, ());
    let dao = SorobanForgeDaoGovernanceClient::new(&env, &dao_id);
    let target_id = env.register(MockTarget, ());
    let target = MockTargetClient::new(&env, &target_id);

    dao.initialize(&token);
    dao.configure_bond(&token, &PROPOSAL_BOND, &accounts.deployer);
    token_admin.mint(&accounts.user1, &1_000);
    token_admin.mint(&accounts.user2, &500);

    let balance_holders = [
        accounts.user1.clone(),
        accounts.user2.clone(),
        dao_id.clone(),
        accounts.deployer.clone(),
        target_id.clone(),
    ];
    let initial_supply_mirror = balance_holders
        .iter()
        .map(|holder| token_client.balance(holder))
        .sum::<i128>();

    let action = Bytes::from_slice(&env, b"settle-royalties");
    let proposal_id = dao.propose(
        &accounts.user1,
        &target_id,
        &action,
        &VOTING_DURATION,
        &soroban_sdk::Vec::new(&env),
        &None,
    );
    dao.vote(&proposal_id, &accounts.user2, &true);
    env.ledger().set_timestamp(START + VOTING_DURATION + 1);

    // First call finalises the passed vote; dispatch happens on the next call.
    dao.execute(&proposal_id);
    assert_eq!(
        dao_event_names(&env, &dao_id),
        vec![String::from("finalised")]
    );
    assert_eq!(
        dao.get_proposal(&proposal_id).state,
        ProposalState::Succeeded
    );
    assert_eq!(token_client.balance(&dao_id), PROPOSAL_BOND);

    target.set_fail_at(&1);
    let dao_events_before_failure = dao_event_names(&env, &dao_id);
    let proposer_balance_before_failure = token_client.balance(&accounts.user1);
    let dao_balance_before_failure = token_client.balance(&dao_id);
    let balance_mirror_before_failure = balance_holders
        .iter()
        .map(|holder| token_client.balance(holder))
        .sum::<i128>();

    assert_eq!(
        dao.try_execute(&proposal_id).unwrap_err().unwrap(),
        ForgeError::ContractInvocationFailed
    );
    assert!(dao_event_names(&env, &dao_id).is_empty());
    let proposal_after_failure = dao.get_proposal(&proposal_id);
    assert_eq!(proposal_after_failure.state, ProposalState::Succeeded);
    assert_eq!(proposal_after_failure.bond_state, BondState::Posted);
    assert_eq!(
        token_client.balance(&accounts.user1),
        proposer_balance_before_failure
    );
    assert_eq!(token_client.balance(&dao_id), dao_balance_before_failure);
    assert_eq!(token_client.balance(&dao_id), PROPOSAL_BOND);
    assert_eq!(target.count(), 0);
    assert_eq!(target.last_payload(), None);
    assert_eq!(
        balance_holders
            .iter()
            .map(|holder| token_client.balance(holder))
            .sum::<i128>(),
        balance_mirror_before_failure
    );
    assert_eq!(balance_mirror_before_failure, initial_supply_mirror);
    assert_eq!(dao_event_names(&env, &dao_id), dao_events_before_failure);

    target.set_fail_at(&0);
    dao.execute(&proposal_id);
    let events_after_retry = dao_event_names(&env, &dao_id);
    assert_eq!(
        events_after_retry,
        vec![String::from("finalised"), String::from("bond_released")]
    );
    let executed = dao.get_proposal(&proposal_id);
    assert_eq!(executed.state, ProposalState::Executed);
    assert_eq!(executed.bond_state, BondState::Refunded);
    assert_eq!(target.count(), 1);
    assert_eq!(target.last_payload(), Some(action));
    assert_eq!(token_client.balance(&dao_id), 0);
    assert_eq!(token_client.balance(&accounts.user1), 1_000);
    assert_eq!(
        balance_holders
            .iter()
            .map(|holder| token_client.balance(holder))
            .sum::<i128>(),
        initial_supply_mirror
    );
}

fn dao_event_names(env: &Env, dao_id: &Address) -> Vec<String> {
    env.events()
        .all()
        .filter_by_contract(dao_id)
        .events()
        .iter()
        .map(|event| {
            let ContractEventBody::V0(body) = &event.body;
            let Some(ScVal::Symbol(name)) = body.topics.first() else {
                panic!("DAO event topic 0 must be a symbol");
            };
            name.to_utf8_string().expect("event name is valid UTF-8")
        })
        .collect()
}
