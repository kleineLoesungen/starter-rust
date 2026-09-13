//! Gemeinsamer Anwendungszustand, den alle Handler ueber `State<AppState>` sehen.
//!
//! Neue globale Abhaengigkeiten (Mailer, S3-Client, Cache ...) kommen hier hinein.

use crate::config::Config;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(db: PgPool, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
        }
    }
}
