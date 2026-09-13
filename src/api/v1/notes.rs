//! JSON-Endpunkte fuer die Beispielressource.
//!
//! MUSTER ZUM KOPIEREN. Zwei Dinge unterscheiden diese Handler von denen in
//! `src/web/notes.rs`:
//!
//! 1. Der Extractor verlangt einen **Scope**, keine Rolle. Hinter einem Token
//!    steht kein Mensch, also gibt es auch keine Rolle zu erben.
//! 2. Es gibt keinen Eigentumsfilter. Ein Maschinen-Client arbeitet systemweit;
//!    der Besitzer einer Notiz wird ausdruecklich mitgegeben.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::api::v1::Data;
use crate::auth::extract::{NotesRead, NotesWrite};
use crate::domain::note::{self, Note, NoteInput};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Optional auf einen Besitzer einschraenken: `?user_id=…`
    #[serde(default)]
    pub user_id: Option<Uuid>,
}

/// Eingabe beim Anlegen ueber die Schnittstelle — mit ausdruecklichem Besitzer.
#[derive(Debug, Deserialize)]
pub struct CreateInput {
    pub user_id: Uuid,
    pub title: String,
    #[serde(default)]
    pub body: String,
}

pub async fn list(
    State(state): State<AppState>,
    _: NotesRead,
    Query(query): Query<ListQuery>,
) -> Result<Json<Data<Vec<Note>>>, ApiError> {
    let notes = note::list_all(&state.db, query.user_id).await?;
    Ok(Data::new(notes))
}

pub async fn show(
    State(state): State<AppState>,
    _: NotesRead,
    Path(id): Path<Uuid>,
) -> Result<Json<Data<Note>>, ApiError> {
    let note = note::get(&state.db, id).await?;
    Ok(Data::new(note))
}

pub async fn create(
    State(state): State<AppState>,
    _: NotesWrite,
    Json(input): Json<CreateInput>,
) -> Result<(StatusCode, Json<Data<Note>>), ApiError> {
    let note = note::create(
        &state.db,
        input.user_id,
        &NoteInput {
            title: input.title,
            body: input.body,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Data::new(note)))
}

pub async fn update(
    State(state): State<AppState>,
    _: NotesWrite,
    Path(id): Path<Uuid>,
    Json(input): Json<NoteInput>,
) -> Result<Json<Data<Note>>, ApiError> {
    let note = note::update(&state.db, id, &input).await?;
    Ok(Data::new(note))
}

pub async fn delete(
    State(state): State<AppState>,
    _: NotesWrite,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    note::delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
