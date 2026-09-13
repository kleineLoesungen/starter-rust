//! Zusammenbau der Anwendung.
//!
//! Bewusst hier und nicht in `main.rs`: So koennen Integrationstests dieselbe
//! Anwendung starten, die auch in Produktion laeuft — inklusive Sessions,
//! CSRF-Pruefung und aller Schichten. Ein Test, der einen anderen Router baut
//! als die Produktion, prueft die Produktion nicht.

use anyhow::Context;
use axum::Router;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use sqlx::PgPool;
use time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tower_sessions::cookie::SameSite;
use tower_sessions::session_store::ExpiredDeletion;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::PostgresStore;

use crate::config::Config;
use crate::state::AppState;
use crate::{api, auth, templates, web};

/// Wie lange eine Anmeldung ohne Aktivitaet gueltig bleibt.
const SESSION_DAYS: i64 = 14;

/// Baut die vollstaendige Anwendung.
///
/// Gibt zusaetzlich den Aufraeum-Task fuer abgelaufene Sessions zurueck —
/// `main` haelt ihn am Leben, Tests werfen ihn weg.
pub async fn build(
    pool: PgPool,
    config: Config,
) -> anyhow::Result<(Router, tokio::task::JoinHandle<()>)> {
    // Sessions liegen in derselben Datenbank. Der Store bringt seine eigene
    // Migration mit und legt die Tabelle `tower_sessions` an.
    let session_store = PostgresStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .context("Session-Tabelle konnte nicht angelegt werden")?;

    let cleanup_store = session_store.clone();
    let cleanup_db = pool.clone();
    let cleanup = tokio::task::spawn(async move {
        // Abgelaufene Anmeldeversuche stuendlich wegraeumen, damit die Tabelle
        // nicht endlos waechst. Fehler sind unkritisch und werden nur geloggt.
        let versuche = async {
            let mut takt = tokio::time::interval(tokio::time::Duration::from_secs(3600));
            loop {
                takt.tick().await;
                match crate::domain::login_versuch::aufraeumen(&cleanup_db).await {
                    Ok(n) if n > 0 => tracing::debug!("{n} alte Anmeldeversuche entfernt"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("Anmeldeversuche aufraeumen fehlgeschlagen: {e}"),
                }
            }
        };

        let sessions = async {
            if let Err(e) = cleanup_store
                .continuously_delete_expired(tokio::time::Duration::from_secs(3600))
                .await
            {
                tracing::warn!("Aufraeumen abgelaufener Sessions beendet: {e}");
            }
        };

        tokio::join!(versuche, sessions);
    });

    let session_layer = SessionManagerLayer::new(session_store)
        .with_name("starter_session")
        // Ohne HTTPS kann das Cookie nicht "secure" sein — sonst funktioniert
        // die lokale Entwicklung nicht. In Produktion COOKIE_SECURE=true.
        .with_secure(config.cookie_secure)
        .with_http_only(true)
        // "Lax" ist der CSRF-Grundschutz: Browser senden das Cookie nicht bei
        // seitenfremden POST-Anfragen. Siehe src/auth/csrf.rs.
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(SESSION_DAYS)));

    let state = AppState::new(pool, config);

    let router = Router::new()
        // HTML-Oberflaeche — mit Herkunftspruefung gegen CSRF.
        .merge(web::routes().layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::csrf::protect,
        )))
        // JSON-Schnittstelle — per Bearer-Token authentifiziert, daher ohne CSRF-Pruefung.
        .nest("/api", api::routes())
        .nest_service("/static", ServeDir::new("static"))
        .fallback(not_found)
        .layer(security_headers())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        // Aeusserste Schicht: Ohne Session koennen die Extractors nicht arbeiten.
        .layer(session_layer)
        .with_state(state);

    Ok((router, cleanup))
}

/// Kopfzeilen, die nichts kosten und viel abfangen.
///
/// Bewusst NICHT dabei: `Content-Security-Policy`. Die Oberflaeche benutzt
/// htmx-Attribute wie `hx-on::after-request`, die eine strenge CSP blockieren
/// wuerde. Wie man sie trotzdem einschaltet, steht in docs/DEPLOY.md.
fn security_headers() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    )
}

async fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Html(templates::render_error_page(
            StatusCode::NOT_FOUND,
            "Diese Seite gibt es nicht.",
        )),
    )
        .into_response()
}
