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
const FREIVERSUCHE: i32 = 3;
/// Wartezeit fuer den ersten Versuch nach den Freiversuchen.
const BASIS_SEKUNDEN: i64 = 5;
/// Faktor, um den die Wartezeit je weiterem Fehlversuch waechst.
const FAKTOR: i64 = 3;
/// Obergrenze, damit ein Konto nicht dauerhaft unbenutzbar wird.
const MAX_SEKUNDEN: i64 = 15 * 60;

/// Wartezeit nach `fehlversuche` Fehlversuchen, in Sekunden.
fn wartezeit(fehlversuche: i32) -> i64 {
    if fehlversuche < FREIVERSUCHE {
        return 0;
    }
    let stufe = (fehlversuche - FREIVERSUCHE).min(10) as u32;
    BASIS_SEKUNDEN
        .saturating_mul(FAKTOR.saturating_pow(stufe))
        .min(MAX_SEKUNDEN)
}

/// Schluessel, unter denen ein Anmeldeversuch gezaehlt wird.
pub fn schluessel(email: &str, ip: Option<&str>) -> Vec<String> {
    let mut keys = vec![format!("email:{}", email.trim().to_lowercase())];
    if let Some(ip) = ip {
        keys.push(format!("ip:{ip}"));
    }
    keys
}

/// Prueft vor dem Anmeldeversuch, ob gerade eine Sperre laeuft.
///
/// Gibt `TooManyRequests` mit der verbleibenden Wartezeit zurueck.
pub async fn pruefen(db: &PgPool, schluessel: &[String]) -> Result<(), Error> {
    // `max()` liefert IMMER eine Zeile — bei keinem Treffer eben NULL. Der
    // Rueckgabetyp muss deshalb `Option` sein, sonst scheitert das Dekodieren.
    let (bis,): (Option<OffsetDateTime>,) = sqlx::query_as(
        "SELECT max(gesperrt_bis) FROM login_versuche
         WHERE schluessel = ANY($1) AND gesperrt_bis > now()",
    )
    .bind(schluessel)
    .fetch_one(db)
    .await?;

    if let Some(bis) = bis {
        let rest = (bis - OffsetDateTime::now_utc()).whole_seconds().max(1);
        return Err(Error::TooManyRequests(rest as u64));
    }
    Ok(())
}

/// Vermerkt einen Fehlversuch und setzt die naechste Sperre.
pub async fn fehlschlag(db: &PgPool, schluessel: &[String]) -> Result<(), Error> {
    for key in schluessel {
        // Zaehler hochsetzen und den neuen Stand zurueckbekommen.
        let (fehlversuche,): (i32,) = sqlx::query_as(
            "INSERT INTO login_versuche (schluessel, fehlversuche, letzter_versuch)
             VALUES ($1, 1, now())
             ON CONFLICT (schluessel) DO UPDATE
               SET fehlversuche = login_versuche.fehlversuche + 1,
                   letzter_versuch = now()
             RETURNING fehlversuche",
        )
        .bind(key)
        .fetch_one(db)
        .await?;

        let sekunden = wartezeit(fehlversuche);
        if sekunden > 0 {
            sqlx::query(
                "UPDATE login_versuche SET gesperrt_bis = now() + $2 WHERE schluessel = $1",
            )
            .bind(key)
            .bind(Duration::seconds(sekunden))
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

/// Nach erfolgreicher Anmeldung ist die Vorgeschichte erledigt.
pub async fn erfolg(db: &PgPool, schluessel: &[String]) -> Result<(), Error> {
    sqlx::query("DELETE FROM login_versuche WHERE schluessel = ANY($1)")
        .bind(schluessel)
        .execute(db)
        .await?;
    Ok(())
}

/// Raeumt Eintraege weg, die lange niemand mehr angefasst hat.
/// Wird beim Start aufgerufen; die Tabelle bleibt dadurch klein.
pub async fn aufraeumen(db: &PgPool) -> Result<u64, Error> {
    let ergebnis = sqlx::query(
        "DELETE FROM login_versuche
         WHERE letzter_versuch < now() - interval '7 days'
           AND (gesperrt_bis IS NULL OR gesperrt_bis < now())",
    )
    .execute(db)
    .await?;
    Ok(ergebnis.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zwei_fehlversuche_sperren_noch_nicht() {
        assert_eq!(wartezeit(1), 0);
        assert_eq!(wartezeit(2), 0);
    }

    #[test]
    fn nach_dem_dritten_fehlversuch_wird_gesperrt() {
        // Drei Versuche frei, der vierte ist damit der erste gesperrte.
        assert_eq!(wartezeit(3), 5);
    }

    #[test]
    fn wartezeit_waechst_und_ist_gedeckelt() {
        assert_eq!(wartezeit(4), 15);
        assert_eq!(wartezeit(5), 45);
        assert_eq!(wartezeit(6), 135);
        assert_eq!(wartezeit(7), 405);
        assert_eq!(wartezeit(8), MAX_SEKUNDEN);
        // Auch bei absurd vielen Versuchen kein Ueberlauf.
        assert_eq!(wartezeit(1000), MAX_SEKUNDEN);
    }

    #[test]
    fn schluessel_werden_normalisiert() {
        let keys = schluessel("  Max@Example.COM ", Some("10.0.0.1"));
        assert_eq!(keys, vec!["email:max@example.com", "ip:10.0.0.1"]);

        // Ohne bekannte IP bleibt nur der Kontoschluessel.
        assert_eq!(schluessel("a@b.de", None), vec!["email:a@b.de"]);
    }
}
