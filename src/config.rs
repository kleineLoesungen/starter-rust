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
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        // .env laden, falls vorhanden. Echte Umgebungsvariablen haben Vorrang.
        let _ = dotenvy::dotenv();

        Ok(Self {
            database_url: require("DATABASE_URL")?,
            bind_addr: optional("BIND_ADDR", "0.0.0.0:3000").parse()?,
            db_max_connections: optional("DB_MAX_CONNECTIONS", "10").parse()?,
            cookie_secure: optional("COOKIE_SECURE", "false").parse()?,
            public_url: optional("PUBLIC_URL", "http://localhost:3000"),
            trust_proxy: optional("TRUST_PROXY", "false").parse()?,
        })
    }
}

fn require(key: &str) -> anyhow::Result<String> {
    std::env::var(key)
        .map_err(|_| anyhow::anyhow!("Umgebungsvariable {key} fehlt. Siehe .env.example."))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
