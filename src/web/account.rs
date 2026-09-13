//! Eigene Kontoseite unter /account.
//!
//! Fuer jeden angemeldeten Benutzer, unabhaengig von der Rolle. Ohne diese
//! Seite koennte niemand sein eigenes Passwort wechseln.

use axum::extract::{Form, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::auth::extract::CurrentUser;
use crate::auth::password;
use crate::domain::user::{self, User};
use crate::error::{Error, WebError};
use crate::state::AppState;
use crate::templates::{AccountPage, Layout, render};

#[derive(Debug, Deserialize)]
pub struct ProfileForm {
    pub display_name: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordForm {
    pub current_password: String,
    pub new_password: String,
    pub new_password_repeat: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountQuery {
    #[serde(default)]
    pub gespeichert: Option<String>,
}

pub async fn show(
    CurrentUser(user): CurrentUser,
    Query(query): Query<AccountQuery>,
) -> Result<Response, WebError> {
    let gespeichert = match query.gespeichert.as_deref() {
        Some("profil") => Some("profil"),
        Some("passwort") => Some("passwort"),
        _ => None,
    };
    seite(user, None, None, gespeichert)
}

fn seite(
    user: User,
    profil_error: Option<String>,
    passwort_error: Option<String>,
    gespeichert: Option<&'static str>,
) -> Result<Response, WebError> {
    render(AccountPage {
        layout: Layout::for_user("Mein Konto", user.clone(), "/account"),
        user,
        min_password_len: password::MIN_PASSWORD_LEN,
        profil_error,
        passwort_error,
        gespeichert,
    })
}

pub async fn update_profile(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Form(form): Form<ProfileForm>,
) -> Result<Response, WebError> {
    match user::update_profile(&state.db, user.id, &form.display_name, &form.email).await {
        Ok(_) => Ok(Redirect::to("/account?gespeichert=profil").into_response()),
        Err(err) => {
            let status = err.status();
            // Die abgelehnten Eingaben stehen lassen, damit nichts verlorengeht.
            let mit_eingabe = User {
                display_name: form.display_name,
                email: form.email,
                ..user
            };
            let body = seite(mit_eingabe, Some(err.public_message()), None, None)?;
            Ok((status, body).into_response())
        }
    }
}

pub async fn change_password(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Form(form): Form<PasswordForm>,
) -> Result<Response, WebError> {
    match passwort_wechseln(&state, &user, &form).await {
        Ok(()) => {
            tracing::info!(user_id = %user.id, "Passwort geändert");
            Ok(Redirect::to("/account?gespeichert=passwort").into_response())
        }
        Err(err) => {
            let status = err.status();
            let body = seite(user, None, Some(err.public_message()), None)?;
            Ok((status, body).into_response())
        }
    }
}

async fn passwort_wechseln(
    state: &AppState,
    user: &User,
    form: &PasswordForm,
) -> Result<(), Error> {
    // Das aktuelle Passwort abfragen, damit ein unbeaufsichtigter Rechner
    // nicht reicht, um das Konto zu uebernehmen.
    let stimmt =
        password::verify(form.current_password.clone(), user.password_hash.clone()).await?;
    if !stimmt {
        return Err(Error::BadRequest(
            "Das aktuelle Passwort stimmt nicht.".into(),
        ));
    }

    if form.new_password != form.new_password_repeat {
        return Err(Error::BadRequest(
            "Die beiden neuen Passwörter stimmen nicht überein.".into(),
        ));
    }

    user::change_password(&state.db, user.id, &form.new_password).await
}
