# Aufbau

Warum das Projekt so geschnitten ist, wie es geschnitten ist.

---

## Die Grundentscheidung: HTML vom Server

Es gibt kein Frontend-Framework. Der Server erzeugt HTML, htmx tauscht bei
Bedarf einzelne Ausschnitte aus.

**Was das bringt:**

- Eine Sprache, eine Werkzeugkette, ein Prozess. Kein `node_modules`, kein
  zweiter Build, kein Vertragsbruch zwischen Frontend und Backend.
- Ein Feature lebt an einem Ort statt verteilt über API-Endpunkt, Typdefinition,
  Client-Funktion und Komponente.
- Askama prüft Templates zur Compile-Zeit. Ein falscher Feldname bricht den
  Build, nicht die Produktion.

**Was das kostet:**

Stark interaktive Oberflächen — Drag-and-drop, Live-Zeichenflächen,
Offline-Betrieb — sind mühsamer als mit einem Framework. Für Formulare,
Listen, Übersichten und Verwaltungsoberflächen ist der Tausch gut.

---

## Die Schichten

```
        HTTP-Anfrage
             │
    ┌────────┴────────┐
    │                 │
src/web/          src/api/          ← dünn: Ein- und Ausgabe
HTML + htmx       JSON                 Extractors, Statuscodes
Session-Cookie    Bearer-Token
    │                 │
    └────────┬────────┘
             │
        src/domain/                 ← Fachlogik. Kennt kein HTTP.
      user · api_token · note          Validierung, Eigentumsprüfung
             │
          sqlx / PostgreSQL
```

**Die Regel:** In `src/domain/` gibt es kein `Request`, kein `StatusCode`,
kein `Json<…>`, kein Template.

Der Nutzen ist unmittelbar: `POST /notes` (HTML) und `POST /api/v1/notes` (JSON)
rufen dieselbe Funktion `note::create` auf. Die Validierung existiert einmal.
Ohne diese Trennung driften die beiden Wege auseinander, und einer von beiden
bekommt irgendwann eine Prüfung nicht mit.

---

## Autorisierung über das Typsystem

Statt einer Prüfung im Handler steht die Anforderung in der Signatur:

```rust
async fn benutzerliste(AdminUser(admin): AdminUser) -> …
```

Der Extractor lädt den Benutzer, prüft die Rolle und weist ab, bevor der
Handler läuft. Eine vergessene Prüfung ist damit kein stiller Fehler, sondern
gar nicht erst formulierbar — man müsste absichtlich den falschen Extractor
wählen.

Rollen sind hierarchisch (`Role::rank()`): Ein Administrator kommt überall
hin, wo ein Moderator hindarf.

---

## Zwei Zugänge, zwei Berechtigungsmodelle

|  | HTML (`src/web/`) | JSON (`src/api/`) |
|---|---|---|
| Wer | ein **Mensch** | ein **fremdes System** |
| Nachweis | Session-Cookie | `Authorization: Bearer sk_…` |
| Berechtigung | **Rolle** (`user`/`moderator`/`admin`) | **Scopes** (`notes:read`, …) |
| Sichtbare Daten | nur die eigenen | systemweit |
| Zustand | Session-Zeile in Postgres | zustandslos, ein Lookup |
| Beenden | Abmelden, sofort wirksam | Token widerrufen |
| CSRF | Schutz nötig | kein Ziel — Browser senden den Header nie von selbst |

Der entscheidende Unterschied: **Hinter einem Token steht kein Benutzer.** Es
gehört der Installation, nicht einer Person, und erbt deshalb keine Rolle. Wo
ein Besitzer gebraucht wird, gibt die Schnittstelle ihn ausdrücklich mit.

Das hat eine Folge, die man im Code sieht: Die Domain-Funktionen gibt es in
zwei Ausführungen — eine, die nach Besitzer filtert (`get_owned`), und eine
systemweite (`get`). Die Weboberfläche darf nur die erste benutzen.

---

## CSRF ohne Formular-Tokens

Zweistufig:

1. Das Session-Cookie hat `SameSite=Lax`. Browser senden es bei seitenfremden
   POST-Anfragen gar nicht erst mit.
2. `src/auth/csrf.rs` prüft zusätzlich bei jeder verändernden Anfrage die
   Herkunft (`Sec-Fetch-Site`, ersatzweise `Origin`).

**Warum keine versteckten Tokens im Formular?** Weil man sie beim Anlegen eines
neuen Formulars vergessen kann — und dann fällt es niemandem auf, weil das
Formular ja funktioniert. Die Middleware greift dagegen für jede Route, auch
für die, an die beim Schreiben niemand gedacht hat.

---

## Datenbank: zur Laufzeit geprüfte Abfragen

Bewusst `sqlx::query_as` statt der `query_as!`-Makros. Die Makros prüfen SQL
zur Compile-Zeit — das ist schön, verlangt aber bei **jedem** Build eine
erreichbare Datenbank oder ein aktuelles `.sqlx`-Verzeichnis. In der Praxis
heißt das: gebrochene Docker-Builds und ein zusätzlicher Handgriff nach jeder
Query-Änderung.

Der Ersatz sind die Tests in `tests/domain.rs`. Sie laufen gegen eine echte
Postgres und fangen fehlerhafte Abfragen ebenso zuverlässig ab — nur eben beim
Testlauf statt beim Kompilieren.

**Migrationen** laufen beim Start (`sqlx::migrate!`). Sie sind idempotent, also
auch bei mehreren Instanzen unproblematisch. Bestehende Migrationen werden nie
geändert, es kommt immer eine neue Datei dazu.

---

## Fehlerbehandlung

Ein fachlicher Fehlertyp `Error`, zwei Hüllen für die Darstellung:

- `WebError` → HTML-Fehlerseite
- `ApiError` → `{"error": {...}}`

Der Handler wählt über seinen Rückgabetyp, was herauskommt. `?` funktioniert
in beiden Fällen wie gewohnt.

Serverfehler (5xx) werden vollständig geloggt, nach außen geht nur „Es ist ein
interner Fehler aufgetreten." Datenbankfehler und Template-Pfade sind nichts
für Fremde.

---

## Ausliefern

Ein mehrstufiges Dockerfile erzeugt ein Abbild ohne Rust-Werkzeugkette. Die
Anwendung läuft als unprivilegierter Benutzer, hört auf SIGTERM und bedient
laufende Anfragen vor dem Beenden zu Ende.

Die Datenbank ist **extern** — der Container hält keinen Zustand und lässt sich
beliebig oft parallel starten. Sessions liegen in Postgres, nicht im
Arbeitsspeicher, also braucht es keine klebrigen Sitzungen im Loadbalancer.

Das Stylesheet wird in einer eigenen Build-Stufe erzeugt. Zur Laufzeit gibt es
keinen CSS-Build und kein Node.
