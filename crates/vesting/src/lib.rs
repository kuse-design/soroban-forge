#![no_std]

//! # Soroban Forge — Vesting contract
//!
//! A token-vesting contract that releases a beneficiary's tokens over time.
//! Two schedule shapes share one id space and one claim path:
//!
//! - **linear** ([`VestingSchedule`], created by `create_schedule`) — an
//!   optional cliff followed by a straight-line ramp to `duration`;
//! - **tranche** ([`TrancheSchedule`], created by `create_tranche_schedule`) —
//!   an explicit, ordered table of discrete unlocks.
//!
//! The two are deliberately **not** merged into one record: the linear
//! record's wire shape and its floor-division math stay exactly as they were,
//! and a tranche schedule lives under its own `DataKey` variant.
//!
//! Timings in both shapes are expressed as **durations in seconds measured
//! from the schedule start** (the ledger timestamp recorded at creation).
//!
//! ## Linear shape
//!
//! ```text
//! start ......... start+cliff ................... start+duration
//!   |             (claims become possible)        (fully vested)
//!   |  Locked    |            Vesting (linear)  |
//! ```
//!
//! The vested amount at ledger time `t` is:
//! - `0` when `t < start + cliff`,
//! - `total_amount` when `t >= start + duration`,
//! - otherwise `total_amount * (t - (start + cliff)) / (duration - cliff)`,
//!   using integer (floor) division so claims never round up.
//!
//! ## Tranche shape
//!
//! A grant agreement of the form "25% at TGE, 25% at +6 months, 50% at +12
//! months" is not expressible as a ramp, so the tranche kind takes the unlock
//! table verbatim: an ordered list of [`Tranche`]s, each an offset from
//! `start` plus the amount that unlocks at it.
//!
//! ```text
//!        t1            t2                t3
//! start   |             |                 |
//!   |     |   tranche 1  |   tranche 2     |   tranche 3
//!   | Locked  (a1)         (a2)              (a3)
//!   |     |  vested:      vested:           vested:
//!   |     |     0  ->     a1  ->            a1+a2  ->  total
//!   +-----+--------------+------------------+---------.
//! ```
//!
//! Offsets are strictly increasing, so the vested amount is a **step function**
//! of the sum of every tranche whose offset has elapsed: `0` before `t1`,
//! `a1` between `t1` and `t2`, `a1 + a2` between `t2` and `t3`, and the full
//! total from `t3` on. Nothing accrues between unlocks, and the vested amount
//! never decreases, so a claim at any moment pays exactly the difference
//! since the last one.
//!
//! Unlock progress is compared on the *elapsed offset* rather than on
//! `start + unlock_at`, so an offset of `u64::MAX` cannot overflow: that
//! tranche simply never unlocks.
//!
//! ## Status
//!
//! Both kinds derive status from the same two inputs — ledger time and the
//! claimed amount: `Locked` before the first unlock, `Vesting` from the first
//! unlock until the final amount is claimed, `Completed` once `claimed ==
//! total_amount`. `Revoked` is reachable only through `revoke`, which
//! freezes the schedule (see *Revocation* below).
//!
//! Authorization model:
//! - `create_schedule` requires the beneficiary; its `funder` argument is
//!   recorded on the linear schedule without authorizing (mirroring escrow's
//!   non-consenting parties). `create_tranche_schedule` also requires its
//!   beneficiary.
//! - `revoke` requires the funder recorded at creation.
//! - `reassign_beneficiary` requires the funder recorded at creation.
//! - `claim` requires the current beneficiary; `claim_for` requires the
//!   explicitly named beneficiary and can pay frozen balances from prior
//!   assignments.
//!
//! ## Revocation
//!
//! `revoke(schedule_id)` terminates a linear schedule before completion:
//! `Locked | Vesting -> Revoked`, permanently stopping further vesting.
//! Only the **funder** recorded at creation may revoke (`require_auth` on
//! it); the beneficiary cannot. The vested amount at the revocation ledger
//! timestamp is frozen into the record (`revoked_vested`): `claim` after
//! revocation succeeds only up to that amount — everything already vested
//! stays claimable — and `claimable` returns `0` once it is claimed.
//! Nothing after the revocation timestamp ever vests.
//!
//! ## Beneficiary reassignment
//!
//! `reassign_beneficiary` is funder-only and applies to linear schedules.
//! At the reassignment timestamp it snapshots the amount vested under the
//! unchanged cliff-and-duration formula. The outgoing beneficiary keeps their
//! vested-but-unclaimed amount; the incoming beneficiary accrues only the
//! increase after that snapshot. Historical balances are stored by schedule
//! id and beneficiary, so `claim_for`/`claimable_for` address former
//! beneficiaries without mixing grants. Reassignment is counted but
//! unlimited, and cannot alter total amount, start, cliff, or duration. A
//! reassignment at the same ledger timestamp gives the new beneficiary zero
//! accrual at that boundary. Revoked schedules reject reassignment; if a
//! reassigned schedule is later revoked, prior frozen balances remain intact
//! and the current beneficiary is capped by `revoked_vested`.
//!
//! Revoking a `Completed` or already-`Revoked` schedule is rejected with
//! [`ForgeError::InvalidInput`], so double-revoke is impossible; a caller
//! other than the funder is rejected by host authorization.
//!
//! Revocation is pure state-machine logic: it writes the frozen amount and
//! the status and moves no tokens. The frozen remainder is settled through
//! the same SEP-41 transfer path as any other claim (issue #50), and the
//! unvested remainder stays custodied by the contract — refunding it to the
//! funder is settlement logic out of scope here. Revocation itself adds no
//! storage tier (issue #55). There is no tranche `revoke`: a tranche table
//! is immutable and fully pre-funded by construction, and an unmet future
//! tranche already never unlocks.
//!
//! ## Upgrade compatibility
//!
//! Adding the funder, revocation, and reassignment fields to the stored linear record is a
//! **storage-breaking upgrade**, following the pattern documented in
//! `docs/contracts/vesting.md`:
//!
//! - `VestingSchedule` contains `funder: Address`, `revoked_vested:
//!   Option<i128>`, `reassignment_vested: i128`, `beneficiary_claimed: i128`,
//!   and `reassignment_count: u32`. Under
//!   Soroban's `#[contracttype]` encoding every struct field is a required
//!   key, so records written by an earlier build do **not**
//!   deserialize into the new type: a deployed contract must migrate or
//!   reset its instance storage when upgrading. The new
//!   `FormerBeneficiaryClaim` key stores frozen historical balances.
//! - The existing `DataKey::Schedule(u64)`,
//!   `DataKey::TrancheSchedule(u64)`, and `DataKey::Count` keys retain their
//!   meaning. `FormerBeneficiaryClaim(u64, Address)` is additive; all remain
//!   in instance storage pending the persistent-storage migration (issue
//!   #55). The SEP-41 settlement path (issue #50) is unchanged.
//! - `claimable`, `claimable_for`, `get_status`, `get_schedule`, and
//!   `get_tranche_schedule` are read-only views.
//!
//! ## Settlement (load-bearing)
//!
//! The contract custodies the configured SEP-41 token and `claim` settles
//! through it: the newly claimable amount is transferred from this contract
//! to the beneficiary, and the schedule is written only after that transfer
//! succeeds. The transfer-before-state ordering mirrors
//! `crates/escrow/src/lib.rs` — a failed transfer (empty contract balance,
//! undeployed token) returns [`ForgeError::TokenTransferFailed`] with
//! `claimed` and `status` untouched. A zero-claim call exits before the
//! transfer, so no empty transfers are ever issued. Soroban's frame rollback
//! is the outer atomicity guarantee: any `Err` returned from `claim` reverts
//! the whole invocation, including sub-invocations.

