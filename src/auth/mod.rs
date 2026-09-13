//! Authentifizierung und Autorisierung.
//!
//! Zwei Wege fuehren zu einer Identitaet:
//!
//! * **Session-Cookie** — die HTML-Oberflaeche in `src/web/`. Siehe [`session`].
//! * **Bearer-Token** — die JSON-API in `src/api/`. Siehe [`token`].
//!
//! Beide muenden in denselben `User` und dieselbe [`Role`]-Pruefung.

pub mod client_ip;
pub mod csrf;
pub mod extract;
pub mod password;
pub mod role;
pub mod scope;
pub mod session;
pub mod token;

pub use role::Role;
