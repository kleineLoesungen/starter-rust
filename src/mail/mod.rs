//! E-Mail-Versand.
//!
//! Benutzung in einem Handler — mehr ist nicht noetig:
//!
//! ```ignore
//! state.mailer.send(
//!     Mail::to(&user.email)
//!         .subject("Willkommen")
//!         .text("Hallo und willkommen!"),
//! ).await?;
//! ```
//!
//! Mit HTML-Fassung aus einem Askama-Template (siehe `templates.rs`):
//!
//! ```ignore
//! let mail = Mail::to(&user.email)
//!     .subject("Willkommen")
//!     .text("Hallo und willkommen!")
//!     .template(&Willkommen { name: user.display_name.clone() })?;
//! state.mailer.send_in_background(mail);
//! ```
//!
//! Wohin die Mail geht, entscheidet allein `SMTP_URL`:
//!
//! | `SMTP_URL` | Wirkung |
//! |---|---|
//! | gesetzt | Versand ueber diesen Server |
//! | leer / fehlt | nichts wird verschickt, Empfaenger und Betreff landen im Log |
//!
//! Lokal laeuft Mailpit (`just mail-up`): Es nimmt alles an und zeigt die
//! Mails unter http://localhost:58025 — es geht nichts an echte Adressen.

pub mod templates;

use std::sync::{Arc, Mutex};

use lettre::message::{Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::MailConfig;
use crate::error::Error;

/// Eine zu verschickende Mail. Aufgebaut wie ein Satz: an wen, worueber, was.
#[derive(Debug, Clone)]
pub struct Mail {
    to: String,
    subject: String,
    text: String,
    html: Option<String>,
}

impl Mail {
    pub fn to(address: impl Into<String>) -> Self {
        Self {
            to: address.into(),
            subject: String::new(),
            text: String::new(),
            html: None,
        }
    }

    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = subject.into();
        self
    }

    /// Reiner Text. Immer setzen — auch neben einer HTML-Fassung. Manche
    /// Programme zeigen nur Text an, und Spamfilter werten reine HTML-Mails ab.
    pub fn text(mut self, body: impl Into<String>) -> Self {
        self.text = body.into();
        self
    }

    pub fn html(mut self, body: impl Into<String>) -> Self {
        self.html = Some(body.into());
        self
    }

    /// Rendert ein Askama-Template als HTML-Fassung.
    pub fn template<T: askama::Template>(self, template: &T) -> Result<Self, Error> {
        let html = template.render()?;
        Ok(self.html(html))
    }

    pub fn recipient(&self) -> &str {
        &self.to
    }

    pub fn subject_line(&self) -> &str {
        &self.subject
    }

    pub fn text_body(&self) -> &str {
        &self.text
    }

    pub fn html_body(&self) -> Option<&str> {
        self.html.as_deref()
    }
}

/// Wohin Mails gehen.
#[derive(Clone)]
enum Transport {
    Smtp(AsyncSmtpTransport<Tokio1Executor>),
    /// Kein SMTP konfiguriert — nur protokollieren.
    Log,
    /// Fuer Tests: Mails werden gesammelt statt verschickt.
    InMemory(Arc<Mutex<Vec<Mail>>>),
}

#[derive(Clone)]
pub struct Mailer {
    transport: Transport,
    from: Mailbox,
    info: TransportInfo,
}

/// Was die Systemseite ueber den TransportInfo anzeigt. Enthaelt bewusst KEIN
/// Passwort — nur, ob eines gesetzt ist.
#[derive(Debug, Clone)]
pub struct TransportInfo {
    pub kind: &'static str,
    pub server: Option<String>,
    pub encryption: Option<&'static str>,
    pub username: Option<String>,
    pub has_password: bool,
}

impl Mailer {
    pub fn from_config(config: &MailConfig) -> anyhow::Result<Self> {
        let from: Mailbox = config.from.parse().map_err(|e| {
            anyhow::anyhow!(
                "MAIL_FROM={:?} ist keine gueltige Adresse: {e}",
                config.from
            )
        })?;

        let Some(url) = &config.smtp_url else {
            tracing::warn!(
                "SMTP_URL ist nicht gesetzt — E-Mails werden NICHT verschickt, sondern nur protokolliert."
            );
            return Ok(Self {
                transport: Transport::Log,
                from,
                info: TransportInfo {
                    kind: "Nur Protokoll (SMTP_URL nicht gesetzt)",
                    server: None,
                    encryption: None,
                    username: None,
                    has_password: false,
                },
            });
        };

        let smtp = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
            .map_err(|e| anyhow::anyhow!("SMTP_URL ist ungueltig: {e}"))?
            .build();

        Ok(Self {
            transport: Transport::Smtp(smtp),
            from,
            info: describe(url),
        })
    }

    /// Ein Mailer, der nichts verschickt, sondern sammelt. Nur fuer Tests.
    pub fn in_memory() -> (Self, Arc<Mutex<Vec<Mail>>>) {
        let outbox = Arc::new(Mutex::new(Vec::new()));
        let mailer = Self {
            transport: Transport::InMemory(outbox.clone()),
            from: "Test <test@localhost>".parse().expect("gueltige Adresse"),
            info: TransportInfo {
                kind: "Testspeicher",
                server: None,
                encryption: None,
                username: None,
                has_password: false,
            },
        };
        (mailer, outbox)
    }

    pub fn transport_info(&self) -> &TransportInfo {
        &self.info
    }

