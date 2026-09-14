# Regeln für dieses Projekt

Diese Datei ist für KI-Assistenten gedacht (Claude Code, Cursor, …) und
gleichzeitig als Kurzreferenz für Menschen brauchbar. Sie beschreibt, was in
diesem Projekt gilt und wo die Fallstricke liegen.

---

## Der Stack in einem Satz

Rust mit Axum, serverseitig gerendertes HTML mit Askama, Teilaktualisierungen
mit htmx, Aussehen über Tailwind-v4-Tokens, Daten in einer externen PostgreSQL.
**Kein Node, kein Build-Schritt für JavaScript, kein Frontend-Framework.**

---

## Festgenagelte Versionen — bitte nicht „aktualisieren"

Diese Versionen sind bewusst gewählt. Ein Upgrade bricht Dinge.

| Paket | Version | Grund |
|---|---|---|
| **htmx** | **2.0.10** | htmx 4 ist erschienen, bringt aber Breaking Changes (`<hx-partial>`, umbenannte Events, Erweiterungs-Allowlist). Sprachmodelle kennen fast ausschließlich htmx-1/2-Syntax und erzeugen für htmx 4 still kaputtes Markup. |
| **sqlx** | **0.8** | `tower-sessions-sqlx-store` hängt an sqlx 0.8. Mit sqlx 0.9 landen zwei inkompatible `PgPool`-Typen im Abhängigkeitsbaum und nichts kompiliert mehr. |
| **tower-sessions** | **0.14** | Version 0.15 benutzt `tower-sessions-core` 0.15, der Postgres-Store aber 0.14. Der `SessionStore`-Trait passt dann nicht. |
| **Tailwind** | **v4.3.3** | Standalone-Binary, festgelegt in `scripts/get-tailwind.sh` und im `Dockerfile`. |

Wer eine Version anheben will: erst `cargo tree -d` prüfen, dann `just check`.

---

## Askama 0.16 — drei Eigenheiten

Askama hat sich zwischen den Versionen mehrfach geändert. Was aus älteren
Beispielen im Netz stammt, funktioniert hier oft nicht:

1. **`{% call %}` braucht ein `{% endcall %}`.**
   ```jinja
   {% call ui::note_item(note) %}{% endcall %}   ✅
   {% call ui::note_item(note) %}                ❌ Compile-Fehler
   ```

2. **Keine eigenen Filter schreiben.** Askama 0.16 verlangt dafür eine
   Builder-Struct mit `Default` und `execute()` — umständlich und schlecht
   merkbar. Stattdessen Methoden benutzen:
   ```jinja
   {{ user.role.label() }}            ✅  Methode am Typ
   {{ user.role|label }}              ❌  eigener Filter
   ```
   Für Zeitstempel gibt es fertige Makros, siehe unten unter „Aussehen ändern".

3. **Templates werden zur Compile-Zeit geprüft.** Ein Tippfehler im Feldnamen
   bricht `cargo build`. Das ist gewollt — nach jeder Template-Änderung
   `cargo check` laufen lassen.

---

## Sprache im Code

* **Bezeichner englisch** — Funktionen, Typen, Felder öffentlicher
  Schnittstellen: `Mail::to(..).subject(..)`, `mailer.send(..)`,
  `Mailer::in_memory()`. So heißen sie auch in den Crates, und so rät man sie.
* **Kommentare, Doku, Oberflächentexte, Testnamen deutsch.**

Dazu gehören auch Datenbanktabellen und -spalten, Template-Felder, Makros,
HTML-IDs, CSS-Klassen, `data`-Attribute, Formularfelder, Query-Parameter und
JavaScript. Lokale Variablen dürfen deutsch sein, im Zweifel englisch.

---

## Die eine wichtige Schichtregel

```
src/domain/   →  Fachlogik. Kennt WEDER Axum NOCH Templates.
src/web/      →  HTML-Routen. Dünn. Ruft domain auf.
src/api/      →  JSON-Routen. Dünn. Ruft dieselben domain-Funktionen auf.
```

Wenn in `src/domain/` ein `Request`, ein `StatusCode`, ein `Json<…>` oder ein
Template auftaucht, ist es an der falschen Stelle.

Der Nutzen ist konkret: HTML-Oberfläche und JSON-API teilen sich dieselbe
Fachlogik. Eine Regel wird einmal geschrieben, nicht zweimal leicht
unterschiedlich.

---

## Autorisierung ist eine Typfrage

Nie `if user.role == …` in einen Handler schreiben. Stattdessen den passenden
Extractor in die Signatur nehmen:

```rust
// Weboberfläche — Rollen
async fn seite(CurrentUser(user): CurrentUser)      // angemeldet
async fn seite(ModeratorUser(user): ModeratorUser)  // ab Moderator
async fn seite(AdminUser(user): AdminUser)          // nur Admin
async fn seite(MaybeUser(user): MaybeUser)          // beides möglich

// JSON-Schnittstelle — Scopes
async fn api(client: ApiClient)                     // irgendein gültiges Token
async fn api(_: NotesRead)                          // Token mit notes:read
async fn api(_: NotesWrite)                         // Token mit notes:write
```

