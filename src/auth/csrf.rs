//! CSRF-Schutz fuer die HTML-Routen.
//!
//! Umgesetzt als Origin-Pruefung statt als Token in jedem Formular. Begruendung:
//!
//! 1. Das Session-Cookie hat `SameSite=Lax`. Browser senden es damit bei
//!    seitenfremden POST/PUT/DELETE-Anfragen gar nicht erst mit — der klassische
//!    CSRF-Angriff scheitert schon daran.
//! 2. Zusaetzlich prueft diese Middleware bei jeder veraendernden Anfrage, ob
//!    sie von der eigenen Herkunft stammt (`Sec-Fetch-Site`, ersatzweise
//!    `Origin`). Das faengt auch aeltere Browser und Sonderfaelle ab.
//!
//! Der Vorteil gegenueber Formular-Tokens: Es gibt kein verstecktes Feld, das
//! man beim Anlegen eines neuen Formulars vergessen kann. Neue Formulare sind
//! automatisch geschuetzt.
//!
//! Die JSON-API unter `/api` ist ausgenommen: Sie authentifiziert ueber einen
//! Bearer-Header, den ein Browser niemals von selbst mitschickt.

use axum::extract::{Request, State};
use axum::http::{Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::{Error, WebError};
use crate::state::AppState;

pub async fn protect(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let is_safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    );

    if is_safe || is_same_origin(&request, &state.config.public_url) {
        return next.run(request).await;
    }

    tracing::warn!(
        path = %request.uri().path(),
        "Veraendernde Anfrage mit fremder Herkunft abgewiesen"
    );
    WebError(Error::Forbidden).into_response()
}

fn is_same_origin(request: &Request, public_url: &str) -> bool {
    let headers = request.headers();

    // Moderne Browser sagen direkt, woher die Anfrage kommt.
    // "none" = direkt eingegeben oder Lesezeichen.
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        return matches!(site, "same-origin" | "none");
    }

    // Rueckfallebene fuer aeltere Browser und Nicht-Browser-Clients.
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(origin) => origin.eq_ignore_ascii_case(public_url.trim_end_matches('/')),
        // Kein Origin-Header: kein Browser-Formular, also kein CSRF-Vektor.
        None => true,
    }
}
