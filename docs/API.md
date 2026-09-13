# JSON-Schnittstelle

Alles unter `/api/v1`. Authentifizierung über **App-Tokens** im
`Authorization`-Header.

---

## Wofür diese Schnittstelle gedacht ist

Für **Maschine-zu-Maschine-Kommunikation**: ein Skript, ein Dienst, eine andere
Anwendung. Nicht als Hintertür in die Weboberfläche.

Der wichtigste Punkt dabei:

> **Hinter einem Token steht kein Benutzer.** Ein Token gehört der Installation,
> nicht einer Person. Es erbt deshalb auch keine Rolle. Was es darf, steht
> ausschließlich in seinen **Scopes**.

Praktische Folgen:

- Ein Token kann nicht „sich selbst" abfragen — es gibt kein `/me`.
- Wo ein Besitzer nötig ist (etwa beim Anlegen einer Notiz), wird er
  **ausdrücklich mitgegeben**, statt aus dem Token abgeleitet zu werden.
- Wer ein Token ausgestellt hat, steht als `created_by` in der Datenbank — nur
  zur Nachvollziehbarkeit. Wird dieses Konto gelöscht, bleibt das Token gültig.

---

## Token erstellen

**Nur Administratoren.** In der Anwendung: **Tokens** in der Kopfzeile → Name
eingeben → Berechtigungen ankreuzen → *Token erstellen*.

Das Token wird **genau einmal** angezeigt. Danach liegt in der Datenbank nur
noch sein Hash — es lässt sich nicht wiederherstellen. Verloren heißt: neues
Token erstellen und das alte widerrufen.

Ein Token **ohne** Berechtigung lässt sich nicht anlegen; es könnte nichts.

---

## Berechtigungen (Scopes)

| Scope | Erlaubt |
|---|---|
| `notes:read` | Notizen auflisten und einzeln abrufen |
| `notes:write` | Notizen anlegen, ändern und löschen |
| `users:read` | Benutzerliste abrufen (ohne Passwortdaten) |

Nur vergeben, was das fremde System wirklich braucht. Ein Auswertungsdienst
bekommt `notes:read`, kein `notes:write`.

