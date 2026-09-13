//! JSON-Schnittstelle mit App-Token-Authentifizierung.
//!
//! Trennung zur HTML-Oberflaeche ist Absicht:
//!
//! | | `src/web/` | `src/api/` |
//! |---|---|---|
//! | Authentifizierung | Session-Cookie | `Authorization: Bearer sk_…` |
//! | Antwort | HTML | JSON |
//! | Fehler | Fehlerseite | `{"error": {...}}` |
//! | CSRF-Schutz | ja (Origin-Pruefung) | nicht noetig (kein Cookie) |
//!
//! Versionierung: Alles liegt unter `/api/v1`. Bei brechenden Aenderungen
//! kommt `v2` daneben, `v1` bleibt bestehen.

pub mod v1;

use axum::Router;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().nest("/v1", v1::routes())
}
