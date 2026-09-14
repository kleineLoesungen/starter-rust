# Rezepte

Fertige Schrittfolgen für die Dinge, die man immer wieder braucht.
Jedes Rezept ist vollständig — abtippen und anpassen.

---

## Inhalt

1. [Eine neue Seite](#1-eine-neue-seite)
2. [Eine neue Ressource mit CRUD](#2-eine-neue-ressource-mit-crud)
3. [Ein Feld zu einer bestehenden Tabelle](#3-ein-feld-zu-einer-bestehenden-tabelle)
4. [Eine Seite auf eine Rolle beschränken](#4-eine-seite-auf-eine-rolle-beschränken)
5. [Etwas per htmx nachladen](#5-etwas-per-htmx-nachladen)
6. [Eine Meldung anzeigen](#6-eine-meldung-anzeigen)
7. [Ein Formular mit Fehlern zurückgeben](#7-ein-formular-mit-fehlern-zurückgeben)
8. [Einen Endpunkt zur JSON-API](#8-einen-endpunkt-zur-json-api)
9. [Einen neuen Scope](#9-einen-neuen-scope)
10. [Eine vierte Rolle](#10-eine-vierte-rolle)
11. [Eine neue Umgebungsvariable](#11-eine-neue-umgebungsvariable)
12. [Eine E-Mail verschicken](#12-eine-e-mail-verschicken)

---

## 1. Eine neue Seite

**Beispiel: `/reports`**, eine Seite „Berichte"

**a) Template** — `templates/pages/reports.html`

```jinja
{% extends "layout/app.html" %}
{% block title %}Berichte{% endblock %}

{% block main %}
<h1 class="text-2xl font-semibold tracking-tight">Berichte</h1>
<p class="mt-1 text-sm text-muted">{{ count }} Einträge</p>
{% endblock %}
```

**b) Struct** — in `src/templates.rs`

```rust
#[derive(Template)]
#[template(path = "pages/reports.html")]
pub struct ReportsPage {
    pub layout: Layout,
    pub count: usize,
}
```

**c) Handler** — in `src/web/pages.rs`

```rust
pub async fn reports(CurrentUser(user): CurrentUser) -> Result<Response, WebError> {
    render(ReportsPage {
        layout: Layout::for_user("Berichte", user, "/reports"),
        count: 0,
    })
}
```

**d) Route** — in `src/web/mod.rs`

```rust
.route("/reports", get(pages::reports))
```

**e) Navigation** (falls gewünscht) — im Makro `nav_links` in `templates/layout/app.html`:

```jinja
<a href="/reports" class="{{ class_name }} {% if layout.is_active("/reports") %}nav-link-active{% endif %}">Berichte</a>
```

Im Makro, nicht direkt im Markup: Es erscheint dann in der Kopfzeile und im
Menü auf dem Telefon.

`cargo check` — Askama meldet Tippfehler im Template sofort.

Bezeichner englisch (`reports`, `ReportsPage`, `count`), sichtbarer Text deutsch
(„Berichte", „Einträge") — siehe [CLAUDE.md](../CLAUDE.md#sprache-im-code).

---

## 2. Eine neue Ressource mit CRUD

**Der schnellste Weg: `note` kopieren und umbenennen.** Die Beispielressource
ist genau dafür da. Am Beispiel „Projekt" (`project`):

```bash
cp src/domain/note.rs src/domain/project.rs
cp src/web/notes.rs   src/web/projects.rs
cp src/api/v1/notes.rs src/api/v1/projects.rs
cp templates/pages/notes.html templates/pages/projects.html
```

Dann in den Kopien `note`→`project`, `Note`→`Project`, `notes`→`projects`
ersetzen und die Module in `src/domain/mod.rs`, `src/web/mod.rs`,
`src/api/v1/mod.rs` eintragen.

**Migration** — `migrations/0005_projects.sql`:

> Die Nummer ist die **nächste freie**. `ls migrations/` zeigt die letzte.
> Eine doppelte Nummer lässt den Start scheitern.

```sql
CREATE TABLE projects (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       TEXT        NOT NULL,
    description TEXT       NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX projects_user_id_created_at_idx ON projects (user_id, created_at DESC);

CREATE TRIGGER projects_updated_at BEFORE UPDATE ON projects
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
```

Migrationen laufen beim nächsten Start automatisch.

> **Immer mitnehmen:** die `user_id`-Spalte mit `ON DELETE CASCADE`, den Index
> auf `(user_id, created_at)` und den `updated_at`-Trigger.
>
> **Und die zwei Fassungen:** Funktionen für die Weboberfläche filtern nach
> `user_id` (`list_for_user`, `get_owned`, `update_owned`, `delete_owned`) —
> das ist die Datentrennung zwischen Benutzern. Funktionen für die
> Maschinen-Schnittstelle arbeiten systemweit (`list_all`, `get`, `update`,
> `delete`). Wer die falsche benutzt, öffnet fremde Daten.

---

## 3. Ein Feld zu einer bestehenden Tabelle

**Nie eine bestehende Migration ändern** — sie ist auf anderen Installationen
schon gelaufen. Immer eine neue Datei anlegen:

`migrations/0005_notes_done.sql` (nächste freie Nummer):

```sql
ALTER TABLE notes ADD COLUMN done BOOLEAN NOT NULL DEFAULT FALSE;
```

Dann in `src/domain/note.rs`:

```rust
pub struct Note {
    …
    pub done: bool,
}

const COLUMNS: &str = "id, user_id, title, body, done, created_at, updated_at";
```

Die `COLUMNS`-Konstante nicht vergessen — sonst kommt zur Laufzeit ein
Decode-Fehler, weil die Spalte in der Abfrage fehlt.

---

## 4. Eine Seite auf eine Rolle beschränken

Nur der Extractor in der Signatur ändert sich:

```rust
pub async fn page(ModeratorUser(user): ModeratorUser) -> …   // ab Moderator
pub async fn page(AdminUser(user): AdminUser) -> …           // nur Admin
```

Wer die Rolle nicht hat, bekommt automatisch eine 403-Seite; wer nicht
angemeldet ist, wird zum Login umgeleitet. **Keine `if`-Abfrage im Handler.**

Navigationspunkt passend ausblenden:

```jinja
{% if layout.is_moderator() %}
  <a href="/moderation" class="nav-link">Moderation</a>
{% endif %}
```

> Das Ausblenden ist reine Anzeige. Die Absicherung ist der Extractor.

---

## 5. Etwas per htmx nachladen

Das Muster hat drei Teile: Auslöser, Ziel, Fragment.

**Markup:**

```html
<button hx-get="/notes/search?q=rust"
        hx-target="#note-list"
        hx-swap="innerHTML">
  <span class="htmx-indicator spinner"></span>
  Suchen
</button>

<div id="note-list"></div>
```

**Handler** — gibt **nur das Fragment** zurück, keine ganze Seite:

```rust
pub async fn search(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Response, WebError> {
    let notes = note::list_for_user(&state.db, user.id).await?;
    render(NoteListPartial { notes })
}
```

Die wichtigsten `hx-swap`-Werte:

| Wert | Wirkung |
|---|---|
| `innerHTML` | Inhalt des Ziels ersetzen (Vorgabe) |
| `outerHTML` | Ziel selbst ersetzen — leere Antwort = Element verschwindet |
| `afterbegin` | oben einfügen (neuer Eintrag in einer Liste) |
| `beforeend` | unten anhängen |

Weitere nützliche Attribute: `hx-confirm="Sicher?"`, `hx-trigger="keyup changed delay:300ms"`,
`hx-on::after-request="if (event.detail.successful) this.reset()"`.

---

## 6. Eine Meldung anzeigen

Meldungen reisen als Out-of-Band-Fragment mit jeder beliebigen Antwort mit.
Das Template braucht dafür nur eine Zeile:

```jinja
{% include "partials/toast_oob.html" %}
```

Und das Struct ein Feld:

```rust
pub struct MyPartial {
    pub data: …,
    pub toast: Option<Toast>,
}
```

Im Handler:

```rust
render(MyPartial {
    data,
    toast: Some(Toast::success("Gespeichert.")),   // oder Toast::error(…)
})
```

htmx setzt das Fragment nach `#toasts`, unabhängig vom eigentlichen `hx-target`.
Die Meldung blendet sich nach vier Sekunden selbst aus.

---

## 7. Ein Formular mit Fehlern zurückgeben

Bei normalen Formularen (ohne htmx) die Seite erneut rendern — mit
Fehlermeldung und **erhaltenen Eingaben**, damit der Benutzer nicht alles
neu tippen muss:

```rust
match domain::do_something(&state.db, &form).await {
    Ok(_) => Ok(Redirect::to("/target").into_response()),
    Err(err) => {
        let status = err.status();
        let body = render(MyPage {
            layout: Layout::new("Titel", None, "/path"),
            error: Some(err.public_message()),
            input: form.input,            // ← stehen lassen
        })?;
        Ok((status, body).into_response())
    }
}
```

Im Template:

```jinja
{% if let Some(message) = error %}
  <div class="alert alert-danger" role="alert">{{ message }}</div>
{% endif %}
```

Vollständiges Beispiel: `src/web/auth.rs`.

---

## 8. Einen Endpunkt zur JSON-API

In `src/api/v1/`, mit einem **Scope-Extractor** als Berechtigung. Nicht mit
einer Rolle — hinter einem Token steht kein Benutzer.

```rust
use crate::api::v1::Data;
use crate::auth::extract::NotesRead;

pub async fn statistics(
    State(state): State<AppState>,
    _: NotesRead,                       // ← verlangt Scope notes:read
) -> Result<Json<Data<serde_json::Value>>, ApiError> {
    let count = note::list_all(&state.db, None).await?.len();
    Ok(Data::new(serde_json::json!({ "notes": count })))
}
```

Route in `src/api/v1/mod.rs` eintragen:

```rust
.route("/statistics", get(statistics))
```

Beachten:

- Rückgabetyp ist `ApiError`, nicht `WebError` — sonst kommt HTML statt JSON.
- Erfolgreiche Antworten immer in `Data::new(…)` hüllen: `{"data": …}`.
- Die **systemweite** Fassung der Domain-Funktionen benutzen (`list_all`, `get`),
  nicht die `_owned`-Fassung. Ein Maschinen-Client arbeitet nicht aus der Sicht
  einer Person.
- Wird ein Besitzer gebraucht, kommt er aus dem Anfragekörper — siehe
  `CreateInput` in `src/api/v1/notes.rs`.
- Endpunkt samt nötigem Scope in [docs/API.md](API.md) ergänzen.

Braucht der Endpunkt nur ein gültiges Token ohne bestimmte Berechtigung:

```rust
pub async fn info(ApiClient { token }: ApiClient) -> …
```

---

## 9. Einen neuen Scope

Am Beispiel `reports:read`. Drei Stellen:

**a) `src/auth/scope.rs`** — Konstante und Eintrag in `ALL`:

```rust
pub const REPORTS_READ: &str = "reports:read";

pub const ALL: &[Scope] = &[
    // … bestehende …
    Scope {
        name: REPORTS_READ,
        label: "Berichte lesen",
        description: "Auswertungen abrufen",
    },
];
```

Das Kontrollkästchen im Token-Formular erscheint automatisch — die Seite
zeigt einfach alles aus `ALL`.

**b) `src/auth/extract.rs`** — Extractor erzeugen:

```rust
scope_extractor!(ReportsRead, scope::REPORTS_READ, "Token mit Scope `reports:read`.");
```

**c) Endpunkte** mit `_: ReportsRead` in der Signatur versehen und in
[docs/API.md](API.md) beschreiben.

> Bestehende Tokens bekommen den neuen Scope **nicht** nachträglich. Das ist
> Absicht: Eine neue Berechtigung soll ausdrücklich vergeben werden. Wer sie
> braucht, stellt ein neues Token aus.

---

## 10. Eine vierte Rolle

Drei Stellen, in dieser Reihenfolge:

**a) Migration** — `migrations/0005_role_editor.sql` (nächste freie Nummer):

```sql
ALTER TYPE user_role ADD VALUE 'editor' AFTER 'moderator';
```

**b) `src/auth/role.rs`** — Variante, `rank()`, `as_str()`, `label()`,
`FromStr` und `ALL` ergänzen. Der Compiler zeigt jede fehlende Stelle an,
weil alle `match`-Ausdrücke vollständig sein müssen.

**c) Extractor** — in `src/auth/extract.rs`:

```rust
pub struct EditorUser(pub user::User);
role_extractor!(EditorUser, Role::Editor);
```

> `ALTER TYPE ... ADD VALUE` läuft in PostgreSQL nicht innerhalb einer
> Transaktion, die den neuen Wert direkt danach benutzt. Deshalb die Migration
> allein lassen und Datenänderungen in eine zweite Datei legen.

---

## 11. Eine neue Umgebungsvariable

Am Beispiel `UPLOAD_MAX_MB`, der größten erlaubten Dateigröße.

**a) `src/config.rs`** — Feld ergänzen und in `from_env()` lesen:

```rust
pub struct Config {
    …
    /// Größte erlaubte Upload-Größe in Megabyte.
    pub upload_max_mb: u32,
}

// in from_env():
upload_max_mb: optional("UPLOAD_MAX_MB", "10").parse()?,
// zwingend erforderlich statt mit Vorgabe:
upload_max_mb: require("UPLOAD_MAX_MB")?.parse()?,
```

Gehören mehrere Werte zusammen, ein eigenes Struct anlegen — so wie `Branding`
(Name, Farben) und `MailConfig` (SMTP, Absender). Farben mit `color(…)` lesen,
das prüft das Format schon beim Start.

**b) `Config::for_tests()`** — dort ebenfalls einen Wert eintragen. Der
Compiler erinnert daran, weil das Struct dort vollständig aufgebaut wird.

