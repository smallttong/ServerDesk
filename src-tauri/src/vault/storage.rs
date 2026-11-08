use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

use super::crypto::KdfParams;

pub const VAULT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultMeta {
    pub version: u32,
    pub vault_id: String,
    pub salt: String,
    pub kdf: KdfParams,
    pub wrapped_dek_nonce: String,
    pub wrapped_dek: String,
    pub created_at: i64,
}

/// On-disk credential record. `secret` is never stored in clear.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRecord {
    pub credential_id: String,
    pub service_id: String,
    pub endpoint_id: String,
    pub origin: String,
    pub username: String,
    pub secret_nonce: String,
    pub secret_cipher: String,
    pub credential_type: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub auto_login_enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultFile {
    pub credentials: Vec<CredentialRecord>,
}

/// A grant lets one credential be decrypted without the master password.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantRecord {
    pub grant_id: String,
    pub credential_id: String,
    pub service_id: String,
    pub endpoint_id: String,
    pub origin: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub wrapped_nonce: String,
    pub wrapped_cek: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantFile {
    /// DPAPI-protected master key that wraps per-grant CEKs.
    #[serde(default)]
    pub master: String,
    #[serde(default)]
    pub grants: Vec<GrantRecord>,
}

pub struct VaultPaths {
    pub root: PathBuf,
}

impl VaultPaths {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let root = app
            .path()
            .app_config_dir()
            .map_err(|e| format!("无法定位配置目录: {e}"))?
            .join("vault");
        fs::create_dir_all(&root).map_err(|e| format!("无法创建保险箱目录: {e}"))?;
        Ok(Self { root })
    }

    pub fn meta(&self) -> PathBuf {
        self.root.join("meta.json")
    }

    pub fn vault(&self) -> PathBuf {
        self.root.join("vault.json")
    }

    pub fn grants(&self) -> PathBuf {
        self.root.join("grants.json")
    }

    pub fn pending(&self) -> PathBuf {
        self.root.join("reset.pending")
    }

    pub fn audit(&self) -> PathBuf {
        self.root.join("audit.log")
    }

    pub fn initialized(&self) -> bool {
        self.meta().exists()
    }

    pub fn pending_reset(&self) -> bool {
        self.pending().exists()
    }
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("读取失败: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析失败: {e}"))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("写入失败: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("替换失败: {e}"))?;
    Ok(())
}

pub fn load_meta(paths: &VaultPaths) -> Result<VaultMeta, String> {
    read_json(&paths.meta())
}

pub fn load_vault(paths: &VaultPaths) -> Result<VaultFile, String> {
    if !paths.vault().exists() {
        return Ok(VaultFile::default());
    }
    read_json(&paths.vault())
}

pub fn save_vault(paths: &VaultPaths, file: &VaultFile) -> Result<(), String> {
    write_json(&paths.vault(), file)
}

pub fn load_grants(paths: &VaultPaths) -> GrantFile {
    if !paths.grants().exists() {
        return GrantFile::default();
    }
    read_json(&paths.grants()).unwrap_or_default()
}

pub fn save_grants(paths: &VaultPaths, file: &GrantFile) -> Result<(), String> {
    write_json(&paths.grants(), file)
}

pub fn mark_reset_pending(paths: &VaultPaths) -> Result<(), String> {
    fs::write(paths.pending(), b"pending").map_err(|e| format!("写入重置标记失败: {e}"))?;
    Ok(())
}

pub fn clear_reset_pending(paths: &VaultPaths) {
    let _ = fs::remove_file(paths.pending());
}

/// Append-only, secret-free operation log.
pub fn audit(paths: &VaultPaths, action: &str, detail: &str) {
    use std::io::Write;
    let line = format!("{}\t{}\t{}\n", now_ms(), action, detail);
    if let Ok(metadata) = fs::metadata(paths.audit()) {
        if metadata.len() > 512 * 1024 {
            let _ = fs::rename(paths.audit(), paths.audit().with_extension("log.old"));
        }
    }
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.audit())
    {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Wipes the vault's own data files. The reset marker is removed by the caller
/// only after every step succeeded.
pub fn destroy_files(paths: &VaultPaths) {
    for path in [paths.vault(), paths.meta(), paths.grants()] {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("json.tmp"));
    }
}
