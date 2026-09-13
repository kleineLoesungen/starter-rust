//! Rollen und deren Rangfolge.
//!
//! Es gibt genau drei Rollen. Wer eine vierte braucht, ergaenzt sie hier,
//! im Postgres-Enum `user_role` (neue Migration mit `ALTER TYPE ... ADD VALUE`)
//! und in `rank()`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Moderator,
    Admin,
}

impl Role {
    /// Hoeherer Rang schliesst niedrigere Raenge ein.
    pub fn rank(self) -> u8 {
        match self {
            Role::User => 1,
            Role::Moderator => 2,
            Role::Admin => 3,
        }
    }

    /// `true`, wenn diese Rolle mindestens so viel darf wie `required`.
    pub fn at_least(self, required: Role) -> bool {
        self.rank() >= required.rank()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Moderator => "moderator",
            Role::Admin => "admin",
        }
    }

    /// Beschriftung fuer die Oberflaeche.
    pub fn label(self) -> &'static str {
        match self {
            Role::User => "Benutzer",
            Role::Moderator => "Moderator",
            Role::Admin => "Administrator",
        }
    }

    pub const ALL: [Role; 3] = [Role::User, Role::Moderator, Role::Admin];
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Role {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Role::User),
            "moderator" => Ok(Role::Moderator),
            "admin" => Ok(Role::Admin),
            other => Err(crate::error::Error::BadRequest(format!(
                "Unbekannte Rolle: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hoehere_rolle_schliesst_niedrigere_ein() {
        assert!(Role::Admin.at_least(Role::User));
        assert!(Role::Admin.at_least(Role::Admin));
        assert!(Role::Moderator.at_least(Role::User));
        assert!(!Role::User.at_least(Role::Moderator));
        assert!(!Role::Moderator.at_least(Role::Admin));
    }
}
