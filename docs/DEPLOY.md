# Produktivbetrieb

---

## Vor dem ersten Ausrollen

Diese Liste durchgehen:

- [ ] `COOKIE_SECURE=true` — sonst reist das Session-Cookie unverschlüsselt.
- [ ] `PUBLIC_URL` auf die echte Adresse setzen (`https://…`). Die
      Herkunftsprüfung gegen CSRF vergleicht dagegen; ein falscher Wert lässt
      Formulare mit 403 scheitern.
- [ ] HTTPS davor. Die Anwendung terminiert selbst kein TLS — das macht der
      Reverse-Proxy (nginx, Caddy, Traefik) oder der Loadbalancer.
- [ ] `TRUST_PROXY=true`, sobald ein Reverse-Proxy davorsteht. Sonst sieht die
      Anwendung nur dessen Adresse, und die Anmeldebremse zählt alle Besucher
      als einen. Ohne Proxy muss es `false` bleiben, sonst lässt sich die
      Bremse pro IP mit einem selbst gesetzten `X-Forwarded-For` umgehen.
- [ ] `DATABASE_URL` mit `?sslmode=require`, wenn die Datenbank über ein
      Netz erreichbar ist.
- [ ] `SMTP_URL` und `MAIL_FROM` gesetzt, danach unter **System** eine Testmail
      verschickt. Ohne `SMTP_URL` geht keine Mail raus — sie erscheinen nur im Log.
- [ ] `APP_NAME`, `APP_SHORT_NAME`, `PWA_THEME_COLOR` gesetzt. Die Werte landen
      im Manifest; eine bereits installierte App übernimmt Namensänderungen je
      nach Gerät erst verzögert oder nach Neuinstallation.
- [ ] Datenbanksicherung eingerichtet **und einmal zurückgespielt**.
- [ ] `RUST_LOG=starter=info` — `debug` protokolliert deutlich mehr.
- [ ] Erstes Konto anlegen und prüfen, dass es Administrator ist.

---

## Container starten

```bash
docker build -t starter:latest .

docker run -d \
  --name starter \
  -p 3000:3000 \
  -e DATABASE_URL="postgres://benutzer:passwort@db.example.com/starter?sslmode=require" \
  -e PUBLIC_URL="https://app.example.com" \
  -e COOKIE_SECURE=true \
  -e RUST_LOG=starter=info \
  --restart unless-stopped \
  starter:latest
```

Das Abbild bringt einen `HEALTHCHECK` mit, der `/api/v1/health` abfragt — und
damit auch die Datenbankverbindung, nicht nur den Prozess.

**Passwörter nicht als `-e` übergeben**, sie stehen sonst in `docker inspect`
und in der Shell-Historie. Docker Secrets, Kubernetes Secrets oder eine
`--env-file` mit engen Dateirechten benutzen.

---

## E-Mail

Der Versandweg hängt allein an `SMTP_URL`:

```bash
# TLS direkt (Port 465)
SMTP_URL=smtps://benutzer:passwort@mail.example.com
# STARTTLS (Port 587)
SMTP_URL=smtp://benutzer:passwort@mail.example.com:587?tls=required
```

Sonderzeichen im Passwort URL-kodieren (`@` → `%40`, `:` → `%3A`, `/` → `%2F`),
sonst wird die Adresse falsch zerlegt.

`MAIL_FROM` in Anführungszeichen setzen, wenn es einen Namen enthält —
`MAIL_FROM="Mein Verein <noreply@example.com>"`. Ohne Anführungszeichen bricht
der Start mit einer Meldung ab; das ist Absicht, siehe unten.

Unter **System** (`/admin/system`) zeigt die Anwendung den wirksamen Versandweg
— ohne Passwort — und verschickt auf Knopfdruck eine Testmail. Schlägt sie
fehl, steht dort die Meldung des Mailservers.

---

## Installierbare App (PWA)

Die Anwendung bringt Manifest, Icons und einen Service Worker mit. Browser
bieten die Installation aber **nur über HTTPS** an (oder auf `localhost`). Im
lokalen Netz per `http://192.168…` gibt es weder Installation noch
Offline-Seite — das ist eine Vorgabe der Browser, kein Fehler.

Der Service Worker speichert **keine Seiten** zwischen, nur Stylesheet, htmx
und Icons. Ohne Netz erscheint eine Offline-Seite. Das ist bewusst so: Eine
angemeldete, serverseitig gerenderte Anwendung aus dem Speicher zu bedienen,
hieße veraltete Daten zeigen — oder nach dem Abmelden noch die Inhalte des
vorherigen Benutzers.

