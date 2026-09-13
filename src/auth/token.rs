//! App-Tokens fuer die JSON-API.
//!
//! Format des ausgegebenen Tokens:
//!
//! ```text
//! sk_<token_id>_<secret>
//!    └ 32 Zeichen  └ 43 Zeichen (256 Bit)
//! ```
//!
//! * `token_id` liegt im Klartext in der Datenbank und dient dem Nachschlagen.
//!   Dadurch braucht die Pruefung genau EINEN indizierten Lookup. Die ID ist
//!   hex-kodiert und enthaelt daher selbst kein `_` — nur deshalb ist das
//!   erste `_` nach dem Praefix eindeutig der Trenner. Das Geheimnis ist
//!   base64url-kodiert und darf `_` enthalten.
//! * `secret` wird nur als SHA-256 gespeichert. Der Klartext ist nach der
//!   Ausgabe unwiederbringlich weg.
//!
//! Warum SHA-256 und nicht Argon2 wie beim Passwort? Weil das Geheimnis 256 Bit
//! Zufall ist und nicht erraten werden kann. Argon2 wuerde jeden API-Aufruf um
//! ~100 ms verlangsamen, ohne Sicherheit zu gewinnen. Genau so macht es auch
//! GitHub mit seinen Personal Access Tokens.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub const PREFIX: &str = "sk_";
const ID_BYTES: usize = 16;
const SECRET_BYTES: usize = 32;

/// Ein frisch erzeugtes Token. Der Klartext existiert nur hier.
pub struct GeneratedToken {
    /// Vollstaendiges Token — wird dem Benutzer genau einmal gezeigt.
    pub plaintext: String,
    /// Oeffentlicher Teil, wandert im Klartext in die Datenbank.
    pub token_id: String,
    /// SHA-256 des Geheimnisses als Hex.
    pub token_hash: String,
}

pub fn generate() -> GeneratedToken {
    let mut id_bytes = [0u8; ID_BYTES];
    let mut secret_bytes = [0u8; SECRET_BYTES];
    rand::fill(&mut id_bytes);
    rand::fill(&mut secret_bytes);

    let token_id = hex(&id_bytes);
    let secret = URL_SAFE_NO_PAD.encode(secret_bytes);

    GeneratedToken {
        plaintext: format!("{PREFIX}{token_id}_{secret}"),
        token_hash: hash_secret(&secret),
        token_id,
    }
}

/// Zerlegt ein eingehendes Token in `(token_id, secret)`.
/// Liefert `None`, wenn das Format nicht stimmt.
pub fn parse(token: &str) -> Option<(&str, &str)> {
    let rest = token.strip_prefix(PREFIX)?;
    let (token_id, secret) = rest.split_once('_')?;
    if token_id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((token_id, secret))
}

pub fn hash_secret(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    hex(&digest)
}

/// Vergleich in konstanter Zeit, damit die Laufzeit nichts ueber den Hash verraet.
pub fn verify_secret(secret: &str, stored_hash: &str) -> bool {
    let computed = hash_secret(secret);
    computed.as_bytes().ct_eq(stored_hash.as_bytes()).into()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erzeugtes_token_laesst_sich_zerlegen_und_pruefen() {
        let t = generate();
        let (id, secret) = parse(&t.plaintext).expect("Format muss passen");
        assert_eq!(id, t.token_id);
        assert!(verify_secret(secret, &t.token_hash));
        assert!(!verify_secret("falsch", &t.token_hash));
    }

    #[test]
    fn geheimnis_mit_unterstrich_wird_korrekt_getrennt() {
        // Regressionstest: base64url erzeugt `_` im Geheimnis. Der Trenner ist
        // trotzdem eindeutig, weil die hex-kodierte ID keines enthaelt.
        let (id, secret) = parse("sk_0a1b2c_abc_def").expect("Format muss passen");
        assert_eq!(id, "0a1b2c");
        assert_eq!(secret, "abc_def");
    }

    #[test]
    fn kaputte_formate_werden_abgewiesen() {
        assert!(parse("").is_none());
        assert!(parse("sk_").is_none());
        assert!(parse("sk_nurid").is_none());
        assert!(parse("pk_abc_def").is_none());
    }

    #[test]
    fn zwei_tokens_sind_verschieden() {
        assert_ne!(generate().plaintext, generate().plaintext);
    }
}
