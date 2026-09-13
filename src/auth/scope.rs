//! Scopes — was ein App-Token darf.
//!
//! Ein Token gehoert keinem Benutzer und erbt deshalb auch keine Rolle. Die
//! einzige Antwort auf „was darf dieser Maschinen-Client?" sind seine Scopes.
//!
//! Eine neue Berechtigung hinzufuegen:
//!   1. Konstante unten ergaenzen und in `ALL` eintragen
//!   2. Extractor in `src/auth/extract.rs` erzeugen:
//!      `scope_extractor!(BerichteLesen, BERICHTE_READ);`
//!   3. Endpunkt in `docs/API.md` beschreiben

pub const NOTES_READ: &str = "notes:read";
pub const NOTES_WRITE: &str = "notes:write";
pub const USERS_READ: &str = "users:read";

/// Ein Scope mit Beschriftung fuer die Oberflaeche.
pub struct Scope {
    pub name: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

/// Alle vergebbaren Scopes. Die Reihenfolge ist die Anzeigereihenfolge.
pub const ALL: &[Scope] = &[
    Scope {
        name: NOTES_READ,
        label: "Notizen lesen",
        description: "Notizen auflisten und einzeln abrufen",
    },
    Scope {
        name: NOTES_WRITE,
        label: "Notizen schreiben",
        description: "Notizen anlegen, ändern und löschen",
    },
    Scope {
        name: USERS_READ,
        label: "Benutzer lesen",
        description: "Benutzerliste abrufen (ohne Passwortdaten)",
    },
];

/// Prueft, ob ein angefragter Scope ueberhaupt existiert. Verhindert Tokens
/// mit Tippfehlern im Scope, die dann still nichts duerfen.
pub fn is_known(name: &str) -> bool {
    ALL.iter().any(|s| s.name == name)
}

pub fn label_for(name: &str) -> &str {
    ALL.iter()
        .find(|s| s.name == name)
        .map_or(name, |s| s.label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bekannte_und_unbekannte_scopes() {
        assert!(is_known(NOTES_READ));
        assert!(is_known("users:read"));
        assert!(!is_known("notes:reed"));
        assert!(!is_known(""));
    }

    #[test]
    fn beschriftung_faellt_auf_den_namen_zurueck() {
        assert_eq!(label_for(NOTES_READ), "Notizen lesen");
        assert_eq!(label_for("unbekannt"), "unbekannt");
    }
}