#[cfg(test)]
extern crate std;

use soroban_forge_shared_utils::ForgeError;
use soroban_sdk::{contract, contractclient, contractimpl, contracttype, token, Address, Env, Vec};

/// Maximum number of tranches a single schedule may carry.
///
/// Grant agreements express unlock tables with a handful of entries (a TGE
/// tranche plus quarterly or half-yearly ones), and every claimable/status
/// call scans the stored table, so the cap bounds the instruction use of a
/// claim. Creation rejects a longer table with [`ForgeError::InvalidInput`]
/// rather than truncating it. The cap is deliberately generous relative to
/// real agreements; raise it only with a reason.
pub const MAX_TRANCHES: u32 = 32;

/// Public interface for the Soroban Forge vesting contract.
///
/// Declared as a `contractclient` trait so SDK consumers (and the TypeScript
/// SDK generator) get a strongly-typed client without coupling to the
/// implementation crate.
#[contractclient(name = "SorobanForgeVestingClient")]
pub trait SorobanForgeVesting {
    /// Create a new vesting schedule for `beneficiary`.
    ///
    /// `cliff` and `duration` are seconds measured from creation
    /// (`cliff <= duration`, `duration > 0`, `total_amount > 0`, and
    /// `funder != beneficiary`). Returns the stable schedule id. The
    /// beneficiary is authorized at creation time; `funder` is recorded on
    /// the schedule without authorizing and is the only party that may
    /// later [`revoke`](Self::revoke) or
    /// [`reassign_beneficiary`](Self::reassign_beneficiary) it.
    fn create_schedule(
        env: Env,
        funder: Address,
        beneficiary: Address,
        token: Address,
        total_amount: i128,
        cliff: u64,
        duration: u64,
    ) -> Result<u64, soroban_forge_shared_utils::ForgeError>;

    /// Create a new tranche (discrete unlock) vesting schedule for
    /// `beneficiary`.
    ///
    /// `tranches` is the unlock table: an ordered list of [`Tranche`]s, each
    /// an offset in seconds from creation plus the amount that unlocks at it.
    /// It must be non-empty, hold at most [`MAX_TRANCHES`] entries with
    /// strictly increasing offsets and positive amounts, and sum to at most
    /// `i128::MAX`. The table is validated, stored once, and immutable
    /// afterwards. The id comes from the same counter as
    /// [`create_schedule`](Self::create_schedule), so both kinds share one id
    /// space.
    ///
    /// # Errors
    ///
    /// * [`ForgeError::InvalidInput`] — the table is empty, longer than
    ///   [`MAX_TRANCHES`], holds a non-positive amount, or an offset that does
    ///   not strictly increase.
    /// * [`ForgeError::ArithmeticOverflow`] — the table's amounts sum past
    ///   `i128::MAX`.
    fn create_tranche_schedule(
        env: Env,
        beneficiary: Address,
        token: Address,
        tranches: Vec<Tranche>,
    ) -> Result<u64, soroban_forge_shared_utils::ForgeError>;

    /// Revoke a linear schedule before completion and permanently stop
    /// further vesting.
    ///
    /// Transitions `Locked | Vesting -> Revoked` and freezes the vested
    /// amount at the revocation ledger timestamp: `claim` afterwards
    /// succeeds only up to that amount, and `claimable` returns `0` once it
    /// is claimed. Pure state-machine logic — no tokens move at revocation.
    /// Requires the `funder` recorded at creation; the beneficiary cannot
    /// revoke.
    ///
    /// # Errors
    ///
    /// * [`ForgeError::NotFound`] — no linear schedule with this id (a
    ///   tranche id is `NotFound` here).
    /// * [`ForgeError::InvalidInput`] — the schedule is `Completed` or
    ///   already `Revoked`; double-revoke is impossible.
    /// * [`ForgeError::Unauthorized`] — the funder authorization is missing
    ///   or invalid (a non-funder caller is rejected at the host).
    fn revoke(env: Env, schedule_id: u64) -> Result<(), soroban_forge_shared_utils::ForgeError>;

