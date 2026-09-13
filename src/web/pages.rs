//! Einfache Seiten ohne eigene Fachlogik.

use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};

use crate::auth::extract::{CurrentUser, MaybeUser, ModeratorUser};
use crate::domain::{api_token, note, user};
use crate::error::WebError;
use crate::state::AppState;
use crate::templates::{
    DashboardPage, LandingPage, Layout, ModerationPage, StyleguidePage, render,
};

/// Startseite. Angemeldete Benutzer landen direkt in der Anwendung.
pub async fn landing(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
) -> Result<Response, WebError> {
    if user.is_some() {
        return Ok(Redirect::to("/dashboard").into_response());
    }
    // Solange kein Konto existiert, fuehrt die Startseite zur Ersteinrichtung.
    let ersteinrichtung = user::count(&state.db).await? == 0;
    render(LandingPage {
        layout: Layout::new("Start", user, "/"),
        ersteinrichtung,
    })
}

pub async fn dashboard(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Response, WebError> {
    let note_count = note::list_for_user(&state.db, user.id).await?.len();

    // Tokens sind systemweite Maschinen-Zugaenge — die Zahl sieht nur, wer sie
    // auch verwalten darf.
    let token_count = if user.is_admin() {
        Some(
            api_token::list(&state.db)
                .await?
                .iter()
                .filter(|t| t.is_valid())
                .count(),
        )
    } else {
        None
    };

    render(DashboardPage {
        layout: Layout::for_user("Übersicht", user.clone(), "/dashboard"),
        user,
        note_count,
        token_count,
    })
}

/// Beispiel fuer eine rollengeschuetzte Seite. Der einzige Unterschied zu einer
/// gewoehnlichen Seite ist der Extractor in der Signatur.
pub async fn moderation(ModeratorUser(user): ModeratorUser) -> Result<Response, WebError> {
    render(ModerationPage {
        layout: Layout::for_user("Moderation", user, "/moderation"),
    })
}

pub async fn styleguide(MaybeUser(user): MaybeUser) -> Result<Response, WebError> {
    render(StyleguidePage {
        layout: Layout::new("Styleguide", user, "/styleguide"),
        swatches: StyleguidePage::swatches(),
    })
}
