//! Verwaltung der App-Tokens unter /admin/tokens.
//!
//! Nur Administratoren. Tokens sind Zugaenge der Installation fuer fremde
//! Systeme — sie gehoeren keinem Benutzer, deshalb sehen alle Administratoren
//! dieselbe Liste.

use axum::extract::{Path, State};
// Bewusst NICHT axum::extract::Form: Das benutzt serde_urlencoded und kann
// wiederholte Felder nicht in ein Vec einlesen — genau das senden aber
// mehrere Kontrollkaestchen mit demselben Namen. axum_extra::Form kann es.
use axum::response::{IntoResponse, Response};
use axum_extra::extract::Form;
use serde::Deserialize;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::auth::extract::AdminUser;
use crate::auth::scope;
use crate::domain::api_token;
use crate::error::WebError;
use crate::state::AppState;
use crate::templates::{Layout, TokenCreatedPartial, TokensPage, render};

#[derive(Debug, Deserialize)]
pub struct CreateTokenForm {
    pub name: String,
    /// Leerer String bedeutet "unbegrenzt".
    #[serde(default)]
    pub expires_in_days: String,
    /// Angekreuzte Berechtigungen.
    ///
    /// Mehrere Kontrollkaestchen mit demselben `name` senden den Schluessel
    /// mehrfach: `scopes=notes:read&scopes=notes:write`. Damit das hier als
    /// Liste ankommt, MUSS der Extractor `axum_extra::extract::Form` sein.
    /// Mit `axum::extract::Form` scheitert die Anfrage an einem 422.
    #[serde(default)]
    pub scopes: Vec<String>,
}

pub async fn index(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
) -> Result<Response, WebError> {
    let tokens = api_token::list(&state.db).await?;
    render(TokensPage {
        layout: Layout::for_user("App-Tokens", admin, "/admin/tokens"),
        tokens,
        scopes: scope::ALL,
        public_url: state.config.public_url.clone(),
    })
}

pub async fn create(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Form(form): Form<CreateTokenForm>,
) -> Result<Response, WebError> {
    let expires_at = form
        .expires_in_days
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|d| *d > 0)
        .map(|days| OffsetDateTime::now_utc() + Duration::days(days));

    let created =
        api_token::create(&state.db, admin.id, &form.name, &form.scopes, expires_at).await?;

    tracing::info!(
        by = %admin.id,
        token_id = %created.token.token_id,
        scopes = ?created.token.scopes,
        "App-Token ausgestellt"
    );

    render(TokenCreatedPartial {
        name: created.token.name.clone(),
        plaintext: created.plaintext,
        token: created.token,
    })
}

pub async fn revoke(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, WebError> {
    api_token::revoke(&state.db, id).await?;
    tracing::info!(by = %admin.id, %id, "App-Token widerrufen");

    // Leere Antwort: die Zeile verschwindet aus der Tabelle.
    Ok(().into_response())
}