    /// Reassign a linear schedule's unvested balance to a new beneficiary.
    /// Vested-but-unclaimed tokens at the reassignment timestamp stay with
    /// the old beneficiary; the new beneficiary accrues only from that point.
    /// Requires the funder recorded at creation.
    fn reassign_beneficiary(
        env: Env,
        schedule_id: u64,
        new_beneficiary: Address,
    ) -> Result<(), soroban_forge_shared_utils::ForgeError>;

    /// Read the full linear schedule record (read-only view).
    ///
    /// The record view for the linear kind, mirroring
    /// `get_tranche_schedule` and the workspace's other record views
    /// (`get_escrow`, `get_tx`, `get_proposal`, `get_subscription`) — the
    /// record-view convention of issue #125. Returns the stored record:
    /// `claimed` reflects completed claims, while `status` is the stored
    /// lifecycle state and is refreshed on claim (between claims
    /// `get_status` derives the current one from ledger time).
    ///
    /// # Errors
    ///
    /// * [`ForgeError::NotFound`] — no linear schedule with this id (a
    ///   tranche id is `NotFound` here, and vice versa).
    fn get_schedule(
        env: Env,
        schedule_id: u64,
    ) -> Result<VestingSchedule, soroban_forge_shared_utils::ForgeError>;

    /// Read the full tranche schedule record, immutable unlock table included
    /// (read-only view).
    ///
    /// The tranche kind's record view, mirroring `get_schedule` (the linear
    /// kind's, issue #125); a linear id is `NotFound` here, and vice versa.
    fn get_tranche_schedule(
        env: Env,
        schedule_id: u64,
    ) -> Result<TrancheSchedule, soroban_forge_shared_utils::ForgeError>;

    /// Claim tokens that have vested as of the current ledger time.
    ///
    /// Requires the current beneficiary. Works for both schedule kinds and
    /// transfers that beneficiary's exact vested-but-unclaimed amount. Use
    /// `claim_for` to claim a frozen balance belonging to a former
    /// beneficiary. Returns `0` without issuing a transfer when nothing is
    /// claimable.
    ///
    /// # Errors
    ///
    /// * [`ForgeError::NotFound`] — no schedule with this id.
    /// * [`ForgeError::TokenTransferFailed`] — the token contract rejected
    ///   the payout (insufficient contract balance, undeployed token).
    /// * [`ForgeError::ArithmeticOverflow`] — the claimed total overflowed.
    fn claim(env: Env, schedule_id: u64) -> Result<i128, soroban_forge_shared_utils::ForgeError>;

    /// Claim for an explicit beneficiary, including frozen amounts owed to a
    /// former beneficiary. The named beneficiary must authorize the claim.
    fn claim_for(
        env: Env,
        schedule_id: u64,
        beneficiary: Address,
    ) -> Result<i128, soroban_forge_shared_utils::ForgeError>;

    /// Return the amount currently claimable by `schedule_id` (read-only).
    ///
    /// Returns the current beneficiary's claimable balance. Kind-aware: a
    /// linear id is measured against its cliff and duration, a tranche id
    /// against its unlock table.
    fn claimable(
        env: Env,
        schedule_id: u64,
    ) -> Result<i128, soroban_forge_shared_utils::ForgeError>;

    /// Return the claimable amount for an explicit beneficiary.
    fn claimable_for(
        env: Env,
        schedule_id: u64,
        beneficiary: Address,
    ) -> Result<i128, soroban_forge_shared_utils::ForgeError>;

    /// Read the current lifecycle status of `schedule_id` (read-only).
    fn get_status(
        env: Env,
        schedule_id: u64,
    ) -> Result<VestingStatus, soroban_forge_shared_utils::ForgeError>;
}

/// Lifecycle state of a vesting schedule.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VestingStatus {
    /// Before the cliff has been reached.
    Locked,
    /// Past the cliff; tokens are vesting linearly.
    Vesting,
    /// Fully vested and claimed.
    Completed,
    /// Schedule was terminated by the funder before completion; the vested
    /// amount at the revocation timestamp is frozen.
    Revoked,
}

/// A single token-vesting schedule.
#[contracttype]
#[derive(Clone, Debug)]
pub struct VestingSchedule {
    /// Party that funded the grant; the only address that may `revoke`.
    pub funder: Address,
    /// Recipient of the vested tokens.
    pub beneficiary: Address,
    /// Token contract whose balance is drawn down.
    pub token: Address,
    /// Total amount to vest linearly between `cliff` and `duration`.
    pub total_amount: i128,
    /// Ledger timestamp at which vesting begins (creation time).
    pub start: u64,
    /// Seconds after `start` at which claims become possible.
    pub cliff: u64,
    /// Seconds after `start` at which the schedule is fully vested.
    pub duration: u64,
    /// Total amount already paid across all beneficiaries.
    pub claimed: i128,
    /// Amount vested when the current beneficiary assignment began.
    pub reassignment_vested: i128,
    /// Amount claimed by the current beneficiary during this assignment.
    pub beneficiary_claimed: i128,
    /// Number of successful beneficiary reassignments.
    pub reassignment_count: u32,
    /// Vested amount frozen at revocation (`Some` exactly when `status` is
    /// `Revoked`); `claim` pays only up to it.
    pub revoked_vested: Option<i128>,
    /// Current lifecycle state.
    pub status: VestingStatus,
}

