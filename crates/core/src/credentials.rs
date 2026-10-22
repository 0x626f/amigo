//! Machine-local credential encryption. The separate random key and files are owner-only.
//! This protects against accidental exports, not a compromised user account.
use crate::storage::atomic_write;
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
use zeroize::Zeroizing;
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Credentials {
    /// Preserve encrypted legacy integration data without using it.
    #[serde(flatten)]
    pub legacy: BTreeMap<String, serde_json::Value>,
    pub calendar_urls: BTreeMap<String, String>,
    pub telegram_token: String,
    pub telegram_chat: String,
    pub telegram_enabled: bool,
}
fn key(root: &Path, create: bool) -> Result<Vec<u8>, String> {
    let p = root.join("credentials.key");
    if p.exists() {
        let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
        if bytes.len() != 32 {
            return Err("Invalid credential key.".into());
        }
        return Ok(bytes);
    }
    if !create {
        return Err("Credential key is missing; existing credentials were preserved.".into());
    }
    let mut bytes = vec![0; 32];
    OsRng.fill_bytes(&mut bytes);
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut f = options.open(p).map_err(|e| e.to_string())?;
    f.write_all(&bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
impl Credentials {
    pub fn load(root: &Path) -> Result<Self, String> {
        let p = root.join("credentials.enc");
        if !p.exists() {
            return Ok(Self::default());
        }
        let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
        if bytes.len() < 28 {
            return Err("Credential file is damaged.".into());
        }
        let k = Zeroizing::new(key(root, false)?);
        let cipher =
            Aes256Gcm::new_from_slice(k.as_slice()).map_err(|_| "Invalid credential key")?;
        let plain = Zeroizing::new(
            cipher
                .decrypt(Nonce::from_slice(&bytes[..12]), &bytes[12..])
                .map_err(|_| "Cannot decrypt credentials. Existing files were preserved.")?,
        );
        serde_json::from_slice(plain.as_slice()).map_err(|e| e.to_string())
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        let k = Zeroizing::new(key(root, !root.join("credentials.enc").exists())?);
        let cipher =
            Aes256Gcm::new_from_slice(k.as_slice()).map_err(|_| "Invalid credential key")?;
        let mut nonce = [0; 12];
        OsRng.fill_bytes(&mut nonce);
        let plain = Zeroizing::new(serde_json::to_vec(self).map_err(|e| e.to_string())?);
        let payload = cipher
            .encrypt(Nonce::from_slice(&nonce), plain.as_slice())
            .map_err(|_| "Credential encryption failed")?;
        let mut bytes = nonce.to_vec();
        bytes.extend(payload);
        atomic_write(&root.join("credentials.enc"), &bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_credentials_remain_encrypted_and_survive_settings_updates() {
        let root = std::env::temp_dir().join(crate::model::id());
        let mut credentials: Credentials = serde_json::from_value(serde_json::json!({
            "oauth": {"google_client": "old-client"},
            "tokens": {"old-account": {"access": "old-secret", "refresh": "old-refresh"}}
        }))
        .unwrap();
        credentials.telegram_chat = "test-chat".into();
        credentials
            .calendar_urls
            .insert("work".into(), "https://example.invalid/feed.ics".into());
        credentials.save(&root).unwrap();
        assert!(Credentials::load(&root).unwrap() == credentials);
        assert!(
            !String::from_utf8_lossy(&std::fs::read(root.join("credentials.enc")).unwrap())
                .contains("old-secret")
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn encrypted_round_trip_and_missing_key_does_not_reset() {
        let root = std::env::temp_dir().join(crate::model::id());
        let c = Credentials {
            telegram_token: "private-token".into(),
            ..Default::default()
        };
        c.save(&root).unwrap();
        assert_eq!(
            Credentials::load(&root).unwrap().telegram_token,
            "private-token"
        );
        assert!(
            !String::from_utf8_lossy(&std::fs::read(root.join("credentials.enc")).unwrap())
                .contains("private-token")
        );
        std::fs::remove_file(root.join("credentials.key")).unwrap();
        assert!(Credentials::load(&root).is_err());
        assert!(c.save(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
