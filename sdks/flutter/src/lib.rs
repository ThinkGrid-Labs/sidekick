//! Flutter FFI layer for Sidekick.
//!
//! Exposes a C ABI so Dart can call these functions via `dart:ffi` with no
//! code-generation step.  A process-global `FlagStore` is used because C FFI
//! functions are stateless from the Dart side.
//!
//! Compile as `staticlib` (iOS) or `cdylib` (Android / desktop).

use std::ffi::{c_char, CStr};
use std::sync::LazyLock;
use std::collections::HashMap;
use sidekick_core::evaluator::{evaluate, Flag, TargetingRule, UserContext};
use sidekick_core::store::FlagStore;

static STORE: LazyLock<FlagStore> = LazyLock::new(FlagStore::new);

/// Upsert a flag into the in-process cache.
///
/// # Arguments
/// - `key`               — Null-terminated flag key.
/// - `is_enabled`        — Global kill-switch.
/// - `rollout_percentage`— 0-100, or -1 for "no rollout cap" (effectively 100%).
/// - `rules_json`        — Null-terminated JSON array of targeting rules.
///                         Pass NULL or `"[]"` when there are no rules.
#[no_mangle]
pub extern "C" fn sidekick_upsert_flag(
    key: *const c_char,
    is_enabled: bool,
    rollout_percentage: i32,
    rules_json: *const c_char,
) {
    let key = unsafe { CStr::from_ptr(key) }.to_string_lossy().into_owned();

    let rules: Vec<TargetingRule> = if !rules_json.is_null() {
        let json = unsafe { CStr::from_ptr(rules_json) }.to_string_lossy();
        serde_json::from_str(&json).unwrap_or_default()
    } else {
        vec![]
    };

    let rollout = if rollout_percentage < 0 {
        None
    } else {
        Some(rollout_percentage.min(100) as u32)
    };

    STORE.upsert_flag(Flag {
        key,
        is_enabled,
        rollout_percentage: rollout,
        description: None,
        rules,
    });
}

/// Remove a flag from the in-process cache.
#[no_mangle]
pub extern "C" fn sidekick_delete_flag(key: *const c_char) {
    let key = unsafe { CStr::from_ptr(key) }.to_string_lossy();
    STORE.delete_flag(&key);
}

/// Clear the entire cache (call before re-bootstrapping on SSE reconnect).
#[no_mangle]
pub extern "C" fn sidekick_clear_store() {
    STORE.clear();
}

/// Evaluate a flag for a given user.
///
/// # Arguments
/// - `flag_key`        — Null-terminated flag key.
/// - `user_key`        — Null-terminated stable user identifier.
/// - `attributes_json` — Null-terminated JSON object of string→string attributes.
///                       Pass NULL or `"{}"` for no attributes.
///
/// # Returns
/// `1` if the flag is enabled for this user, `0` otherwise.
#[no_mangle]
pub extern "C" fn sidekick_is_enabled(
    flag_key: *const c_char,
    user_key: *const c_char,
    attributes_json: *const c_char,
) -> i32 {
    let flag_key = unsafe { CStr::from_ptr(flag_key) }.to_string_lossy();
    let user_key = unsafe { CStr::from_ptr(user_key) }.to_string_lossy().into_owned();

    let flag = match STORE.get_flag(&flag_key) {
        Some(f) => f,
        None => return 0,
    };

    let attributes: HashMap<String, String> = if !attributes_json.is_null() {
        let json = unsafe { CStr::from_ptr(attributes_json) }.to_string_lossy();
        serde_json::from_str(&json).unwrap_or_default()
    } else {
        HashMap::new()
    };

    let ctx = UserContext {
        key: user_key,
        attributes,
    };

    if evaluate(&flag, &ctx) { 1 } else { 0 }
}
