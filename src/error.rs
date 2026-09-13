//! Fehlerbehandlung.
//!
//! Es gibt einen fachlichen Fehlertyp [`Error`] und zwei duenne Huellen, die
//! bestimmen, WIE er ausgeliefert wird:
//!
//! * [`WebError`] rendert eine HTML-Fehlerseite  -> fuer Routen in `src/web/`
//! * [`ApiError`] rendert ein JSON-Objekt        -> fuer Routen in `src/api/`
//!
//! In einem Handler schreibt man `Result<T, WebError>` bzw. `Result<T, ApiError>`
//! und benutzt `?` wie gewohnt.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Nicht gefunden")]
    NotFound,

    #[error("Nicht angemeldet")]
    Unauthorized,

    #[error("Keine Berechtigung")]
    Forbidden,

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Conflict(String),

    /// Zu viele Versuche. Der Wert ist die verbleibende Wartezeit in Sekunden.
    #[error("Zu viele Versuche. Bitte in {0} Sekunden erneut versuchen.")]
    TooManyRequests(u64),

    #[error("Datenbankfehler")]
    Database(#[from] sqlx::Error),

    #[error("Template-Fehler")]
    Template(#[from] askama::Error),

    #[error("Interner Fehler")]
    Internal(#[from] anyhow::Error),
}

impl Error {
    pub fn status(&self) -> StatusCode {
        match self {
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::Unauthorized => StatusCode::UNAUTHORIZED,
            Error::Forbidden => StatusCode::FORBIDDEN,
            Error::BadRequest(_) => StatusCode::BAD_REQUEST,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Error::Database(sqlx::Error::RowNotFound) => StatusCode::NOT_FOUND,
            Error::Database(_) | Error::Template(_) | Error::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    /// Text, der nach aussen gehen darf. Interne Details bleiben im Log.
    pub fn public_message(&self) -> String {
        match self {
            Error::Database(_) | Error::Template(_) | Error::Internal(_) => {
                "Es ist ein interner Fehler aufgetreten.".to_string()
            }
            other => other.to_string(),
        }
    }

    /// Stabiler Fehlercode fuer API-Clients.
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotFound => "not_found",
            Error::Unauthorized => "unauthorized",
            Error::Forbidden => "forbidden",
            Error::BadRequest(_) => "bad_request",
            Error::Conflict(_) => "conflict",
            Error::TooManyRequests(_) => "too_many_requests",
            Error::Database(sqlx::Error::RowNotFound) => "not_found",
            _ => "internal_error",
        }
    }

    fn log(&self) {
        if self.status().is_server_error() {
            tracing::error!(error = ?self, "Serverfehler");
        } else {
            tracing::debug!(error = %self, "Client-Fehler");
        }
    }
}

/// Erzeugt eine Fehler-Huelle samt `From`-Implementierungen fuer die
/// gaengigen Ursprungsfehler, damit `?` in Handlern funktioniert.
macro_rules! error_wrapper {
    ($name:ident) => {
        #[derive(Debug)]
        pub struct $name(pub Error);

        impl From<Error> for $name {
            fn from(e: Error) -> Self {
                Self(e)
            }
        }
        impl From<sqlx::Error> for $name {
            fn from(e: sqlx::Error) -> Self {
                Self(Error::Database(e))
            }
        }
        impl From<askama::Error> for $name {
            fn from(e: askama::Error) -> Self {
                Self(Error::Template(e))
            }
        }
        impl From<anyhow::Error> for $name {
            fn from(e: anyhow::Error) -> Self {
                Self(Error::Internal(e))
            }
        }
    };
}

error_wrapper!(WebError);
error_wrapper!(ApiError);

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        self.0.log();
        let status = self.0.status();
        let body = crate::templates::render_error_page(status, &self.0.public_message());
        (status, axum::response::Html(body)).into_response()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        self.0.log();
        let status = self.0.status();
        // Bei 401 hilft der Hinweis auf den erwarteten Header mehr als
        // ein blosses "Nicht angemeldet".
        let message = match self.0 {
            Error::Unauthorized => {
                "Gueltiges App-Token erforderlich. Header: Authorization: Bearer sk_...".to_string()
            }
            ref other => other.public_message(),
        };

        let body = json!({
            "error": {
                "code": self.0.code(),
                "message": message,
            }
        });
        (status, Json(body)).into_response()
    }
}
