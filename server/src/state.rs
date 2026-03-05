use sidekick_core::store::FlagStore;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis_client: redis::Client,
    pub store: Arc<FlagStore>, // Holds the in-memory state of the latest flags across all environments
    /// SDK key for bearer-token auth. `None` disables auth (dev mode).
    pub sdk_key: Option<String>,
}
