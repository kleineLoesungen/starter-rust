//! Beispielressource "Notiz".
//!
//! Sie existiert als vollstaendiges, kopierbares Muster: Domain-Service hier,
//! HTML-Routen in `src/web/notes.rs`, JSON-API in `src/api/v1/notes.rs`.
//! Fuer ein eigenes Projekt: umbenennen oder loeschen.
//!
//! Die Funktionen kommen in zwei Ausfuehrungen, und der Unterschied ist
//! sicherheitsrelevant:
//!
//! * `*_owned` / `*_for_user` — auf einen Benutzer eingeschraenkt.
//!   Fuer die HTML-Oberflaeche, wo ein Mensch nur seine eigenen Daten sehen darf.
//! * `list_all` / `get` / `update` / `delete` — systemweit, ohne Eigentumsfilter.
//!   Fuer die Maschinen-Schnittstelle, deren Zugriff ueber Scopes geregelt ist.
//!
//! Wer in einem Web-Handler versehentlich die systemweite Fassung benutzt,
//! oeffnet fremde Daten. Im Zweifel die `_owned`-Fassung nehmen.

use crate::error::Error;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Note {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub body: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// Eingabe fuer Anlegen und Aendern. Wird sowohl aus einem HTML-Formular
/// als auch aus einem JSON-Body befuellt.
#[derive(Debug, Deserialize)]
pub struct NoteInput {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

impl NoteInput {
    fn validate(&self) -> Result<(), Error> {
        if self.title.trim().is_empty() {
            return Err(Error::BadRequest("Der Titel darf nicht leer sein.".into()));
        }
        if self.title.chars().count() > 200 {
            return Err(Error::BadRequest(
                "Der Titel darf hoechstens 200 Zeichen haben.".into(),
            ));
        }
        Ok(())
    }
}

const COLUMNS: &str = "id, user_id, title, body, created_at, updated_at";

pub async fn create(db: &PgPool, user_id: Uuid, input: &NoteInput) -> Result<Note, Error> {
    input.validate()?;
    let note = sqlx::query_as::<_, Note>(&format!(
        "INSERT INTO notes (user_id, title, body) VALUES ($1, $2, $3) RETURNING {COLUMNS}"
    ))
    .bind(user_id)
    .bind(input.title.trim())
    .bind(&input.body)
    .fetch_one(db)
    .await
    .map_err(|e| match &e {
        // Kommt vor, wenn die Schnittstelle eine unbekannte user_id schickt.
        sqlx::Error::Database(db_err) if db_err.is_foreign_key_violation() => {
            Error::BadRequest("Diesen Benutzer gibt es nicht.".into())
        }
        _ => Error::Database(e),
    })?;
    Ok(note)
}

pub async fn list_for_user(db: &PgPool, user_id: Uuid) -> Result<Vec<Note>, Error> {
    let notes = sqlx::query_as::<_, Note>(&format!(
        "SELECT {COLUMNS} FROM notes WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(notes)
}

/// Laedt eine Notiz und stellt dabei sicher, dass sie dem Benutzer gehoert.
/// Fremde Notizen ergeben `NotFound` statt `Forbidden` — so verraet die API
/// nicht, welche IDs existieren.
pub async fn get_owned(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<Note, Error> {
    let note = sqlx::query_as::<_, Note>(&format!(
        "SELECT {COLUMNS} FROM notes WHERE id = $1 AND user_id = $2"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)?;
    Ok(note)
}

pub async fn update_owned(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    input: &NoteInput,
) -> Result<Note, Error> {
    input.validate()?;
    let note = sqlx::query_as::<_, Note>(&format!(
        "UPDATE notes SET title = $3, body = $4
         WHERE id = $1 AND user_id = $2 RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(input.title.trim())
    .bind(&input.body)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)?;
    Ok(note)
}

pub async fn delete_owned(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<(), Error> {
    let result = sqlx::query("DELETE FROM notes WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Systemweit — fuer die Maschinen-Schnittstelle.
// Kein Eigentumsfilter. Der Zugriff wird ueber Scopes geregelt, nicht hier.
// ---------------------------------------------------------------------------

/// Alle Notizen, wahlweise auf einen Besitzer eingeschraenkt.
pub async fn list_all(db: &PgPool, user_id: Option<Uuid>) -> Result<Vec<Note>, Error> {
    let notes = match user_id {
        Some(uid) => list_for_user(db, uid).await?,
        None => {
            sqlx::query_as::<_, Note>(&format!(
                "SELECT {COLUMNS} FROM notes ORDER BY created_at DESC"
            ))
            .fetch_all(db)
            .await?
        }
    };
    Ok(notes)
}

pub async fn get(db: &PgPool, id: Uuid) -> Result<Note, Error> {
    let note = sqlx::query_as::<_, Note>(&format!("SELECT {COLUMNS} FROM notes WHERE id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(Error::NotFound)?;
    Ok(note)
}

pub async fn update(db: &PgPool, id: Uuid, input: &NoteInput) -> Result<Note, Error> {
    input.validate()?;
    let note = sqlx::query_as::<_, Note>(&format!(
        "UPDATE notes SET title = $2, body = $3 WHERE id = $1 RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(input.title.trim())
    .bind(&input.body)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)?;
    Ok(note)
}

pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), Error> {
    let result = sqlx::query("DELETE FROM notes WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}
