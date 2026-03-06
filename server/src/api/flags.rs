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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/flags", get(list_flags).post(create_flag))
        .route(
            "/flags/:key",
            get(get_flag).delete(delete_flag).patch(patch_flag),
        )
}

async fn list_flags(State(state): State<AppState>) -> Json<Vec<Flag>> {
    // Serve directly from the in-memory cache — no DB round-trip needed.
    Json(state.store.list_flags())
}

async fn create_flag(
    State(state): State<AppState>,
    Json(payload): Json<Flag>,
) -> Result<Json<Flag>, StatusCode> {
    let data = serde_json::to_value(&payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Save to DB
    sqlx::query(
        "INSERT INTO flags (key, data) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data",
    )
    .bind(&payload.key)
    .bind(&data)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update local cache immediately
    state.store.upsert_flag(payload.clone());

    // Fan-out to other server instances & connected SDKs via Redis
    let mut redis_conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let msg = json!({"type": "UPSERT", "flag": payload}).to_string();
    let _: () = redis_conn
        .publish("sidekick_updates", msg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(payload))
}

async fn get_flag(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Flag>, StatusCode> {
    if let Some(flag) = state.store.get_flag(&key) {
        return Ok(Json(flag));
    }
    Err(StatusCode::NOT_FOUND)
}

async fn delete_flag(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, StatusCode> {
    sqlx::query("DELETE FROM flags WHERE key = $1")
        .bind(&key)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state.store.delete_flag(&key);

    let mut redis_conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let msg = json!({"type": "DELETE", "key": key}).to_string();
    let _: () = redis_conn
        .publish("sidekick_updates", msg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /api/flags/:key — partial update via JSON merge.
/// Only provided fields are changed; omitted fields retain their current values.
async fn patch_flag(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(patch): Json<serde_json::Value>,
) -> Result<Json<Flag>, StatusCode> {
    // Read current flag from DB as raw JSON so we can merge safely.
    let rec = sqlx::query("SELECT data FROM flags WHERE key = $1")
        .bind(&key)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut flag_val: serde_json::Value = rec
        .try_get("data")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // JSON merge: overwrite only the keys present in the patch body.
    if let (serde_json::Value::Object(map), serde_json::Value::Object(patch_map)) =
        (&mut flag_val, patch)
    {
        for (k, v) in patch_map {
            map.insert(k, v);
        }
    }

    // Validate the merged result deserialises to a valid Flag.
    let flag: Flag =
        serde_json::from_value(flag_val.clone()).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;

    sqlx::query("UPDATE flags SET data = $1 WHERE key = $2")
        .bind(&flag_val)
        .bind(&key)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state.store.upsert_flag(flag.clone());

    let mut redis_conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let msg = json!({"type": "UPSERT", "flag": flag}).to_string();
    let _: () = redis_conn
        .publish("sidekick_updates", msg)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(flag))
}
