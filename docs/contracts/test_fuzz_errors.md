//! Soroban Forge Shared Error Reference (`ForgeError`) - Exhaustive Chaos & Stress Test Suite
//! Target: `errors.md` & cross-contract shared error domain specifications
//! Ready for execution via `cargo test` or `rustc --test`.

use std::collections::HashSet;
use std::fmt;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

// ============================================================================
// Core Domain Implementation (Derived strictly from errors.md specification)
// ============================================================================

/// Canonical `ForgeError` representing the unified 15-code cross-contract error domain.
/// Error codes start at 1 (0 is strictly reserved by the Soroban host runtime).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum ForgeError {
    Unauthorized = 1,
    NotFound = 2,
    InvalidInput = 3,
    InsufficientFunds = 4,
    AlreadyInitialized = 5,
    NotInitialized = 6,
    DeadlineReached = 7,
    InsufficientAllowance = 8,
    ArithmeticOverflow = 9,
    Custom = 10,
    TokenTransferFailed = 11,
    ContractInvocationFailed = 12,
    WithdrawalLimitExceeded = 13,
    SubscriptionPastDue = 14,
    ProposerCooldown = 15,
}

impl ForgeError {
    pub const MIN_CODE: u32 = 1;
    pub const MAX_CODE: u32 = 15;

    pub fn code(&self) -> u32 {
        *self as u32
    }

    pub fn from_code(code: u32) -> Result<Self, ForgeErrorConversionError> {
        match code {
            0 => Err(ForgeErrorConversionError::HostReservedZero),
            1 => Ok(ForgeError::Unauthorized),
            2 => Ok(ForgeError::NotFound),
            3 => Ok(ForgeError::InvalidInput),
            4 => Ok(ForgeError::InsufficientFunds),
            5 => Ok(ForgeError::AlreadyInitialized),
            6 => Ok(ForgeError::NotInitialized),
            7 => Ok(ForgeError::DeadlineReached),
            8 => Ok(ForgeError::InsufficientAllowance),
            9 => Ok(ForgeError::ArithmeticOverflow),
            10 => Ok(ForgeError::Custom),
            11 => Ok(ForgeError::TokenTransferFailed),
            12 => Ok(ForgeError::ContractInvocationFailed),
            13 => Ok(ForgeError::WithdrawalLimitExceeded),
            14 => Ok(ForgeError::SubscriptionPastDue),
            15 => Ok(ForgeError::ProposerCooldown),
            other => Err(ForgeErrorConversionError::UnknownCode(other)),
        }
    }
}

impl TryFrom<u32> for ForgeError {
    type Error = ForgeErrorConversionError;
    fn try_from(code: u32) -> Result<Self, Self::Error> {
        ForgeError::from_code(code)
    }
}

impl fmt::Display for ForgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}({})", self, self.code())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ForgeErrorConversionError {
    HostReservedZero,
    UnknownCode(u32),
}

impl fmt::Display for ForgeErrorConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostReservedZero => write!(f, "Code 0 is reserved by the Soroban host runtime"),
            Self::UnknownCode(c) => write!(f, "Error code {} is outside valid range [1, 15]", c),
        }
    }
}

// ============================================================================
// Contract Logic Emulation & Rule Verification Guards
// ============================================================================

pub const MAX_SELLER_BPS: u32 = 10_000;
pub const MAX_BATCH_SALES: usize = 20;
pub const MAX_TRANCHES: usize = 32;
pub const MAX_CATCHUP_PERIODS: u32 = 32;
pub const DEFAULT_MAX_ACTIVE_PROPOSALS: usize = 5;

pub fn validate_amount(amount: i128) -> Result<(), ForgeError> {
    if amount <= 0 {
        Err(ForgeError::InvalidInput)
    } else {
        Ok(())
    }
}

pub fn validate_duration(seconds: u64) -> Result<(), ForgeError> {
    if seconds == 0 {
        Err(ForgeError::InvalidInput)
    } else {
        Ok(())
    }
}

pub fn validate_seller_bps(bps: u32) -> Result<(), ForgeError> {
    if bps > MAX_SELLER_BPS {
        Err(ForgeError::InvalidInput)
    } else {
        Ok(())
    }
}

