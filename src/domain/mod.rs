//! Fachlogik. Diese Schicht kennt WEDER Axum NOCH Templates —
//! sie nimmt einfache Typen entgegen und gibt einfache Typen zurueck.
//!
//! Das ist die wichtigste Regel im Projekt: Wenn eine Funktion hier ein
//! `Request`, ein `StatusCode` oder ein Template beruehrt, gehoert sie
//! stattdessen nach `src/web/` oder `src/api/`.

pub mod api_token;
pub mod login_versuch;
pub mod note;
pub mod user;