Wer die Prüfung vergisst, bekommt keinen unsicheren Endpunkt, sondern einen
Typfehler.

Zusätzlich gilt: **Eigentumsprüfung gehört in die Domain-Schicht.** Für die
Weboberfläche filtern alle Abfragen nach `user_id` (siehe `note::get_owned`).
Fremde Datensätze ergeben `NotFound`, nicht `Forbidden` — sonst verrät die
Antwort, welche IDs existieren.

---

## App-Tokens sind Maschinen, keine Menschen

Ein App-Token steht für ein **fremdes System**, nicht für eine Person:

* Es gehört keinem Benutzer und erbt **keine Rolle**.
* Was es darf, steht ausschließlich in seinen **Scopes** (`src/auth/scope.rs`).
* Ausstellen dürfen nur Administratoren, unter `/admin/tokens`.
* Wo ein Besitzer gebraucht wird, gibt die Schnittstelle ihn **ausdrücklich mit**
  (`{"user_id": "…", …}`), statt ihn aus dem Token abzuleiten.

Daraus folgt für die Domain-Schicht: Die Notiz-Funktionen gibt es in zwei
Ausführungen, und der Unterschied ist sicherheitsrelevant.

| Fassung | Für | Verhalten |
|---|---|---|
| `list_for_user`, `get_owned`, `update_owned`, `delete_owned` | `src/web/` | filtert nach Besitzer |
| `list_all`, `get`, `update`, `delete` | `src/api/` | systemweit, ohne Filter |

**Wer in einem Web-Handler versehentlich die systemweite Fassung benutzt,
öffnet fremde Daten.** Im Zweifel die `_owned`-Fassung nehmen.

---

## Anmeldung ist gebremst

Drei Fehlversuche sind frei, danach greift eine Sperre, die mit jedem weiteren
Fehlversuch wächst (5 s → 15 s → 45 s → … → 15 min). Gezählt wird gleichzeitig
pro Konto und pro IP-Adresse; siehe `src/domain/login_attempt.rs`.

Zwei Dinge dabei nicht kaputtmachen:

* Die Prüfung läuft **vor** `user::authenticate`. Ein gesperrter Versuch soll
  keine Argon2-Rechenzeit kosten.
* Ein gesperrter Versuch erhöht den Zähler **nicht**, sonst verlängert stures
  Weiterklicken die eigene Sperre endlos.

Hinter einem Reverse-Proxy muss `TRUST_PROXY=true` gesetzt sein, sonst zählen
alle Anfragen auf die Adresse des Proxys. Ohne Proxy muss es `false` bleiben —
sonst kann sich jeder Client per `X-Forwarded-For` eine neue Identität geben.

---

## E-Mail

```rust
state.mailer.send(Mail::to(&adresse).subject("…").text("…")).await?;   // wartet
state.mailer.send_in_background(mail);                               // wartet nicht
```

* Immer eine Textfassung mitgeben, HTML nur zusätzlich über `.template(&vorlage)?`.
* Mail-Templates unter `templates/mail/`, erben von `mail/base.html`,
  **nur Inline-Styles** — keine Tailwind-Klassen.
* In Tests `Mailer::in_memory()` und `app::build_with_mailer` benutzen, nie
  echten SMTP.