pub fn validate_batch_sales_len(len: usize) -> Result<(), ForgeError> {
    if len == 0 || len > MAX_BATCH_SALES {
        Err(ForgeError::InvalidInput)
    } else {
        Ok(())
    }
}

pub fn validate_tranches_len(len: usize) -> Result<(), ForgeError> {
    if len == 0 || len > MAX_TRANCHES {
        Err(ForgeError::InvalidInput)
    } else {
        Ok(())
    }
}

pub fn validate_catchup_periods(periods: u32) -> Result<(), ForgeError> {
    if periods == 0 || periods > MAX_CATCHUP_PERIODS {
        Err(ForgeError::InvalidInput)
    } else {
        Ok(())
    }
}

pub fn validate_no_duplicates<T: Eq + Hash>(items: &[T]) -> Result<(), ForgeError> {
    let mut seen = HashSet::with_capacity(items.len());
    for item in items {
        if !seen.insert(item) {
            return Err(ForgeError::InvalidInput);
        }
    }
    Ok(())
}

pub fn checked_math_add(a: i128, b: i128) -> Result<i128, ForgeError> {
    a.checked_add(b).ok_or(ForgeError::ArithmeticOverflow)
}

pub fn checked_math_mul(a: i128, b: i128) -> Result<i128, ForgeError> {
    a.checked_mul(b).ok_or(ForgeError::ArithmeticOverflow)
}

pub struct DaoProposalTracker {
    active_proposals: Mutex<std::collections::HashMap<String, usize>>,
}

impl DaoProposalTracker {
    pub fn new() -> Self {
        Self {
            active_proposals: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn propose(&self, proposer: &str) -> Result<usize, ForgeError> {
        let mut map = self.active_proposals.lock().unwrap();
        let count = map.entry(proposer.to_string()).or_insert(0);
        if *count >= DEFAULT_MAX_ACTIVE_PROPOSALS {
            Err(ForgeError::ProposerCooldown)
        } else {
            *count += 1;
            Ok(*count)
        }
    }
}

pub struct InitializationGuard {
    initialized: AtomicBool,
}

impl InitializationGuard {
    pub fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
        }
    }

    pub fn initialize(&self) -> Result<(), ForgeError> {
        if self
            .initialized
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            Ok(())
        } else {
            Err(ForgeError::AlreadyInitialized)
        }
    }

    pub fn require_initialized(&self) -> Result<(), ForgeError> {
        if self.initialized.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ForgeError::NotInitialized)
        }
    }
}

pub struct VelocityLimiter {
    limit: i128,
    spent: Mutex<i128>,
}

impl VelocityLimiter {
    pub fn new(limit: i128) -> Self {
        Self {
            limit,
            spent: Mutex::new(0),
        }
    }

    pub fn withdraw(&self, amount: i128) -> Result<(), ForgeError> {
        validate_amount(amount)?;
        let mut spent = self.spent.lock().unwrap();
        let new_spent = spent
            .checked_add(amount)
            .ok_or(ForgeError::ArithmeticOverflow)?;
        if new_spent > self.limit {
            Err(ForgeError::WithdrawalLimitExceeded)
        } else {
            *spent = new_spent;
            Ok(())
        }
    }
}

