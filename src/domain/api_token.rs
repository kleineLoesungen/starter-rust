//! App-Tokens: Zugaenge fuer Maschinen, nicht fuer Menschen.
//!
//! Ein Token gehoert keinem Benutzer. Es steht fuer ein fremdes System, das
//! die dokumentierte JSON-Schnittstelle benutzt — ein Skript, ein Dienst, eine
//! andere Anwendung. `created_by` haelt nur fest, welcher Administrator es
//! ausgestellt hat.
//!
//! Was ein Token darf, steht ausschliesslich in seinen `scopes`.
//! Siehe `crate::auth::scope`.

use crate::auth::{scope, token};
use crate::error::Error;
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ApiToken {
    pub id: Uuid,
    pub name: String,
    /// Oeffentlicher Teil. Taugt zum Wiedererkennen, nicht zum Authentifizieren.
    pub token_id: String,
    #[serde(skip)]
    pub token_hash: String,
    pub scopes: Vec<String>,
    /// Wer das Token ausgestellt hat. Nur zur Nachvollziehbarkeit — leer,
    /// wenn dieses Konto inzwischen geloescht wurde.
    pub created_by: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_used_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl ApiToken {
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at
            .is_some_and(|e| e <= OffsetDateTime::now_utc())
    }

    pub fn is_valid(&self) -> bool {
        !self.is_revoked() && !self.is_expired()
    }

    /// Die einzige Autorisierungsfrage, die ein Token beantwortet.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }

    /// Zum Anzeigen in Listen: `sk_a1b2c3d4…`
    pub fn display_hint(&self) -> String {
        format!(
            "{}{}…",
            token::PREFIX,
            &self.token_id[..8.min(self.token_id.len())]
        )
    }

    pub fn status_label(&self) -> &'static str {
        if self.is_revoked() {
            "widerrufen"
        } else if self.is_expired() {
            "abgelaufen"
        } else if self.scopes.is_empty() {
            // Gueltig, aber ohne jede Berechtigung — fast immer ein Versehen.
            "ohne Rechte"
        } else {
            "aktiv"
        }
    }

    /// Beschriftete Scopes fuer die Oberflaeche.
    pub fn scope_labels(&self) -> Vec<&str> {
        self.scopes.iter().map(|s| scope::label_for(s)).collect()
    }
}

const COLUMNS: &str = "id, name, token_id, token_hash, scopes, created_by, last_used_at, expires_at, revoked_at, created_at";

/// Ergebnis von [`create`]. Der Klartext ist die EINZIGE Gelegenheit,
/// das Token zu sehen — danach existiert nur noch der Hash.
pub struct CreatedToken {
    pub token: ApiToken,
    pub plaintext: String,
}

pub async fn create(
    db: &PgPool,
    created_by: Uuid,
    name: &str,
    scopes: &[String],
    expires_at: Option<OffsetDateTime>,
) -> Result<CreatedToken, Error> {
    if name.trim().is_empty() {
        return Err(Error::BadRequest(
            "Bitte einen Namen für das Token angeben.".into(),
        ));
    }
    if scopes.is_empty() {
        return Err(Error::BadRequest(
            "Bitte mindestens eine Berechtigung auswählen — ohne Scope kann das Token nichts."
                .into(),
        ));
    }
    // Ein Tippfehler im Scope wuerde sonst ein Token erzeugen, das still nichts darf.
    if let Some(unbekannt) = scopes.iter().find(|s| !scope::is_known(s)) {
        return Err(Error::BadRequest(format!(
            "Unbekannte Berechtigung: {unbekannt}"
        )));
    }

    let generated = token::generate();

    let stored = sqlx::query_as::<_, ApiToken>(&format!(
        "INSERT INTO api_tokens (name, token_id, token_hash, scopes, created_by, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
    ))
    .bind(name.trim())
    .bind(&generated.token_id)
    .bind(&generated.token_hash)
    .bind(scopes)
    .bind(created_by)
    .bind(expires_at)
    .fetch_one(db)
    .await?;

    Ok(CreatedToken {
        token: stored,
        plaintext: generated.plaintext,
    })
}

/// Prueft ein eingehendes Bearer-Token.
///
/// Bewusst NICHT nach aussen unterscheidbar: unbekannt, widerrufen, abgelaufen
/// und falsches Geheimnis fuehren alle zu `Unauthorized`.
pub async fn authenticate(db: &PgPool, presented: &str) -> Result<ApiToken, Error> {
    let Some((token_id, secret)) = token::parse(presented) else {
        return Err(Error::Unauthorized);
    };

    let found = sqlx::query_as::<_, ApiToken>(&format!(
        "SELECT {COLUMNS} FROM api_tokens WHERE token_id = $1"
    ))
    .bind(token_id)
    .fetch_optional(db)
    .await?;

    let Some(record) = found else {
        return Err(Error::Unauthorized);
    };

    if !token::verify_secret(secret, &record.token_hash) || !record.is_valid() {
        return Err(Error::Unauthorized);
    }

    Ok(record)
}

/// Vermerkt die Benutzung. Fehler werden bewusst nur geloggt — eine
/// fehlgeschlagene Statistik darf keinen API-Aufruf scheitern lassen.
pub async fn touch(db: &PgPool, id: Uuid) {
    if let Err(e) = sqlx::query("UPDATE api_tokens SET last_used_at = now() WHERE id = $1")
        .bind(id)
        .execute(db)
        .await
    {
        tracing::warn!("last_used_at konnte nicht aktualisiert werden: {e}");
    }
}

/// Alle Tokens der Installation. Tokens sind systemweit, nicht persoenlich —
/// jeder Administrator sieht deshalb dieselbe Liste.
pub async fn list(db: &PgPool) -> Result<Vec<ApiToken>, Error> {
    let tokens = sqlx::query_as::<_, ApiToken>(&format!(
        "SELECT {COLUMNS} FROM api_tokens ORDER BY revoked_at NULLS FIRST, created_at DESC"
    ))
    .fetch_all(db)
    .await?;
    Ok(tokens)
}

/// Widerruft ein Token. Der Datensatz bleibt erhalten, damit die Historie
/// nachvollziehbar bleibt.
pub async fn revoke(db: &PgPool, id: Uuid) -> Result<(), Error> {
    let result = sqlx::query(
        "UPDATE api_tokens SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(Error::NotFound);
    }
    Ok(())
}
