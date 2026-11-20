# Soroban Forge Shared Error Reference

The [`ForgeError`](../../crates/shared-utils/src/errors.rs) enum provides a unified, cross-contract error domain for Soroban Forge. It ensures consistent error semantics across CLI tools, indexers, and client SDKs.

Error codes start at `1` (`0` is reserved by the Soroban host runtime).

---

## Error Catalog

| Code | Variant | Description | Contract Contexts |
| :---: | :--- | :--- | :--- |
| `1` | `Unauthorized` | Caller lacks authorization for the action. | Escrow, Vesting, Multi-Sig, DAO, Subscription, Royalties |
| `2` | `NotFound` | The requested entity or configuration does not exist. | Escrow, Vesting, Multi-Sig, DAO, Subscription, Royalties |
| `3` | `InvalidInput` | One or more input arguments failed validation. | Escrow, Vesting, Multi-Sig, DAO, Subscription, Royalties |
| `4` | `InsufficientFunds` | Insufficient contract balance to satisfy operation. | Multi-Sig Wallet, Subscription Payments |
| `5` | `AlreadyInitialized` | Entity or configuration was already initialized. | Multi-Sig Wallet, DAO Governance |
| `6` | `NotInitialized` | Contract requires configuration before executing this call. | Multi-Sig Wallet, DAO Governance |
| `7` | `DeadlineReached` | Operation attempted after its deadline or before due time. | Escrow, Vesting, Multi-Sig, DAO |
| `8` | `InsufficientAllowance` | Required token allowance is lower than amount spent. | Shared token utilities |
| `9` | `ArithmeticOverflow` | Mathematical computation overflowed integer bounds. | Escrow, Vesting, Multi-Sig, DAO, Subscription, Royalties |
| `10` | `Custom` | Contract-specific error not mapped to standard variants. | Generic extension |
| `11` | `TokenTransferFailed` | SEP-41 token invocation failed (transfer rejected or aborted). | Escrow, Vesting, Multi-Sig, DAO, Subscription, Royalties |
| `12` | `ContractInvocationFailed` | Cross-contract execution reverted or host aborted. | Multi-Sig Wallet, DAO Governance |
| `13` | `WithdrawalLimitExceeded` | Withdrawal exceeds configured rolling velocity limit. | Multi-Sig Wallet |
| `14` | `SubscriptionPastDue` | Subscription is past due and must be caught up. | Subscription Payments |
| `15` | `ProposerCooldown` | Proposer reached maximum allowed concurrent active proposals. | DAO Governance |

---

## Detailed Error Semantics

### `ForgeError::Unauthorized` (1)
Emitted when an identity check fails before or during contract logic execution.
- **Escrow**: Caller attempting to cancel, release, or refund without being the designated party.
- **Vesting**: Non-funder calling `revoke` or `reassign_beneficiary`.
- **Multi-Sig**: Non-owner calling `submit`, `confirm`, `reject`, or governance functions.
- **DAO**: Non-proposer calling `cancel_proposal`.
- **Subscription**: Non-subscriber calling `pause`, `resume`, `cancel`, or `set_quotas`.

### `ForgeError::NotFound` (2)
Emitted when an identifier does not resolve to an existing record.
- Non-existent `escrow_id`, `schedule_id`, `tx_id`, `proposal_id`, or `subscription_id`.
- Querying unconfigured royalty records or missing custody balances.

### `ForgeError::InvalidInput` (3)
Emitted when an argument violates validation constraints or state preconditions.
- Non-positive amounts (`amount <= 0`).
- Non-positive time durations (`period == 0`, `timeout == 0`, `window_seconds == 0`).
- State mismatch: calling `release` on an unfunded escrow, or `confirm` on an executed transaction.
- Duplicate entries: duplicate owners in multi-sig initialization, duplicate tokens in an escrow basket, duplicate metrics in subscription quotas.
- Exceeding bounds: `seller_bps > 10_000`, `max_periods > 32`, `sales.len() > 20`, `tranches.len() > 32`.

### `ForgeError::InsufficientFunds` (4)
Emitted when a multi-sig withdrawal exceeds custodied token balance, or when a prepaid subscription withdrawal exceeds deposited balance.

### `ForgeError::AlreadyInitialized` (5)
Emitted when invoking one-time initialization functions more than once:
- `MultiSigWallet::initialize`
- `DaoGovernance::initialize`
- `DaoGovernance::configure_bond`

### `ForgeError::NotInitialized` (6)
Emitted when an operation requires prior configuration:
- Calling `MultiSigWallet::submit` on an uninitialized wallet.
- Calling `DaoGovernance::vote` before setting the governance token.
- Calling `DaoGovernance::propose` before configuring the proposal bond.

### `ForgeError::DeadlineReached` (7)
Emitted when a time condition is violated:
- Voting after `voting_ends`.
- Calling `refund_expired` before timeout has elapsed.
- Submitting or confirming an expired multi-sig transaction.
- Dependent proposal required an unexecuted prerequisite.

### `ForgeError::ArithmeticOverflow` (9)
Emitted when an arithmetic calculation would exceed maximum integer bounds (`i128::MAX` or `u64::MAX`).

### `ForgeError::TokenTransferFailed` (11)
Emitted when a SEP-41 token contract invocation fails. All token failures (insufficient balance, missing trustline, undeployed token contract) are bucketed here to avoid ambiguous error forwarding.

### `ForgeError::ContractInvocationFailed` (12)
Emitted when a target cross-contract call reverts:
- Multi-sig wallet executing an opaque or typed cross-contract call.
- DAO governance executing an approved proposal action against a target contract.

### `ForgeError::WithdrawalLimitExceeded` (13)
Emitted when a multi-sig withdrawal exceeds the rolling window token velocity limit.

### `ForgeError::SubscriptionPastDue` (14)
Emitted when an operation cannot proceed because a subscription is in arrears (`PastDue`).

### `ForgeError::ProposerCooldown` (15)
Emitted when a proposer attempts to create a proposal while already having 5 concurrent active proposals (`DEFAULT_MAX_ACTIVE_PROPOSALS`).
