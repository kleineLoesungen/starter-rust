//! Startpunkt: Konfiguration lesen, Datenbank verbinden, Server starten.
//!
//! Die Anwendung selbst wird in `starter::app::build` zusammengesetzt, damit
//! Integrationstests exakt dieselbe Anwendung hochfahren koennen.

use anyhow::Context;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use starter::config::Config;
use starter::{app, db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    init_tracing();

    let pool = db::connect(&config)
        .await
        .context("Datenbank nicht erreichbar")?;
    db::migrate(&pool)
        .await
        .context("Migrationen fehlgeschlagen")?;

    let bind_addr = config.bind_addr;
    let (router, cleanup) = app::build(pool, config).await?;

    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("Port {bind_addr} ist belegt"))?;

    tracing::info!("Server laeuft auf http://{bind_addr}");

    // `into_make_service_with_connect_info` reicht die Adresse des Clients an
    // die Handler durch. Ohne sie kann die Anmeldebremse nur pro Konto zaehlen,
    // nicht pro Rechner — siehe src/auth/client_ip.rs.
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    cleanup.abort();
    tracing::info!("Server beendet");
    Ok(())
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            // Standardmaessig die eigene Anwendung auf INFO, den Rest ruhig halten.
            "starter=info,tower_http=warn,sqlx=warn".into()
        }))
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Sauberes Herunterfahren: Laufende Anfragen werden noch zu Ende bedient.
/// SIGTERM ist wichtig, weil Container-Umgebungen damit stoppen.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Signal-Handler fehlgeschlagen");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM-Handler fehlgeschlagen")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Strg+C empfangen"),
        _ = terminate => tracing::info!("SIGTERM empfangen"),
    }
}