Eigene Icons: SVG unter `assets/icons/` ersetzen, dann `./scripts/icons.sh`.

---

## `.env` im Betrieb

Eine fehlerhafte `.env` bricht den Start ab, statt still ignoriert zu werden.
Der Hintergrund: Der Parser hört an der ersten kaputten Zeile auf, und alle
Zeilen danach fehlen ebenfalls. Ein ungequotetes `MAIL_FROM=Name <a@b>` hätte
so still ein späteres `COOKIE_SECURE=true` verschluckt.

---

## Umgekehrter Proxy

nginx als Beispiel:

```nginx
server {
    listen 443 ssl http2;
    server_name app.example.com;

    ssl_certificate     /etc/letsencrypt/live/app.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/app.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host              $host;
        proxy_set_header X-Real-IP         $remote_addr;
        proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # Stylesheet und htmx dürfen lange im Cache liegen.
    location /static/ {
        proxy_pass http://127.0.0.1:3000;
        expires 7d;
        add_header Cache-Control "public";
    }
}
```

---

## Skalierung

Die Anwendung ist zustandslos: Sessions liegen in Postgres, hochgeladene
Dateien gibt es (noch) nicht. Mehrere Instanzen können ohne klebrige Sitzungen
hinter demselben Loadbalancer laufen.

Was dabei zu beachten ist:

- **Verbindungen.** `DB_MAX_CONNECTIONS` × Anzahl Instanzen muss unter
  `max_connections` der Datenbank bleiben. Vorgabe ist 10 pro Instanz.
- **Migrationen.** Laufen beim Start jeder Instanz, sind aber durch eine Sperre
  abgesichert — gleichzeitiger Start ist unkritisch.
- **Aufräumen abgelaufener Sessions.** Läuft in jeder Instanz stündlich. Bei
  vielen Instanzen unnötige Doppelarbeit, aber harmlos.

---

## Strenge Content-Security-Policy einschalten

Standardmäßig setzt die Anwendung nur `X-Content-Type-Options: nosniff`.
Eine strenge CSP fehlt bewusst — die Oberfläche benutzt htmx-Attribute wie
`hx-on::after-request` und einige `onclick`-Attribute, die eine CSP ohne
`unsafe-inline`/`unsafe-eval` blockieren würde.

Wer eine CSP braucht, geht so vor:

1. Inline-Skripte in Dateien unter `static/` auslagern (Theme-Umschalter,
   Toast-Beobachter, Styleguide-Regler).
2. `onclick`-Attribute in `templates/partials/token_created.html` und
   `templates/pages/styleguide.html` durch Ereignis-Listener in diesen Dateien
   ersetzen.
3. `hx-on::…` durch `htmx.on(...)` in einer eigenen Datei ersetzen.
4. In `src/app.rs` bei `security_headers()` ergänzen:

```rust
SetResponseHeaderLayer::overriding(
    HeaderName::from_static("content-security-policy"),
    HeaderValue::from_static(
        "default-src 'self'; script-src 'self'; style-src 'self'; \
         img-src 'self' data:; frame-ancestors 'none'; base-uri 'self'"
    ),
)
```

Weitere lohnende Kopfzeilen: `X-Frame-Options: DENY`,
`Referrer-Policy: strict-origin-when-cross-origin`,
`Strict-Transport-Security` (besser im Proxy).

---

## Betrieb

**Logs** gehen als strukturierter Text nach stdout — `docker logs`,
`journalctl` oder der Log-Sammler der Plattform.

```bash
RUST_LOG=starter=debug,sqlx=info    # vorübergehend ausführlicher, inkl. SQL
```

**Zustand prüfen:**

```bash
curl -fsS https://app.example.com/api/v1/health
```

**Sicherung** — die Anwendung selbst hält keinen Zustand, es genügt die
Datenbank:

```bash
pg_dump "$DATABASE_URL" --format=custom --file=sicherung-$(date +%F).dump
```

Wiederherstellen mit `pg_restore`. Mindestens einmal geübt haben.

---

## Aktualisieren

```bash
docker build -t starter:neu .
docker stop starter && docker rm starter
docker run -d --name starter … starter:neu
```

Migrationen laufen beim Start automatisch. Bei mehreren Instanzen erst eine
umstellen, Zustand prüfen, dann die übrigen.

**Rückwärtskompatibel migrieren.** Während einer rollenden Umstellung laufen
alte und neue Version gleichzeitig. Eine Spalte erst hinzufügen und befüllen,
sie erst in einer späteren Version entfernen — nie beides in einem Schritt.
