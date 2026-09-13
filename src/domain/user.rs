//! Benutzerverwaltung.

use crate::auth::{Role, password};
use crate::error::Error;
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    /// Nie nach aussen geben — deshalb aus der Serialisierung ausgeschlossen.
    #[serde(skip)]
    pub password_hash: String,
    pub role: Role,
    pub is_active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    pub fn can(&self, required: Role) -> bool {
        self.role.at_least(required)
    }

    /// Initialen fuer den Avatar in der Kopfzeile.
    pub fn initials(&self) -> String {
        self.display_name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase()
    }
}

const COLUMNS: &str =
    "id, email, display_name, password_hash, role, is_active, created_at, updated_at";

/// Legt einen Benutzer an. E-Mail wird normalisiert, Passwort mit Argon2id gehasht.
pub async fn create(
    db: &PgPool,
    email: &str,
    display_name: &str,
    plain_password: &str,
    role: Role,
) -> Result<User, Error> {
    let email = normalize_email(email);
    if !email.contains('@') {
        return Err(Error::BadRequest(
            "Bitte eine gueltige E-Mail-Adresse angeben.".into(),
        ));
    }
    if display_name.trim().is_empty() {
        return Err(Error::BadRequest(
            "Bitte einen Anzeigenamen angeben.".into(),
        ));
    }
    password::validate(plain_password)?;

    let hash = password::hash(plain_password.to_string()).await?;

    let user = sqlx::query_as::<_, User>(&format!(
        "INSERT INTO users (email, display_name, password_hash, role)
         VALUES ($1, $2, $3, $4) RETURNING {COLUMNS}"
    ))
    .bind(&email)
    .bind(display_name.trim())
    .bind(&hash)
    .bind(role)
    .fetch_one(db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            Error::Conflict("Diese E-Mail-Adresse ist bereits vergeben.".into())
        }
        _ => Error::Database(e),
    })?;

    Ok(user)
}

/// Prueft E-Mail und Passwort. Liefert `Unauthorized` bei jedem Fehlschlag —
/// bewusst ohne Unterscheidung, damit nicht erkennbar wird, ob es das Konto gibt.
pub async fn authenticate(db: &PgPool, email: &str, plain_password: &str) -> Result<User, Error> {
    let email = normalize_email(email);
    let user = find_by_email(db, &email).await?;

    let Some(user) = user else {
        // Trotzdem hashen, damit die Antwortzeit nicht verraet, dass das Konto fehlt.
        let _ = password::hash(plain_password.to_string()).await;
        return Err(Error::Unauthorized);
    };

    if !user.is_active {
        return Err(Error::Forbidden);
    }
    if !password::verify(plain_password.to_string(), user.password_hash.clone()).await? {
        return Err(Error::Unauthorized);
    }
    Ok(user)
}

