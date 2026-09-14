//! Anmelden, Registrieren, Abmelden.
//!
//! Diese Formulare sind bewusst normale HTML-Formulare ohne htmx: Sie enden in
//! einer Weiterleitung, und dabei soll der Browser die Adresszeile aendern.

use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_sessions::Session;

use crate::auth::client_ip::ClientIp;
use crate::auth::{Role, password, session};
use crate::domain::{login_attempt, user};
use crate::error::{Error, WebError};
use crate::state::AppState;
use crate::templates::{Layout, LoginPage, RegisterPage, render};

#[derive(Debug, Deserialize)]
pub struct NextQuery {
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub next: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    pub display_name: String,
    pub email: String,
    pub password: String,
}

pub async fn login_page(
    State(state): State<AppState>,
    Query(query): Query<NextQuery>,
) -> Result<Response, WebError> {
    render(LoginPage {
        layout: Layout::new("Anmelden", None, "/login"),
        error: None,
        first_setup: is_first_setup(&state).await?,
        email: String::new(),
        next: safe_next(query.next.as_deref()),
    })
}

/// `true`, solange die Installation noch kein einziges Konto hat.
///
/// Die Selbstregistrierung ist geschlossen: Sie dient nur dazu, den ersten
/// Administrator anzulegen. Danach legt dieser alle weiteren Konten an.
async fn is_first_setup(state: &AppState) -> Result<bool, WebError> {
    Ok(user::count(&state.db).await? == 0)
}

pub async fn login(
    State(state): State<AppState>,
    session: Session,
    ClientIp(ip): ClientIp,
    Form(form): Form<LoginForm>,
) -> Result<Response, WebError> {
    let attempt_keys = login_attempt::keys(&form.email, ip.as_deref());

    // Erst die Bremse, dann das Passwort: Ein gesperrter Versuch soll den
    // Server keine Argon2-Rechenzeit kosten.
    let lock = login_attempt::check(&state.db, &attempt_keys).await;

    let result = match lock {
        Err(e) => Err(e),
        Ok(()) => user::authenticate(&state.db, &form.email, &form.password).await,
    };

    match result {
        Ok(user) => {
            login_attempt::record_success(&state.db, &attempt_keys).await?;
            session::login(&session, user.id).await?;
            tracing::info!(user_id = %user.id, "Anmeldung erfolgreich");
            Ok(Redirect::to(&safe_next(Some(&form.next))).into_response())
        }
        Err(err) => {
            // Nur echte Fehlversuche zaehlen. Wer schon gesperrt ist, soll die
            // Sperre nicht durch stures Weiterklicken verlaengern koennen.
            let message = match err {
                Error::TooManyRequests(seconds) => {
                    tracing::warn!(email = %form.email, ip = ?ip, "Anmeldung gesperrt");
                    format!(
                        "Zu viele Fehlversuche. Bitte {} erneut versuchen.",
                        wait_text(seconds)
                    )
                }
                Error::Forbidden => {
                    login_attempt::record_failure(&state.db, &attempt_keys).await?;
                    "Dieses Konto ist gesperrt.".to_string()
                }
                Error::Unauthorized => {
                    login_attempt::record_failure(&state.db, &attempt_keys).await?;
                    "E-Mail oder Passwort stimmt nicht.".to_string()
                }
                // Alles andere ist ein echter Fehler — Datenbank weg, Hashing
                // kaputt. Den als "Passwort stimmt nicht" auszugeben, wuerde
                // einen 500er als 401 tarnen und die Fehlersuche verhindern.
                other => return Err(WebError(other)),
            };

            let body = render(LoginPage {
                layout: Layout::new("Anmelden", None, "/login"),
                error: Some(message),
                first_setup: is_first_setup(&state).await?,
                email: form.email,
                next: safe_next(Some(&form.next)),
            })?;

            Ok((StatusCode::UNAUTHORIZED, body).into_response())
        }
    }
}

/// Wartezeit so ausdruecken, wie ein Mensch sie liest.
fn wait_text(seconds: u64) -> String {
    match seconds {
        0..=90 => format!("in {seconds} Sekunden"),
        s => {
            let minutes = s.div_ceil(60);
            format!("in {minutes} Minuten")
        }
    }
}

pub async fn register_page(State(state): State<AppState>) -> Result<Response, WebError> {
    if !is_first_setup(&state).await? {
        return Ok(registration_closed());
    }

    render(RegisterPage {
        layout: Layout::new("Konto anlegen", None, "/register"),
        error: None,
        display_name: String::new(),
        email: String::new(),
        min_password_len: password::MIN_PASSWORD_LEN,
    })
}

pub async fn register(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<RegisterForm>,
) -> Result<Response, WebError> {
    // Die Selbstregistrierung existiert nur fuer die Ersteinrichtung. Sobald
    // ein Konto da ist, legt der Administrator alle weiteren an.
    //
    // Wichtig: Diese Pruefung steht HIER und nicht nur im Template. Sonst
    // liesse sich das Formular einfach von Hand abschicken.
    if !is_first_setup(&state).await? {
        return Ok(registration_closed());
    }

    // Das erste Konto wird Administrator — sonst kaeme niemand an die
    // Benutzerverwaltung heran.
    let role = Role::Admin;

    match user::create(
        &state.db,
        &form.email,
        &form.display_name,
        &form.password,
        role,
    )
    .await
    {
        Ok(user) => {
            session::login(&session, user.id).await?;
            tracing::info!(user_id = %user.id, %role, "Konto angelegt");
            Ok(Redirect::to("/dashboard").into_response())
        }
        Err(err) => {
            let status = err.status();
            let body = render(RegisterPage {
                layout: Layout::new("Konto anlegen", None, "/register"),
                error: Some(err.public_message()),
                display_name: form.display_name,
                email: form.email,
                min_password_len: password::MIN_PASSWORD_LEN,
            })?;
            Ok((status, body).into_response())
        }
    }
}

/// Antwort, wenn die Selbstregistrierung geschlossen ist.
/// Weiterleitung statt 404, damit ein alter Lesezeichen-Link sinnvoll landet.
fn registration_closed() -> Response {
    Redirect::to("/login").into_response()
}

pub async fn logout(session: Session) -> Result<Response, WebError> {
    session::logout(&session).await?;
    Ok(Redirect::to("/").into_response())
}

/// Verhindert eine offene Weiterleitung: Nur Pfade auf dieser Seite sind
/// erlaubt, keine absoluten URLs und kein protokollrelatives `//fremde.tld`.
fn safe_next(next: Option<&str>) -> String {
    match next {
        Some(path) if path.starts_with('/') && !path.starts_with("//") => path.to_string(),
        _ => "/dashboard".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::safe_next;

    #[test]
    fn offene_weiterleitung_wird_verhindert() {
        assert_eq!(safe_next(Some("/notes")), "/notes");
        assert_eq!(safe_next(Some("/notes?a=1")), "/notes?a=1");
        assert_eq!(safe_next(Some("https://boese.tld")), "/dashboard");
        assert_eq!(safe_next(Some("//boese.tld")), "/dashboard");
        assert_eq!(safe_next(Some("")), "/dashboard");
        assert_eq!(safe_next(None), "/dashboard");
    }
}
