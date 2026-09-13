//! Datenbankverbindung und Migrationen.
//!
//! Hinweis zu den Queries im Projekt: Wir benutzen bewusst `sqlx::query_as`
//! (zur Laufzeit geprueft) statt der `query_as!`-Makros (zur Compile-Zeit
//! geprueft). Grund: Die Makros brauchen bei JEDEM Build eine erreichbare
//! Datenbank oder ein aktuelles `.sqlx`-Verzeichnis. Das bricht Docker-Builds
//! und macht schnelles Iterieren muehsam. Die Tests in `tests/` fangen
//! fehlerhafte Queries stattdessen ab.

use crate::config::Config;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

pub async fn connect(config: &Config) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&config.database_url)
        .await?;

    tracing::info!("Datenbankverbindung steht");
    Ok(pool)
}

/// Fuehrt alle Migrationen aus `migrations/` aus. Wird beim Start aufgerufen,
/// ist idempotent und damit auch bei mehreren Instanzen unproblematisch.
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("Migrationen aktuell");
    Ok(())
}
