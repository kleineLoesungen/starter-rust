# Starter — Webanwendungen in Rust

Startpunkt für Webanwendungen mit **Axum**, **Askama**, **htmx** und **Tailwind CSS v4**
auf einer **externen PostgreSQL**-Datenbank, ausgeliefert als **Container**.

Anmeldung, Rollen, App-Tokens und ein anpassbares Design sind bereits eingebaut.
Es gibt nur eine Sprache (Rust), eine Werkzeugkette (Cargo) und kein Node.

---

## In fünf Minuten laufen

Voraussetzungen: [Rust](https://rustup.rs) ≥ 1.90, Docker, [just](https://github.com/casey/just)
(`cargo install just`).

```bash
just setup    # Tailwind holen, .env anlegen, Postgres starten, Stylesheet bauen
just dev      # Server starten
```

Dann [http://localhost:3000](http://localhost:3000) öffnen und die
**Ersteinrichtung** durchlaufen.

> **Das erste Konto wird Administrator** — sonst käme niemand an die
> Benutzerverwaltung heran. Danach ist die Selbstregistrierung geschlossen:
> alle weiteren Konten legt der Administrator unter **Benutzer** an.

Ohne `just` geht es auch von Hand:

```bash
./scripts/get-tailwind.sh
cp .env.example .env
docker compose up -d db
.bin/tailwindcss -i assets/css/app.css -o static/app.css
cargo run
```

---

## Eigenes Projekt aus dem Kit

Das Kit selbst bleibt unverändert. Für jede Anwendung eine eigene Kopie:

```bash
git clone <pfad-oder-url-zum-kit> vereinsportal
cd vereinsportal
./scripts/new-project.sh vereinsportal "Vereinsportal"
just setup
just check
```

Das Skript benennt alles um, was „starter" heißt, und wählt freie Ports.
**Direkt nach dem Klonen ausführen, vor jedem `just`-Befehl:** Bis dahin heißt
der Klon noch „starter" und würde die Container des Kits benutzen — ein
`just db-reset` löschte dann dessen Datenbank.
**Nicht auslassen**, auch wenn nur ein Projekt geplant ist: Zwei Projekte mit
dem Namen „starter" teilen sich unbemerkt Container und Datenbank — das zweite
überschreibt die Daten des ersten.

Die Ports stehen danach in `.env` und `compose.yaml`; das Skript nennt sie am
Ende. Zeigt `git remote -v` noch auf das Kit: `git remote remove origin`.

---

## Was drin ist

| Bereich | Umsetzung |
|---|---|
| HTTP | Axum 0.8 |
| HTML | Askama — Templates werden **zur Compile-Zeit geprüft** |
| Interaktivität | htmx 2 — Teilaktualisierungen ohne eigenes JavaScript |
| Aussehen | Tailwind v4, komplett über Design-Tokens steuerbar |
| Datenbank | PostgreSQL über sqlx, Migrationen laufen beim Start |
| Anmeldung | Session-Cookie, Session-Daten in Postgres |
| Passwörter | Argon2id |
| Rollen | `user` · `moderator` · `admin`, erzwungen über Typen |
| Benutzer | Anlegen, bearbeiten, sperren, löschen, Passwort zurücksetzen |
| Eigenes Konto | Name, E-Mail und Passwort selbst ändern |
| API | JSON unter `/api/v1` für Maschinen, Zugriff über Scopes |
| CSRF | SameSite-Cookie plus Herkunftsprüfung |
| Mobil | unter 1024 px Navigationsleiste unten, „Mehr"-Blatt, kein waagerechtes Scrollen |
| Anmeldeschutz | 3 Versuche frei, danach wachsende Sperre pro Konto und IP |
| Zeitangaben | in UTC gespeichert, in der Ortszeit des Betrachters angezeigt |
| Installierbar | PWA mit Manifest, Icons und Offline-Seite; Name und Farben per Umgebung |
| E-Mail | SMTP per `SMTP_URL`, lokal Mailpit, Testmail unter **System** |
| Container | Mehrstufiges Dockerfile, unprivilegierter Benutzer |
| Tests | 91 Tests, jeder mit eigener frischer Datenbank |

---

## Wo was liegt

```
src/
├── main.rs             Startpunkt — nur Hochfahren
├── app.rs              Zusammenbau des Routers (auch von Tests benutzt)
├── config.rs           Alle Umgebungsvariablen an einer Stelle
├── error.rs            Fehlertypen: HTML-Fehlerseite vs. JSON-Fehler
├── templates.rs        Ein Struct je Seite, Name und Farben (branding)
├── pwa.rs              Manifest, Service Worker, Offline-Seite
│
├── domain/             ← FACHLOGIK. Kennt weder HTTP noch Templates.
│   ├── user.rs
│   ├── api_token.rs
│   ├── login_attempt.rs  Bremse gegen Passwort-Raten
│   └── note.rs           Beispielressource zum Kopieren
│
├── auth/
│   ├── role.rs           Rollen und ihre Rangfolge
│   ├── scope.rs          Was ein App-Token darf
│   ├── password.rs       Argon2id
│   ├── token.rs          Erzeugen und Prüfen der App-Tokens
│   ├── session.rs        Anmelden/Abmelden
│   ├── csrf.rs           Herkunftsprüfung
│   ├── client_ip.rs      Client-Adresse, auch hinter einem Proxy
│   └── extract.rs        CurrentUser, AdminUser, ApiClient, NotesRead …
│
├── mail/               ← E-Mail: Mail::to(..).subject(..).text(..)
│   ├── mod.rs            Mailer und Mail
│   └── templates.rs      Ein Struct je HTML-Mail
│
├── web/                ← HTML-Routen (Session-Cookie, Rollen)
│   ├── account.rs        eigenes Konto
│   ├── admin.rs          Benutzerverwaltung
│   ├── tokens.rs         App-Tokens (nur Administratoren)
│   └── system.rs         wirksame Konfiguration, Testmail
└── api/                ← JSON-Routen (Bearer-Token, Scopes)

templates/
├── layout/             Grundgerüst und angemeldeter Rahmen
├── pages/              eine Datei je Seite
├── partials/           htmx-Fragmente
├── components/         Makros (Zeitstempel, Listeneinträge)
└── mail/               HTML-Mails, nur Inline-Styles
assets/
├── css/theme.css       ← HIER das Aussehen ändern
├── css/components.css  Komponentenklassen (.btn, .card, …)
└── icons/              Quelle der App-Icons (SVG)
static/icons/           erzeugte PNG-Icons — nach Änderung ./scripts/icons.sh
migrations/             SQL-Migrationen
docs/                   Ausführliche Dokumentation
```

---

## Weiterlesen

| Datei | Inhalt |
|---|---|
| [docs/RECIPES.md](docs/RECIPES.md) | **Hier anfangen.** Wie füge ich eine Seite, eine Ressource, eine Rolle hinzu? |
| [docs/DESIGN.md](docs/DESIGN.md) | Design-Guide: Farben, Abstände, Komponenten anpassen |
| [docs/API.md](docs/API.md) | JSON-Schnittstelle und App-Tokens |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Warum es so aufgebaut ist, wie es aufgebaut ist |
| [docs/DEPLOY.md](docs/DEPLOY.md) | Produktivbetrieb, Sicherheitseinstellungen |
| [CLAUDE.md](CLAUDE.md) | Regeln für KI-Assistenten in diesem Projekt |

Der **Styleguide** unter [/styleguide](http://localhost:3000/styleguide) zeigt jede
Komponente im echten Zustand — mit Reglern zum Ausprobieren von Farbe und Rundung.

---

## Häufige Befehle

```bash
just dev          # Server starten (baut das Stylesheet vorher)
just css-watch    # Stylesheet beobachten (zweites Terminal)
just test         # Tests
just check        # Formatierung + Clippy + Tests
just db-shell     # psql öffnen
just mail-up      # Mailpit starten, Mails ansehen unter http://localhost:58025
just db-reset     # Datenbank verwerfen und neu anlegen
just up           # alles im Container starten
```

## Lizenz

Vorlage zur freien Verwendung. Eigene Lizenz ergänzen.
