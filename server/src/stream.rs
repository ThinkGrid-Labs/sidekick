use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::{convert::Infallible, time::Duration};
use crate::state::AppState;
use tokio_stream::StreamExt;
use tracing::info;

pub async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("New SDK client connected via SSE");

    let stream = async_stream::stream! {
        // 1. Subscribe to Redis FIRST — ensures no delta published between bootstrap
        //    and subscription can be missed.
        let mut con = state.redis_client
            .get_async_pubsub()
            .await
            .expect("Failed to get pubsub connection");
        con.subscribe("sidekick_updates").await.expect("Failed to subscribe to sidekick_updates");
        let mut msg_stream = con.into_on_message();

        // 2. Signal connection established (SDK clears its local cache on this event
        //    so the full state below replaces any stale entries).
        yield Ok(Event::default().event("connected").data("true"));

        // 3. Send the complete current flag set so the SDK rebuilds from a clean state.
        for flag in state.store.list_flags() {
            let payload = serde_json::json!({"type": "UPSERT", "flag": flag}).to_string();
            yield Ok(Event::default().event("update").data(payload));
        }

        // 4. Forward live delta updates from Redis pub/sub.
        while let Some(msg) = msg_stream.next().await {
            if let Ok(payload) = msg.get_payload::<String>() {
                yield Ok(Event::default().event("update").data(payload));
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive-text"),
    )
}
