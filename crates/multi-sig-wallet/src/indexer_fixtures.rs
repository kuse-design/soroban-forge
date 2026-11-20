//! Deterministic ground-truth events for the multi-signature wallet.
//!
//! The scenario below drives submit, confirm, and execute against a real
//! Soroban test environment. The committed JSON is regenerated before it is
//! compared, so event-schema drift cannot be silently accepted.

extern crate std;

use crate::{MultiSigWallet, SorobanForgeMultiSigWalletClient};
use serde_json::Value;
use soroban_forge_test_utils::MockTarget;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::xdr::{self, WriteXdr};
use soroban_sdk::{Address, Bytes, Env, Vec as SorobanVec};
use std::format;
use std::path::Path;
use std::string::ToString;
use std::vec::Vec;

const XDR_LIMITS: xdr::Limits = xdr::Limits {
    depth: 500,
    len: 0x1_000_000,
};
const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../indexer/fixtures");
const FIXTURE_FILE: &str = "multi-sig-wallet-events.json";

fn capture(env: &Env, contract: &Address, events: &mut Vec<Value>) {
    for (index, event) in env.events().all().events().iter().enumerate() {
        let xdr::ContractEventBody::V0(body) = &event.body;
        events.push(serde_json::json!({
            "type": "contract",
            "ledger": index,
            "ledgerClosedAt": "2026-09-23T00:00:00.000Z",
            "contractId": contract.to_string().to_string(),
            "id": format!("0-{index}"),
            "pagingToken": format!("0-{index}"),
            "inSuccessfulContractCall": true,
            "eventIndex": index,
            "transactionHash": "0000000000000000000000000000000000000000000000000000000000000000",
            "operationIndex": 0,
            "topic": body.topics.iter().map(|v| v.to_xdr_base64(XDR_LIMITS).unwrap()).collect::<std::vec::Vec<_>>(),
            "value": body.data.to_xdr_base64(XDR_LIMITS).unwrap(),
        }));
    }
}

fn generate() -> Value {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(MultiSigWallet, ());
    let target = Address::generate(&env);
    env.register_at(&target, MockTarget, ());
    let client = SorobanForgeMultiSigWalletClient::new(&env, &contract);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);
    let owner_c = Address::generate(&env);
    let owners = SorobanVec::from_array(&env, [owner_a.clone(), owner_b.clone(), owner_c]);
    client.initialize(&owners, &2);

    let payload = Bytes::from_slice(&env, b"fixture");
    let tx_id = client.submit(&owner_a, &target, &payload, &None);
    let mut events = std::vec::Vec::new();
    capture(&env, &contract, &mut events);
    client.confirm(&tx_id, &owner_b);
    capture(&env, &contract, &mut events);
    client.confirm(&tx_id, &owner_a);
    capture(&env, &contract, &mut events);
    client.execute(&tx_id);
    capture(&env, &contract, &mut events);

    serde_json::json!({
        "schema": 1,
        "description": "Deterministic submit/confirm/execute events from the Soroban Forge multi-signature wallet.",
        "contract": { "name": "soroban-forge-multi-sig-wallet", "id": contract.to_string().to_string() },
        "events": events,
    })
}

#[test]
fn indexer_fixtures_regenerate_and_are_stable() {
    let fixture = generate();
    assert_eq!(
        fixture,
        generate(),
        "fixture generation is not deterministic"
    );
    let path = Path::new(FIXTURE_DIR).join(FIXTURE_FILE);
    std::fs::create_dir_all(FIXTURE_DIR).unwrap();
    let pretty = format!("{}\n", serde_json::to_string_pretty(&fixture).unwrap());
    if std::fs::read_to_string(&path).ok().as_deref() != Some(pretty.as_str()) {
        std::fs::write(&path, pretty).unwrap();
        panic!("fixture changed; review and commit {}", path.display());
    }
}