    pub fn from_address(&self) -> String {
        self.from.to_string()
    }

    /// Verschickt und wartet auf das Ergebnis. Richtig, wenn der Benutzer
    /// wissen muss, ob es geklappt hat — etwa bei einer Testmail.
    pub async fn send(&self, mail: Mail) -> Result<(), Error> {
        if mail.subject.trim().is_empty() {
            return Err(Error::BadRequest("Eine Mail braucht einen Betreff.".into()));
        }
        // Fuer ALLE Versandwege pruefen, nicht nur fuer SMTP — sonst verhalten
        // sich Tests und lokale Entwicklung anders als die Produktion.
        parse_recipient(&mail.to)?;

        match &self.transport {
            Transport::Log => {
                tracing::info!(an = %mail.to, betreff = %mail.subject, "Mail (nur protokolliert)");
                // Der Inhalt nur auf debug: Er kann spaeter Links zum Zuruecksetzen
                // eines Passworts enthalten. Die sollen nicht im Produktionslog
                // landen, nur weil SMTP vergessen wurde.
                tracing::debug!(text = %mail.text, "Mailinhalt");
                Ok(())
            }
            Transport::InMemory(outbox) => {
                outbox.lock().expect("Mailspeicher vergiftet").push(mail);
                Ok(())
            }
            Transport::Smtp(smtp) => {
                let message = self.build_message(&mail)?;
                smtp.send(message).await.map_err(|e| {
                    Error::Internal(anyhow::anyhow!(
                        "Mailversand an {} fehlgeschlagen: {e}",
                        mail.to
                    ))
                })?;
                tracing::info!(an = %mail.to, betreff = %mail.subject, "Mail verschickt");
                Ok(())
            }
        }
    }

    /// Verschickt im Hintergrund und kehrt sofort zurueck. Fehler landen im
    /// Log. Richtig fuer Benachrichtigungen, auf die niemand warten soll —
    /// ein langsamer Mailserver haelt dann keine Seite auf.
    pub fn send_in_background(&self, mail: Mail) {
        let mailer = self.clone();
        tokio::spawn(async move {
            let to = mail.to.clone();
            if let Err(e) = mailer.send(mail).await {
                tracing::error!(%to, "Mail im Hintergrund fehlgeschlagen: {e:?}");
            }
        });
    }

    fn build_message(&self, mail: &Mail) -> Result<Message, Error> {
        let an = parse_recipient(&mail.to)?;

        let builder = Message::builder()
            .from(self.from.clone())
            .to(an)
            .subject(&mail.subject);

        let result = match &mail.html {
            Some(html) => builder.multipart(MultiPart::alternative_plain_html(
                mail.text.clone(),
                html.clone(),
            )),
            None => builder.singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(mail.text.clone()),
            ),
        };

        result.map_err(|e| Error::Internal(anyhow::anyhow!("Mail nicht baubar: {e}")))
    }
}

fn parse_recipient(address: &str) -> Result<Mailbox, Error> {
    address
        .parse::<Mailbox>()
        .map_err(|_| Error::BadRequest(format!("{address:?} ist keine gültige E-Mail-Adresse.")))
}

/// Beschreibt eine SMTP-URL fuer die Anzeige — ohne das Passwort.
fn describe(smtp_url: &str) -> TransportInfo {
    let Ok(url) = url::Url::parse(smtp_url) else {
        return TransportInfo {
            kind: "SMTP",
            server: None,
            encryption: None,
            username: None,
            has_password: false,
        };
    };

    let tls_parameter = url
        .query_pairs()
        .find(|(k, _)| k == "tls")
        .map(|(_, v)| v.into_owned());

    let encryption = match (url.scheme(), tls_parameter.as_deref()) {
        ("smtps", _) => "TLS (direkt)",
        (_, Some("required")) => "STARTTLS (erzwungen)",
        (_, Some("opportunistic")) => "STARTTLS (wenn verfuegbar)",
        _ => "keine",
    };

    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();

    TransportInfo {
        kind: "SMTP",
        server: url.host_str().map(|h| format!("{h}{port}")),
        encryption: Some(encryption),
        username: Some(url.username().to_string()).filter(|u| !u.is_empty()),
        has_password: url.password().is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beschreibung_verraet_kein_passwort() {
        let weg = describe("smtps://max:geheim123@mail.example.com:465");
        assert_eq!(weg.server.as_deref(), Some("mail.example.com:465"));
        assert_eq!(weg.username.as_deref(), Some("max"));
        assert!(weg.has_password);
        assert_eq!(weg.encryption, Some("TLS (direkt)"));
        assert!(!format!("{weg:?}").contains("geheim123"));
    }

    #[test]
    fn starttls_wird_erkannt() {
        let weg = describe("smtp://mail.example.com:587?tls=required");
        assert_eq!(weg.encryption, Some("STARTTLS (erzwungen)"));
        assert!(!weg.has_password);
    }

    #[test]
    fn ohne_smtp_url_wird_protokolliert() {
        let config = MailConfig {
            smtp_url: None,
            from: "Test <a@b.de>".into(),
        };
        let mailer = Mailer::from_config(&config).unwrap();
        assert!(matches!(mailer.transport, Transport::Log));
    }

    #[test]
    fn ungueltiger_absender_faellt_beim_start_auf() {
        let config = MailConfig {
            smtp_url: None,
            from: "keine adresse".into(),
        };
        assert!(Mailer::from_config(&config).is_err());
    }
}
