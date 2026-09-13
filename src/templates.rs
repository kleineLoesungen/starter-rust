//! Template-Infrastruktur.
//!
//! Jede Seite ist ein Struct mit `#[derive(Template)]`. Askama prueft die
//! Templates ZUR COMPILE-ZEIT: Ein Tippfehler im Feldnamen oder ein fehlender
//! Block bricht `cargo build`, nicht erst die laufende Anwendung.
//!
//! Muster fuer eine neue Seite:
//!
//! ```ignore
//! #[derive(Template)]
//! #[template(path = "pages/meine_seite.html")]
//! pub struct MeineSeite {
//!     pub layout: Layout,
//!     pub eintraege: Vec<Eintrag>,
//! }
//! ```

use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::auth::Role;
use crate::domain::user::User;
use crate::error::{Error, WebError};

/// Name der Anwendung. Erscheint in Titelzeile und Kopfbereich.
pub const APP_NAME: &str = "Starter";

/// Daten, die jede Seite im Rahmen braucht: Titel, angemeldeter Benutzer,
/// aktueller Pfad fuer die Navigationsmarkierung.
pub struct Layout {
    pub title: String,
    pub app_name: &'static str,
    pub user: Option<User>,
    pub path: String,
}

impl Layout {
    pub fn new(title: impl Into<String>, user: Option<User>, path: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            app_name: APP_NAME,
            user,
            path: path.into(),
        }
    }

    /// Fuer angemeldete Seiten, wo der Benutzer garantiert vorhanden ist.
    pub fn for_user(title: impl Into<String>, user: User, path: impl Into<String>) -> Self {
        Self::new(title, Some(user), path)
    }

    pub fn app_initial(&self) -> String {
        self.app_name
            .chars()
            .next()
            .unwrap_or('S')
            .to_uppercase()
            .to_string()
    }

    /// Markiert den Navigationspunkt, unter dem der aktuelle Pfad liegt.
    pub fn is_active(&self, prefix: &str) -> bool {
        self.path == prefix || self.path.starts_with(&format!("{prefix}/"))
    }

    pub fn is_admin(&self) -> bool {
        self.has_role(Role::Admin)
    }

    pub fn is_moderator(&self) -> bool {
        self.has_role(Role::Moderator)
    }

    fn has_role(&self, required: Role) -> bool {
        self.user.as_ref().is_some_and(|u| u.can(required))
    }
}

/// Meldung, die eine Antwort nebenbei mitschicken kann.
/// Wird als Out-of-Band-Fragment in `#toasts` eingesetzt.
pub struct Toast {
    /// "success", "danger" oder "" fuer neutral — steuert die Randfarbe.
    pub kind: &'static str,
    pub message: String,
}

impl Toast {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            kind: "success",
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            kind: "danger",
            message: message.into(),
        }
    }
}

/// Datumsformatierung fuer Templates.
///
/// ACHTUNG: `datum()` und `datum_kurz()` formatieren in der Zeitzone des Wertes.
/// Aus der Datenbank kommen alle Zeitstempel in UTC — die Ausgabe ist damit
/// UTC, nicht Ortszeit. Im Sommer sind das zwei Stunden Unterschied zu Berlin.
/// Deshalb rendern die Templates sie nicht direkt, sondern ueber das Makro
/// `zeit()`, das die Umrechnung dem Browser ueberlaesst. Diese Methoden sind
/// nur noch der Rueckfall, wenn kein JavaScript laeuft.
///
/// Bewusst als Trait mit Methoden statt als Askama-Filter: Filter haben in
/// Askama 0.16 eine eigenwillige Builder-Schnittstelle, Methodenaufrufe sind
/// dagegen gewoehnliches Rust und im Template sofort verstaendlich.
///
/// Im Template: `{{ note.created_at.datum() }}`
pub trait DatumAnzeige {
    /// Datum und Uhrzeit: 09.09.2026, 14:30
    fn datum(&self) -> String;
    /// Nur das Datum: 09.09.2026
    fn datum_kurz(&self) -> String;
    /// Maschinenlesbar nach RFC 3339, immer in UTC.
    ///
    /// Gehoert in das `datetime`-Attribut eines `<time>`-Elements. Der Browser
    /// rechnet daraus die Ortszeit des Betrachters aus — siehe `zeit()` in
    /// `templates/components/macros.html`.
    fn iso(&self) -> String;
}

