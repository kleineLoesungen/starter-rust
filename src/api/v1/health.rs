//! Zustandspruefung. Bewusst ohne Authentifizierung, damit Loadbalancer und
//! Container-Orchestrierung sie abfragen koennen.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Response {
    // Eine echte Abfrage, nicht nur "der Prozess laeuft": Ohne Datenbank
    // ist die Anwendung nicht arbeitsfaehig und soll aus dem Verkehr.
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (
            StatusCode::OK,
            axum::Json(json!({ "status": "ok", "database": "ok" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Zustandspruefung fehlgeschlagen: {e}");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(json!({ "status": "degraded", "database": "unreachable" })),
            )
                .into_response()
        }
    }
}