pub async fn find_by_id(db: &PgPool, id: Uuid) -> Result<Option<User>, Error> {
    let user = sqlx::query_as::<_, User>(&format!("SELECT {COLUMNS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(user)
}

pub async fn find_by_email(db: &PgPool, email: &str) -> Result<Option<User>, Error> {
    let user = sqlx::query_as::<_, User>(&format!("SELECT {COLUMNS} FROM users WHERE email = $1"))
        .bind(normalize_email(email))
        .fetch_optional(db)
        .await?;
    Ok(user)
}

/// Alle Benutzer, wahlweise auf einen Suchbegriff eingeschraenkt.
///
/// Gesucht wird in Anzeigename und E-Mail, Gross-/Kleinschreibung egal.
/// Bewusst ohne Paginierung: Bei der Groessenordnung, fuer die dieses Kit
/// gedacht ist, kostet ein vollstaendiger Durchlauf nichts. Wird die Liste
/// wirklich lang, gehoert hier `LIMIT`/`OFFSET` dazu — und in die Oberflaeche
/// eine Blaetterleiste.
pub async fn list(db: &PgPool, suche: Option<&str>) -> Result<Vec<User>, Error> {
    let muster = suche
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}%", maskieren(s)));

    let users = sqlx::query_as::<_, User>(&format!(
        r#"SELECT {COLUMNS} FROM users
           WHERE $1::text IS NULL
              OR display_name ILIKE $1
              OR email ILIKE $1
           ORDER BY created_at DESC"#
    ))
    .bind(muster)
    .fetch_all(db)
    .await?;
    Ok(users)
}

/// Entschaerft die Platzhalter von LIKE, damit ein eingegebenes `%` auch
/// wirklich nach einem Prozentzeichen sucht und nicht nach „irgendwas".
///
/// Eine ESCAPE-Klausel braucht die Abfrage dafuer nicht: In PostgreSQL ist der
/// Rueckstrich schon das voreingestellte Maskierzeichen von LIKE.
fn maskieren(eingabe: &str) -> String {
    eingabe
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub async fn count(db: &PgPool) -> Result<i64, Error> {
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(db)
        .await?;
    Ok(n)
}

pub async fn set_role(db: &PgPool, id: Uuid, role: Role) -> Result<User, Error> {
    if role != Role::Admin {
        ensure_not_last_admin(db, id).await?;
    }

    let user = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET role = $2 WHERE id = $1 RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(role)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)?;
    Ok(user)
}

pub async fn set_active(db: &PgPool, id: Uuid, active: bool) -> Result<User, Error> {
    if !active {
        ensure_not_last_admin(db, id).await?;
    }

    let user = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET is_active = $2 WHERE id = $1 RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(active)
    .fetch_optional(db)
    .await?
    .ok_or(Error::NotFound)?;
    Ok(user)
}

/// Aendert Anzeigename und E-Mail.
pub async fn update_profile(
    db: &PgPool,
    id: Uuid,
    display_name: &str,
    email: &str,
) -> Result<User, Error> {
    let email = normalize_email(email);
    if !email.contains('@') {
        return Err(Error::BadRequest(
            "Bitte eine gültige E-Mail-Adresse angeben.".into(),
        ));
    }
    if display_name.trim().is_empty() {
        return Err(Error::BadRequest(
            "Bitte einen Anzeigenamen angeben.".into(),
        ));
    }

    let user = sqlx::query_as::<_, User>(&format!(
        "UPDATE users SET display_name = $2, email = $3 WHERE id = $1 RETURNING {COLUMNS}"
    ))
    .bind(id)
    .bind(display_name.trim())
    .bind(&email)
    .fetch_optional(db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            Error::Conflict("Diese E-Mail-Adresse ist bereits vergeben.".into())
        }
        _ => Error::Database(e),
    })?
    .ok_or(Error::NotFound)?;

    Ok(user)
}

/// Loescht ein Konto samt aller zugehoerigen Daten (ON DELETE CASCADE).
///
/// Ausgestellte App-Tokens ueberleben: Sie gehoeren der Installation, nicht
/// der Person — `api_tokens.created_by` wird nur auf NULL gesetzt.
pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), Error> {
    ensure_not_last_admin(db, id).await?;

    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}

/// Anzahl der Konten, die sich anmelden UND verwalten koennen.
pub async fn count_active_admins(db: &PgPool) -> Result<i64, Error> {
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM users WHERE role = 'admin' AND is_active = TRUE")
            .fetch_one(db)
            .await?;
    Ok(n)
}

/// Verhindert, dass die Installation ohne handlungsfaehigen Administrator
/// zurueckbleibt. Wird vor Loeschen, Sperren und Degradieren geprueft.
async fn ensure_not_last_admin(db: &PgPool, id: Uuid) -> Result<(), Error> {
    let Some(betroffen) = find_by_id(db, id).await? else {
        return Ok(()); // Gibt es nicht — der Aufrufer meldet das selbst.
    };

    let ist_aktiver_admin = betroffen.role == Role::Admin && betroffen.is_active;
    if ist_aktiver_admin && count_active_admins(db).await? <= 1 {
        return Err(Error::BadRequest(
            "Das ist der letzte aktive Administrator. Erst einen weiteren ernennen.".into(),
        ));
    }
    Ok(())
}

pub async fn change_password(db: &PgPool, id: Uuid, new_password: &str) -> Result<(), Error> {
    password::validate(new_password)?;
    let hash = password::hash(new_password.to_string()).await?;
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(hash)
        .execute(db)
        .await?;
    Ok(())
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}