**c) `.env.example`** — mit Kommentar eintragen. Werte mit Leerzeichen oder
`< >` in Anführungszeichen: Eine fehlerhafte `.env` bricht den Start ab.

**d) `compose.yaml`** — unter `app.environment` ergänzen, falls im Container nötig.

**e) Systemseite** — betriebsrelevante Werte in `templates/pages/admin_system.html`
anzeigen, samt Variablennamen. Geheimnisse (Passwörter, Schlüssel) **nie**
anzeigen, höchstens „gesetzt / nicht gesetzt".

Benutzung im Handler: `state.config.upload_max_mb`.

> Umgebungsvariablen werden **nur** in `config.rs` gelesen. Kein
> `std::env::var` irgendwo sonst — sonst weiß niemand mehr, was die Anwendung
> alles an Einstellungen erwartet.

---

## 12. Eine E-Mail verschicken

**Die kürzeste Form** — reiner Text, direkt im Handler:

```rust
use crate::mail::Mail;

state.mailer.send(
    Mail::to(&user.email)
        .subject("Dein Konto ist eingerichtet")
        .text("Hallo, du kannst dich jetzt anmelden."),
).await?;
```

**`send` oder `send_in_background`?**

| | wartet | Fehler | richtig für |
|---|---|---|---|
| `send(mail).await?` | ja | kommen im Handler an | Der Benutzer muss wissen, ob es geklappt hat |
| `send_in_background(mail)` | nein | nur im Log | Benachrichtigungen — ein langsamer Mailserver hält dann keine Seite auf |

