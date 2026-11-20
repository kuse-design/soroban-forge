# Soroban Forge — Smart Contract References

Comprehensive, synchronized reference documentation for all smart contracts in the Soroban Forge ecosystem.

Each contract in Soroban Forge implements verified, production-grade WebAssembly smart contracts targeting the Soroban runtime (`wasm32v1-none`, Soroban SDK v28 / Protocol 22+).

---

## Contract Catalog

| Contract | Crate | Primary Focus | Key Features |
| :--- | :--- | :--- | :--- |
| [**Escrow**](./escrow.md) | `soroban-forge-escrow` | Three-party token escrow | Single-token & multi-asset baskets, partial releases, split dispute resolution, post-expiry recovery, participant indexing |
| [**Vesting**](./vesting.md) | `soroban-forge-vesting` | Token release schedules | Linear (cliff + duration) schedules, discrete multi-tranche schedules, funder revocation, beneficiary reassignment |
| [**Multi-Sig Wallet**](./multi-sig-wallet.md) | `soroban-forge-multi-sig-wallet` | Shared custody wallet | $M$-of-$N$ threshold approvals, opaque & typed cross-contract calls, rolling withdrawal velocity limits, single-veto rejections, on-chain governance |
| [**DAO Governance**](./dao-governance.md) | `soroban-forge-dao-governance` | Token-weighted governance | Propose, vote, finalise & dispatch, mandatory proposal anti-spam bonds, iterative DFS dependency graphs, concurrent active proposal caps |
| [**Subscription Payments**](./subscription-payments.md) | `soroban-forge-subscription-payments` | Recurring billing & metered billing | Pull payments, prepaid custody debit, provider opt-in authorization, multi-period atomic catch-up, metered usage quotas & overage pricing, pause/resume |
| [**Marketplace Royalties**](./marketplace-royalties.md) | `soroban-forge-marketplace-royalties` | Creator secondary sale splits | Atomic sale settlements, multi-recipient proportional splits, batch settlement (up to 20 sales), quote simulations, floor rounding |
| [**Shared Errors**](./errors.md) | `soroban-forge-shared-utils` | Canonical error model | Standardized 15-code `ForgeError` enum across all contracts |

---

## Shared Architectural Patterns

All contracts across the workspace share foundational patterns ensuring deterministic execution, fund safety, and interoperability:

### 1. Transfer-Before-State Discipline
Every entrypoint that moves real tokens issues the SEP-41 token transfer **before** committing state changes to contract storage.
- If a token transfer reverts (due to insufficient funds, missing trustline, deauthorized account, or undeployed token contract), the host's automatic transaction rollback reverts all prior storage modifications.
- Funds can never be locked or stranded behind a failed state update.

### 2. Explicit Error Bucketing
All third-party or sub-contract token transfer errors are collapsed into a canonical error code:
- **`ForgeError::TokenTransferFailed` (Code 11)**: Traps token reverts and host aborts. Discarding raw external error discriminants prevents downstream callers from confusing token errors with calling contract errors.
- **`ForgeError::ContractInvocationFailed` (Code 12)**: Traps cross-contract target execution reverts (in DAO execution and Multi-Sig execution), leaving the invoking transaction or proposal in a retryable state.

### 3. Authorization Modeling
- **Direct Authentication**: Signers are authenticated via `Address::require_auth()` at entrypoints. Signatures must match both the principal address and the exact wire arguments.
- **Nested Sub-Invocations**: Functions that transfer tokens from an external account (such as `escrow::deposit`, `subscription_payments::charge`, or `marketplace_royalties::settle_sale`) require caller authorization envelopes that include the nested `transfer` invocation.
- **Implicit Host Self-Authorization**: Payouts originated from a contract's own custody address rely on Soroban's native contract self-authorization; no synthetic external signature is required for outgoing disbursements.

### 4. Storage & TTL Tiering Policy
Contracts balance persistent state durability against instance storage efficiency:
- **Instance Storage**: Used for immutable contract-wide configurations (governance tokens, wallet owners and thresholds, total monotonic ID counters).
- **Persistent Storage**: Used for per-record entities (individual escrows, proposals, transactions, balances, and limit tracking).
- **Standardized Keepers**: Persistent entries are extended via `bump_entry()` using a uniform 30-day policy:
  - `DAY_IN_LEDGERS = 17,280` ledgers (~24 hours at 5-second ledger closes).
  - `BUMP_AMOUNT = 518,400` ledgers (30 days).
  - `BUMP_THRESHOLD = 501,120` ledgers (threshold at 29 days).

---

## Standard Error Reference (`ForgeError`)

The shared error enum [`ForgeError`](./errors.md) standardizes failure modes across all contracts:

| Code | Variant | Meaning | Typical Trigger |
| :---: | :--- | :--- | :--- |
| `1` | `Unauthorized` | Caller lacks permission | Non-owner/non-beneficiary attempting a protected call |
| `2` | `NotFound` | Entity does not exist | Invalid escrow ID, proposal ID, or missing configuration |
| `3` | `InvalidInput` | Argument failed validation | Zero amount, zero duration, duplicate owner/token |
| `4` | `InsufficientFunds` | Insufficient contract balance | Contract balance cannot satisfy requested withdrawal |
| `5` | `AlreadyInitialized` | Entity already initialized | Attempting to re-initialize wallet, bond, or token config |
| `6` | `NotInitialized` | Contract requires configuration | Operating on an uninitialized contract or unconfigured bond |
| `7` | `DeadlineReached` | Action attempted past cutoff | Voting on closed proposal, confirming expired multi-sig tx |
| `8` | `InsufficientAllowance` | Token allowance too low | Token allowance insufficient for requested spend |
| `9` | `ArithmeticOverflow` | Math operation overflowed | Math calculation exceeded integer range |
| `10` | `Custom` | Domain-specific failure | Uncategorized contract-specific failure |
| `11` | `TokenTransferFailed` | SEP-41 token transfer failed | Insufficient balance, missing trustline, undeployed token |
| `12` | `ContractInvocationFailed` | Cross-contract call reverted | Dispatched target contract execution failed |
| `13` | `WithdrawalLimitExceeded` | Velocity limit breached | Withdrawal exceeds rolling window limit |
| `14` | `SubscriptionPastDue` | Subscription is in arrears | Operation blocked while subscription is in `PastDue` |
| `15` | `ProposerCooldown` | Concurrency limit reached | Proposer exceeded max concurrent active proposals (5) |

---

## Further Documentation

- [Escrow Reference](./escrow.md)
- [Vesting Reference](./vesting.md)
- [Multi-Sig Wallet Reference](./multi-sig-wallet.md)
- [DAO Governance Reference](./dao-governance.md)
- [Subscription Payments Reference](./subscription-payments.md)
- [Marketplace Royalties Reference](./marketplace-royalties.md)
- [Shared Error Catalog](./errors.md)
