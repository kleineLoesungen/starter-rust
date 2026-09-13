//! Auskunft ueber das benutzte Token. Praktisch, um beim Einrichten zu pruefen,
//! ob das Token stimmt und welche Berechtigungen es hat.

use axum::Json;
use serde_json::json;

use crate::api::v1::Data;
use crate::auth::extract::ApiClient;
use crate::error::ApiError;

pub async fn info(
    ApiClient { token }: ApiClient,
) -> Result<Json<Data<serde_json::Value>>, ApiError> {
    Ok(Data::new(json!({
        "id": token.id,
        "name": token.name,
        "scopes": token.scopes,
        "expires_at": token.expires_at
            .map(|t| t.format(&time::format_description::well_known::Rfc3339))
            .transpose()
            .map_err(|e| crate::error::Error::Internal(anyhow::anyhow!(e)))?,
    })))
}