/// One entry of a tranche schedule's unlock table: an amount that becomes
/// claimable at a fixed offset from the schedule start.
///
/// A grant agreement's "25% at TGE, 25% at +6 months, 50% at +12 months"
/// is three of these, not a ramp.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tranche {
    /// Seconds after `start` at which this tranche unlocks. Compared as an
    /// offset, so `u64::MAX` is a valid (never-reached) boundary rather than
    /// an overflow.
    pub unlock_at: u64,
    /// Amount that unlocks at `unlock_at`; must be positive.
    pub amount: i128,
}

/// A vesting schedule whose unlock table is an explicit list of tranches
/// instead of a cliff-plus-ramp.
///
/// A separate record from [`VestingSchedule`] on purpose: the linear record's
/// wire shape stays untouched, and the two kinds sit under distinct
/// `DataKey` variants while sharing one monotonic id counter.
#[contracttype]
#[derive(Clone, Debug)]
pub struct TrancheSchedule {
    /// Recipient of the unlocked tokens.
    pub beneficiary: Address,
    /// Token contract whose balance is drawn down.
    pub token: Address,
    /// Sum of every tranche amount; the amount unlocked at the last unlock.
    pub total_amount: i128,
    /// Ledger timestamp the tranche offsets are measured from (creation
    /// time).
    pub start: u64,
    /// The immutable unlock table, ordered by strictly increasing
    /// `unlock_at`. Non-empty and at most [`MAX_TRANCHES`] long.
    pub tranches: Vec<Tranche>,
    /// Amount already claimed by the beneficiary.
    pub claimed: i128,
    /// Current lifecycle state.
    pub status: VestingStatus,
}

/// Instance-storage keys.
///
/// Both schedule kinds are instance-only entries (see the persistent-storage
/// migration tracked in issue #55) keyed by the same id, so the variants
/// partition the id space: an id addresses a linear record or a tranche
/// record, never both.
#[contracttype]
enum DataKey {
    /// The linear vesting record for `u64` id.
    Schedule(u64),
    /// The tranche vesting record for `u64` id, unlock table included.
    TrancheSchedule(u64),
    /// Monotonic id counter, shared by both kinds.
    Count,
    /// Frozen, unclaimed amount owed to a former beneficiary.
    FormerBeneficiaryClaim(u64, Address),
}

/// A stored schedule of either kind, as returned by [`Vesting::load`].
enum Stored {
    /// A linear (cliff + ramp) schedule.
    Linear(VestingSchedule),
    /// A tranche (discrete unlock table) schedule.
    Tranche(TrancheSchedule),
}

/// The deployable vesting contract.
#[contract]
pub struct Vesting;

#[contractimpl]
impl Vesting {
    /// Create a new vesting schedule and return its stable id.
    ///
    /// Requires `total_amount > 0`, `duration > 0`, `cliff <= duration`, and
    /// `funder != beneficiary`. The beneficiary is authorized at creation
    /// time; `funder` is recorded without authorizing (mirroring escrow's
    /// non-consenting parties) and is the only party that may later revoke or
    /// reassign the schedule.
    pub fn create_schedule(
        env: Env,
        funder: Address,
        beneficiary: Address,
        token: Address,
        total_amount: i128,
        cliff: u64,
        duration: u64,
    ) -> Result<u64, ForgeError> {
        if total_amount <= 0 {
            return Err(ForgeError::InvalidInput);
        }
        if duration == 0 {
            return Err(ForgeError::InvalidInput);
        }
        if cliff > duration {
            return Err(ForgeError::InvalidInput);
        }
        if funder == beneficiary {
            return Err(ForgeError::InvalidInput);
        }
        beneficiary.require_auth();

        let id = Self::next_id(&env)?;
        let start = env.ledger().timestamp();
        let mut schedule = VestingSchedule {
            funder,
            beneficiary,
            token,
            total_amount,
            start,
            cliff,
            duration,
            claimed: 0,
            reassignment_vested: 0,
            beneficiary_claimed: 0,
            reassignment_count: 0,
            revoked_vested: None,
            status: VestingStatus::Locked,
        };
        // Derive the initial status from time (cliff == 0 starts `Vesting`).
        schedule.status = Self::current_status(&schedule, start)?;
        env.storage()
            .instance()
            .set(&DataKey::Schedule(id), &schedule);
        Ok(id)
    }

    /// Create a new tranche vesting schedule and return its stable id.
    ///
    /// `tranches` is the unlock table, ordered by strictly increasing
    /// `unlock_at` offsets in seconds from the creation timestamp. Requires a
    /// non-empty table of at most [`MAX_TRANCHES`] entries, every
    /// `amount > 0`, and a cumulative sum that fits in `i128`; the table is
    /// then stored once and treated as immutable. The beneficiary is
    /// authorized at creation time, mirroring `create_schedule`.
    ///
    /// A first tranche at `unlock_at == 0` is allowed: it unlocks at the
    /// creation timestamp, the "TGE tranche" of a typical grant agreement.
    pub fn create_tranche_schedule(
        env: Env,
        beneficiary: Address,
        token: Address,
        tranches: Vec<Tranche>,
    ) -> Result<u64, ForgeError> {
        let total_amount = Self::validate_tranches(&tranches)?;
        beneficiary.require_auth();

        let id = Self::next_id(&env)?;
        let start = env.ledger().timestamp();
        let mut schedule = TrancheSchedule {
            beneficiary,
            token,
            total_amount,
            start,
            tranches,
            claimed: 0,
            status: VestingStatus::Locked,
        };
        // Derive the initial status from time (a table starting at 0 begins
        // `Vesting`, mirroring the linear `cliff == 0` case).
        schedule.status = Self::tranche_status(&schedule, start)?;
        env.storage()
            .instance()
            .set(&DataKey::TrancheSchedule(id), &schedule);
        Ok(id)
    }