impl DatumAnzeige for time::OffsetDateTime {
    fn datum(&self) -> String {
        let fmt = time::macros::format_description!("[day].[month].[year], [hour]:[minute]");
        self.format(&fmt).unwrap_or_else(|_| "—".into())
    }

    fn datum_kurz(&self) -> String {
        let fmt = time::macros::format_description!("[day].[month].[year]");
        self.format(&fmt).unwrap_or_else(|_| "—".into())
    }

    fn iso(&self) -> String {
        self.format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    }
}

/// Rendert ein Template zu einer HTML-Antwort.
///
/// In Handlern: `render(MeineSeite { layout, ... })`
pub fn render<T: Template>(template: T) -> Result<Response, WebError> {
    let body = template
        .render()
        .map_err(|e| WebError(Error::Template(e)))?;
    Ok(Html(body).into_response())
}

/// Fehlerseite. Bewusst ohne Askama gebaut: Wenn ein Template-Fehler die
/// Ursache ist, darf die Fehlerseite nicht am selben Problem scheitern.
pub fn render_error_page(status: StatusCode, message: &str) -> String {
    let code = status.as_u16();
    format!(
        r##"<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{code} · {APP_NAME}</title>
<link rel="stylesheet" href="/static/app.css">
</head>
<body class="min-h-dvh bg-surface text-content">
<main class="mx-auto flex min-h-dvh max-w-md flex-col items-center justify-center gap-4 px-6 text-center">
  <p class="font-mono text-6xl font-bold text-subtle">{code}</p>
  <h1 class="text-lg font-semibold">{message}</h1>
  <a href="/" class="btn btn-secondary">Zur Startseite</a>
</main>
</body>
</html>"##
    )
}

// ============================================================================
// SEITEN
// ----------------------------------------------------------------------------
// Ein Struct je Template. Die Felder sind genau das, was das Template sieht —
// mehr Kontext gibt es nicht, und das ist Absicht.
// ============================================================================

use crate::auth::scope::Scope;
use crate::domain::api_token::ApiToken;
use crate::domain::note::Note;
use uuid::Uuid;

#[derive(Template)]
#[template(path = "pages/landing.html")]
pub struct LandingPage {
    pub layout: Layout,
    /// Nur waehrend der Ersteinrichtung wahr — danach legt der Administrator
    /// alle Konten an.
    pub ersteinrichtung: bool,
}

#[derive(Template)]
#[template(path = "pages/login.html")]
pub struct LoginPage {
    pub layout: Layout,
    pub error: Option<String>,
    /// Zeigt den Hinweis auf die Ersteinrichtung nur, solange es kein Konto gibt.
    pub ersteinrichtung: bool,
    /// Eingegebene Adresse bleibt nach einem Fehlversuch stehen.
    pub email: String,
    /// Ziel nach erfolgreicher Anmeldung.
    pub next: String,
}

#[derive(Template)]
#[template(path = "pages/register.html")]
pub struct RegisterPage {
    pub layout: Layout,
    pub error: Option<String>,
    pub display_name: String,
    pub email: String,
    pub min_password_len: usize,
}

#[derive(Template)]
#[template(path = "pages/dashboard.html")]
pub struct DashboardPage {
    pub layout: Layout,
    pub user: User,
    pub note_count: usize,
    /// Nur fuer Administratoren gefuellt — Tokens sind systemweite
    /// Maschinen-Zugaenge, keine persoenliche Angelegenheit.
    pub token_count: Option<usize>,
}

