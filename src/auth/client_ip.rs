//! Ermitteln der Client-Adresse.
//!
//! Gebraucht wird sie fuer die Anmeldebremse in `domain::login_attempt`.
//!
//! Hinter einem Reverse-Proxy steht in der TCP-Verbindung nur der Proxy. Die
//! echte Adresse liefert dann `X-Forwarded-For` — allerdings kann diesen Kopf
//! jeder Client frei setzen. Ihm zu glauben ist deshalb NUR richtig, wenn ein
//! Proxy davorsteht, der ihn ueberschreibt. Genau das schaltet `TRUST_PROXY`.

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use std::convert::Infallible;
use std::net::SocketAddr;

use crate::state::AppState;

/// Die Adresse des Clients, soweit ermittelbar.
///
/// Schlaegt nie fehl: Laesst sich keine Adresse bestimmen, ist der Wert `None`
/// und die Anmeldebremse greift nur noch pro Konto. Ein Handler benutzt ihn so:
///
/// ```ignore
/// async fn login(ClientIp(ip): ClientIp) { … }
/// ```
pub struct ClientIp(pub Option<String>);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // ConnectInfo liegt als Extension am Request — sie fehlt, wenn der
        // Server ohne `into_make_service_with_connect_info` gestartet wurde
        // (zum Beispiel in Tests).
        let connection = parts.extensions.get::<ConnectInfo<SocketAddr>>();
        Ok(ClientIp(resolve(
            &parts.headers,
            connection,
            state.config.trust_proxy,
        )))
    }
}

/// Liefert die Adresse als Zeichenkette, oder `None`, wenn sie nicht zu
/// ermitteln ist. `None` ist unkritisch: Dann greift nur die Bremse pro Konto.
pub fn resolve(
    headers: &HeaderMap,
    connection: Option<&ConnectInfo<SocketAddr>>,
    trust_proxy: bool,
) -> Option<String> {
    if trust_proxy {
        // Der erste Eintrag ist der urspruengliche Client, danach folgen die Proxys.
        if let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return Some(forwarded.to_string());
        }
    }

    connection.map(|ConnectInfo(addr)| addr.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn forwarded_header(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        h
    }

    fn connection(s: &str) -> ConnectInfo<SocketAddr> {
        ConnectInfo(s.parse().unwrap())
    }

    #[test]
    fn ohne_proxy_zaehlt_die_verbindung() {
        let v = connection("203.0.113.5:44321");
        // Selbst wenn jemand den Kopf mitschickt: ohne TRUST_PROXY zaehlt er nicht.
        let ip = resolve(&forwarded_header("1.2.3.4"), Some(&v), false);
        assert_eq!(ip.as_deref(), Some("203.0.113.5"));
    }

    #[test]
    fn mit_proxy_zaehlt_der_erste_eintrag() {
        let v = connection("10.0.0.1:8080");
        let ip = resolve(&forwarded_header("203.0.113.5, 10.0.0.1"), Some(&v), true);
        assert_eq!(ip.as_deref(), Some("203.0.113.5"));
    }

    #[test]
    fn ohne_kopf_faellt_es_auf_die_verbindung_zurueck() {
        let v = connection("203.0.113.9:1234");
        assert_eq!(
            resolve(&HeaderMap::new(), Some(&v), true).as_deref(),
            Some("203.0.113.9")
        );
    }

    #[test]
    fn ohne_alles_gibt_es_keine_adresse() {
        assert_eq!(resolve(&HeaderMap::new(), None, true), None);
    }
}
