# Mehrstufiger Build. Ergebnis ist ein schlankes Abbild ohne Rust-Werkzeugkette.

# ---------------------------------------------------------------------------
# Stufe 1: Stylesheet bauen
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS css
ARG TAILWIND_VERSION=v4.3.3
ARG TARGETARCH

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY scripts/get-tailwind.sh scripts/
# TARGETARCH von Docker ("amd64"/"arm64") auf die Namen der Release-Dateien abbilden.
RUN case "${TARGETARCH}" in \
      amd64) ASSET=tailwindcss-linux-x64 ;; \
      arm64) ASSET=tailwindcss-linux-arm64 ;; \
      *) echo "Nicht unterstuetzte Architektur: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && curl -sSL --fail -o /usr/local/bin/tailwindcss \
       "https://github.com/tailwindlabs/tailwindcss/releases/download/${TAILWIND_VERSION}/${ASSET}" \
    && chmod +x /usr/local/bin/tailwindcss

# Tailwind durchsucht Templates UND Rust-Quellen nach Klassennamen.
COPY assets/ assets/
COPY templates/ templates/
COPY src/ src/
RUN tailwindcss -i assets/css/app.css -o /out/app.css --minify

# ---------------------------------------------------------------------------
# Stufe 2: Rust uebersetzen
# ---------------------------------------------------------------------------
FROM rust:1.96-slim-bookworm AS build
WORKDIR /build

# Erst nur die Abhaengigkeiten bauen. Diese Schicht bleibt im Cache, solange
# sich Cargo.toml/Cargo.lock nicht aendern — spart bei jedem Build Minuten.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY src/ src/
COPY templates/ templates/
COPY migrations/ migrations/
# Askama liest die Templates zur Compile-Zeit ein, sqlx::migrate! die Migrationen.
# Beide muessen daher VOR dem Build vorliegen.
RUN touch src/main.rs src/lib.rs && cargo build --release --locked

# ---------------------------------------------------------------------------
# Stufe 3: Laufzeit
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home app

WORKDIR /app

COPY --from=build /build/target/release/starter /usr/local/bin/starter
COPY --from=css   /out/app.css                  /app/static/app.css
COPY static/htmx.min.js static/favicon.svg      /app/static/
# Icons fuer die installierbare App — ohne sie verweist das Manifest ins Leere.
COPY static/icons/                              /app/static/icons/

USER app
EXPOSE 3000

ENV BIND_ADDR=0.0.0.0:3000 \
    RUST_LOG=starter=info,tower_http=warn

# Prueft mit, ob die Datenbank erreichbar ist — nicht nur, ob der Prozess laeuft.
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/api/v1/health || exit 1

CMD ["starter"]
