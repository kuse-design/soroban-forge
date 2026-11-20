use soroban_sdk::{Env, IntoVal, Val};

pub const DAY_IN_LEDGERS: u32 = 17_280;
pub const BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const BUMP_THRESHOLD: u32 = BUMP_AMOUNT - DAY_IN_LEDGERS;
pub const DEFAULT_TTL: u32 = BUMP_AMOUNT;

pub fn bump_entry<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, BUMP_THRESHOLD, BUMP_AMOUNT);
}

pub struct TTLHelper {
    storage: soroban_sdk::storage::Storage,
    min_ttl: u32,
}

impl TTLHelper {
    pub fn new(storage: soroban_sdk::storage::Storage, min_ttl: u32) -> Self {
        Self { storage, min_ttl }
    }

    pub fn bump<K>(&self, key: &K) -> Result<(), crate::ForgeError>
    where
        K: IntoVal<Env, Val>,
    {
        self.storage.persistent().extend_ttl(
            key,
            self.min_ttl.saturating_sub(DAY_IN_LEDGERS),
            self.min_ttl,
        );
        Ok(())
    }

    pub fn touch<K>(&self, _env: &Env, keys: &[K]) -> Result<(), crate::ForgeError>
    where
        K: IntoVal<Env, Val> + Clone,
    {
        for key in keys {
            if self.storage.persistent().has(key) {
                self.bump(key)?;
            }
        }
        Ok(())
    }
}
