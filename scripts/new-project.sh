#!/usr/bin/env bash
# Macht aus einer Kopie des Kits ein eigenes Projekt.
#
#   git clone <kit> vereinsportal
#   cd vereinsportal
#   ./scripts/new-project.sh vereinsportal "Vereinsportal"
#
# Warum es das braucht: Das Kit heisst an rund zwanzig Stellen "starter".
# Zwei davon sind stille Fallen, wenn man sie vergisst:
#
#   * Der Compose-Projektname ist fest. Zwei Projekte mit demselben Namen
#     teilen sich Container UND das Datenbank-Volume — das zweite Projekt
#     sieht und ueberschreibt die Daten des ersten, ohne jede Fehlermeldung.
#   * Der Standard-Logfilter lautet "starter=info". Nach dem Umbenennen der
#     Crate erscheinen sonst keine eigenen Log-Meldungen mehr.
#
# Ersetzt werden nur gezielte, eindeutige Muster — nie blind jedes "starter",
# damit Fliesstext wie "Starter-Kit" unangetastet bleibt.
set -euo pipefail

fehler() { echo "Fehler: $*" >&2; exit 1; }

# --- Argumente ------------------------------------------------------------

NAME="${1:-}"
ANZEIGENAME="${2:-}"
PORT_OFFSET="${3:-}"

if [ -z "$NAME" ]; then
  cat >&2 <<'HILFE'
Aufruf: ./scripts/new-project.sh <name> ["Anzeigename"] [port-offset]

  name          technischer Name: Kleinbuchstaben, Ziffern, Bindestrich,
                z. B. vereinsportal oder mein-verein
  Anzeigename   Name in Titel und Kopfzeile, z. B. "Mein Verein"
                (Vorgabe: name mit grossem Anfangsbuchstaben)
  port-offset   1–99, verschiebt alle Ports um diesen Wert
                (Vorgabe: der erste Wert, bei dem alle Ports frei sind)
HILFE
  exit 1
fi

[[ "$NAME" =~ ^[a-z][a-z0-9-]{1,39}$ ]] \
  || fehler "\"$NAME\" ist kein gültiger Name. Erlaubt: Kleinbuchstaben, Ziffern, Bindestrich; mit Buchstabe beginnend."
[[ "$NAME" != *- ]] || fehler "Der Name darf nicht mit einem Bindestrich enden."
[ "$NAME" != "starter" ] || fehler "Bitte einen eigenen Namen wählen, nicht \"starter\"."

# Rust-Bezeichner (use vereinsportal::…) und Datenbankname duerfen keinen Bindestrich enthalten.
IDENT="${NAME//-/_}"

if [ -z "$ANZEIGENAME" ]; then
  ANZEIGENAME="$(printf '%s' "${NAME:0:1}" | tr '[:lower:]' '[:upper:]')${NAME:1}"
fi
# Der Kurzname unter dem App-Icon hat kaum Platz.
KURZNAME="${ANZEIGENAME:0:12}"

# --- Voraussetzungen ------------------------------------------------------

cd "$(dirname "$0")/.."

grep -q '^name = "starter"$' Cargo.toml \
  || fehler "Cargo.toml heißt nicht mehr \"starter\" — das Projekt wurde offenbar schon umbenannt."

if git rev-parse --git-dir >/dev/null 2>&1 && [ -n "$(git status --porcelain)" ]; then
  fehler "Es gibt nicht committete Änderungen. Bitte erst committen, damit sich die Umbenennung mit 'git diff' nachvollziehen lässt."
fi

command -v perl >/dev/null 2>&1 || fehler "perl wird benötigt (auf macOS und Linux üblicherweise vorhanden)."

# --- Ports ----------------------------------------------------------------

port_belegt() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }

if [ -z "$PORT_OFFSET" ]; then
  for n in $(seq 1 99); do
    if ! port_belegt $((3000 + n)) && ! port_belegt $((55432 + n)) \
       && ! port_belegt $((51025 + n)) && ! port_belegt $((58025 + n)); then
      PORT_OFFSET=$n
      break
    fi
  done
  [ -n "$PORT_OFFSET" ] || fehler "Kein freier Port-Bereich gefunden. Offset als drittes Argument angeben."
