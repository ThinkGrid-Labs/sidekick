#![deny(clippy::all)]

#[macro_use]
extern crate napi_derive;

use napi::bindgen_prelude::*;
use sidekick_core::evaluator::{evaluate, Flag, UserContext};
use sidekick_core::store::FlagStore;
use std::collections::HashMap;
use std::sync::Arc;

#[napi]
pub struct SidekickCore {
    store: Arc<FlagStore>,
}

#[napi]
impl SidekickCore {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            store: Arc::new(FlagStore::new()),
        }
    }

    /// Load a flag directly into the in-memory cache
    #[napi]
    pub fn upsert_flag(
        &self,
        key: String,
        is_enabled: bool,
        rollout_percentage: Option<u32>,
        description: Option<String>,
        rules_js: napi::bindgen_prelude::Unknown,
    ) -> napi::Result<()> {
        let env = rules_js.env;
        // Attempt to deserialize the JS array of rules, default to empty if null/missing
        let rules = env
            .from_js_value::<Vec<sidekick_core::evaluator::TargetingRule>, _>(rules_js)
            .unwrap_or_default();
        let flag = Flag {
            key,
            is_enabled,
            rollout_percentage,
            description,
            rules,
        };
        self.store.upsert_flag(flag);
        Ok(())
    }

    /// Remove a flag from the local cache
    #[napi]
    pub fn delete_flag(&self, key: String) {
        self.store.delete_flag(&key);
    }

    /// Clear the entire local cache (called on SSE reconnect before re-bootstrap)
    #[napi]
    pub fn clear_store(&self) {
        self.store.clear();
    }

    /// Evaluate a flag for a specific user
    #[napi]
    pub fn is_enabled(
        &self,
        flag_key: String,
        user_key: String,
        user_attributes: HashMap<String, String>,
    ) -> bool {
        let flag = match self.store.get_flag(&flag_key) {
            Some(f) => f,
            None => return false, // Default fallback if flag not found locally
        };

        let ctx = UserContext {
            key: user_key,
            attributes: user_attributes,
        };

        evaluate(&flag, &ctx)
    }
}
