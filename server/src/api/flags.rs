use crate::state::AppState;
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use redis::AsyncCommands;
use serde_json::json;
use sidekick_core::evaluator::Flag;
use sqlx::Row;
use std::sync::Arc;
use tracing::error;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/flags", get(list_flags).post(create_flag))
        .route(
            "/flags/:key",
            get(get_flag).delete(delete_flag).patch(patch_flag),
        )
}

async fn list_flags(State(state): State<AppState>) -> Json<Vec<Arc<Flag>>> {
    Json(state.store.list_flags())
}

async fn create_flag(
    State(state): State<AppState>,
    Json(payload): Json<Flag>,
) -> Result<Json<Flag>, StatusCode> {
    let data = serde_json::to_value(&payload).map_err(|e| {
        error!("POST /flags: failed to serialize flag: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    sqlx::query(
        "INSERT INTO flags (key, data) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data",
    )
    .bind(&payload.key)
    .bind(&data)
    .execute(&state.db)
    .await
    .map_err(|e| {
        error!("POST /flags: DB write failed for key '{}': {e}", payload.key);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    state.store.upsert_flag(payload.clone());

    let mut redis_conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| {
            error!("POST /flags: failed to get Redis connection: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let msg = json!({"type": "UPSERT", "flag": payload}).to_string();
    if let Err(e) = redis_conn
        .publish::<_, _, ()>("sidekick_updates", &msg)
        .await
    {
        // DB write succeeded; log the Redis failure but do not fail the request.
        // Other server instances will be stale until their next restart or reconnect.
        error!(
            "POST /flags: DB write succeeded but Redis publish failed for key '{}': {e}. \
             Other instances may serve stale data until restarted.",
            payload.key
        );
    }

    Ok(Json(payload))
}

async fn get_flag(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Arc<Flag>>, StatusCode> {
    state
        .store
        .get_flag(&key)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_flag(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("DELETE FROM flags WHERE key = $1")
        .bind(&key)
        .execute(&state.db)
        .await
        .map_err(|e| {
            error!("DELETE /flags/{key}: DB delete failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    state.store.delete_flag(&key);

    let mut redis_conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| {
            error!("DELETE /flags/{key}: failed to get Redis connection: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let msg = json!({"type": "DELETE", "key": key}).to_string();
    if let Err(e) = redis_conn
        .publish::<_, _, ()>("sidekick_updates", &msg)
        .await
    {
        error!(
            "DELETE /flags/{key}: DB delete succeeded but Redis publish failed: {e}. \
             Other instances may serve stale data until restarted."
        );
    }

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/flags/:key — partial update via JSON merge.
///
/// Only provided fields are changed; omitted fields retain their current values.
/// The `key` field is explicitly excluded from the patch to prevent key aliasing,
/// and the read-modify-write is wrapped in a serializable transaction to prevent
/// concurrent-patch races.
async fn patch_flag(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(mut patch): Json<serde_json::Value>,
) -> Result<Json<Flag>, StatusCode> {
    // Prevent the key from being mutated through a PATCH body.
    // Allowing this creates a split-brain between the DB row key column and the
    // stored JSONB, causing the flag to be indexed under the wrong key after reload.
    if let serde_json::Value::Object(ref mut m) = patch {
        m.remove("key");
    }

    // Serializable transaction + SELECT FOR UPDATE prevents concurrent PATCH races.
    let mut db_tx = state.db.begin().await.map_err(|e| {
        error!("PATCH /flags/{key}: failed to begin transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let rec = sqlx::query("SELECT data FROM flags WHERE key = $1 FOR UPDATE")
        .bind(&key)
        .fetch_optional(&mut *db_tx)
        .await
        .map_err(|e| {
            error!("PATCH /flags/{key}: DB read failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut flag_val: serde_json::Value = rec.try_get("data").map_err(|e| {
        error!("PATCH /flags/{key}: failed to deserialize stored data: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let (serde_json::Value::Object(map), serde_json::Value::Object(patch_map)) =
        (&mut flag_val, patch)
    {
        for (k, v) in patch_map {
            map.insert(k, v);
        }
    }

    let flag: Flag = serde_json::from_value(flag_val.clone()).map_err(|e| {
        error!("PATCH /flags/{key}: merged result is not a valid Flag: {e}");
        StatusCode::UNPROCESSABLE_ENTITY
    })?;

    sqlx::query("UPDATE flags SET data = $1 WHERE key = $2")
        .bind(&flag_val)
        .bind(&key)
        .execute(&mut *db_tx)
        .await
        .map_err(|e| {
            error!("PATCH /flags/{key}: DB update failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    db_tx.commit().await.map_err(|e| {
        error!("PATCH /flags/{key}: transaction commit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    state.store.upsert_flag(flag.clone());

    let mut redis_conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| {
            error!("PATCH /flags/{key}: failed to get Redis connection: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let msg = json!({"type": "UPSERT", "flag": flag}).to_string();
    if let Err(e) = redis_conn
        .publish::<_, _, ()>("sidekick_updates", &msg)
        .await
    {
        error!(
            "PATCH /flags/{key}: DB update succeeded but Redis publish failed: {e}. \
             Other instances may serve stale data until restarted."
        );
    }

    Ok(Json(flag))
}
