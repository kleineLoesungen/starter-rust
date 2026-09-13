//! Extractors — hier wird Autorisierung zur Typfrage.
//!
//! Statt in jedem Handler `if !user.can(...)` zu schreiben, sagt die Signatur,
//! wer die Route aufrufen darf:
//!
//! ```ignore
//! async fn dashboard(CurrentUser(user): CurrentUser) { ... }        // angemeldet
//! async fn queue(ModeratorUser(user): ModeratorUser) { ... }        // ab Moderator
//! async fn users(AdminUser(user): AdminUser) { ... }                // nur Admin
//! async fn public(MaybeUser(user): MaybeUser) { ... }               // beides
//! async fn api_list(client: ApiClient) { ... }                     // gültiges App-Token
//! async fn api_read(_: NotesRead) { ... }                          // Token mit Scope notes:read
//! ```
//!
//! Wer die Rollenpruefung vergisst, bekommt keinen unsicheren Endpunkt,
//! sondern einen falschen Typ.

use axum::extract::{FromRequestParts, OptionalFromRequestParts};
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use tower_sessions::Session;

use crate::auth::{Role, scope, session};
use crate::domain::{api_token, user};
use crate::error::{Error, WebError};
use crate::state::AppState;

/// Ein angemeldeter Benutzer. Fehlt die Anmeldung, wird auf `/login`
/// umgeleitet — mit `?next=`, damit es nach dem Login dort weitergeht.
pub struct CurrentUser(pub user::User);

/// Angemeldet oder nicht. Fuer Seiten, die beides koennen (z. B. die Startseite).
pub struct MaybeUser(pub Option<user::User>);

/// Mindestens Moderator.
pub struct ModeratorUser(pub user::User);

/// Nur Administratoren.
pub struct AdminUser(pub user::User);

/// Ein Maschinen-Client, der sich mit einem gueltigen App-Token ausgewiesen hat.
///
/// Wichtig: Dahinter steht KEIN Benutzer. Ein Token gehoert der Installation,
/// nicht einer Person, und erbt deshalb auch keine Rolle. Was der Client darf,
/// steht in `token.scopes` — geprueft ueber die Scope-Extractors weiter unten.
pub struct ApiClient {
    pub token: api_token::ApiToken,
}

// --- Gemeinsame Logik -------------------------------------------------------

async fn user_from_session(
    parts: &mut Parts,
    state: &AppState,
) -> Result<Option<user::User>, WebError> {
    let session = Session::from_request_parts(parts, state)
        .await
        .map_err(|_| WebError(Error::Internal(anyhow::anyhow!("Session-Layer fehlt"))))?;

    let Some(user_id) = session::current_user_id(&session).await? else {
        return Ok(None);
    };

    let Some(user) = user::find_by_id(&state.db, user_id).await? else {
        // Konto wurde geloescht, Session zeigt ins Leere.
        session::logout(&session).await?;
        return Ok(None);
    };

    if !user.is_active {
        session::logout(&session).await?;
        return Ok(None);
    }

    Ok(Some(user))
}

/// Umleitung zum Login inklusive Rueckkehrziel.
fn redirect_to_login(parts: &Parts) -> Response {
    let next = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let encoded = urlencode(next);
    Redirect::to(&format!("/login?next={encoded}")).into_response()
}

fn urlencode(input: &str) -> String {
    input
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

// --- Implementierungen ------------------------------------------------------

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match user_from_session(parts, state).await {
            Ok(Some(user)) => Ok(CurrentUser(user)),
            Ok(None) => Err(redirect_to_login(parts)),
            Err(e) => Err(e.into_response()),
        }
    }
}

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match user_from_session(parts, state).await {
            Ok(user) => Ok(MaybeUser(user)),
            Err(e) => Err(e.into_response()),
        }
    }
}

/// Erzeugt einen Extractor, der eine Mindestrolle erzwingt.
macro_rules! role_extractor {
    ($name:ident, $required:expr) => {
        impl FromRequestParts<AppState> for $name {
            type Rejection = Response;

            async fn from_request_parts(
                parts: &mut Parts,
                state: &AppState,
            ) -> Result<Self, Self::Rejection> {
                let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
                if user.can($required) {
                    Ok($name(user))
                } else {
                    Err(WebError(Error::Forbidden).into_response())
                }
            }
        }
    };
}

role_extractor!(ModeratorUser, Role::Moderator);
role_extractor!(AdminUser, Role::Admin);

impl FromRequestParts<AppState> for ApiClient {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let presented = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .filter(|v| !v.is_empty());

        let Some(presented) = presented else {
            return Err(unauthorized_api());
        };

        let token = match api_token::authenticate(&state.db, presented).await {
            Ok(t) => t,
            Err(_) => return Err(unauthorized_api()),
        };

        api_token::touch(&state.db, token.id).await;

        Ok(ApiClient { token })
    }
}

/// Erzeugt einen Extractor, der einen bestimmten Scope verlangt.
///
/// Damit bleibt die Autorisierung auch in der Schnittstelle eine Typfrage:
/// Die Signatur des Handlers sagt, welche Berechtigung noetig ist.
macro_rules! scope_extractor {
    ($name:ident, $scope:expr, $doku:expr) => {
        #[doc = $doku]
        pub struct $name(pub api_token::ApiToken);

        impl FromRequestParts<AppState> for $name {
            type Rejection = Response;

            async fn from_request_parts(
                parts: &mut Parts,
                state: &AppState,
            ) -> Result<Self, Self::Rejection> {
                // Voll qualifiziert, weil ApiClient auch OptionalFromRequestParts hat.
                let ApiClient { token } =
                    <ApiClient as FromRequestParts<AppState>>::from_request_parts(parts, state)
                        .await?;

                if token.has_scope($scope) {
                    Ok($name(token))
                } else {
                    Err(missing_scope($scope))
                }
            }
        }
    };
}

scope_extractor!(
    NotesRead,
    scope::NOTES_READ,
    "Token mit Scope `notes:read`."
);
scope_extractor!(
    NotesWrite,
    scope::NOTES_WRITE,
    "Token mit Scope `notes:write`."
);
scope_extractor!(
    UsersRead,
    scope::USERS_READ,
    "Token mit Scope `users:read`."
);

/// 401 mit `WWW-Authenticate`, wie es RFC 6750 fuer Bearer-Tokens vorsieht.
fn unauthorized_api() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer realm=\"api\"")],
        axum::Json(serde_json::json!({
            "error": {
                "code": "unauthorized",
                "message": "Gültiges App-Token erforderlich. Header: Authorization: Bearer sk_..."
            }
        })),
    )
        .into_response()
}

/// 403 mit Angabe des fehlenden Scopes — der Client soll wissen, was ihm fehlt.
fn missing_scope(scope: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        [(
            header::WWW_AUTHENTICATE,
            format!("Bearer realm=\"api\", scope=\"{scope}\""),
        )],
        axum::Json(serde_json::json!({
            "error": {
                "code": "insufficient_scope",
                "message": format!("Dieses Token hat die Berechtigung \"{scope}\" nicht."),
                "required_scope": scope,
            }
        })),
    )
        .into_response()
}

/// Erlaubt `Option<ApiClient>` in Handlern, die mit und ohne Token funktionieren.
impl OptionalFromRequestParts<AppState> for ApiClient {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, Self::Rejection> {
        if !parts.headers.contains_key(header::AUTHORIZATION) {
            return Ok(None);
        }
        <ApiClient as FromRequestParts<AppState>>::from_request_parts(parts, state)
            .await
            .map(Some)
    }
}