#[derive(Template)]
#[template(path = "pages/notes.html")]
pub struct NotesPage {
    pub layout: Layout,
    pub notes: Vec<Note>,
}

#[derive(Template)]
#[template(path = "partials/note_item.html")]
pub struct NoteItemPartial {
    pub note: Note,
    pub toast: Option<Toast>,
}

#[derive(Template)]
#[template(path = "pages/tokens.html")]
pub struct TokensPage {
    pub layout: Layout,
    pub tokens: Vec<ApiToken>,
    pub scopes: &'static [Scope],
    pub public_url: String,
}

#[derive(Template)]
#[template(path = "partials/token_created.html")]
pub struct TokenCreatedPartial {
    pub name: String,
    /// Der Klartext. Existiert nur in dieser einen Antwort.
    pub plaintext: String,
    pub token: ApiToken,
}

#[derive(Template)]
#[template(path = "pages/admin_users.html")]
pub struct AdminUsersPage {
    pub layout: Layout,
    pub users: Vec<User>,
    /// Aktueller Suchbegriff — bleibt im Feld stehen.
    pub suche: String,
    /// Gesamtzahl aller Konten, damit erkennbar ist, dass gefiltert wird.
    pub gesamt: i64,
    /// Fehler aus dem Anlegen-Formular.
    pub error: Option<String>,
    /// Eingaben bleiben nach einem Fehlversuch stehen.
    pub neu_display_name: String,
    pub neu_email: String,
    pub roles: Vec<Role>,
    pub min_password_len: usize,
    pub current_user_id: Uuid,
}

/// Detailseite eines Benutzers. Hier passieren alle Aenderungen.
#[derive(Template)]
#[template(path = "pages/admin_user.html")]
pub struct AdminUserPage {
    pub layout: Layout,
    /// Der bearbeitete Benutzer — nicht der angemeldete Administrator.
    pub bearbeitet: User,
    pub roles: Vec<Role>,
    pub min_password_len: usize,
    /// `true`, wenn der Administrator sich gerade selbst ansieht.
    pub ist_selbst: bool,
    /// `true`, wenn dieses Konto der letzte aktive Administrator ist.
    pub ist_letzter_admin: bool,
    pub error: Option<String>,
    pub gespeichert: bool,
}

/// Eigene Kontoseite. Fuer jeden angemeldeten Benutzer, unabhaengig von der Rolle.
#[derive(Template)]
#[template(path = "pages/account.html")]
pub struct AccountPage {
    pub layout: Layout,
    pub user: User,
    pub min_password_len: usize,
    pub profil_error: Option<String>,
    pub passwort_error: Option<String>,
    /// Welcher Abschnitt gerade erfolgreich gespeichert wurde.
    pub gespeichert: Option<&'static str>,
}

#[derive(Template)]
#[template(path = "pages/moderation.html")]
pub struct ModerationPage {
    pub layout: Layout,
}

/// Ein Farbfeld im Styleguide.
pub struct Swatch {
    pub class: &'static str,
    pub role: &'static str,
}

#[derive(Template)]
#[template(path = "pages/styleguide.html")]
pub struct StyleguidePage {
    pub layout: Layout,
    pub swatches: Vec<Swatch>,
}

impl StyleguidePage {
    /// Die Farbrollen in der Reihenfolge, in der man sie beim Gestalten braucht.
    pub fn swatches() -> Vec<Swatch> {
        [
            ("bg-surface", "Seitenhintergrund"),
            ("bg-surface-raised", "Karten, Kopfzeile"),
            ("bg-surface-sunken", "Vertiefungen, Tabellenkopf"),
            ("bg-brand", "Markenfarbe, primärer Knopf"),
            ("bg-brand-subtle", "zurückhaltende Markenfläche"),
            ("bg-success", "Erfolg"),
            ("bg-warning", "Warnung"),
            ("bg-danger", "Fehler, Löschen"),
        ]
        .into_iter()
        .map(|(class, role)| Swatch { class, role })
        .collect()
    }
}
