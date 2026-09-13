//! Version 1 der JSON-Schnittstelle.

pub mod health;
pub mod notes;
pub mod token;
pub mod users;

use axum::Router;
use axum::routing::get;

use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .route("/token", get(token::info))
        .route("/users", get(users::list))
        .route("/notes", get(notes::list).post(notes::create))
        .route(
            "/notes/{id}",
            get(notes::show).put(notes::update).delete(notes::delete),
        )
}

/// Einheitliche Huelle fuer erfolgreiche Antworten: `{"data": ...}`.
///
/// Warum eine Huelle? Damit spaeter Felder wie `meta` oder `pagination`
/// danebenpassen, ohne dass sich die Form der Nutzdaten aendert.
#[derive(serde::Serialize)]
pub struct Data<T> {
    pub data: T,
}

impl<T> Data<T> {
    pub fn new(data: T) -> axum::Json<Self> {
        axum::Json(Self { data })
    }
}
