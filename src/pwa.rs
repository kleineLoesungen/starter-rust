//! Installierbare App (PWA): Manifest, Service Worker, Offline-Seite.
//!
//! Was davon konfigurierbar ist, kommt aus Umgebungsvariablen:
//! `APP_NAME`, `APP_SHORT_NAME`, `PWA_THEME_COLOR`, `PWA_BACKGROUND_COLOR`.
//! Die Icons sind Dateien unter `static/icons/` (erzeugt von `scripts/icons.sh`).
//!
//! **Was der Service Worker bewusst NICHT tut:** Seiten zwischenspeichern.
//! Die Anwendung ist angemeldet und serverseitig gerendert — eine gecachte
//! Seite koennte veraltete Daten zeigen oder, schlimmer, nach dem Abmelden
//! noch den Inhalt des vorherigen Benutzers. Deshalb gilt:
//!
//! * Seiten: immer vom Server. Ohne Netz erscheint die Offline-Seite.
//! * `/static/…`: vom Server, ohne Netz aus dem Zwischenspeicher.
//! * API, htmx-Fragmente, Formulare: gar nicht angefasst.

use axum::Router;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use crate::error::WebError;
use crate::state::AppState;
use crate::templates::{Layout, OfflinePage, branding, render};

pub fn routes() -> Router<AppState> {
    // Jeder Serverstart bekommt eine neue Cache-Version. Der Browser erkennt
    // daran einen geaenderten Service Worker, installiert ihn neu und wirft
    // den alten Zwischenspeicher weg — nach einem Deployment gibt es also nie
    // ein veraltetes Stylesheet.
    let version = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();

    Router::new()
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(move || service_worker(version)))
        .route("/offline", get(offline))
}

async fn manifest() -> Response {
    let brand = branding();

    let body = json!({
        "id": "/",
        "name": brand.name,
        "short_name": brand.short_name,
        "lang": "de",
        "start_url": "/",
        "scope": "/",
        "display": "standalone",
        "theme_color": brand.theme_color,
        "background_color": brand.background_color,
        "icons": [
            { "src": "/static/icons/icon-192.png", "sizes": "192x192", "type": "image/png" },
            { "src": "/static/icons/icon-512.png", "sizes": "512x512", "type": "image/png" },
            { "src": "/static/icons/icon-maskable-512.png", "sizes": "512x512",
              "type": "image/png", "purpose": "maskable" },
            { "src": "/static/icons/icon.svg", "sizes": "any", "type": "image/svg+xml" }
        ]
    });

    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        body.to_string(),
    )
        .into_response()
}

async fn service_worker(version: u64) -> Response {
    let script = SERVICE_WORKER.replace("__VERSION__", &version.to_string());
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            // Der Browser soll bei jedem Laden nachfragen, ob sich der Worker
            // geaendert hat. Sonst haengt ein altes Deployment tagelang fest.
            (header::CACHE_CONTROL, "no-cache"),
        ],
        script,
    )
        .into_response()
}

async fn offline() -> Result<Response, WebError> {
    render(OfflinePage {
        layout: Layout::new("Keine Verbindung", None, "/offline"),
    })
}

/// Der Service Worker. Liegt als Konstante hier und nicht unter `static/`,
/// weil er unter `/sw.js` ausgeliefert werden MUSS: Ein Worker darf nur Seiten
/// unterhalb seines eigenen Pfads steuern, `/static/sw.js` saehe also nur
/// `/static/…`.
const SERVICE_WORKER: &str = r#"// Erzeugt von src/pwa.rs — hier nichts aendern, es wird ueberschrieben.
const CACHE = "starter-__VERSION__";

// Was ohne Netz verfuegbar sein muss: die Offline-Seite und ihr Aussehen.
const PRECACHE = [
  "/offline",
  "/static/app.css",
  "/static/htmx.min.js",
  "/static/favicon.svg",
  "/static/icons/icon-192.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE)
      .then((cache) => cache.addAll(PRECACHE))
      .then(() => self.skipWaiting())
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys()
      .then((names) => Promise.all(
        names.filter((name) => name !== CACHE).map((name) => caches.delete(name))
      ))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  // Seiten: nie aus dem Speicher. Ohne Netz die Offline-Seite.
  if (request.mode === "navigate") {
    event.respondWith(fetch(request).catch(() => caches.match("/offline")));
    return;
  }

  // Statische Dateien: frisch vom Server, ohne Netz aus dem Speicher.
  if (url.pathname.startsWith("/static/")) {
    event.respondWith(
      fetch(request)
        .then((response) => {
          if (response.ok) {
            const copy = response.clone();
            caches.open(CACHE).then((cache) => cache.put(request, copy));
          }
          return response;
        })
        .catch(() => caches.match(request))
    );
  }

  // Alles andere (API, htmx-Fragmente) bleibt unangetastet.
});
"#;