Definiert sind die Scopes in `src/auth/scope.rs`; wie man einen neuen hinzufügt,
steht in [RECIPES.md](RECIPES.md#9-einen-neuen-scope).

---

## Benutzung

```bash
curl https://example.com/api/v1/notes \
  -H "Authorization: Bearer sk_a1b2c3…"
```

**Einrichtung prüfen** — zeigt Name und Berechtigungen des Tokens:

```bash
curl https://example.com/api/v1/token -H "Authorization: Bearer sk_…"
```

```json
{
  "data": {
    "id": "e5a5766c-…",
    "name": "Rechnungssystem",
    "scopes": ["notes:read", "notes:write"],
    "expires_at": "2026-12-09T00:59:12Z"
  }
}
```

---

## Antwortformat

Erfolg — Nutzdaten immer unter `data`:

```json
{ "data": { "id": "…", "title": "Beispiel" } }
```

Fehler — immer unter `error`, mit stabilem `code`:

```json
{ "error": { "code": "not_found", "message": "Nicht gefunden" } }
```

| HTTP | `code` | Bedeutung |
|---|---|---|
| 400 | `bad_request` | Eingabe ungültig (`message` ist für Menschen gedacht) |
| 401 | `unauthorized` | Token fehlt, ist unbekannt, widerrufen oder abgelaufen |
| 403 | `insufficient_scope` | Token gültig, aber ohne die nötige Berechtigung |
| 404 | `not_found` | Nicht vorhanden |
| 409 | `conflict` | Kollision, z. B. E-Mail schon vergeben |
| 500 | `internal_error` | Serverfehler. Details stehen nur im Log |

Auf `code` prüfen, nicht auf `message` — der Text kann sich ändern.

Bei `403` nennt die Antwort die fehlende Berechtigung, damit klar ist, was zu
tun ist:

```json
{
  "error": {
    "code": "insufficient_scope",
    "message": "Dieses Token hat die Berechtigung \"notes:write\" nicht.",
    "required_scope": "notes:write"
  }
}
```

Zusätzlich steht sie im `WWW-Authenticate`-Kopf:
`Bearer realm="api", scope="notes:write"`.

---

## Endpunkte

### `GET /api/v1/health`

Ohne Authentifizierung. Prüft auch die Datenbankverbindung.

```json
{ "status": "ok", "database": "ok" }
```

`503` mit `"status": "degraded"`, wenn die Datenbank nicht erreichbar ist.
Für Loadbalancer und Container-Orchestrierung gedacht.

### `GET /api/v1/token`

Kein Scope nötig, nur ein gültiges Token. Gibt Name, Berechtigungen und
Ablaufdatum zurück.

### `GET /api/v1/users` — Scope `users:read`

Alle Benutzer. Passwortdaten sind aus der Serialisierung ausgeschlossen und
können die Schnittstelle nicht verlassen.

### Notizen

Die Beispielressource. Zeigt das vollständige Muster.

| Methode | Pfad | Scope | Erfolg |
|---|---|---|---|
| `GET` | `/api/v1/notes` | `notes:read` | 200 |
| `GET` | `/api/v1/notes?user_id={id}` | `notes:read` | 200 |
| `GET` | `/api/v1/notes/{id}` | `notes:read` | 200 |
| `POST` | `/api/v1/notes` | `notes:write` | 201 |
| `PUT` | `/api/v1/notes/{id}` | `notes:write` | 200 |
| `DELETE` | `/api/v1/notes/{id}` | `notes:write` | 204 |

Ohne `?user_id=` liefert die Liste **alle** Notizen der Installation. Das ist
Absicht: Ein Maschinen-Client arbeitet systemweit, nicht aus der Sicht einer
Person.

**Anlegen** — der Besitzer wird ausdrücklich mitgegeben:

```bash
curl -X POST https://example.com/api/v1/notes \
  -H "Authorization: Bearer sk_…" \
  -H "Content-Type: application/json" \
  -d '{"user_id": "b7beae3c-…", "title": "Neue Notiz", "body": "Inhalt"}'
```

```json
{
  "data": {
    "id": "387185a7-…",
    "user_id": "b7beae3c-…",
    "title": "Neue Notiz",
    "body": "Inhalt",
    "created_at": "2026-09-10T00:59:12Z",
    "updated_at": "2026-09-10T00:59:12Z"
  }
}
```

Eine unbekannte `user_id` ergibt `400 bad_request`, nicht `500`.

Zeitangaben sind durchgehend **RFC 3339 in UTC**.

---

## Wie die Tokens aufgebaut sind

```
sk_a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6_x7Y8z9…
│  └─ token_id, 32 Zeichen hex ───────┘ └─ Geheimnis, 256 Bit ─┘
└─ Präfix
```

- **`token_id`** liegt im Klartext in der Datenbank und ist indiziert. Das
  Prüfen eines Tokens braucht dadurch genau einen Lookup.
- **Das Geheimnis** wird als SHA-256 gespeichert und in konstanter Zeit
  verglichen.

**Warum SHA-256 und nicht Argon2 wie bei Passwörtern?** Weil das Geheimnis
256 Bit Zufall ist und nicht erraten werden kann. Argon2 würde jeden
API-Aufruf um rund 100 ms verlangsamen, ohne Sicherheit zu gewinnen. GitHub
handhabt seine Personal Access Tokens genauso.

Ein `401` unterscheidet bewusst nicht zwischen *unbekannt*, *widerrufen*,
*abgelaufen* und *falsches Geheimnis*.

---

## Ablauf und Widerruf

- **Ablauf** wird beim Erstellen gewählt (30/90/365 Tage oder unbegrenzt).
- **Widerruf** wirkt sofort. Der Datensatz bleibt für die Nachvollziehbarkeit
  erhalten und erscheint als „widerrufen" in der Liste.
- **`last_used_at`** wird bei jedem Aufruf fortgeschrieben — daran erkennt man
  Tokens, die niemand mehr braucht.
- Ein Token mit leeren Scopes erscheint als „ohne Rechte". Das kann nur bei
  Tokens aus dem alten Modell vorkommen (vor Migration `0002`); sie sind
  wirkungslos und sollten neu ausgestellt werden.

---

## Noch nicht enthalten

Bewusst weggelassen, weil die richtige Antwort vom Einsatz abhängt:

- **Ratenbegrenzung.** Für den Anfang genügt meist der Reverse-Proxy
  (nginx `limit_req`, Traefik-Middleware). In der Anwendung ginge
  `tower_governor`.
- **Blättern.** `list` liefert alles. Ab einigen hundert Einträgen
  `LIMIT`/`OFFSET` oder Cursor ergänzen.
- **CORS.** Nicht eingeschaltet, weil die Schnittstelle für
  Server-zu-Server-Aufrufe gedacht ist. Für Browser-Clients
  `tower_http::cors` in `src/app.rs` ergänzen.
- **Schreibzugriff auf Benutzer.** `users:read` ist bewusst nur lesend. Konten
  legt ein Mensch in der Oberfläche an.