* Rezept: [docs/RECIPES.md](docs/RECIPES.md#12-eine-e-mail-verschicken).

---

## PWA: Der Service Worker speichert keine Seiten

`src/pwa.rs` liefert Manifest, Service Worker und Offline-Seite. Der Worker
speichert nur `/static/…` zwischen. **Keine Seiten, keine API, keine
htmx-Fragmente in den Cache aufnehmen** — die Anwendung ist angemeldet und
serverseitig gerendert. Eine gecachte Seite zeigt veraltete Daten oder nach dem
Abmelden die Inhalte des vorherigen Benutzers.

Name, Kurzname und Farben kommen aus `APP_NAME`, `APP_SHORT_NAME`,
`PWA_THEME_COLOR`, `PWA_BACKGROUND_COLOR`. In Templates über `layout.app_name`,
in Rust über `templates::branding()`.

---

## Selbstregistrierung ist geschlossen

`/register` funktioniert nur, solange die Installation **kein einziges Konto**
hat — das ist die Ersteinrichtung, und dieses Konto wird Administrator. Danach
legt der Administrator alle weiteren Konten unter `/admin/users` an.

Die Prüfung steht im **Handler**, nicht nur im Template. Ein ausgeblendeter Link
hält niemanden davon ab, das Formular von Hand abzuschicken.

---

## Formulare mit Mehrfachauswahl brauchen `axum_extra::extract::Form`

Mehrere Kontrollkästchen mit demselben `name` senden den Schlüssel wiederholt:

```
scopes=notes:read&scopes=notes:write
```

`axum::extract::Form` benutzt `serde_urlencoded` und **kann das nicht** in ein
`Vec<String>` einlesen — die Anfrage scheitert mit einem 422 und der Meldung
„invalid type: string, expected a sequence". Der Fehler sieht aus wie ein
Serde-Problem, ist aber eine Einschränkung des Extractors.

Lösung — für jedes Formular mit Mehrfachauswahl:

```rust
use axum_extra::extract::Form;   // ✅ kann wiederholte Felder
// use axum::extract::Form;      // ❌ 422 bei mehreren Werten
```

Beispiel: `src/web/tokens.rs`. Abgesichert durch
`token_formular_nimmt_mehrere_berechtigungen_entgegen` in `tests/http.rs`.

---

## Datenbank: `query_as`, nicht `query_as!`

Dieses Projekt benutzt die **zur Laufzeit geprüften** sqlx-Funktionen:

```rust
sqlx::query_as::<_, Note>("SELECT … FROM notes WHERE user_id = $1")
    .bind(user_id)
    .fetch_all(db)
    .await?
```

Nicht die `!`-Makros. Grund: Die Makros brauchen bei jedem Build eine
erreichbare Datenbank oder ein aktuelles `.sqlx`-Verzeichnis. Das bricht
Docker-Builds und macht schnelles Iterieren mühsam. Abgesichert wird
stattdessen durch die Tests in `tests/domain.rs`, die gegen eine echte
Datenbank laufen.

**Immer `.bind()` benutzen, niemals Werte in den SQL-String formatieren.**
(Die `format!`-Aufrufe in den Domain-Modulen setzen ausschließlich die feste
`COLUMNS`-Konstante ein, nie Benutzereingaben.)

---

## Aussehen ändern

Nur semantische Tokens verwenden:

```html
<div class="bg-surface-raised text-content border-border">   ✅
<div class="bg-white text-gray-900 border-gray-200">         ❌ bricht im Dunkelmodus
```

Verfügbar sind `surface`, `surface-raised`, `surface-sunken`, `border`,
`border-strong`, `content`, `muted`, `subtle`, `brand`, `brand-subtle`,
`brand-content`, `success`, `warning`, `danger` — jeweils als `bg-`, `text-`
und `border-`. Dazu `rounded-ui` für die einheitliche Eckenrundung.

Wiederkehrendes Markup wird zu einer Klasse in `assets/css/components.css`,
nicht zu einer wiederholten Utility-Kette. Details: [docs/DESIGN.md](docs/DESIGN.md).

Zwei Dinge, die auf schmalen Bildschirmen regelmäßig schiefgehen:

* **Grid-Spalten brauchen `min-w-0`**, sonst schrumpfen sie nie unter die
  Mindestbreite ihres Inhalts und drücken das Layout über den Rand. `.card`
  bringt es mit; eigene Spalten-`div`s nicht.
* **Menüpunkte nur im Makro `nav_links`** in `templates/layout/app.html`
  ergänzen. Es wird zweimal aufgerufen — Kopfzeile (breit) und Blatt hinter
  „Mehr" (schmal). Die untere Leiste auf dem Telefon ist eine handverlesene
  Abkürzung und bleibt bewusst separat.
* **Zeitstempel nur über die Makros `ui::datetime` / `ui::date`** ausgeben.
  `{{ value.date_time() }}` direkt im Template liefert UTC statt Ortszeit — im
  Sommer zwei Stunden daneben.

---

## Nach jeder Änderung

```bash
just check     # Formatierung, Clippy (streng), alle Tests
```

Bei Template- oder CSS-Änderungen zusätzlich `just css`.
Nach einer Änderung an `assets/icons/*.svg` zusätzlich `./scripts/icons.sh`.
Wer `--brand-hue` in `theme.css` ändert, zieht `PWA_THEME_COLOR` und die
Icon-Farbe nach — siehe [docs/DESIGN.md](docs/DESIGN.md#1-der-schnellste-weg-eine-zahl).

---

## Wiederkehrende Aufgaben

Für „wie füge ich eine Seite / eine Ressource / ein Feld hinzu" gibt es
fertige Schrittfolgen in **[docs/RECIPES.md](docs/RECIPES.md)**. Diese zuerst
lesen, statt ein eigenes Muster zu erfinden.

---

## Was hier nicht passieren soll

- Ein Frontend-Framework einführen (React, Svelte, Vue). Der Stack ist bewusst
  serverseitig gerendert.
- Node oder npm einführen. Tailwind läuft als Standalone-Binary.
- Fachlogik in Handler schreiben, statt sie in `domain/` zu legen.
- Rollenprüfungen von Hand statt über Extractors.
- Scope-Prüfungen von Hand statt über die Scope-Extractors.
- Einem Token eine Benutzeridentität andichten. Es gehört der Installation.
- `/register` wieder öffnen, ohne die Prüfung im Handler mitzudenken.
- Rohe Tailwind-Farben (`bg-blue-500`) statt semantischer Tokens.
- Geheimnisse in den Quellcode. Alles kommt aus `Config::from_env()`.
- HTML per `format!` zusammensetzen, ohne Eingaben mit `html_escape` zu
  behandeln. Askama maskiert automatisch — Handarbeit nicht (siehe
  `render_error_page`, dort gab es genau diesen Fehler).
- In `.env` Werte mit Leerzeichen oder `< >` ohne Anführungszeichen.
