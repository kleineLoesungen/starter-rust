//! HTML-Routen fuer die Beispielressource.
//!
//! MUSTER ZUM KOPIEREN. Beachtenswert:
//!
//! * Die Seite liefert vollstaendiges HTML, die Aenderungen nur ein Fragment.
//! * Fachlogik steht in `domain::note`, hier nur Ein- und Ausgabe.
//! * Eigentumspruefung passiert in der Domain-Schicht, nicht hier.

use axum::extract::{Form, Path, State};
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::auth::extract::CurrentUser;
use crate::domain::note::{self, NoteInput};
use crate::error::WebError;
use crate::state::AppState;
use crate::templates::{Layout, NoteItemPartial, NotesPage, Toast, render};

/// GET /notes — vollstaendige Seite.
pub async fn index(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Response, WebError> {
    let notes = note::list_for_user(&state.db, user.id).await?;
    render(NotesPage {
        layout: Layout::for_user("Notizen", user, "/notes"),
        notes,
    })
}

/// POST /notes — htmx. Antwort ist genau der neue Eintrag; htmx haengt ihn
/// oben in die Liste (`hx-swap="afterbegin"`).
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Form(input): Form<NoteInput>,
) -> Result<Response, WebError> {
    let note = note::create(&state.db, user.id, &input).await?;

    render(NoteItemPartial {
        note,
        toast: Some(Toast::success("Notiz gespeichert.")),
    })
}

/// DELETE /notes/:id — htmx. Leere Antwort, weil das Element durch
/// `hx-swap="outerHTML"` einfach verschwindet.
pub async fn delete(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Response, WebError> {
    note::delete_owned(&state.db, user.id, id).await?;
    Ok(([("HX-Trigger", "note-deleted")], "").into_response())
}
