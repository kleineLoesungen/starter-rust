//! Passwort-Hashing mit Argon2id.
//!
//! Hashen und Pruefen kosten absichtlich ~100 ms CPU. Deshalb laufen beide
//! Operationen ueber `spawn_blocking` und blockieren nicht den Async-Executor.

use crate::error::Error;
use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::phc::PasswordHash};

/// Minimale Passwortlaenge. Bewusst laengenbasiert statt Zeichenklassen-Regeln —
/// das entspricht der aktuellen NIST-Empfehlung.
pub const MIN_PASSWORD_LEN: usize = 12;

pub fn validate(password: &str) -> Result<(), Error> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(Error::BadRequest(format!(
            "Das Passwort muss mindestens {MIN_PASSWORD_LEN} Zeichen lang sein."
        )));
    }
    Ok(())
}

pub async fn hash(password: String) -> Result<String, Error> {
    tokio::task::spawn_blocking(move || {
        let hash: PasswordHash = Argon2::default()
            .hash_password(password.as_bytes())
            .map_err(|e| Error::Internal(anyhow::anyhow!("Hashing fehlgeschlagen: {e}")))?;
        Ok(hash.to_string())
    })
    .await
    .map_err(|e| Error::Internal(e.into()))?
}

/// Prueft ein Passwort gegen einen gespeicherten PHC-String.
/// Liefert `false` statt eines Fehlers, wenn das Passwort nicht passt.
pub async fn verify(password: String, stored_hash: String) -> Result<bool, Error> {
    tokio::task::spawn_blocking(move || {
        let parsed = match PasswordHash::new(&stored_hash) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Ungueltiger Passwort-Hash in der Datenbank: {e}");
                return Ok(false);
            }
        };
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|e| Error::Internal(e.into()))?
}
