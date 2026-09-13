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

---

## 1. Eine neue Seite

**Beispiel: `/berichte`**

**a) Template** — `templates/pages/berichte.html`

```jinja
{% extends "layout/app.html" %}
{% block title %}Berichte{% endblock %}

{% block main %}
<h1 class="text-2xl font-semibold tracking-tight">Berichte</h1>
<p class="mt-1 text-sm text-muted">{{ anzahl }} Einträge</p>
{% endblock %}
```

**b) Struct** — in `src/templates.rs`

```rust
#[derive(Template)]
#[template(path = "pages/berichte.html")]
pub struct BerichtePage {
    pub layout: Layout,
    pub anzahl: usize,
}
```

**c) Handler** — in `src/web/pages.rs`

```rust
pub async fn berichte(CurrentUser(user): CurrentUser) -> Result<Response, WebError> {
    render(BerichtePage {
        layout: Layout::for_user("Berichte", user, "/berichte"),
        anzahl: 0,
    })
}
```

**d) Route** — in `src/web/mod.rs`

```rust
.route("/berichte", get(pages::berichte))
```

**e) Navigation** (falls gewünscht) — in `templates/layout/app.html`

```jinja
<a href="/berichte" class="nav-link {% if layout.is_active("/berichte") %}nav-link-active{% endif %}">Berichte</a>
```

`cargo check` — Askama meldet Tippfehler im Template sofort.

---

## 2. Eine neue Ressource mit CRUD

**Der schnellste Weg: `note` kopieren und umbenennen.** Die Beispielressource
ist genau dafür da. Am Beispiel „Projekt":

```bash
cp src/domain/note.rs src/domain/projekt.rs
cp src/web/notes.rs   src/web/projekte.rs
cp src/api/v1/notes.rs src/api/v1/projekte.rs
cp templates/pages/notes.html templates/pages/projekte.html
```

Dann in den Kopien `note`→`projekt`, `Note`→`Projekt`, `notes`→`projekte`
ersetzen und die Module in `src/domain/mod.rs`, `src/web/mod.rs`,
`src/api/v1/mod.rs` eintragen.

**Migration** — `migrations/0002_projekte.sql`:

```sql
CREATE TABLE projekte (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       TEXT        NOT NULL,
    notiz      TEXT        NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX projekte_user_id_created_at_idx ON projekte (user_id, created_at DESC);

CREATE TRIGGER projekte_updated_at BEFORE UPDATE ON projekte
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

`migrations/0003_notes_erledigt.sql`:

```sql
ALTER TABLE notes ADD COLUMN erledigt BOOLEAN NOT NULL DEFAULT FALSE;
```

Dann in `src/domain/note.rs`:

```rust
pub struct Note {
    …
    pub erledigt: bool,
}

const COLUMNS: &str = "id, user_id, title, body, erledigt, created_at, updated_at";
```

Die `COLUMNS`-Konstante nicht vergessen — sonst kommt zur Laufzeit ein
Decode-Fehler, weil die Spalte in der Abfrage fehlt.

---

## 4. Eine Seite auf eine Rolle beschränken

Nur der Extractor in der Signatur ändert sich:

```rust
pub async fn seite(ModeratorUser(user): ModeratorUser) -> …   // ab Moderator
pub async fn seite(AdminUser(user): AdminUser) -> …           // nur Admin
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
<button hx-get="/notes/suche?q=rust"
        hx-target="#note-list"
        hx-swap="innerHTML">
  <span class="htmx-indicator spinner"></span>
  Suchen
</button>

<div id="note-list"></div>
```

**Handler** — gibt **nur das Fragment** zurück, keine ganze Seite:

```rust
pub async fn suche(
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
pub struct MeinPartial {
    pub daten: …,
    pub toast: Option<Toast>,
}
```

Im Handler:

```rust
render(MeinPartial {
    daten,
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
match domain::etwas_tun(&state.db, &form).await {
    Ok(_) => Ok(Redirect::to("/ziel").into_response()),
    Err(err) => {
        let status = err.status();
        let body = render(MeineSeite {
            layout: Layout::new("Titel", None, "/pfad"),
            error: Some(err.public_message()),
            eingabe: form.eingabe,        // ← stehen lassen
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

pub async fn statistik(
    State(state): State<AppState>,
    _: NotesRead,                       // ← verlangt Scope notes:read
) -> Result<Json<Data<serde_json::Value>>, ApiError> {
    let anzahl = note::list_all(&state.db, None).await?.len();
    Ok(Data::new(serde_json::json!({ "notizen": anzahl })))
}
```

Route in `src/api/v1/mod.rs` eintragen:

```rust
.route("/statistik", get(statistik))
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

Am Beispiel `berichte:read`. Drei Stellen:

**a) `src/auth/scope.rs`** — Konstante und Eintrag in `ALL`:

```rust
pub const BERICHTE_READ: &str = "berichte:read";

pub const ALL: &[Scope] = &[
    // … bestehende …
    Scope {
        name: BERICHTE_READ,
        label: "Berichte lesen",
        description: "Auswertungen abrufen",
    },
];
```

Das Kontrollkästchen im Token-Formular erscheint automatisch — die Seite
zeigt einfach alles aus `ALL`.

**b) `src/auth/extract.rs`** — Extractor erzeugen:

```rust
scope_extractor!(BerichteRead, scope::BERICHTE_READ, "Token mit Scope `berichte:read`.");
```

**c) Endpunkte** mit `_: BerichteRead` in der Signatur versehen und in
[docs/API.md](API.md) beschreiben.

> Bestehende Tokens bekommen den neuen Scope **nicht** nachträglich. Das ist
> Absicht: Eine neue Berechtigung soll ausdrücklich vergeben werden. Wer sie
> braucht, stellt ein neues Token aus.

---

## 10. Eine vierte Rolle

Drei Stellen, in dieser Reihenfolge:

**a) Migration** — `migrations/000X_rolle_redakteur.sql`:

```sql
ALTER TYPE user_role ADD VALUE 'redakteur' AFTER 'moderator';
```

**b) `src/auth/role.rs`** — Variante, `rank()`, `as_str()`, `label()`,
`FromStr` und `ALL` ergänzen. Der Compiler zeigt jede fehlende Stelle an,
weil alle `match`-Ausdrücke vollständig sein müssen.

**c) Extractor** — in `src/auth/extract.rs`:

```rust
pub struct RedakteurUser(pub user::User);
role_extractor!(RedakteurUser, Role::Redakteur);
```

> `ALTER TYPE ... ADD VALUE` läuft in PostgreSQL nicht innerhalb einer
> Transaktion, die den neuen Wert direkt danach benutzt. Deshalb die Migration
> allein lassen und Datenänderungen in eine zweite Datei legen.

---

## 11. Eine neue Umgebungsvariable

**a) `src/config.rs`** — Feld ergänzen und in `from_env()` lesen:

```rust
pub struct Config {
    …
    pub smtp_url: String,
}

// in from_env():
smtp_url: optional("SMTP_URL", "smtp://localhost:1025"),
// oder, wenn zwingend:
smtp_url: require("SMTP_URL")?,
```

**b) `.env.example`** — mit Kommentar eintragen.

**c) `compose.yaml`** — unter `app.environment` ergänzen, falls im Container nötig.

Benutzung im Handler: `state.config.smtp_url`.

> Umgebungsvariablen werden **nur** in `config.rs` gelesen. Kein
> `std::env::var` irgendwo sonst — sonst weiß niemand mehr, was die Anwendung
> alles an Einstellungen erwartet.
