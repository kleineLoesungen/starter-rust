//! HTML-Oberflaeche. Alle Routen hier liefern HTML oder htmx-Fragmente.
//!
//! Die Authentifizierung laeuft ueber das Session-Cookie; die JSON-Schnittstelle
//! mit App-Tokens liegt getrennt davon in `crate::api`.

pub mod account;
pub mod admin;
pub mod auth;
pub mod notes;
pub mod pages;
pub mod tokens;

use axum::Router;
use axum::routing::{delete, get, post};

use crate::state::AppState;

/// Baut alle HTML-Routen zusammen.
///
/// Neue Seite hinzufuegen:
///   1. Template unter `templates/pages/` anlegen
///   2. Struct in `src/templates.rs` ergaenzen
///   3. Handler schreiben und hier eintragen
pub fn routes() -> Router<AppState> {
    Router::new()
        // Oeffentlich
        .route("/", get(pages::landing))
        .route("/styleguide", get(pages::styleguide))
        .route("/login", get(auth::login_page).post(auth::login))
        // Nur fuer die Ersteinrichtung: greift, solange es kein Konto gibt.
        .route("/register", get(auth::register_page).post(auth::register))
        .route("/logout", post(auth::logout))
        // Angemeldet
        .route("/dashboard", get(pages::dashboard))
        .route("/account", get(account::show).post(account::update_profile))
        .route("/account/password", post(account::change_password))
        .route("/notes", get(notes::index).post(notes::create))
        .route("/notes/{id}", delete(notes::delete))
        // Ab Moderator
        .route("/moderation", get(pages::moderation))
        // Nur Administrator
        .route("/admin/users", get(admin::index).post(admin::create))
        .route("/admin/users/{id}", get(admin::show).post(admin::update))
        .route("/admin/users/{id}/password", post(admin::set_password))
        .route("/admin/users/{id}/delete", post(admin::delete))
        .route("/admin/tokens", get(tokens::index).post(tokens::create))
        .route("/admin/tokens/{id}/revoke", post(tokens::revoke))
}