    /// Revoke a linear schedule before completion and permanently stop
    /// further vesting.
    ///
    /// Transitions `Locked | Vesting -> Revoked` and freezes the vested
    /// amount at this ledger timestamp into `revoked_vested`: `claim`
    /// afterwards succeeds only up to that amount, and `claimable` returns
    /// `0` once it is claimed. Pure state-machine logic — no tokens move at
    /// revocation. Requires the `funder` recorded at creation; the
    /// beneficiary cannot revoke.
    ///
    /// # Errors
    ///
    /// * [`ForgeError::NotFound`] — no linear schedule with this id (a
    ///   tranche id is `NotFound` here).
    /// * [`ForgeError::InvalidInput`] — the schedule is `Completed` or
    ///   already `Revoked`; double-revoke is impossible.
    /// * [`ForgeError::Unauthorized`] — the funder authorization is missing
    ///   or invalid (a non-funder caller is rejected at the host).
    pub fn revoke(env: Env, schedule_id: u64) -> Result<(), ForgeError> {
        let mut schedule: VestingSchedule = env
            .storage()
            .instance()
            .get(&DataKey::Schedule(schedule_id))
            .ok_or(ForgeError::NotFound)?;
        schedule.funder.require_auth();
        match schedule.status {
            VestingStatus::Completed | VestingStatus::Revoked => Err(ForgeError::InvalidInput),
            VestingStatus::Locked | VestingStatus::Vesting => {
                let now = env.ledger().timestamp();
                schedule.revoked_vested = Some(Self::vested_amount(&schedule, now)?);
                schedule.status = VestingStatus::Revoked;
                env.storage()
                    .instance()
                    .set(&DataKey::Schedule(schedule_id), &schedule);
                Ok(())
            }
        }
    }

    /// Reassign only a linear schedule's unvested balance.
    ///
    /// The old beneficiary's vested-but-unclaimed amount is frozen under a
    /// schedule-scoped claim key; cliff and duration remain unchanged.
    ///
    /// # Errors
    ///
    /// * [`ForgeError::NotFound`] — no linear schedule with this id.
    /// * [`ForgeError::InvalidInput`] — the schedule is completed or revoked,
    ///   or the new beneficiary is already the current beneficiary or funder.
    /// * [`ForgeError::Unauthorized`] — the funder's authorization is missing
    ///   or invalid.
    pub fn reassign_beneficiary(
        env: Env,
        schedule_id: u64,
        new_beneficiary: Address,
    ) -> Result<(), ForgeError> {
        let mut schedule: VestingSchedule = env
            .storage()
            .instance()
            .get(&DataKey::Schedule(schedule_id))
            .ok_or(ForgeError::NotFound)?;
        if schedule.revoked_vested.is_some() || schedule.claimed >= schedule.total_amount {
            return Err(ForgeError::InvalidInput);
        }
        if new_beneficiary == schedule.beneficiary || new_beneficiary == schedule.funder {
            return Err(ForgeError::InvalidInput);
        }
        schedule.funder.require_auth();

        let now = env.ledger().timestamp();
        let vested = Self::vested_amount(&schedule, now)?;
        let old_entitlement = vested
            .checked_sub(schedule.reassignment_vested)
            .and_then(|amount| amount.checked_sub(schedule.beneficiary_claimed))
            .ok_or(ForgeError::ArithmeticOverflow)?;
        let reassignment_count = schedule
            .reassignment_count
            .checked_add(1)
            .ok_or(ForgeError::ArithmeticOverflow)?;
        let old_key = DataKey::FormerBeneficiaryClaim(schedule_id, schedule.beneficiary.clone());
        let prior_balance: i128 = env.storage().instance().get(&old_key).unwrap_or(0);
        let old_balance = prior_balance
            .checked_add(old_entitlement)
            .ok_or(ForgeError::ArithmeticOverflow)?;

        events::beneficiary_reassigned(
            &env,
            schedule_id,
            &schedule.beneficiary,
            &new_beneficiary,
            old_entitlement,
            reassignment_count,
        );
        if old_balance > 0 {
            env.storage().instance().set(&old_key, &old_balance);
        }
        schedule.beneficiary = new_beneficiary;
        schedule.reassignment_vested = vested;
        schedule.beneficiary_claimed = 0;
        schedule.reassignment_count = reassignment_count;
        env.storage()
            .instance()
            .set(&DataKey::Schedule(schedule_id), &schedule);
        Ok(())
    }

    /// Read the full linear schedule record (read-only view; no state
    /// change).
    ///
    /// Mirrors `get_tranche_schedule`: the stored record, unchanged. A
    /// tranche id is `NotFound` here; use `get_tranche_schedule` for those.
    pub fn get_schedule(env: Env, schedule_id: u64) -> Result<VestingSchedule, ForgeError> {
        env.storage()
            .instance()
            .get(&DataKey::Schedule(schedule_id))
            .ok_or(ForgeError::NotFound)
    }

    /// Read the full tranche schedule record, unlock table included
    /// (read-only view; no state change).
    pub fn get_tranche_schedule(env: Env, schedule_id: u64) -> Result<TrancheSchedule, ForgeError> {
        env.storage()
            .instance()
            .get(&DataKey::TrancheSchedule(schedule_id))
            .ok_or(ForgeError::NotFound)
    }

    /// Claim the vested-but-unclaimed amount.
    ///
    /// Requires the beneficiary. Works for both schedule kinds — the id
    /// resolves to a linear or a tranche record and the matching math runs.
    /// Returns exactly what vested since the last claim (or `0` when nothing
    /// is claimable), so repeated claims can never overpay or underpay. On a
    /// `Revoked` linear schedule only the amount frozen at revocation is
    /// payable; a drained revoked schedule claims `0` with no transfer.
    ///
    /// Ordering: the SEP-41 transfer runs **before** the schedule write —
    /// see the module docs. A zero-claim call returns before either.
    pub fn claim(env: Env, schedule_id: u64) -> Result<i128, ForgeError> {
        let now = env.ledger().timestamp();
        match Self::load(&env, schedule_id)? {
            Stored::Linear(schedule) => {
                let beneficiary = schedule.beneficiary.clone();
                Self::settle_linear(&env, schedule_id, schedule, beneficiary, now)
            }
            Stored::Tranche(schedule) => {
                let beneficiary = schedule.beneficiary.clone();
                Self::settle_tranche(&env, schedule_id, schedule, beneficiary, now)
            }
        }
    }