fi
[[ "$PORT_OFFSET" =~ ^[0-9]+$ ]] && [ "$PORT_OFFSET" -ge 1 ] && [ "$PORT_OFFSET" -le 99 ] \
  || fehler "port-offset muss zwischen 1 und 99 liegen."

P_APP=$((3000 + PORT_OFFSET))
P_DB=$((55432 + PORT_OFFSET))
P_SMTP=$((51025 + PORT_OFFSET))
P_UI=$((58025 + PORT_OFFSET))

# --- Ersetzen -------------------------------------------------------------

# ersetze <datei> <von> <nach> — woertlich, ohne regulaere Ausdruecke.
# Die Werte laufen ueber Umgebungsvariablen, damit keine Sonderzeichen
# (Leerzeichen, <, >, @) in den Perl-Befehl geraten.
ersetze() {
  local datei="$1"
  [ -f "$datei" ] || return 0
  VON="$2" NACH="$3" perl -pi -e 's/\Q$ENV{VON}\E/$ENV{NACH}/g' "$datei"
}

echo "Benenne um: starter → $NAME  (Rust: $IDENT, Anzeige: \"$ANZEIGENAME\")"

# Crate
ersetze Cargo.toml 'name = "starter"' "name = \"$NAME\""

# Rust-Code: Crate-Pfad, Logfilter, Cookie, Cache
for f in src/main.rs tests/*.rs; do
  VON="starter::" NACH="$IDENT::" perl -pi -e 's/\bstarter::/$ENV{NACH}/g' "$f"
done
ersetze src/main.rs '"starter=info' "\"$IDENT=info"
ersetze src/app.rs '"starter_session"' "\"${IDENT}_session\""
ersetze src/pwa.rs '"starter-__VERSION__"' "\"$NAME-__VERSION__\""

# Vorgabe-Branding (greift, wenn APP_NAME nicht gesetzt ist)
ersetze src/config.rs 'name: "Starter".into()' "name: \"$ANZEIGENAME\".into()"
ersetze src/config.rs 'short_name: "Starter".into()' "short_name: \"$KURZNAME\".into()"
ersetze src/config.rs 'from: "Starter <noreply@localhost>".into()' "from: \"$ANZEIGENAME <noreply@localhost>\".into()"

# Docker Compose
ersetze compose.yaml 'name: starter' "name: $NAME"
ersetze compose.yaml 'POSTGRES_USER: starter' "POSTGRES_USER: $IDENT"
ersetze compose.yaml 'POSTGRES_PASSWORD: starter' "POSTGRES_PASSWORD: $IDENT"
ersetze compose.yaml 'POSTGRES_DB: starter' "POSTGRES_DB: $IDENT"
ersetze compose.yaml 'pg_isready -U starter' "pg_isready -U $IDENT"
ersetze compose.yaml 'postgres://starter:starter@db:5432/starter' "postgres://$IDENT:$IDENT@db:5432/$IDENT"
ersetze compose.yaml 'RUST_LOG: starter=info' "RUST_LOG: $IDENT=info"
ersetze compose.yaml 'MAIL_FROM: Starter <noreply@localhost>' "MAIL_FROM: $ANZEIGENAME <noreply@localhost>"
ersetze compose.yaml '"55432:5432"' "\"$P_DB:5432\""
ersetze compose.yaml '55432 statt 5432' "$P_DB statt 5432"
ersetze compose.yaml '"51025:1025"' "\"$P_SMTP:1025\""
ersetze compose.yaml '"58025:8025"' "\"$P_UI:8025\""
ersetze compose.yaml '"3000:3000"' "\"$P_APP:3000\""
ersetze compose.yaml 'PUBLIC_URL: http://localhost:3000' "PUBLIC_URL: http://localhost:$P_APP"

# Justfile und Dockerfile
ersetze Justfile 'pg_isready -U starter' "pg_isready -U $IDENT"
ersetze Justfile 'psql -U starter -d starter' "psql -U $IDENT -d $IDENT"
ersetze Justfile 'docker build -t starter:latest' "docker build -t $NAME:latest"
ersetze Dockerfile 'release/starter /usr/local/bin/starter' "release/$NAME /usr/local/bin/$NAME"
ersetze Dockerfile 'RUST_LOG=starter=' "RUST_LOG=$IDENT="
ersetze Dockerfile 'CMD ["starter"]' "CMD [\"$NAME\"]"

# Beispielkonfiguration — und eine vorhandene .env gleich mit, sonst zeigte
# sie weiter auf die Datenbank des Kits.
for f in .env.example .env; do
  ersetze "$f" 'postgres://starter:starter@localhost:55432/starter' "postgres://$IDENT:$IDENT@localhost:$P_DB/$IDENT"
  ersetze "$f" 'BIND_ADDR=0.0.0.0:3000' "BIND_ADDR=0.0.0.0:$P_APP"
  ersetze "$f" 'PUBLIC_URL=http://localhost:3000' "PUBLIC_URL=http://localhost:$P_APP"
  ersetze "$f" 'SMTP_URL=smtp://localhost:51025' "SMTP_URL=smtp://localhost:$P_SMTP"
  ersetze "$f" 'APP_NAME=Starter' "APP_NAME=\"$ANZEIGENAME\""
  ersetze "$f" 'APP_SHORT_NAME=Starter' "APP_SHORT_NAME=\"$KURZNAME\""
  ersetze "$f" 'MAIL_FROM="Starter <noreply@localhost>"' "MAIL_FROM=\"$ANZEIGENAME <noreply@localhost>\""
  ersetze "$f" 'RUST_LOG=starter=' "RUST_LOG=$IDENT="
  ersetze "$f" '#   starter=debug' "#   $IDENT=debug"
done

# Ports und Namen in Doku und Kommentaren, damit weder Menschen noch
# KI-Assistenten auf die Werte des Kits verwiesen werden.
for f in README.md CLAUDE.md docs/*.md .env.example Justfile compose.yaml src/mail/mod.rs; do
  ersetze "$f" 'localhost:58025' "localhost:$P_UI"
  ersetze "$f" 'Port 58025' "Port $P_UI"
  ersetze "$f" 'localhost:3000' "localhost:$P_APP"
done
ersetze docs/DEPLOY.md 'RUST_LOG=starter=' "RUST_LOG=$IDENT="
ersetze docs/DEPLOY.md 'starter:latest' "$NAME:latest"
ersetze docs/DEPLOY.md 'starter:neu' "$NAME:neu"
ersetze docs/DEPLOY.md '--name starter' "--name $NAME"
ersetze docs/DEPLOY.md 'docker stop starter && docker rm starter' "docker stop $NAME && docker rm $NAME"
ersetze docs/DEPLOY.md 'db.example.com/starter?' "db.example.com/$IDENT?"

# Cargo.lock nachziehen: Nur der Name des eigenen Pakets aendert sich,
# keine Abhaengigkeit wird aktualisiert. Wichtig fuer "cargo build --locked"
# im Dockerfile.
if command -v cargo >/dev/null 2>&1; then
  cargo metadata --format-version 1 --offline >/dev/null 2>&1 \
    || cargo metadata --format-version 1 >/dev/null

  # rustfmt sortiert Importe alphabetisch. "use starter::…" stand vor
  # "use tower::…", "use vereinsportal::…" steht dahinter — ohne diesen
  # Schritt scheitert "just check" im frisch angelegten Projekt an der
  # Formatierung.
  cargo fmt
fi

# --- Kontrolle ------------------------------------------------------------

# Alles, was nach Code aussieht und noch "starter" heisst, ist ein Fehler dieses Skripts.
REST="$(grep -rnE '\bstarter\b|starter_session|starter::|starter=' \
          Cargo.toml compose.yaml Justfile Dockerfile .env.example src tests 2>/dev/null \
        | grep -v 'scripts/new-project.sh' || true)"
if [ -n "$REST" ]; then
  echo ""
  echo "WARNUNG: Diese Stellen heißen noch \"starter\":" >&2
  echo "$REST" >&2
fi

cat <<FERTIG

Fertig. Ports dieses Projekts (Offset $PORT_OFFSET):

  Anwendung   http://localhost:$P_APP
  Postgres    localhost:$P_DB
  Mailpit     http://localhost:$P_UI   (SMTP $P_SMTP)

Nächste Schritte:

  git diff --stat              Änderungen ansehen
  just setup                   Tailwind, .env, Datenbank, Mailpit
  just check                   alles muss grün sein
  git commit -am "Projekt $NAME aus dem Kit angelegt"

Danach nach Belieben: README-Titel, Icons (assets/icons/), --brand-hue in
assets/css/theme.css. Zeigt 'git remote -v' noch auf das Kit:
  git remote remove origin
FERTIG
