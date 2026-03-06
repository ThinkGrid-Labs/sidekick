use crate::state::AppState;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::{convert::Infallible, time::Duration};
use tokio::sync::broadcast;
use tracing::{info, warn};

pub async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("New SDK client connected via SSE");

    // Subscribe BEFORE bootstrap so no update can slip through between the
    // snapshot and the live stream. No per-client Redis connection is opened.
    let mut rx = state.flag_tx.subscribe();

    let stream = async_stream::stream! {
        // 1. Signal connection established — SDK clears its local cache on this event.
        yield Ok(Event::default().event("connected").data("true"));

        // 2. Bootstrap: replay the full flag set so the SDK starts from a clean state.
        for flag in state.store.list_flags() {
            let payload = serde_json::json!({"type": "UPSERT", "flag": flag}).to_string();
            yield Ok(Event::default().event("update").data(payload));
        }

        // 3. Stream live deltas from the shared broadcast channel.
        loop {
            match rx.recv().await {
                Ok(payload) => {
                    yield Ok(Event::default().event("update").data(payload));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // Client is too slow; force a reconnect so it re-bootstraps cleanly.
                    warn!("SSE client lagged, missed {n} updates — closing for reconnect");
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive-text"),
    )
}