    /// Claim for an explicit beneficiary, including a frozen former balance.
    pub fn claim_for(env: Env, schedule_id: u64, beneficiary: Address) -> Result<i128, ForgeError> {
        let now = env.ledger().timestamp();
        match Self::load(&env, schedule_id)? {
            Stored::Linear(schedule) => {
                Self::settle_linear(&env, schedule_id, schedule, beneficiary, now)
            }
            Stored::Tranche(schedule) => {
                Self::settle_tranche(&env, schedule_id, schedule, beneficiary, now)
            }
        }
    }

    /// Amount currently claimable (read-only view; no state change).
    ///
    /// On a `Revoked` linear schedule this is the frozen vested amount minus
    /// what has been claimed — `0` once that amount is claimed.
    pub fn claimable(env: Env, schedule_id: u64) -> Result<i128, ForgeError> {
        let now = env.ledger().timestamp();
        match Self::load(&env, schedule_id)? {
            Stored::Linear(schedule) => {
                Self::linear_claimable_for(&env, schedule_id, &schedule, &schedule.beneficiary, now)
            }
            Stored::Tranche(schedule) => Self::tranche_claimable(&schedule, now),
        }
    }

    /// Amount currently claimable by the explicit beneficiary.
    pub fn claimable_for(
        env: Env,
        schedule_id: u64,
        beneficiary: Address,
    ) -> Result<i128, ForgeError> {
        let now = env.ledger().timestamp();
        match Self::load(&env, schedule_id)? {
            Stored::Linear(schedule) => {
                Self::linear_claimable_for(&env, schedule_id, &schedule, &beneficiary, now)
            }
            Stored::Tranche(schedule) => {
                if beneficiary == schedule.beneficiary {
                    Self::tranche_claimable(&schedule, now)
                } else {
                    Ok(0)
                }
            }
        }
    }

    /// Read the current lifecycle status (read-only view).
    ///
    /// The status is derived from the ledger time and claimed amount rather
    /// than the stored field, so it is always current between claims. A
    /// revoked schedule reports `Revoked` regardless of ledger time.
    pub fn get_status(env: Env, schedule_id: u64) -> Result<VestingStatus, ForgeError> {
        let now = env.ledger().timestamp();
        match Self::load(&env, schedule_id)? {
            Stored::Linear(schedule) => Self::current_status(&schedule, now),
            Stored::Tranche(schedule) => Self::tranche_status(&schedule, now),
        }
    }

    /// Load a schedule of either kind by id.
    ///
    /// The two kinds occupy distinct [`DataKey`] variants under one id
    /// counter, so the linear read is attempted first and a tranche id simply
    /// falls through to the tranche read.
    fn load(env: &Env, schedule_id: u64) -> Result<Stored, ForgeError> {
        if let Some(schedule) = env
            .storage()
            .instance()
            .get(&DataKey::Schedule(schedule_id))
        {
            return Ok(Stored::Linear(schedule));
        }
        if let Some(schedule) = env
            .storage()
            .instance()
            .get(&DataKey::TrancheSchedule(schedule_id))
        {
            return Ok(Stored::Tranche(schedule));
        }
        Err(ForgeError::NotFound)
    }

    /// Settle a claim against a linear schedule: transfer-before-state, then
    /// record `claimed` and the derived status.
    fn settle_linear(
        env: &Env,
        schedule_id: u64,
        mut schedule: VestingSchedule,
        beneficiary: Address,
        now: u64,
    ) -> Result<i128, ForgeError> {
        let amount = Self::linear_claimable_for(env, schedule_id, &schedule, &beneficiary, now)?;
        if !Self::authorize_and_pay(env, &beneficiary, &schedule.token, amount)? {
            return Ok(0);
        }
        let former_key = DataKey::FormerBeneficiaryClaim(schedule_id, beneficiary.clone());
        let former_balance: i128 = env.storage().instance().get(&former_key).unwrap_or(0);
        if former_balance > 0 {
            env.storage().instance().remove(&former_key);
        }
        if beneficiary == schedule.beneficiary {
            let current_accrual = Self::current_beneficiary_accrual(&schedule, now)?;
            schedule.beneficiary_claimed = schedule
                .beneficiary_claimed
                .checked_add(current_accrual)
                .ok_or(ForgeError::ArithmeticOverflow)?;
        }
        schedule.claimed = schedule
            .claimed
            .checked_add(amount)
            .ok_or(ForgeError::ArithmeticOverflow)?;
        schedule.status = Self::current_status(&schedule, now)?;
        env.storage()
            .instance()
            .set(&DataKey::Schedule(schedule_id), &schedule);
        Ok(amount)
    }

    /// Settle a claim against a tranche schedule: the same
    /// transfer-before-state discipline as [`Self::settle_linear`], over the
    /// unlock table's step function.
    fn settle_tranche(
        env: &Env,
        schedule_id: u64,
        mut schedule: TrancheSchedule,
        beneficiary: Address,
        now: u64,
    ) -> Result<i128, ForgeError> {
        let amount = if beneficiary == schedule.beneficiary {
            Self::tranche_claimable(&schedule, now)?
        } else {
            0
        };
        if !Self::authorize_and_pay(env, &beneficiary, &schedule.token, amount)? {
            return Ok(0);
        }
        schedule.claimed = schedule
            .claimed
            .checked_add(amount)
            .ok_or(ForgeError::ArithmeticOverflow)?;
        schedule.status = Self::tranche_status(&schedule, now)?;
        env.storage()
            .instance()
            .set(&DataKey::TrancheSchedule(schedule_id), &schedule);
        Ok(amount)
    }

