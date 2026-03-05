//! React Native JSI — C FFI layer for Sidekick.
//!
//! These `extern "C"` functions are the Rust side of the JSI bridge.
//! The C++ JSI host (`SidekickJSI.cpp`) calls them synchronously
//! via the generated `sidekick_core.h` header.
//!
//! A global `FlagStore` singleton is used because C FFI functions are
//! stateless — the JS side holds no Rust handles.

use std::ffi::{c_char, CStr};
use std::sync::LazyLock;
use std::collections::HashMap;
use sidekick_core::evaluator::{evaluate, Flag, TargetingRule, UserContext};
use sidekick_core::store::FlagStore;

static STORE: LazyLock<FlagStore> = LazyLock::new(FlagStore::new);

/// Upsert a flag into the in-memory store.
///
/// # Arguments
/// - `key`               — null-terminated flag key
/// - `is_enabled`        — global kill-switch
/// - `rollout_percentage`— 0-100, or -1 to mean "no rollout limit" (100%)
/// - `rules_json`        — null-terminated JSON array of targeting rules,
///                         e.g. `[{"attribute":"email","operator":"EndsWith","values":["@acme.com"]}]`
///                         Pass `"[]"` or NULL when there are no rules.
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

    let flag = Flag {
        key,
        is_enabled,
        rollout_percentage: rollout,
        description: None,
        rules,
    };
    STORE.upsert_flag(flag);
}

/// Remove a flag from the in-memory store.
#[no_mangle]
pub extern "C" fn sidekick_delete_flag(key: *const c_char) {
    let key = unsafe { CStr::from_ptr(key) }.to_string_lossy();
    STORE.delete_flag(&key);
}

/// Clear all flags from the in-memory store (called on SSE reconnect).
#[no_mangle]
pub extern "C" fn sidekick_clear_store() {
    STORE.clear();
}

/// Evaluate a flag for a given user.
///
/// # Arguments
/// - `flag_key`        — null-terminated flag key
/// - `user_key`        — null-terminated stable user identifier
/// - `attributes_json` — null-terminated JSON object of user attributes,
///                       e.g. `{"email":"u@acme.com","country":"US"}`.
///                       Pass `"{}"` or NULL for no attributes.
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
