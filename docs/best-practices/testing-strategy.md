# Testing Strategy

## Unit Tests

Each contract should have inline unit tests for core business logic.

```rust
#{cfg(test)}
mod tests {
    use super::*;
    // ... tests
}
```

## Integration Tests

Located in `tests/integration/`. Spin up a soroban environment and interact with deployed WASM.

## Cross-Contract Composition Tests

The `test-utils` crate provides a `composition` module with a `CrossContractHarness` for verifying interactions between multiple contracts in a single Soroban `Env`. The harness deploys real contract WASM and exercises cross-contract calls via `try_invoke_contract`.

### Tested Composition Patterns

- **Escrow + Vesting:** Vesting schedule funds escrow for milestone payments.
- **Multi-sig + Escrow:** Multi-sig controls escrow release authority.
- `DAO* + Multi-sig:** DAO proposal executes a multi-sig transaction.
- **Marketplace + Vesting:** Royalties distributed to a vesting schedule.
- `Subscription + Escrow:** Subscription deposits are held in escrow.

### Property Testing

Cross-contract property tests use `proptest` to generate randomized sequences of cross-contract calls and verify:

- **Conservation properties:** total value is preserved across contract boundaries.
- **Failure isolation:** a failure in one contract does not corrupt the state of others.
- `Authorization:** unauthorized cross-contract calls are rejected.

### Example

```rust
use soroban-forge-test-utils::composition::CrossContractHarness;

#`[cfg(test)]
fn test_milestone_vesting_escrow() {
    let harness = CrossContractHarness::new();
    harness.setup_milestone_vesting_escrow();
    harness.run_milestone_property_test();
}
```

## Fuzzing

Consider using `cargo-fuzz` for parsing inputs and complex state machines.

## Chaos Testing

Persistent-storage contracts in this workspace must keep their records alive
between multi-step interactions. A silent failure mode is an entry whose TTL
expires between two legitimate calls: the next read returns `NotFound` (or
worse, a recreated default), corrupting state without surfacing an error.

Use the shared TTL chaos harness in
[`soroban-forge-test-utils::ttl`](crates/test-utils/src/ttl.rs) to exercise
contracts across arbitrary ledger gaps:

- [`advance_ledger`](crates/test-utils/src/ttl.rs) moves the test [`Env`]
  ledger sequence forward deterministically.
- [`entry_is_live`](crates/test-utils/src/ttl.rs) and
  [`entry_ttl`](crates/test-utils/src/ttl.rs) introspect a contract's
  persistent storage TTL from the test harness.
- [`chaos_drive`](crates/test-utils/src/ttl.rs) runs a seeded schedule of
  randomized operations and ledger advances against any contract that
  implements the [`ChaosTarget`](crates/test-utils/src/ttl.rs) trait. The same
  seed reproduces the same trace, so CI failures are replayable.

Demo suites for the two richest persistent-storage contracts are in:

- `crates/escrow/src/ttl_chaos.rs` — create → deposit → release/refund over
  arbitrary ledger gaps.
- `crates/multi-sig-wallet/src/ttl_chaos.rs` — submit → confirm → execute
  over arbitrary ledger gaps.

Each demo asserts the **no `NotFound` during legitimate flows** property and
prints a sample trace for inspection. When a storage refactor breaks TTL
discipline, the harness reports the seed and the exact step so the violation
can be replayed locally.

Guidelines for adding a new contract to the harness:

1. Define a small, closed enum of operations (`Create`, `Deposit`, ...).
2. Implement `ChaosTarget` with `enabled_ops` that only exposes operations
   valid in the current state.
3. In `apply_op`, classify contract errors: expected state-machine failures
   are `ExpectedFailure`; `ForgeError::NotFound` on a record that should exist
   is a `Violation`.
4. In `check_invariants`, assert every persistent record touched by the
   legitimate flow remains live via `entry_is_live`.
5. Pin the seed and assert `assert_no_violations(&trace)`.

## Coverage

Target >= 90% line coverage for stable contracts.

## CI Gates

All PRs must pass:
- `cargo test --workspace`:
- `cargo test -p soroban-forge-test-utils --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked - -D warnings`
- `cargo fmt --all -- --check`
