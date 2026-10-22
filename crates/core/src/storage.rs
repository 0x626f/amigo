#[cfg(test)]
use crate::model::FinancePlan;
use crate::model::{Workspace, plan_from_value};
#[cfg(test)]
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
#[cfg(test)]
use base64::{Engine, engine::general_purpose::STANDARD};
#[cfg(test)]
use pbkdf2::pbkdf2_hmac;
#[cfg(test)]
use rand::{RngCore, rngs::OsRng};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
#[cfg(test)]
use sha2::Sha256;
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Preserve HOME overrides while supporting native Windows user environments.
pub fn user_home(get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let value = |key| get(key).filter(|value| !value.is_empty());
    value("HOME")
        .or_else(|| value("USERPROFILE"))
        .or_else(|| {
            let mut drive = value("HOMEDRIVE")?;
            drive.push(value("HOMEPATH")?);
            Some(drive)
        })
        .map(PathBuf::from)
}

pub fn default_path() -> PathBuf {
    if let Some(path) =
        std::env::var_os("AMIGO_DATA_FILE").or_else(|| std::env::var_os("CLERK_DATA_FILE"))
    {
        return PathBuf::from(path);
    }
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| user_home(|key| std::env::var_os(key)).map(|p| p.join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    let current = root.join("amigo/workspace.json");
    let previous = root.join("clerk/workspace.json");
    if current.exists() || !previous.exists() {
        current
    } else {
        previous
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    let temporary = parent.join(format!(".amigo-{}.tmp", crate::model::id()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|e| format!("Cannot save {}: {e}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub fn load_workspace(path: &Path) -> Result<Workspace, String> {
    if !path.exists() {
        return Ok(Workspace::default());
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("Cannot read saved workspace: {e}"))?;
    workspace_from_value(value)
}
pub fn workspace_from_value(mut value: Value) -> Result<Workspace, String> {
    if value.get("finance").is_none()
        || value.get("tasks").is_none()
        || !matches!(value["version"].as_u64(), Some(1 | 2))
    {
        return Err("This JSON does not contain a supported Amigo workspace.".into());
    }
    if value["version"] == 1
        && let Some(tasks) = value.get_mut("tasks").and_then(Value::as_array_mut)
    {
        for task in tasks {
            if let Some(status) = task.get("status").and_then(Value::as_u64) {
                task["status"] =
                    Value::String(crate::model::DEFAULT_STATUS_IDS[status.min(3) as usize].into());
            }
        }
    }
    let finance = plan_from_value(value["finance"].clone())?;
    let mut workspace: Workspace = serde_json::from_value(value).map_err(|e| e.to_string())?;
    workspace.finance = finance;
    workspace.normalize();
    Ok(workspace)
}
#[cfg(test)]
pub fn save_workspace(path: &Path, workspace: &Workspace) -> Result<(), String> {
    atomic_write(
        path,
        &serde_json::to_vec_pretty(workspace).map_err(|e| e.to_string())?,
    )
}
#[cfg(test)]
pub fn decode(text: &str, password: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("Invalid JSON: {e}"))?;
    if value["encrypted"] != true {
        return Ok(value);
    }
    if password.is_empty() {
        return Err("Enter the file's export password to import it.".into());
    }
    if value["version"] != 1 || value["algorithm"] != "AES-GCM" || value["kdf"] != "PBKDF2-SHA-256"
    {
        return Err("Unsupported encrypted file format.".into());
    }
    let iterations = value["iterations"]
        .as_u64()
        .filter(|n| (1..=2_000_000).contains(n))
        .ok_or("Invalid encryption iteration count.")? as u32;
    let get_bytes = |key: &str| {
        STANDARD
            .decode(value[key].as_str().unwrap_or(""))
            .map_err(|_| "Invalid encrypted data.".to_string())
    };
    let salt = get_bytes("salt")?;
    let iv = get_bytes("iv")?;
    let data = get_bytes("data")?;
    if salt.len() != 16 || iv.len() != 12 {
        return Err("Invalid encrypted file salt or IV.".into());
    }
    let mut key = [0; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, iterations, &mut key);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&iv), data.as_ref())
        .map_err(|_| {
            "Could not decrypt this file. Check the password and try again.".to_string()
        })?;
    serde_json::from_slice(&plain).map_err(|e| e.to_string())
}
#[cfg(test)]
pub fn encode(value: &Value, password: &str) -> Result<Vec<u8>, String> {
    if password.is_empty() {
        return serde_json::to_vec_pretty(value).map_err(|e| e.to_string());
    }
    let mut salt = [0; 16];
    let mut iv = [0; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut iv);
    let mut key = [0; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, 210_000, &mut key);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let plain = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&iv), plain.as_ref())
        .map_err(|e| e.to_string())?;
    serde_json::to_vec_pretty(&json!({"version":1,"encrypted":true,"algorithm":"AES-GCM","kdf":"PBKDF2-SHA-256","iterations":210_000,"salt":STANDARD.encode(salt),"iv":STANDARD.encode(iv),"data":STANDARD.encode(encrypted)})).map_err(|e| e.to_string())
}
#[cfg(test)]
pub fn import_plan(text: &str, password: &str) -> Result<FinancePlan, String> {
    plan_from_value(decode(text, password)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_roundtrip_wrong_password_and_tampering() {
        let value = serde_json::to_value(FinancePlan::empty()).unwrap();
        let bytes = encode(&value, "test password").unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(decode(&text, "test password").unwrap(), value);
        assert!(decode(&text, "wrong").is_err());
        let mut envelope: Value = serde_json::from_str(&text).unwrap();
        envelope["data"] = json!(STANDARD.encode([0; 32]));
        assert!(decode(&envelope.to_string(), "test password").is_err());
        envelope["iv"] = json!("");
        assert!(decode(&envelope.to_string(), "test password").is_err());
    }
    #[test]
    fn workspace_save_and_reload() {
        let path = std::env::temp_dir().join(format!("amigo-test-{}.json", crate::model::id()));
        let mut workspace = Workspace::default();
        workspace
            .tasks
            .push(crate::model::Task::new(1, "in-progress"));
        workspace.normalize();
        save_workspace(&path, &workspace).unwrap();
        assert_eq!(load_workspace(&path).unwrap(), workspace);
        fs::remove_file(path).unwrap();
        let mut legacy = serde_json::to_value(Workspace::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("darkMode");
        assert!(!workspace_from_value(legacy).unwrap().dark_mode);
        assert!(workspace_from_value(json!({"hello":"world"})).is_err());
    }
    #[test]
    #[ignore = "requires Node.js for independent browser Web Crypto verification"]
    fn browser_webcrypto_interoperability() {
        use std::process::{Command, Stdio};
        let plan = crate::model::plan_from_value(json!({"version":2,"meta":{"name":"Interop ✓","reportingCurrency":"EUR"},"income":[{"grossAmount":"1234.56","currency":"EUR","source":"Synthetic client"}]})).unwrap();
        let payload = serde_json::to_value(&plan).unwrap();
        let envelope: Value =
            serde_json::from_slice(&encode(&payload, "interop password").unwrap()).unwrap();
        let mut child = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/webcrypto-interop.mjs"
            ))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                json!({"envelope":envelope,"password":"interop password"})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            crate::model::plan_from_value(result["payload"].clone()).unwrap(),
            plan
        );
        let browser_text = result["envelope"].to_string();
        assert_eq!(
            import_plan(&browser_text, "interop password").unwrap(),
            plan
        );
    }
}
