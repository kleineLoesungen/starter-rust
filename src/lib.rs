//! Anwendungsbibliothek. `src/main.rs` ist nur der Startpunkt; die eigentliche
//! Anwendung lebt hier, damit Integrationstests in `tests/` sie benutzen koennen.

pub mod api;
pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod state;
pub mod templates;
pub mod web;

pub use error::{ApiError, Error, WebError};
pub use state::AppState;
