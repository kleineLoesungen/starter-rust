//! Session-Handling fuer die HTML-Oberflaeche.
//!
//! Die Session liegt in Postgres (Tabelle `tower_sessions`), im Cookie steht
//! nur eine zufaellige ID. Dadurch ist ein Logout serverseitig sofort wirksam
//! und es landen keine Nutzdaten beim Client.

use crate::error::Error;
use tower_sessions::Session;
use uuid::Uuid;

const USER_ID_KEY: &str = "user_id";

/// Meldet einen Benutzer an.
///
/// Wichtig ist `cycle_id()`: Es vergibt eine neue Session-ID und verhindert
/// damit Session Fixation — ein Angreifer, der dem Opfer vorher eine ID
/// untergeschoben hat, kann sie nach dem Login nicht weiterbenutzen.
pub async fn login(session: &Session, user_id: Uuid) -> Result<(), Error> {
    session.cycle_id().await.map_err(session_error)?;
    session
        .insert(USER_ID_KEY, user_id)
        .await
        .map_err(session_error)?;
    Ok(())
}

/// Meldet ab und loescht die Session serverseitig.
pub async fn logout(session: &Session) -> Result<(), Error> {
    session.flush().await.map_err(session_error)?;
    Ok(())
}

pub async fn current_user_id(session: &Session) -> Result<Option<Uuid>, Error> {
    let id = session
        .get::<Uuid>(USER_ID_KEY)
        .await
        .map_err(session_error)?;
    Ok(id)
}

fn session_error(e: tower_sessions::session::Error) -> Error {
    Error::Internal(anyhow::anyhow!("Session-Fehler: {e}"))
}
