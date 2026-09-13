# Alle wiederkehrenden Handgriffe an einem Ort.
# `just` ohne Argument zeigt die Liste.
#
# Installation von just:  cargo install just   (oder: brew install just)

# .env auch fuer Rezepte laden (Tests brauchen DATABASE_URL).
set dotenv-load := true

_default:
    @just --list --unsorted

# --- Erster Start ----------------------------------------------------------

# Einmalige Einrichtung: Tailwind holen, .env anlegen, Datenbank starten.
setup:
    ./scripts/get-tailwind.sh
    [ -f .env ] || cp .env.example .env
    just db-up
    just css
    @echo ""
    @echo "Fertig. Weiter mit:  just dev"

# --- Entwicklung -----------------------------------------------------------

# Server starten. Vorher wird das Stylesheet frisch gebaut.
dev: css
    cargo run

# Stylesheet einmalig bauen.
css:
    .bin/tailwindcss -i assets/css/app.css -o static/app.css

# Stylesheet beobachten (zweites Terminal, neben `just dev`).
css-watch:
    .bin/tailwindcss -i assets/css/app.css -o static/app.css --watch

# Server bei Codeaenderung neu starten (benoetigt: cargo install cargo-watch).
watch:
    cargo watch -x run

# --- Datenbank -------------------------------------------------------------

# Lokale Postgres im Container starten.
db-up:
    docker compose up -d db
    @echo "Warte auf Postgres ..."
    @until docker compose exec -T db pg_isready -U starter -q; do sleep 1; done
    @echo "Postgres bereit."

db-down:
    docker compose down

# ACHTUNG: loescht alle lokalen Daten und legt die Datenbank neu an.
db-reset:
    docker compose down -v
    just db-up

# psql-Eingabeaufforderung oeffnen.
db-shell:
    docker compose exec db psql -U starter -d starter

# --- Qualitaet -------------------------------------------------------------

# Alles pruefen, was auch die Auslieferung pruefen wuerde.
check: fmt-check lint test

test:
    cargo test

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

lint:
    cargo clippy --all-targets -- -D warnings

# --- Container -------------------------------------------------------------

# Abbild bauen.
build:
    docker build -t starter:latest .

# Vollstaendig im Container starten (Anwendung + Datenbank).
up:
    docker compose --profile app up --build
