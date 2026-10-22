//! Account model, offline UUID generation, and authentication representations.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Account authentication type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Account {
    /// Offline (non-authenticated / local) account.
    Offline {
        username: String,
        uuid: String,
    },
    /// Microsoft / Xbox Live authenticated account.
    Microsoft {
        username: String,
        uuid: String,
        access_token: String,
    },
}

impl Account {
    /// Creates a new offline account with standard Minecraft offline UUID.
    pub fn new_offline(username: impl Into<String>) -> Self {
        let username = username.into();
        let uuid = offline_uuid(&username).to_string();
        Self::Offline { username, uuid }
    }

    /// Returns the display player name.
    pub fn username(&self) -> &str {
        match self {
            Self::Offline { username, .. } => username,
            Self::Microsoft { username, .. } => username,
        }
    }

    /// Returns the player UUID string.
    pub fn uuid(&self) -> &str {
        match self {
            Self::Offline { uuid, .. } => uuid,
            Self::Microsoft { uuid, .. } => uuid,
        }
    }

    /// Returns the access token (or `"0"` / dummy token for offline mode).
    pub fn access_token(&self) -> &str {
        match self {
            Self::Offline { .. } => "0",
            Self::Microsoft { access_token, .. } => access_token,
        }
    }

    /// Returns the user type (`"legacy"` for offline, `"msa"` for Microsoft).
    pub fn user_type(&self) -> &str {
        match self {
            Self::Offline { .. } => "legacy",
            Self::Microsoft { .. } => "msa",
        }
    }

    /// Checks if player username conforms to vanilla Minecraft restrictions (1-16 chars, [a-zA-Z0-9_]).
    pub fn validate_username(name: &str) -> bool {
        if name.is_empty() || name.len() > 16 {
            return false;
        }
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
}

/// Generates a standard Minecraft offline UUID (Java `UUID.nameUUIDFromBytes("OfflinePlayer:username")`).
pub fn offline_uuid(username: &str) -> Uuid {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{username}").as_bytes());
    let mut hash = hasher.finalize();

    // RFC 4122 version 3
    hash[6] = (hash[6] & 0x0f) | 0x30;
    // RFC 4122 variant IETF
    hash[8] = (hash[8] & 0x3f) | 0x80;

    Uuid::from_bytes(hash.into())
}