**Mit HTML-Fassung** — drei Schritte:

**a) Template** — `templates/mail/welcome.html`, erbt vom Mail-Rahmen:

```jinja
{% extends "mail/base.html" %}
{% block subject %}Willkommen{% endblock %}

{% block content %}
<p style="margin:0 0 16px;">Hallo {{ name }},</p>
<p style="margin:0;">dein Konto bei {{ brand.name }} ist eingerichtet.</p>
{% endblock %}
```

In Mails **nur Inline-Styles** — Mailprogramme laden kein Stylesheet, und
Tailwind-Klassen bedeuten dort nichts.

**b) Struct** — in `src/mail/templates.rs`:

```rust
#[derive(Template)]
#[template(path = "mail/welcome.html")]
pub struct Welcome {
    pub brand: Branding,   // Name und Farbe für den Rahmen
    pub name: String,
}
```

**c) Versenden:**

```rust
use crate::mail::{Mail, templates::Welcome};
use crate::templates::branding;

let template = Welcome { brand: branding().clone(), name: user.display_name.clone() };

let mail = Mail::to(&user.email)
    .subject(format!("Willkommen bei {}", branding().name))
    .text(format!("Hallo {},\n\ndein Konto ist eingerichtet.", user.display_name))
    .template(&template)?;

state.mailer.send_in_background(mail);
```

Die Textfassung **immer** mitgeben. Manche Programme zeigen nur Text, und
Spamfilter werten reine HTML-Mails ab.

**Ansehen beim Entwickeln:** `just mail-up` startet Mailpit. Es nimmt jede Mail
an, verschickt nichts und zeigt alles unter <http://localhost:58025>.

**Testen:** `Mailer::in_memory()` sammelt Mails statt sie zu verschicken —
Beispiel in `tests/http.rs`, `testmail_kommt_mit_text_und_html_an`.
