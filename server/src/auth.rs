use crate::state::AppState;
use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

/// Validates the SDK key from either:
///   - `Authorization: Bearer <key>` header  (Node.js, Flutter, React Native)
///   - `?sdk_key=<key>` query parameter       (Browser EventSource — can't send headers)
///
/// If `SDK_KEY` is not set in the environment, auth is skipped (dev convenience).
pub async fn require_auth(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth entirely when no key is configured (local dev)
    let Some(ref expected) = state.sdk_key else {
        return Ok(next.run(req).await);
    };

    // Check Authorization header first
    let header_key = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    if header_key == Some(expected.as_str()) {
        return Ok(next.run(req).await);
    }

    // Fall back to ?sdk_key= query param (for browser SSE which can't set headers)
    let query = req.uri().query().unwrap_or("");
    let query_key = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("sdk_key="));

    if query_key == Some(expected.as_str()) {
        return Ok(next.run(req).await);
    }

    Err(StatusCode::UNAUTHORIZED)
}
