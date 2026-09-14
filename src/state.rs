//! Gemeinsamer Anwendungszustand, den alle Handler ueber `State<AppState>` sehen.
//!
//! Neue globale Abhaengigkeiten (S3-Client, Cache ...) kommen hier hinein.

use crate::config::Config;
use crate::mail::Mailer;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// E-Mail-Versand. Benutzung: `state.mailer.send(Mail::to(..)...)`.
    pub mailer: Mailer,
}

impl AppState {
    pub fn new(db: PgPool, config: Config, mailer: Mailer) -> Self {
        Self {
            db,
            config: Arc::new(config),
            mailer,
        }
    }
}
