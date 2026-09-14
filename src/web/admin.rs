//! Benutzerverwaltung. Erreichbar nur mit der Rolle Administrator —
//! erzwungen durch den `AdminUser`-Extractor in jeder Signatur.
//!
//! Aufteilung: Die Liste zeigt und legt an, die Detailseite aendert. Alle
//! Aenderungen laufen als normales Formular mit anschliessender Weiterleitung
//! (Post/Redirect/Get). Dadurch loest ein Neuladen keine zweite Aenderung aus,
//! und Fehler koennen als vollstaendige Seite mit erhaltenen Eingaben
//! zurueckkommen.

use axum::extract::{Form, Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::Role;
use crate::auth::extract::AdminUser;
use crate::auth::password;
use crate::domain::user::{self, User};
use crate::error::{Error, WebError};
use crate::state::AppState;
use crate::templates::{AdminUserPage, AdminUsersPage, Layout, render};

#[derive(Debug, Deserialize)]
pub struct CreateUserForm {
    pub display_name: String,
    pub email: String,
    pub password: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserForm {
    pub display_name: String,
    pub email: String,
    pub role: String,
    /// Ein nicht angekreuztes Kontrollkaestchen sendet gar nichts.
    #[serde(default)]
    pub is_active: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PasswordForm {
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct DetailQuery {
    #[serde(default)]
    pub saved: Option<String>,
}

// --- Liste ------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Suchbegriff aus dem Formular. Leer bedeutet: alle anzeigen.
    #[serde(default)]
    pub q: String,
}

pub async fn index(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Query(query): Query<SearchQuery>,
) -> Result<Response, WebError> {
    render_list(&state, admin, None, "", "", &query.q).await
}

/// Rendert die Liste. Bei einem Fehler im Anlegen-Formular bleiben die
/// Eingaben stehen, damit nichts neu getippt werden muss.
async fn render_list(
    state: &AppState,
    admin: User,
    error: Option<String>,
    new_display_name: &str,
    new_email: &str,
    search: &str,
) -> Result<Response, WebError> {
    let users = user::list(&state.db, Some(search)).await?;
    let total = user::count(&state.db).await?;
    let current_user_id = admin.id;

    render(AdminUsersPage {
        layout: Layout::for_user("Benutzer", admin, "/admin/users"),
        users,
        search: search.to_string(),
        total,
        error,
        new_display_name: new_display_name.to_string(),
        new_email: new_email.to_string(),
        roles: Role::ALL.to_vec(),
        min_password_len: password::MIN_PASSWORD_LEN,
        current_user_id,
    })
}

pub async fn create(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Form(form): Form<CreateUserForm>,
) -> Result<Response, WebError> {
    let role: Role = form.role.parse()?;

    match user::create(
        &state.db,
        &form.email,
        &form.display_name,
        &form.password,
        role,
    )
    .await
    {
        Ok(created) => {
            tracing::info!(by = %admin.id, user_id = %created.id, %role, "Benutzer angelegt");
            Ok(Redirect::to(&format!("/admin/users/{}?saved=created", created.id)).into_response())
        }
        Err(err) => {
            let status = err.status();
            let body = render_list(
                &state,
                admin,
                Some(err.public_message()),
                &form.display_name,
                &form.email,
                "",
            )
            .await?;
            Ok((status, body).into_response())
        }
    }
}

// --- Detailseite ------------------------------------------------------------

pub async fn show(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
    Query(query): Query<DetailQuery>,
) -> Result<Response, WebError> {
    render_detail(&state, admin, id, None, query.saved.is_some()).await
}

async fn render_detail(
    state: &AppState,
    admin: User,
    id: Uuid,
    error: Option<String>,
    saved: bool,
) -> Result<Response, WebError> {
    let edited = user::find_by_id(&state.db, id)
        .await?
        .ok_or(Error::NotFound)?;

    // Steuert, welche Bedienelemente die Seite sperrt. Die eigentliche
    // Absicherung liegt in der Domain-Schicht, nicht in dieser Anzeige.
    let is_self = edited.id == admin.id;
    let is_last_admin = edited.role == Role::Admin
        && edited.is_active
        && user::count_active_admins(&state.db).await? <= 1;

    render(AdminUserPage {
        layout: Layout::for_user(edited.display_name.clone(), admin, "/admin/users"),
        edited,
        roles: Role::ALL.to_vec(),
        min_password_len: password::MIN_PASSWORD_LEN,
        is_self,
        is_last_admin,
        error,
        saved,
    })
}

pub async fn update(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
    Form(form): Form<UpdateUserForm>,
) -> Result<Response, WebError> {
    let role: Role = form.role.parse()?;
    let should_be_active = form.is_active.is_some();

    match apply_changes(&state, &admin, id, &form, role, should_be_active).await {
        Ok(()) => {
            tracing::info!(by = %admin.id, user_id = %id, %role, active = should_be_active, "Benutzer geändert");
            Ok(Redirect::to(&format!("/admin/users/{id}?saved=1")).into_response())
        }
        Err(err) => {
            let status = err.0.status();
            let body =
                render_detail(&state, admin, id, Some(err.0.public_message()), false).await?;
            Ok((status, body).into_response())
        }
    }
}

/// Die drei Teiländerungen in einem Schritt — Stammdaten, Rolle, Status.
async fn apply_changes(
    state: &AppState,
    admin: &User,
    id: Uuid,
    form: &UpdateUserForm,
    role: Role,
    should_be_active: bool,
) -> Result<(), WebError> {
    // Sonst koennte sich ein Administrator selbst degradieren oder aussperren
    // und danach nicht mehr an die Verwaltung kommen.
    if id == admin.id && (role != Role::Admin || !should_be_active) {
        return Err(WebError(Error::BadRequest(
            "Die eigene Rolle und der eigene Kontostatus lassen sich hier nicht ändern.".into(),
        )));
    }

    user::update_profile(&state.db, id, &form.display_name, &form.email).await?;

    // Reihenfolge beachtet: Erst degradieren, dann sperren. Beide Aufrufe
    // pruefen selbst, ob dadurch der letzte Administrator verschwaende.
    let current = user::find_by_id(&state.db, id)
        .await?
        .ok_or(Error::NotFound)?;
    if current.role != role {
        user::set_role(&state.db, id, role).await?;
    }
    if current.is_active != should_be_active {
        user::set_active(&state.db, id, should_be_active).await?;
    }
    Ok(())
}

pub async fn set_password(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
    Form(form): Form<PasswordForm>,
) -> Result<Response, WebError> {
    match user::change_password(&state.db, id, &form.password).await {
        Ok(()) => {
            tracing::info!(by = %admin.id, user_id = %id, "Passwort zurückgesetzt");
            Ok(Redirect::to(&format!("/admin/users/{id}?saved=password")).into_response())
        }
        Err(err) => {
            let status = err.status();
            let body = render_detail(&state, admin, id, Some(err.public_message()), false).await?;
            Ok((status, body).into_response())
        }
    }
}

pub async fn delete(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<Uuid>,
) -> Result<Response, WebError> {
    if id == admin.id {
        let body = render_detail(
            &state,
            admin,
            id,
            Some("Das eigene Konto lässt sich nicht löschen.".into()),
            false,
        )
        .await?;
        return Ok((axum::http::StatusCode::BAD_REQUEST, body).into_response());
    }

    match user::delete(&state.db, id).await {
        Ok(()) => {
            tracing::info!(by = %admin.id, user_id = %id, "Benutzer gelöscht");
            Ok(Redirect::to("/admin/users").into_response())
        }
        Err(err) => {
            let status = err.status();
            let body = render_detail(&state, admin, id, Some(err.public_message()), false).await?;
            Ok((status, body).into_response())
        }
    }
}