    /// Authorize the beneficiary, then pay `amount` from this contract.
    ///
    /// Returns `Ok(true)` when a transfer ran and `Ok(false)` when `amount` is
    /// zero, so a zero claim exits without ever issuing an empty transfer.
    /// The payment happens here — before the caller's state write — so a
    /// failed transfer leaves the schedule's `claimed`/`status` untouched.
    fn authorize_and_pay(
        env: &Env,
        beneficiary: &Address,
        token: &Address,
        amount: i128,
    ) -> Result<bool, ForgeError> {
        // A revoked schedule needs no separate gate here: revocation freezes
        // the vested amount, so the payable amount below goes to zero once
        // the frozen remainder is claimed.
        beneficiary.require_auth();
        if amount == 0 {
            return Ok(false);
        }
        transfer_from_contract(env, token, beneficiary, amount)?;
        Ok(true)
    }

    /// Validate an unlock table and return the sum of its amounts.
    ///
    /// Rejects an empty table, one longer than [`MAX_TRANCHES`], a
    /// non-positive amount, and an offset that does not strictly increase
    /// (a repeat or a rewind), and checks the cumulative sum so a table
    /// totalling past `i128::MAX` never reaches storage. The first offset is
    /// unconstrained; only the ordering between entries is.
    fn validate_tranches(tranches: &Vec<Tranche>) -> Result<i128, ForgeError> {
        if tranches.is_empty() || tranches.len() > MAX_TRANCHES {
            return Err(ForgeError::InvalidInput);
        }
        let mut total: i128 = 0;
        let mut previous_unlock: Option<u64> = None;
        for tranche in tranches.iter() {
            if tranche.amount <= 0 {
                return Err(ForgeError::InvalidInput);
            }
            if let Some(previous) = previous_unlock {
                if tranche.unlock_at <= previous {
                    return Err(ForgeError::InvalidInput);
                }
            }
            previous_unlock = Some(tranche.unlock_at);
            total = total
                .checked_add(tranche.amount)
                .ok_or(ForgeError::ArithmeticOverflow)?;
        }
        Ok(total)
    }

    /// Allocate the next monotonic schedule id.
    fn next_id(env: &Env) -> Result<u64, ForgeError> {
        let count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
        let id = count.checked_add(1).ok_or(ForgeError::ArithmeticOverflow)?;
        env.storage().instance().set(&DataKey::Count, &id);
        Ok(id)
    }

    /// Derive the lifecycle status from ledger time and claimed amount.
    ///
    /// `Revoked` overrides the time/claimed derivation: a frozen schedule
    /// stays `Revoked` even while its frozen remainder is unclaimed.
    fn current_status(schedule: &VestingSchedule, now: u64) -> Result<VestingStatus, ForgeError> {
        if schedule.revoked_vested.is_some() {
            return Ok(VestingStatus::Revoked);
        }
        if schedule.claimed >= schedule.total_amount {
            return Ok(VestingStatus::Completed);
        }
        let cliff_time = schedule
            .start
            .checked_add(schedule.cliff)
            .ok_or(ForgeError::ArithmeticOverflow)?;
        if now < cliff_time {
            return Ok(VestingStatus::Locked);
        }
        Ok(VestingStatus::Vesting)
    }

    /// Vested amount at ledger time `now`, using floor division so claims
    /// never round up.
    ///
    /// A revoked schedule stopped accruing at its revocation timestamp, so
    /// the frozen amount is returned for every later time.
    fn vested_amount(schedule: &VestingSchedule, now: u64) -> Result<i128, ForgeError> {
        if let Some(frozen) = schedule.revoked_vested {
            return Ok(frozen);
        }
        let cliff_time = schedule
            .start
            .checked_add(schedule.cliff)
            .ok_or(ForgeError::ArithmeticOverflow)?;
        if now < cliff_time {
            return Ok(0);
        }
        let end_time = schedule
            .start
            .checked_add(schedule.duration)
            .ok_or(ForgeError::ArithmeticOverflow)?;
        if now >= end_time {
            return Ok(schedule.total_amount);
        }

        // `cliff <= duration` is enforced at creation, so the period is
        // non-negative; a zero period (cliff == duration) means everything
        // vests at once, which the `now >= end_time` branch above already
        // returned. Guard defensively against division by zero.
        let period = end_time - cliff_time;
        if period == 0 {
            return Ok(schedule.total_amount);
        }
        let elapsed = now - cliff_time;
        let vested = schedule
            .total_amount
            .checked_mul(elapsed as i128)
            .ok_or(ForgeError::ArithmeticOverflow)?
            / period as i128;
        Ok(vested)
    }

    /// Claimable amount belonging to a beneficiary for one linear schedule.
    fn linear_claimable_for(
        env: &Env,
        schedule_id: u64,
        schedule: &VestingSchedule,
        beneficiary: &Address,
        now: u64,
    ) -> Result<i128, ForgeError> {
        let key = DataKey::FormerBeneficiaryClaim(schedule_id, beneficiary.clone());
        let former_balance: i128 = env.storage().instance().get(&key).unwrap_or(0);
        let current_accrual = if beneficiary == &schedule.beneficiary {
            Self::current_beneficiary_accrual(schedule, now)?
        } else {
            0
        };
        former_balance
            .checked_add(current_accrual)
            .ok_or(ForgeError::ArithmeticOverflow)
    }

