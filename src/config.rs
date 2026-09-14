//! Konfiguration — ausschliesslich aus Umgebungsvariablen.
//!
//! Regel: Es gibt genau EINE Stelle, an der Umgebungsvariablen gelesen werden.
//! Wer eine neue Einstellung braucht, ergaenzt sie hier und in `.env.example`.

use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    /// Verbindung zur externen Postgres-Datenbank.
    pub database_url: String,
    /// Adresse, auf der der HTTP-Server lauscht.
    pub bind_addr: SocketAddr,
    /// Maximale Anzahl gleichzeitiger DB-Verbindungen.
    pub db_max_connections: u32,
    /// `true` in Produktion: Session-Cookie nur ueber HTTPS ausliefern.
    pub cookie_secure: bool,
    /// Oeffentliche Basis-URL, z. B. fuer Links in E-Mails.
    pub public_url: String,
    /// `true`, wenn ein Reverse-Proxy davorsteht, der `X-Forwarded-For` setzt.
    ///
    /// Nur dann darf diesem Kopf geglaubt werden — sonst kann sich jeder Client
    /// beliebige Adressen ausdenken und die Anmeldebremse pro IP aushebeln.
    pub trust_proxy: bool,
    /// Name, Farben und alles, was eine installierte App (PWA) ausmacht.
    pub branding: Branding,
    /// E-Mail-Versand.
    pub mail: MailConfig,
}

/// Erscheinungsbild nach aussen: Seitentitel, installierte App, Browserleiste.
#[derive(Debug, Clone)]
pub struct Branding {
    /// Voller Name. Seitentitel, Kopfzeile, Name der installierten App.
    pub name: String,
    /// Kurzname fuer den Startbildschirm — dort ist fuer mehr als etwa
    /// 12 Zeichen kein Platz, ein langer Name wird sonst abgeschnitten.
    pub short_name: String,
    /// Farbe der Browser- bzw. Statusleiste. Hex, z. B. `#3370c7`.
    pub theme_color: String,
    /// Hintergrund des Startbildschirms, bevor die App geladen ist.
    pub background_color: String,
}

impl Default for Branding {
    fn default() -> Self {
        Self {
            name: "Starter".into(),
            short_name: "Starter".into(),
            // Entspricht --ui-brand bei der Vorgabe --brand-hue: 258.
            theme_color: "#3370c7".into(),
            background_color: "#f9fafc".into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MailConfig {
    /// Verbindung zum Mailserver, z. B. `smtps://benutzer:passwort@mail.example.com`.
    /// `None` bedeutet: Mails werden nicht verschickt, sondern ins Log geschrieben.
    pub smtp_url: Option<String>,
    /// Absender, z. B. `Starter <noreply@example.com>`.
    pub from: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        load_env_file()?;

        let defaults = Branding::default();
        let name = optional("APP_NAME", &defaults.name);

        let branding = Branding {
            short_name: optional("APP_SHORT_NAME", &name),
            theme_color: color("PWA_THEME_COLOR", &defaults.theme_color)?,
            background_color: color("PWA_BACKGROUND_COLOR", &defaults.background_color)?,
            name,
        };

        let mail = MailConfig {
            // Leerer Wert zaehlt wie nicht gesetzt — so laesst sich SMTP in der
            // .env abschalten, ohne die Zeile zu loeschen.
            smtp_url: std::env::var("SMTP_URL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            from: optional(
                "MAIL_FROM",
                &format!("{} <noreply@localhost>", branding.name),
            ),
        };

        Ok(Self {
            database_url: require("DATABASE_URL")?,
            bind_addr: optional("BIND_ADDR", "0.0.0.0:3000").parse()?,
            db_max_connections: optional("DB_MAX_CONNECTIONS", "10").parse()?,
            cookie_secure: optional("COOKIE_SECURE", "false").parse()?,
            public_url: optional("PUBLIC_URL", "http://localhost:3000"),
            trust_proxy: optional("TRUST_PROXY", "false").parse()?,
            branding,
            mail,
        })
    }

    /// Vernuenftige Werte fuer Tests — ohne Umgebungsvariablen und ohne SMTP.
    pub fn for_tests() -> Self {
        Self {
            database_url: String::new(),
            bind_addr: "127.0.0.1:0".parse().expect("gueltige Adresse"),
            db_max_connections: 5,
            cookie_secure: false,
            public_url: "http://localhost:3000".into(),
            trust_proxy: false,
            branding: Branding::default(),
            mail: MailConfig {
                smtp_url: None,
                from: "Starter <noreply@localhost>".into(),
            },
        }
    }
}

/// Laedt `.env`, falls vorhanden. Echte Umgebungsvariablen haben Vorrang.
///
/// Eine FEHLENDE Datei ist in Ordnung (Container bekommen ihre Werte direkt).
/// Eine FEHLERHAFTE Datei dagegen bricht den Start ab. Grund: dotenvy hoert an
/// der ersten kaputten Zeile auf zu lesen — und alle Zeilen danach fehlen
/// ebenfalls, ohne jede Meldung. Ein ungequotetes `MAIL_FROM=Name <a@b>` wuerde
/// so still etwa ein spaeteres `COOKIE_SECURE=true` verschlucken.
fn load_env_file() -> anyhow::Result<()> {
    match dotenvy::dotenv() {
        Ok(_) => Ok(()),
        Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => anyhow::bail!(
            ".env ist fehlerhaft: {e}\n\
             Werte mit Leerzeichen oder < > in Anfuehrungszeichen setzen, \
             z. B. MAIL_FROM=\"Name <noreply@example.com>\""
        ),
    }
}

fn require(key: &str) -> anyhow::Result<String> {
    std::env::var(key)
        .map_err(|_| anyhow::anyhow!("Umgebungsvariable {key} fehlt. Siehe .env.example."))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Liest eine Farbe als `#rgb` oder `#rrggbb`.
///
/// Streng, weil der Wert unveraendert im Manifest und in einem Meta-Tag landet —
/// ein Tippfehler faellt dann schon beim Start auf und nicht erst, wenn sich
/// jemand wundert, warum die installierte App grau bleibt.
fn color(key: &str, default: &str) -> anyhow::Result<String> {
    let value = optional(key, default);
    if is_hex_color(&value) {
        Ok(value.to_lowercase())
    } else {
        anyhow::bail!("{key}={value:?} ist keine Farbe. Erwartet wird #rgb oder #rrggbb.")
    }
}

fn is_hex_color(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('#') else {
        return false;
    };
    matches!(digits.len(), 3 | 6) && digits.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::is_hex_color;

    #[test]
    fn hexfarben_werden_erkannt() {
        assert!(is_hex_color("#3370c7"));
        assert!(is_hex_color("#FFF"));
        assert!(!is_hex_color("3370c7"));
        assert!(!is_hex_color("#3370c"));
        assert!(!is_hex_color("#zzzzzz"));
        assert!(!is_hex_color("red"));
        assert!(!is_hex_color("#3370c7\" onload=\""));
    }
}
