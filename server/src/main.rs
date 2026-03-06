mod api;
mod auth;
mod rate_limit;
mod state;
mod stream;

use axum::{Router, extract::DefaultBodyLimit, http::Method, middleware, routing::get};
use rate_limit::new_rate_limiter;
use sidekick_core::evaluator::Flag;
use sidekick_core::store::FlagStore;
use sqlx::{Row, postgres::PgPoolOptions};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting Sidekick Control Plane...");

    // Connect to PostgreSQL
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://sidekick:password@localhost/sidekick".to_string());
    let db = PgPoolOptions::new()
        .max_connections(
            std::env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        )
        .connect(&db_url)
        .await?;
    info!("Connected to PostgreSQL.");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS flags (
            key VARCHAR(255) PRIMARY KEY,
            data JSONB NOT NULL
        );
        "#,
    )
    .execute(&db)
    .await?;

    // Connect to Redis
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let redis_client = redis::Client::open(redis_url)?;
    let _test_conn = redis_client.get_multiplexed_async_connection().await?;
    info!("Connected to Redis.");

    // Read optional SDK key from environment
    let sdk_key = match std::env::var("SDK_KEY") {
        Ok(k) if !k.is_empty() => {
            info!("SDK_KEY configured — API authentication enabled.");
            Some(k)
        }
        _ => {
            warn!("SDK_KEY not set — API authentication disabled. Set SDK_KEY in production.");
            None
        }
    };

    // Broadcast channel: a single Redis subscriber task pushes flag update payloads
    // here; SSE handlers subscribe instead of each opening their own Redis connection.
    let (flag_tx, _) = broadcast::channel::<String>(256);

    let store = Arc::new(FlagStore::new());

    // Load existing flags into in-memory store from Postgres
    let records = sqlx::query("SELECT data FROM flags").fetch_all(&db).await?;
    let mut count = 0;
    for rec in records {
        let data: serde_json::Value = rec.try_get("data")?;
        if let Ok(flag) = serde_json::from_value::<Flag>(data) {
            store.upsert_flag(flag);
            count += 1;
        }
    }
    info!("Loaded {count} flags into memory cache.");

    // Spawn the single Redis pub/sub subscriber.
    // It keeps all server instances in sync by updating the local store, then
    // broadcasts payloads to SSE handlers via the in-process broadcast channel.
    {
        let redis_sub = redis_client.clone();
        let tx = flag_tx.clone();
        let store_ref = Arc::clone(&store);
        tokio::spawn(async move {
            loop {
                match redis_sub.get_async_pubsub().await {
                    Err(e) => {
                        error!("Redis subscriber: connection failed: {e}. Retrying in 2s.");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    Ok(mut con) => {
                        if let Err(e) = con.subscribe("sidekick_updates").await {
                            error!("Redis subscriber: subscribe failed: {e}. Retrying in 2s.");
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            continue;
                        }
                        info!("Redis subscriber: listening on sidekick_updates.");
                        let mut msg_stream = con.into_on_message();
                        while let Some(msg) = msg_stream.next().await {
                            if let Ok(payload) = msg.get_payload::<String>() {
                                // Sync local store so this instance reflects writes from peers.
                                if let Ok(event) =
                                    serde_json::from_str::<serde_json::Value>(&payload)
                                {
                                    match event.get("type").and_then(|t| t.as_str()) {
                                        Some("UPSERT") => {
                                            if let Some(f) = event.get("flag").and_then(|f| {
                                                serde_json::from_value::<Flag>(f.clone()).ok()
                                            }) {
                                                store_ref.upsert_flag(f);
                                            }
                                        }
                                        Some("DELETE") => {
                                            if let Some(k) =
                                                event.get("key").and_then(|k| k.as_str())
                                            {
                                                store_ref.delete_flag(k);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                // Forward to all connected SSE handlers.
                                let _ = tx.send(payload);
                            }
                        }
                        warn!("Redis subscriber: connection dropped. Reconnecting.");
                    }
                }
            }
        });
    }

    let app_state = state::AppState {
        db,
        redis_client,
        store,
        flag_tx,
        sdk_key,
        rate_limiter: new_rate_limiter(),
    };

    // CORS — permissive defaults for self-hosted deployment.
    let cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .allow_origin(Any);

    // Static dashboard
    let public_dir = std::env::var("PUBLIC_DIR").unwrap_or_else(|_| "public".to_string());
    let serve_dashboard =
        ServeDir::new(&public_dir).fallback(ServeFile::new(format!("{public_dir}/index.html")));

    // API routes: 64 KB body limit + per-IP rate limiting
    let api_router =
        api::router()
            .layer(DefaultBodyLimit::max(65_536))
            .layer(middleware::from_fn_with_state(
                app_state.clone(),
                rate_limit::rate_limit,
            ));

    let app = Router::new()
        .nest("/api", api_router)
        .route("/stream", get(stream::sse_handler))
        .layer(middleware::from_fn_with_state(
            app_state.clone(),
            auth::require_auth,
        ))
        .layer(cors)
        .fallback_service(serve_dashboard)
        .with_state(app_state);

    let bind_addr = format!(
        "0.0.0.0:{}",
        std::env::var("PORT").unwrap_or_else(|_| "3000".to_string())
    );
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("Server listening on {}", listener.local_addr()?);

    // Serve with ConnectInfo (needed for per-IP rate limiting) and graceful shutdown.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    info!("Server shut down gracefully.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { info!("Received Ctrl+C, shutting down."); },
        _ = terminate => { info!("Received SIGTERM, shutting down."); },
    }
}
