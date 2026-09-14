//! Bremse gegen das Durchprobieren von Passwoertern.
//!
//! Drei Versuche sind frei — Vertipper sollen niemanden bestrafen. Nach dem
//! dritten Fehlversuch wird gesperrt, und die Sperre waechst mit jedem
//! weiteren Fehlversuch:
//!
//! | Fehlversuche bisher | naechster Versuch moeglich in |
//! |---|---|
//! | 1–2 | sofort |
//! | 3 | 5 Sekunden |
//! | 4 | 15 Sekunden |
//! | 5 | 45 Sekunden |
//! | 6 | 2 Minuten 15 |
//! | 7 | 6 Minuten 45 |
//! | 8 und mehr | 15 Minuten (Obergrenze) |
//!
//! **Warum eine Sperre und kein echtes Warten?** Die naheliegende Umsetzung
//! waere, die Antwort einfach zu verzoegern. Das haelt aber pro Versuch eine
//! Verbindung und einen Task offen — ein Angreifer koennte den Server damit
//! gezielt zustellen. Die Sperre antwortet stattdessen sofort und sagt, wie
//! lange es noch dauert. Fuer den Angreifer ist die Bremse dieselbe, fuer den
//! Server kostet sie nichts.
//!
//! **Bekannte Grenze:** Wer die E-Mail-Adresse eines anderen kennt, kann dessen
//! Konto durch absichtliche Fehlversuche voruebergehend sperren. Das ist der
//! uebliche Tausch — ohne Sperre pro Konto nuetzt ein Angreifer mit vielen
//! Adressen die IP-Bremse einfach aus. Die Sperre laeuft von selbst ab und
//! trifft nie das Konto selbst, nur den Anmeldeversuch.

use crate::error::Error;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

/// So viele Versuche stehen frei zur Verfuegung. Der Versuch DANACH wird
/// gebremst — bei 3 ist also der vierte der erste gesperrte.
const FREE_ATTEMPTS: i32 = 3;
/// Wartezeit fuer den ersten Versuch nach den Freiversuchen.
const BASE_SECONDS: i64 = 5;
/// Faktor, um den die Wartezeit je weiterem Fehlversuch waechst.
const GROWTH_FACTOR: i64 = 3;
/// Obergrenze, damit ein Konto nicht dauerhaft unbenutzbar wird.
const MAX_SECONDS: i64 = 15 * 60;

/// Wartezeit nach `failures` Fehlversuchen, in Sekunden.
fn delay_seconds(failures: i32) -> i64 {
    if failures < FREE_ATTEMPTS {
        return 0;
    }
    let step = (failures - FREE_ATTEMPTS).min(10) as u32;
    BASE_SECONDS
        .saturating_mul(GROWTH_FACTOR.saturating_pow(step))
        .min(MAX_SECONDS)
}

/// Schluessel, unter denen ein Anmeldeversuch gezaehlt wird.
pub fn keys(email: &str, ip: Option<&str>) -> Vec<String> {
    let mut keys = vec![format!("email:{}", email.trim().to_lowercase())];
    if let Some(ip) = ip {
        keys.push(format!("ip:{ip}"));
    }
    keys
}

/// Prueft vor dem Anmeldeversuch, ob gerade eine Sperre laeuft.
///
/// Gibt `TooManyRequests` mit der verbleibenden Wartezeit zurueck.
pub async fn check(db: &PgPool, keys: &[String]) -> Result<(), Error> {
    // `max()` liefert IMMER eine Zeile — bei keinem Treffer eben NULL. Der
    // Rueckgabetyp muss deshalb `Option` sein, sonst scheitert das Dekodieren.
    let (locked_until,): (Option<OffsetDateTime>,) = sqlx::query_as(
        "SELECT max(locked_until) FROM login_attempts
         WHERE attempt_key = ANY($1) AND locked_until > now()",
    )
    .bind(keys)
    .fetch_one(db)
    .await?;

    if let Some(until) = locked_until {
        let remaining = (until - OffsetDateTime::now_utc()).whole_seconds().max(1);
        return Err(Error::TooManyRequests(remaining as u64));
    }
    Ok(())
}

/// Vermerkt einen Fehlversuch und setzt die naechste Sperre.
pub async fn record_failure(db: &PgPool, keys: &[String]) -> Result<(), Error> {
    for key in keys {
        // Zaehler hochsetzen und den neuen Stand zurueckbekommen.
        let (failures,): (i32,) = sqlx::query_as(
            "INSERT INTO login_attempts (attempt_key, failures, last_attempt_at)
             VALUES ($1, 1, now())
             ON CONFLICT (attempt_key) DO UPDATE
               SET failures = login_attempts.failures + 1,
                   last_attempt_at = now()
             RETURNING failures",
        )
        .bind(key)
        .fetch_one(db)
        .await?;

        let seconds = delay_seconds(failures);
        if seconds > 0 {
            sqlx::query(
                "UPDATE login_attempts SET locked_until = now() + $2 WHERE attempt_key = $1",
            )
            .bind(key)
            .bind(Duration::seconds(seconds))
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

/// Nach erfolgreicher Anmeldung ist die Vorgeschichte erledigt.
pub async fn record_success(db: &PgPool, keys: &[String]) -> Result<(), Error> {
    sqlx::query("DELETE FROM login_attempts WHERE attempt_key = ANY($1)")
        .bind(keys)
        .execute(db)
        .await?;
    Ok(())
}

/// Raeumt Eintraege weg, die lange niemand mehr angefasst hat.
/// Wird beim Start aufgerufen; die Tabelle bleibt dadurch klein.
pub async fn cleanup(db: &PgPool) -> Result<u64, Error> {
    let result = sqlx::query(
        "DELETE FROM login_attempts
         WHERE last_attempt_at < now() - interval '7 days'
           AND (locked_until IS NULL OR locked_until < now())",
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zwei_fehlversuche_sperren_noch_nicht() {
        assert_eq!(delay_seconds(1), 0);
        assert_eq!(delay_seconds(2), 0);
    }

    #[test]
    fn nach_dem_dritten_fehlversuch_wird_gesperrt() {
        // Drei Versuche frei, der vierte ist damit der erste gesperrte.
        assert_eq!(delay_seconds(3), 5);
    }

    #[test]
    fn wartezeit_waechst_und_ist_gedeckelt() {
        assert_eq!(delay_seconds(4), 15);
        assert_eq!(delay_seconds(5), 45);
        assert_eq!(delay_seconds(6), 135);
        assert_eq!(delay_seconds(7), 405);
        assert_eq!(delay_seconds(8), MAX_SECONDS);
        // Auch bei absurd vielen Versuchen kein Ueberlauf.
        assert_eq!(delay_seconds(1000), MAX_SECONDS);
    }

    #[test]
    fn schluessel_werden_normalisiert() {
        let result = keys("  Max@Example.COM ", Some("10.0.0.1"));
        assert_eq!(result, vec!["email:max@example.com", "ip:10.0.0.1"]);

        // Ohne bekannte IP bleibt nur der Kontoschluessel.
        assert_eq!(keys("a@b.de", None), vec!["email:a@b.de"]);
    }
}
