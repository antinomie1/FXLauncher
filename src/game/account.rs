//! Account model, offline UUID generation, and the persisted account list (档案).

use crate::env::paths::{default_minecraft_dir, ensure_dir};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Refresh the Minecraft token when it has less than this many seconds left.
const REFRESH_MARGIN_SECS: u64 = 300;

/// Account authentication type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Account {
    /// Offline (non-authenticated / local) account.
    Offline {
        username: String,
        uuid: String,
    },
    /// Microsoft account that owns Minecraft: Java Edition.
    Microsoft {
        username: String,
        uuid: String,
        /// Minecraft services access token, passed to the game.
        access_token: String,
        /// Microsoft OAuth refresh token, used to sign in again without the browser.
        #[serde(default)]
        refresh_token: String,
        /// When `access_token` expires, in seconds since the Unix epoch.
        #[serde(default)]
        expires_at: u64,
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

    pub fn is_microsoft(&self) -> bool {
        matches!(self, Self::Microsoft { .. })
    }

    /// Whether the Minecraft token is expired or about to expire at `now` (Unix seconds).
    pub fn needs_refresh(&self, now: u64) -> bool {
        match self {
            Self::Offline { .. } => false,
            Self::Microsoft { expires_at, .. } => expires_at.saturating_sub(REFRESH_MARGIN_SECS) <= now,
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

/// The launcher's saved accounts and which one launches the game.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountStore {
    pub accounts: Vec<Account>,
    /// Index into `accounts` of the account used to launch.
    #[serde(default)]
    pub selected: usize,
}

impl AccountStore {
    /// Fixed location: `fxlauncher_accounts.json` beside the launcher settings in the default `.minecraft`.
    pub fn default_path() -> PathBuf {
        default_minecraft_dir().join("fxlauncher_accounts.json")
    }

    /// Loads the store, or an empty one when the file is absent or unreadable.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the store; on Unix the file is readable by the owner only, since it holds tokens.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            ensure_dir(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options.open(path)?.write_all(json.as_bytes())
    }

    /// The account used to launch, if any.
    pub fn selected(&self) -> Option<&Account> {
        self.accounts.get(self.selected)
    }

    pub fn position(&self, uuid: &str) -> Option<usize> {
        self.accounts.iter().position(|a| a.uuid() == uuid)
    }

    /// Adds the account, or replaces the saved one with the same UUID, and selects it.
    pub fn upsert(&mut self, account: Account) -> usize {
        let index = match self.position(account.uuid()) {
            Some(i) => {
                self.accounts[i] = account;
                i
            }
            None => {
                self.accounts.push(account);
                self.accounts.len() - 1
            }
        };
        self.selected = index;
        index
    }

    /// Replaces the saved account with the same UUID without changing the selection.
    pub fn update(&mut self, account: Account) {
        if let Some(i) = self.position(account.uuid()) {
            self.accounts[i] = account;
        }
    }

    /// Removes the account with this UUID; the selection stays on the same account where possible.
    pub fn remove(&mut self, uuid: &str) -> Option<Account> {
        let index = self.position(uuid)?;
        let removed = self.accounts.remove(index);
        if self.selected > index || self.selected >= self.accounts.len() {
            self.selected = self.selected.saturating_sub(1);
        }
        Some(removed)
    }
}
