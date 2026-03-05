use crate::evaluator::Flag;
use dashmap::DashMap;
use std::sync::Arc;

/// The central in-memory store for feature flags.
/// Optimized for heavily concurrent READS via DashMap.
#[derive(Clone, Default)]
pub struct FlagStore {
    // Maps flag_key -> Flag configuration
    flags: Arc<DashMap<String, Flag>>,
}

impl FlagStore {
    pub fn new() -> Self {
        Self {
            flags: Arc::new(DashMap::new()),
        }
    }

    pub fn upsert_flag(&self, flag: Flag) {
        self.flags.insert(flag.key.clone(), flag);
    }

    pub fn get_flag(&self, key: &str) -> Option<Flag> {
        self.flags.get(key).map(|r| r.value().clone())
    }

    pub fn delete_flag(&self, key: &str) {
        self.flags.remove(key);
    }

    pub fn list_flags(&self) -> Vec<Flag> {
        self.flags.iter().map(|r| r.value().clone()).collect()
    }

    pub fn clear(&self) {
        self.flags.clear();
    }
}