    /// Current assignment's vested-but-unclaimed accrual, excluding anything
    /// vested before that assignment began.
    fn current_beneficiary_accrual(
        schedule: &VestingSchedule,
        now: u64,
    ) -> Result<i128, ForgeError> {
        Self::vested_amount(schedule, now)?
            .checked_sub(schedule.reassignment_vested)
            .and_then(|amount| amount.checked_sub(schedule.beneficiary_claimed))
            .ok_or(ForgeError::ArithmeticOverflow)
    }

    /// Seconds elapsed since `start`, saturating at zero.
    ///
    /// Tranche progress is compared on this offset instead of on
    /// `start + unlock_at`, which keeps a `u64::MAX` offset from overflowing:
    /// the tranche is simply never reached. A ledger timestamp before `start`
    /// yields `0`, i.e. nothing unlocked — the conservative direction.
    fn elapsed_since(start: u64, now: u64) -> u64 {
        now.saturating_sub(start)
    }

    /// Derive the lifecycle status of a tranche schedule.
    ///
    /// Same two inputs as the linear kind — ledger time and claimed amount —
    /// with the first unlock standing in for the cliff: `Locked` before it,
    /// `Vesting` from it until the final amount is claimed, `Completed` once
    /// `claimed == total_amount`.
    fn tranche_status(schedule: &TrancheSchedule, now: u64) -> Result<VestingStatus, ForgeError> {
        if schedule.claimed >= schedule.total_amount {
            return Ok(VestingStatus::Completed);
        }
        // Creation rejects an empty table, so the first tranche is always
        // there; the arm is defensive only.
        let Some(first) = schedule.tranches.get(0) else {
            return Ok(VestingStatus::Locked);
        };
        if Self::elapsed_since(schedule.start, now) < first.unlock_at {
            return Ok(VestingStatus::Locked);
        }
        Ok(VestingStatus::Vesting)
    }

    /// Unlocked amount of a tranche schedule at ledger time `now`: the sum of
    /// every tranche whose offset has elapsed.
    ///
    /// The scan stops at the first tranche that has not unlocked yet, which is
    /// exact because creation guarantees strictly increasing offsets (and
    /// bounded because the table is capped at [`MAX_TRANCHES`]). The
    /// cumulative sum is checked even though creation already proved it fits
    /// in `i128`, so a tampered record fails loudly instead of wrapping.
    fn tranche_unlocked(schedule: &TrancheSchedule, now: u64) -> Result<i128, ForgeError> {
        let elapsed = Self::elapsed_since(schedule.start, now);
        let mut total: i128 = 0;
        for tranche in schedule.tranches.iter() {
            if tranche.unlock_at > elapsed {
                break;
            }
            total = total
                .checked_add(tranche.amount)
                .ok_or(ForgeError::ArithmeticOverflow)?;
        }
        Ok(total)
    }

    /// Claimable amount of a tranche schedule at ledger time `now` (unlocked
    /// minus claimed).
    fn tranche_claimable(schedule: &TrancheSchedule, now: u64) -> Result<i128, ForgeError> {
        let unlocked = Self::tranche_unlocked(schedule, now)?;
        // `unlocked` is monotonic in `now` and `claimed` only ever rises to a
        // previously unlocked value, so the subtraction cannot underflow; use
        // checked arithmetic to fail loudly if the invariant is ever broken.
        unlocked
            .checked_sub(schedule.claimed)
            .ok_or(ForgeError::ArithmeticOverflow)
    }
}

/// Events emitted to both beneficiaries when a linear schedule is reassigned.
pub mod events {
    use soroban_sdk::{contractevent, Address, Env};

    #[contractevent]
    pub struct BeneficiaryReassignedFrom {
        #[topic]
        pub schedule_id: u64,
        #[topic]
        pub beneficiary: Address,
        pub new_beneficiary: Address,
        pub vested_unclaimed: i128,
        pub reassignment_count: u32,
    }

    #[contractevent]
    pub struct BeneficiaryReassignedTo {
        #[topic]
        pub schedule_id: u64,
        #[topic]
        pub beneficiary: Address,
        pub old_beneficiary: Address,
        pub vested_unclaimed: i128,
        pub reassignment_count: u32,
    }

    pub fn beneficiary_reassigned(
        env: &Env,
        schedule_id: u64,
        old_beneficiary: &Address,
        new_beneficiary: &Address,
        vested_unclaimed: i128,
        reassignment_count: u32,
    ) {
        BeneficiaryReassignedFrom {
            schedule_id,
            beneficiary: old_beneficiary.clone(),
            new_beneficiary: new_beneficiary.clone(),
            vested_unclaimed,
            reassignment_count,
        }
        .publish(env);
        BeneficiaryReassignedTo {
            schedule_id,
            beneficiary: new_beneficiary.clone(),
            old_beneficiary: old_beneficiary.clone(),
            vested_unclaimed,
            reassignment_count,
        }
        .publish(env);
    }
}

/// Move `amount` of `token` from this contract to `to`.
///
/// Token failures are bucketed into [`ForgeError::TokenTransferFailed`]
/// rather than forwarded — the same policy as escrow: a client receiving
/// `Error(Contract, #N)` cannot know whether `N` came from the token or this
/// contract, and the root cause remains visible in the transaction's
/// diagnostic events.
fn transfer_from_contract(
    env: &Env,
    token: &Address,
    to: &Address,
    amount: i128,
) -> Result<(), ForgeError> {
    match token::TokenClient::new(env, token).try_transfer(
        &env.current_contract_address(),
        to,
        &amount,
    ) {
        Ok(Ok(())) => Ok(()),
        // Token returned a typed error (insufficient balance, custom token
        // logic) or the host aborted (most commonly an undeployed token
        // address). The raw discriminant is intentionally discarded.
        _ => Err(ForgeError::TokenTransferFailed),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod authz;

#[cfg(test)]
mod props;
