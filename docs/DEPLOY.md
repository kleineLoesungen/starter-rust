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
