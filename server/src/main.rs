mod api;
mod auth;
mod state;
mod stream;

use axum::{Router, middleware, routing::get};
use sidekick_core::store::FlagStore;
use sqlx::{Row, postgres::PgPoolOptions};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting Sidekick Control Plane...");

    // Connect to PostgreSQL
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://sidekick:password@localhost/sidekick".to_string());
    let db = PgPoolOptions::new()
        .max_connections(5)
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
    let mut _test_conn = redis_client.get_multiplexed_async_connection().await?;
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

    // Setup Shared State
    let state = state::AppState {
        db,
        redis_client,
        store: Arc::new(FlagStore::new()),
        sdk_key,
    };

    // Load existing flags into in-memory store from Postgres
    let records = sqlx::query("SELECT data FROM flags")
        .fetch_all(&state.db)
        .await?;
    let mut count = 0;
    for rec in records {
        let data: serde_json::Value = rec.try_get("data")?;
        if let Ok(flag) = serde_json::from_value::<sidekick_core::evaluator::Flag>(data) {
            state.store.upsert_flag(flag);
            count += 1;
        }
    }
    info!("Loaded {} flags into memory cache.", count);

    // Static dashboard — served at GET / (falls back to index.html for SPA routing)
    let public_dir = std::env::var("PUBLIC_DIR").unwrap_or_else(|_| "public".to_string());
    let serve_dashboard =
        ServeDir::new(&public_dir).fallback(ServeFile::new(format!("{}/index.html", public_dir)));

    // Build Axum app — auth middleware wraps API + stream routes only
    let app = Router::new()
        .nest("/api", api::router())
        .route("/stream", get(stream::sse_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .fallback_service(serve_dashboard)
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
