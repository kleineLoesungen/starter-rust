//! Benutzerliste fuer Maschinen-Clients. Nur lesend, ohne Passwortdaten.
//!
//! Braucht den Scope `users:read`.

use axum::Json;
use axum::extract::State;

use crate::api::v1::Data;
use crate::auth::extract::UsersRead;
use crate::domain::user::{self, User};
use crate::error::ApiError;
use crate::state::AppState;

pub async fn list(
    State(state): State<AppState>,
    _: UsersRead,
) -> Result<Json<Data<Vec<User>>>, ApiError> {
    let users = user::list(&state.db, None).await?;
    Ok(Data::new(users))
}