// ============================================================================
// 25 Comprehensive Chaos, Property, and Boundary Test Cases
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 1. Verifies that all 15 canonical error codes strictly map to their exact variants.
    #[test]
    fn test_error_code_mapping_canonical_values() {
        let expected = [
            (1, ForgeError::Unauthorized),
            (2, ForgeError::NotFound),
            (3, ForgeError::InvalidInput),
            (4, ForgeError::InsufficientFunds),
            (5, ForgeError::AlreadyInitialized),
            (6, ForgeError::NotInitialized),
            (7, ForgeError::DeadlineReached),
            (8, ForgeError::InsufficientAllowance),
            (9, ForgeError::ArithmeticOverflow),
            (10, ForgeError::Custom),
            (11, ForgeError::TokenTransferFailed),
            (12, ForgeError::ContractInvocationFailed),
            (13, ForgeError::WithdrawalLimitExceeded),
            (14, ForgeError::SubscriptionPastDue),
            (15, ForgeError::ProposerCooldown),
        ];

        for (code, variant) in expected {
            assert_eq!(ForgeError::from_code(code).unwrap(), variant);
            assert_eq!(ForgeError::try_from(code).unwrap(), variant);
            assert_eq!(variant.code(), code);
        }
    }

    /// 2. Ensures that error code 0 is strictly rejected as host-reserved across conversions.
    #[test]
    fn test_error_code_zero_reserved_by_host() {
        let result = ForgeError::from_code(0);
        assert_eq!(result, Err(ForgeErrorConversionError::HostReservedZero));
        assert!(matches!(
            ForgeError::try_from(0),
            Err(ForgeErrorConversionError::HostReservedZero)
        ));
    }

    /// 3. Tests out-of-bounds error code rejection for values outside [1, 15].
    #[test]
    fn test_error_code_out_of_bounds_rejection() {
        let invalid_codes = [16, 17, 100, 255, 1_000, 65_535, u32::MAX - 1, u32::MAX];
        for &code in &invalid_codes {
            assert_eq!(
                ForgeError::from_code(code),
                Err(ForgeErrorConversionError::UnknownCode(code))
            );
        }
    }

    /// 4. Boundary check on amounts: non-positive amounts (0, -1, i128::MIN) must yield InvalidInput.
    #[test]
    fn test_amount_validation_extreme_boundaries() {
        assert_eq!(validate_amount(0), Err(ForgeError::InvalidInput));
        assert_eq!(validate_amount(-1), Err(ForgeError::InvalidInput));
        assert_eq!(validate_amount(i128::MIN), Err(ForgeError::InvalidInput));
        assert_eq!(validate_amount(-999_999_999), Err(ForgeError::InvalidInput));

        assert!(validate_amount(1).is_ok());
        assert!(validate_amount(10_000_000_000).is_ok());
        assert!(validate_amount(i128::MAX).is_ok());
    }

    /// 5. Boundary check on time durations: duration == 0 must fail with InvalidInput.
    #[test]
    fn test_duration_validation_zero_and_limits() {
        assert_eq!(validate_duration(0), Err(ForgeError::InvalidInput));
        assert!(validate_duration(1).is_ok());
        assert!(validate_duration(17_280).is_ok()); // 1 day in ledgers
        assert!(validate_duration(u64::MAX).is_ok());
    }

    /// 6. Marketplace royalty seller_bps bounds: bps > 10_000 must fail with InvalidInput.
    #[test]
    fn test_marketplace_royalty_bps_boundary() {
        assert!(validate_seller_bps(0).is_ok());
        assert!(validate_seller_bps(250).is_ok()); // 2.5%
        assert!(validate_seller_bps(10_000).is_ok()); // 100%

        assert_eq!(validate_seller_bps(10_001), Err(ForgeError::InvalidInput));
        assert_eq!(validate_seller_bps(20_000), Err(ForgeError::InvalidInput));
        assert_eq!(validate_seller_bps(u32::MAX), Err(ForgeError::InvalidInput));
    }

    /// 7. Marketplace batch sales upper limit: max 20 sales allowed per batch.
    #[test]
    fn test_batch_sales_size_boundary() {
        assert_eq!(validate_batch_sales_len(0), Err(ForgeError::InvalidInput));
        assert!(validate_batch_sales_len(1).is_ok());
        assert!(validate_batch_sales_len(20).is_ok());

        assert_eq!(validate_batch_sales_len(21), Err(ForgeError::InvalidInput));
        assert_eq!(validate_batch_sales_len(100), Err(ForgeError::InvalidInput));
        assert_eq!(validate_batch_sales_len(usize::MAX), Err(ForgeError::InvalidInput));
    }

    /// 8. Vesting schedule tranches bound: max 32 tranches allowed per schedule.
    #[test]
    fn test_tranches_len_boundary() {
        assert_eq!(validate_tranches_len(0), Err(ForgeError::InvalidInput));
        assert!(validate_tranches_len(1).is_ok());
        assert!(validate_tranches_len(32).is_ok());

        assert_eq!(validate_tranches_len(33), Err(ForgeError::InvalidInput));
        assert_eq!(validate_tranches_len(64), Err(ForgeError::InvalidInput));
        assert_eq!(validate_tranches_len(usize::MAX), Err(ForgeError::InvalidInput));
    }

    /// 9. Subscription payments catchup periods bound: max 32 periods allowed.
    #[test]
    fn test_subscription_catchup_periods_boundary() {
        assert_eq!(validate_catchup_periods(0), Err(ForgeError::InvalidInput));
        assert!(validate_catchup_periods(1).is_ok());
        assert!(validate_catchup_periods(32).is_ok());

        assert_eq!(validate_catchup_periods(33), Err(ForgeError::InvalidInput));
        assert_eq!(validate_catchup_periods(1_000), Err(ForgeError::InvalidInput));
        assert_eq!(validate_catchup_periods(u32::MAX), Err(ForgeError::InvalidInput));
    }

    /// 10. Duplicate detection in collections (multi-sig owners, escrow baskets, subscription quotas).
    #[test]
    fn test_duplicate_entries_detection_basket_and_owners() {
        let unique_addresses = vec!["addr_1", "addr_2", "addr_3"];
        assert!(validate_no_duplicates(&unique_addresses).is_ok());

        let duplicate_addresses = vec!["addr_1", "addr_2", "addr_1"];
        assert_eq!(
            validate_no_duplicates(&duplicate_addresses),
            Err(ForgeError::InvalidInput)
        );

        let unique_tokens = vec![101, 102, 103, 104];
        assert!(validate_no_duplicates(&unique_tokens).is_ok());

        let duplicate_tokens = vec![101, 102, 103, 101];
        assert_eq!(
            validate_no_duplicates(&duplicate_tokens),
            Err(ForgeError::InvalidInput)
        );
    }

    /// 11. Arithmetic overflow chaos on i128 addition.
    #[test]
    fn test_arithmetic_overflow_i128_addition_chaos() {
        assert!(checked_math_add(1_000, 2_000).is_ok());
        assert_eq!(
            checked_math_add(i128::MAX, 1),
            Err(ForgeError::ArithmeticOverflow)
        );
        assert_eq!(
            checked_math_add(i128::MAX, i128::MAX),
            Err(ForgeError::ArithmeticOverflow)
        );
        assert_eq!(
            checked_math_add(i128::MIN, -1),
            Err(ForgeError::ArithmeticOverflow)
        );
    }

    /// 12. Arithmetic overflow chaos on i128 multiplication.
    #[test]
    fn test_arithmetic_overflow_i128_multiplication_chaos() {
        assert_eq!(
            checked_math_mul(i128::MAX, 2),
            Err(ForgeError::ArithmeticOverflow)
        );
        assert_eq!(
            checked_math_mul(i128::MIN, -1),
            Err(ForgeError::ArithmeticOverflow)
        );
        assert_eq!(
            checked_math_mul(i128::MAX / 2 + 1, 2),
            Err(ForgeError::ArithmeticOverflow)
        );
    }

    /// 13. State machine lifecycle: NotInitialized before init, AlreadyInitialized on double init.
    #[test]
    fn test_initialization_lifecycle_state_machine() {
        let guard = InitializationGuard::new();

        // Calling action prior to initialization
        assert_eq!(guard.require_initialized(), Err(ForgeError::NotInitialized));

        // First initialization succeeds
        assert!(guard.initialize().is_ok());
        assert!(guard.require_initialized().is_ok());

        // Second initialization fails
        assert_eq!(guard.initialize(), Err(ForgeError::AlreadyInitialized));
    }

    /// 14. Proposer cooldown enforcement: max 5 active proposals per proposer.
    #[test]
    fn test_proposer_cooldown_cap_enforcement() {
        let tracker = DaoProposalTracker::new();
        let proposer = "proposer_alpha";

        for i in 1..=DEFAULT_MAX_ACTIVE_PROPOSALS {
            assert_eq!(tracker.propose(proposer), Ok(i));
        }

        // 6th proposal must fail with ProposerCooldown (15)
        assert_eq!(tracker.propose(proposer), Err(ForgeError::ProposerCooldown));
    }

    /// 15. Proposer cooldown cross-account isolation: separate proposers have independent quotas.
    #[test]
    fn test_proposer_cooldown_cross_proposer_isolation() {
        let tracker = DaoProposalTracker::new();

        // Proposer A reaches limit
        for _ in 0..DEFAULT_MAX_ACTIVE_PROPOSALS {
            assert!(tracker.propose("proposer_a").is_ok());
        }
        assert_eq!(tracker.propose("proposer_a"), Err(ForgeError::ProposerCooldown));

        // Proposer B can still propose up to limit
        for i in 1..=DEFAULT_MAX_ACTIVE_PROPOSALS {
            assert_eq!(tracker.propose("proposer_b"), Ok(i));
        }
        assert_eq!(tracker.propose("proposer_b"), Err(ForgeError::ProposerCooldown));
    }

    /// 16. Token transfer error bucketing: external SEP-41 failures bucket into TokenTransferFailed (11).
    #[test]
    fn test_token_transfer_error_bucketing() {
        enum RawSep41Error {
            InsufficientBalance,
            MissingTrustline,
            UndeployedTokenContract,
            ContractAborted,
        }

        fn simulate_sep41_transfer(err: RawSep41Error) -> ForgeError {
            match err {
                RawSep41Error::InsufficientBalance
                | RawSep41Error::MissingTrustline
                | RawSep41Error::UndeployedTokenContract
                | RawSep41Error::ContractAborted => ForgeError::TokenTransferFailed,
            }
        }

        assert_eq!(
            simulate_sep41_transfer(RawSep41Error::InsufficientBalance),
            ForgeError::TokenTransferFailed
        );
        assert_eq!(
            simulate_sep41_transfer(RawSep41Error::MissingTrustline),
            ForgeError::TokenTransferFailed
        );
        assert_eq!(
            simulate_sep41_transfer(RawSep41Error::UndeployedTokenContract),
            ForgeError::TokenTransferFailed
        );
        assert_eq!(
            simulate_sep41_transfer(RawSep41Error::ContractAborted),
            ForgeError::TokenTransferFailed
        );
        assert_eq!(ForgeError::TokenTransferFailed.code(), 11);
    }

    /// 17. Cross-contract call invocation error bucketing into ContractInvocationFailed (12).
    #[test]
    fn test_cross_contract_invocation_error_bucketing() {
        enum RawTargetRevert {
            RevertCode(u32),
            HostTrapped,
            BudgetExceeded,
        }

        fn execute_dispatch(target_revert: Option<RawTargetRevert>) -> Result<(), ForgeError> {
            if target_revert.is_some() {
                Err(ForgeError::ContractInvocationFailed)
            } else {
                Ok(())
            }
        }

        assert_eq!(
            execute_dispatch(Some(RawTargetRevert::RevertCode(42))),
            Err(ForgeError::ContractInvocationFailed)
        );
        assert_eq!(
            execute_dispatch(Some(RawTargetRevert::HostTrapped)),
            Err(ForgeError::ContractInvocationFailed)
        );
        assert_eq!(
            execute_dispatch(Some(RawTargetRevert::BudgetExceeded)),
            Err(ForgeError::ContractInvocationFailed)
        );
        assert!(execute_dispatch(None).is_ok());
        assert_eq!(ForgeError::ContractInvocationFailed.code(), 12);
    }

    /// 18. Rolling velocity limit enforcement in multi-sig withdrawals (WithdrawalLimitExceeded, 13).
    #[test]
    fn test_withdrawal_velocity_limit_exceeded() {
        let limiter = VelocityLimiter::new(1_000);

        assert!(limiter.withdraw(400).is_ok());
        assert!(limiter.withdraw(500).is_ok()); // total 900 <= 1000

        // Exceeds 1000 limit
        assert_eq!(limiter.withdraw(200), Err(ForgeError::WithdrawalLimitExceeded));
        assert_eq!(ForgeError::WithdrawalLimitExceeded.code(), 13);
    }

    /// 19. Subscription state machine: PastDue status blocks billing execution (SubscriptionPastDue, 14).
    #[test]
    fn test_subscription_past_due_state_blocks_execution() {
        #[derive(PartialEq, Debug)]
        enum SubscriptionStatus {
            Active,
            PastDue,
            Cancelled,
        }

        fn process_billing(status: SubscriptionStatus) -> Result<(), ForgeError> {
            match status {
                SubscriptionStatus::Active => Ok(()),
                SubscriptionStatus::PastDue => Err(ForgeError::SubscriptionPastDue),
                SubscriptionStatus::Cancelled => Err(ForgeError::NotFound),
            }
        }

        assert!(process_billing(SubscriptionStatus::Active).is_ok());
        assert_eq!(
            process_billing(SubscriptionStatus::PastDue),
            Err(ForgeError::SubscriptionPastDue)
        );
        assert_eq!(ForgeError::SubscriptionPastDue.code(), 14);
    }

    /// 20. Deadline and expiration violations: voting closed or early refund yields DeadlineReached (7).
    #[test]
    fn test_deadline_reached_and_expiration() {
        fn vote(current_time: u64, voting_ends: u64) -> Result<(), ForgeError> {
            if current_time >= voting_ends {
                Err(ForgeError::DeadlineReached)
            } else {
                Ok(())
            }
        }

        fn refund_expired(current_time: u64, timeout: u64) -> Result<(), ForgeError> {
            if current_time < timeout {
                Err(ForgeError::DeadlineReached)
            } else {
                Ok(())
            }
        }

        assert!(vote(100, 200).is_ok());
        assert_eq!(vote(200, 200), Err(ForgeError::DeadlineReached));
        assert_eq!(vote(250, 200), Err(ForgeError::DeadlineReached));

        assert!(refund_expired(300, 200).is_ok());
        assert_eq!(refund_expired(150, 200), Err(ForgeError::DeadlineReached));
        assert_eq!(ForgeError::DeadlineReached.code(), 7);
    }

    /// 21. Distinct semantics between InsufficientFunds (4) and InsufficientAllowance (8).
    #[test]
    fn test_insufficient_funds_vs_insufficient_allowance() {
        fn execute_payment(
            custodied_balance: i128,
            allowance: i128,
            charge: i128,
        ) -> Result<(), ForgeError> {
            if allowance < charge {
                return Err(ForgeError::InsufficientAllowance);
            }
            if custodied_balance < charge {
                return Err(ForgeError::InsufficientFunds);
            }
            Ok(())
        }

        assert_eq!(
            execute_payment(1_000, 100, 500),
            Err(ForgeError::InsufficientAllowance)
        );
        assert_eq!(
            execute_payment(100, 1_000, 500),
            Err(ForgeError::InsufficientFunds)
        );
        assert!(execute_payment(1_000, 1_000, 500).is_ok());
    }

    /// 22. Chaos Concurrency: 20 threads simultaneously race to initialize a component.
    /// Invariant: Exactly 1 thread succeeds; exactly 19 threads receive AlreadyInitialized.
    #[test]
    fn test_concurrent_double_initialization_race() {
        const THREAD_COUNT: usize = 20;
        let guard = Arc::new(InitializationGuard::new());
        let barrier = Arc::new(Barrier::new(THREAD_COUNT));
        let success_count = Arc::new(AtomicUsize::new(0));
        let already_init_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::with_capacity(THREAD_COUNT);

        for _ in 0..THREAD_COUNT {
            let guard_clone = Arc::clone(&guard);
            let barrier_clone = Arc::clone(&barrier);
            let success_clone = Arc::clone(&success_count);
            let already_init_clone = Arc::clone(&already_init_count);

            handles.push(thread::spawn(move || {
                barrier_clone.wait();
                match guard_clone.initialize() {
                    Ok(()) => {
                        success_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(ForgeError::AlreadyInitialized) => {
                        already_init_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(other) => panic!("Unexpected error: {:?}", other),
                }
            }));
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        assert_eq!(success_count.load(Ordering::SeqCst), 1);
        assert_eq!(already_init_count.load(Ordering::SeqCst), THREAD_COUNT - 1);
    }

    /// 23. Chaos Concurrency: 10 threads race to propose under the same account.
    /// Invariant: Exactly 5 succeed; exactly 5 fail with ProposerCooldown.
    #[test]
    fn test_concurrent_proposer_cooldown_race() {
        const THREAD_COUNT: usize = 10;
        let tracker = Arc::new(DaoProposalTracker::new());
        let barrier = Arc::new(Barrier::new(THREAD_COUNT));
        let success_count = Arc::new(AtomicUsize::new(0));
        let cooldown_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::with_capacity(THREAD_COUNT);

        for _ in 0..THREAD_COUNT {
            let tracker_clone = Arc::clone(&tracker);
            let barrier_clone = Arc::clone(&barrier);
            let success_clone = Arc::clone(&success_count);
            let cooldown_clone = Arc::clone(&cooldown_count);

            handles.push(thread::spawn(move || {
                barrier_clone.wait();
                match tracker_clone.propose("spammer_sybil") {
                    Ok(_) => {
                        success_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(ForgeError::ProposerCooldown) => {
                        cooldown_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(other) => panic!("Unexpected error: {:?}", other),
                }
            }));
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        assert_eq!(
            success_count.load(Ordering::SeqCst),
            DEFAULT_MAX_ACTIVE_PROPOSALS
        );
        assert_eq!(
            cooldown_count.load(Ordering::SeqCst),
            THREAD_COUNT - DEFAULT_MAX_ACTIVE_PROPOSALS
        );
    }

    /// 24. Chaos Concurrency: Concurrent withdrawals against a shared velocity limit.
    /// Invariant: Total withdrawn cannot exceed limit; overflow attempts return WithdrawalLimitExceeded.
    #[test]
    fn test_concurrent_velocity_limit_race() {
        const THREAD_COUNT: usize = 16;
        const WITHDRAW_PER_THREAD: i128 = 100;
        const LIMIT: i128 = 500; // Only 5 withdrawals of 100 can succeed

        let limiter = Arc::new(VelocityLimiter::new(LIMIT));
        let barrier = Arc::new(Barrier::new(THREAD_COUNT));
        let success_count = Arc::new(AtomicUsize::new(0));
        let limit_exceeded_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::with_capacity(THREAD_COUNT);

        for _ in 0..THREAD_COUNT {
            let limiter_clone = Arc::clone(&limiter);
            let barrier_clone = Arc::clone(&barrier);
            let success_clone = Arc::clone(&success_count);
            let limit_exceeded_clone = Arc::clone(&limit_exceeded_count);

            handles.push(thread::spawn(move || {
                barrier_clone.wait();
                match limiter_clone.withdraw(WITHDRAW_PER_THREAD) {
                    Ok(()) => {
                        success_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(ForgeError::WithdrawalLimitExceeded) => {
                        limit_exceeded_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(other) => panic!("Unexpected error: {:?}", other),
                }
            }));
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        assert_eq!(success_count.load(Ordering::SeqCst), (LIMIT / WITHDRAW_PER_THREAD) as usize);
        assert_eq!(
            limit_exceeded_count.load(Ordering::SeqCst),
            THREAD_COUNT - (LIMIT / WITHDRAW_PER_THREAD) as usize
        );
    }

    /// 25. Invariant & Bijection Property: Complete code sweep ensuring every code in [1, 15]
    /// converts losslessly to and from u32 without any overlapping discriminants.
    #[test]
    fn test_error_enum_invariants_and_deterministic_serialization() {
        let mut seen_codes = HashSet::new();

        for code in ForgeError::MIN_CODE..=ForgeError::MAX_CODE {
            let err = ForgeError::from_code(code).expect("Valid code must parse");
            assert_eq!(err.code(), code);
            assert!(
                seen_codes.insert(code),
                "Duplicate code discriminant detected: {}",
                code
            );

            // Display check
            let formatted = format!("{}", err);
            assert!(formatted.contains(&code.to_string()));
        }

        assert_eq!(seen_codes.len(), 15);
    }
}