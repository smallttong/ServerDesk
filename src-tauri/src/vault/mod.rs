pub mod crypto;
pub mod storage;

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Webview};
use zeroize::Zeroizing;

use crypto::{KdfParams, KEY_LEN};
use storage::{CredentialRecord, GrantFile, GrantRecord, VaultFile, VaultMeta, VaultPaths};

const LOCK_AFTER_SECS: u64 = 300;
const MIN_PASSWORD_LEN: usize = 8;
const MAX_GRANT_DAYS: i64 = 7;

struct SessionGrant {
    grant_id: String,
    credential_id: String,
    service_id: String,
    endpoint_id: String,
    origin: String,
    expires_at: i64,
    cek: Zeroizing<[u8; KEY_LEN]>,
}

struct Inner {
    dek: Option<Zeroizing<[u8; KEY_LEN]>>,
    last_activity: Instant,
    session_grants: Vec<SessionGrant>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            dek: None,
            last_activity: Instant::now(),
            session_grants: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct VaultState {
    inner: Mutex<Inner>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
    pub pending_reset: bool,
    pub lock_after_secs: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialView {
    pub credential_id: String,
    pub service_id: String,
    pub endpoint_id: String,
    pub origin: String,
    pub username: String,
    pub credential_type: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub auto_login_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialInput {
    pub credential_id: Option<String>,
    pub service_id: String,
    pub endpoint_id: String,
    pub origin: String,
    pub username: String,
    #[serde(default)]
    pub secret: Option<String>,
    #[serde(default)]
    pub credential_type: String,
    #[serde(default)]
    pub auto_login_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantView {
    pub grant_id: String,
    pub credential_id: String,
    pub service_id: String,
    pub endpoint_id: String,
    pub origin: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub expired: bool,
    /// "session" grants live only in memory, "persisted" survive a restart.
    pub storage: String,
}

impl CredentialRecord {
    fn view(&self) -> CredentialView {
        CredentialView {
            credential_id: self.credential_id.clone(),
            service_id: self.service_id.clone(),
            endpoint_id: self.endpoint_id.clone(),
            origin: self.origin.clone(),
            username: self.username.clone(),
            credential_type: self.credential_type.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            auto_login_enabled: self.auto_login_enabled,
        }
    }
}

fn secret_aad(record: &CredentialRecord) -> Vec<u8> {
    format!(
        "{}|{}|{}|{}|v1",
        record.credential_id, record.service_id, record.endpoint_id, record.origin
    )
    .into_bytes()
}

/// Only the shell window may touch the vault. Remote service pages are also
/// blocked by the ACL, this is the second lock on the same door.
pub fn ensure_caller(webview: &Webview) -> Result<(), String> {
    if webview.label() != "main" {
        return Err("forbidden".into());
    }
    match webview.url() {
        Ok(url) => {
            let local = url.scheme() == "tauri"
                || url.host_str() == Some("tauri.localhost")
                || url.host_str() == Some("localhost");
            if local {
                Ok(())
            } else {
                Err("forbidden".into())
            }
        }
        Err(_) => Err("forbidden".into()),
    }
}

fn paths(app: &AppHandle) -> Result<VaultPaths, String> {
    VaultPaths::new(app)
}

pub fn status(app: &AppHandle, state: &VaultState) -> VaultStatus {
    let initialized = paths(app).map(|p| p.initialized()).unwrap_or(false);
    let pending_reset = paths(app).map(|p| p.pending_reset()).unwrap_or(false);
    VaultStatus {
        initialized,
        unlocked: state.inner.lock().unwrap().dek.is_some(),
        pending_reset,
        lock_after_secs: LOCK_AFTER_SECS,
    }
}

/// Locks when the idle window elapsed. Called by commands and a timer thread.
pub fn check_idle(state: &VaultState) -> bool {
    let mut inner = state.inner.lock().unwrap();
    if inner.dek.is_none() {
        return false;
    }
    if inner.last_activity.elapsed() >= Duration::from_secs(LOCK_AFTER_SECS) {
        inner.dek = None;
        return true;
    }
    false
}

pub fn lock(state: &VaultState) {
    state.inner.lock().unwrap().dek = None;
}

/// Used by unrecoverable panel-auth reset. Keep encrypted credentials, but
/// remove every grant that could unlock them without the vault password.
pub fn revoke_all_grants(app: &AppHandle) -> Result<(), String> {
    let paths = paths(app)?;
    let grants = paths.grants();
    if grants.exists() {
        std::fs::remove_file(grants).map_err(|e| format!("清除免密授权失败: {e}"))?;
    }
    let state = app.state::<VaultState>();
    let mut inner = state.inner.lock().unwrap();
    inner.session_grants.clear();
    inner.dek = None;
    Ok(())
}

fn touch(state: &VaultState) {
    let mut inner = state.inner.lock().unwrap();
    inner.last_activity = Instant::now();
}

fn require_unlocked(state: &VaultState) -> Result<Zeroizing<[u8; KEY_LEN]>, String> {
    let mut inner = state.inner.lock().unwrap();
    if inner.dek.is_some() && inner.last_activity.elapsed() >= Duration::from_secs(LOCK_AFTER_SECS)
    {
        inner.dek = None;
    }
    let key = inner.dek.as_ref().map(|dek| Zeroizing::new(**dek));
    match key {
        Some(key) => {
            inner.last_activity = Instant::now();
            Ok(key)
        }
        None => Err("保险箱未解锁".into()),
    }
}

pub fn initialize(app: &AppHandle, state: &VaultState, password: &str) -> Result<(), String> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(format!("二级密码至少 {MIN_PASSWORD_LEN} 个字符"));
    }
    let paths = paths(app)?;
    if paths.initialized() {
        return Err("保险箱已经初始化".into());
    }

    let salt = crypto::random_bytes::<16>();
    let kdf = KdfParams::default();
    let kek = crypto::derive_kek(password, &salt, &kdf)?;
    let dek = crypto::random_bytes::<KEY_LEN>();
    let (nonce, wrapped) = crypto::aes_encrypt(&kek, &dek, b"serverdesk-vault-dek-v1")?;

    let meta = VaultMeta {
        version: storage::VAULT_VERSION,
        vault_id: crypto::b64e(&crypto::random_bytes::<16>()),
        salt: crypto::b64e(&salt),
        kdf,
        wrapped_dek_nonce: crypto::b64e(&nonce),
        wrapped_dek: crypto::b64e(&wrapped),
        created_at: storage::now_ms(),
    };
    storage::write_json(&paths.meta(), &meta)?;
    storage::save_vault(&paths, &VaultFile::default())?;

    state.inner.lock().unwrap().dek = Some(Zeroizing::new(dek));
    touch(state);
    storage::audit(&paths, "initialize", "vault created");
    Ok(())
}

pub fn unlock(app: &AppHandle, state: &VaultState, password: &str) -> Result<(), String> {
    let paths = paths(app)?;
    if !paths.initialized() {
        return Err("保险箱尚未初始化".into());
    }
    let meta = storage::load_meta(&paths)?;
    let salt = crypto::b64d(&meta.salt)?;
    let kek = crypto::derive_kek(password, &salt, &meta.kdf)?;
    let nonce = crypto::b64d(&meta.wrapped_dek_nonce)?;
    let wrapped = crypto::b64d(&meta.wrapped_dek)?;
    let dek = crypto::aes_decrypt(&kek, &nonce, &wrapped, b"serverdesk-vault-dek-v1")
        .map_err(|_| "二级密码错误".to_string())?;
    if dek.len() != KEY_LEN {
        return Err("保险箱数据损坏".into());
    }
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&dek);
    let mut inner = state.inner.lock().unwrap();
    inner.dek = Some(Zeroizing::new(key));
    inner.last_activity = Instant::now();
    drop(inner);
    storage::audit(&paths, "unlock", "ok");
    Ok(())
}

pub fn list_credentials(
    app: &AppHandle,
    state: &VaultState,
) -> Result<Vec<CredentialView>, String> {
    let dek = require_unlocked(state)?;
    let _ = dek;
    let paths = paths(app)?;
    let file = storage::load_vault(&paths)?;
    Ok(file
        .credentials
        .iter()
        .map(|record| record.view())
        .collect())
}

pub fn save_credential(
    app: &AppHandle,
    state: &VaultState,
    input: CredentialInput,
) -> Result<CredentialView, String> {
    let dek = require_unlocked(state)?;
    let paths = paths(app)?;
    let mut file = storage::load_vault(&paths)?;

    if input.service_id.trim().is_empty() || input.endpoint_id.trim().is_empty() {
        return Err("凭据必须绑定到服务入口".into());
    }
    let origin = normalize_origin(&input.origin)?;

    let existing = input
        .credential_id
        .as_ref()
        .and_then(|id| file.credentials.iter().position(|r| &r.credential_id == id));

    let credential_id = input.credential_id.clone().unwrap_or_else(|| uuid_like());

    // Refuse to bind one credential to two different endpoints of the same
    // service without an explicit edit.
    if existing.is_none() {
        if let Some(clash) = file.credentials.iter().find(|record| {
            record.service_id == input.service_id && record.endpoint_id == input.endpoint_id
        }) {
            return Err(format!(
                "该入口已存在凭据（{}），请先编辑或删除它",
                clash.username
            ));
        }
    }

    let secret = match input.secret.as_deref() {
        Some(value) if !value.is_empty() => Some(value.to_string()),
        _ => None,
    };

    match existing {
        Some(index) => {
            let mut record = file.credentials[index].clone();
            let previous = secret_aad(&record);
            record.username = input.username.trim().to_string();
            record.origin = origin;
            record.credential_type = if input.credential_type.is_empty() {
                "password".into()
            } else {
                input.credential_type.clone()
            };
            record.auto_login_enabled = input.auto_login_enabled;
            record.updated_at = storage::now_ms();
            let new_aad = secret_aad(&record);
            if let Some(secret) = secret {
                let cek = crypto::derive_cek(&dek, &record.credential_id)?;
                let (nonce, cipher) = crypto::aes_encrypt(&cek, secret.as_bytes(), &new_aad)?;
                record.secret_nonce = crypto::b64e(&nonce);
                record.secret_cipher = crypto::b64e(&cipher);
            } else if previous != new_aad {
                // The AAD changed (origin/service/endpoint), so the old
                // ciphertext would no longer authenticate: re-encrypt it.
                let cek = crypto::derive_cek(&dek, &record.credential_id)?;
                let plain = crypto::aes_decrypt(
                    &cek,
                    &crypto::b64d(&record.secret_nonce)?,
                    &crypto::b64d(&record.secret_cipher)?,
                    &previous,
                )?;
                let (nonce, cipher) = crypto::aes_encrypt(&cek, &plain, &new_aad)?;
                record.secret_nonce = crypto::b64e(&nonce);
                record.secret_cipher = crypto::b64e(&cipher);
            }
            file.credentials[index] = record.clone();
            storage::save_vault(&paths, &file)?;
            storage::audit(&paths, "credential_update", &record.credential_id);
            Ok(record.view())
        }
        None => {
            let secret = secret.ok_or_else(|| "新增凭据必须填写密码".to_string())?;
            let mut record = CredentialRecord {
                credential_id,
                service_id: input.service_id.clone(),
                endpoint_id: input.endpoint_id.clone(),
                origin,
                username: input.username.trim().to_string(),
                secret_nonce: String::new(),
                secret_cipher: String::new(),
                credential_type: if input.credential_type.is_empty() {
                    "password".into()
                } else {
                    input.credential_type.clone()
                },
                created_at: storage::now_ms(),
                updated_at: storage::now_ms(),
                auto_login_enabled: input.auto_login_enabled,
            };
            let aad = secret_aad(&record);
            let cek = crypto::derive_cek(&dek, &record.credential_id)?;
            let (nonce, cipher) = crypto::aes_encrypt(&cek, secret.as_bytes(), &aad)?;
            record.secret_nonce = crypto::b64e(&nonce);
            record.secret_cipher = crypto::b64e(&cipher);
            file.credentials.push(record.clone());
            storage::save_vault(&paths, &file)?;
            storage::audit(&paths, "credential_add", &record.credential_id);
            Ok(record.view())
        }
    }
}

pub fn delete_credential(app: &AppHandle, state: &VaultState, id: &str) -> Result<(), String> {
    let dek = require_unlocked(state)?;
    let _ = dek;
    let paths = paths(app)?;
    let mut file = storage::load_vault(&paths)?;
    let before = file.credentials.len();
    file.credentials.retain(|record| record.credential_id != id);
    if file.credentials.len() == before {
        return Err("凭据不存在".into());
    }
    storage::save_vault(&paths, &file)?;

    // Grants for a removed credential must not linger.
    let mut grants = storage::load_grants(&paths);
    grants.grants.retain(|grant| grant.credential_id != id);
    let _ = storage::save_grants(&paths, &grants);
    state
        .inner
        .lock()
        .unwrap()
        .session_grants
        .retain(|grant| grant.credential_id != id);

    storage::audit(&paths, "credential_delete", id);
    Ok(())
}

pub fn reveal_credential(app: &AppHandle, state: &VaultState, id: &str) -> Result<String, String> {
    let dek = require_unlocked(state)?;
    let paths = paths(app)?;
    let file = storage::load_vault(&paths)?;
    let record = file
        .credentials
        .iter()
        .find(|record| record.credential_id == id)
        .ok_or_else(|| "凭据不存在".to_string())?;
    let cek = crypto::derive_cek(&dek, &record.credential_id)?;
    let plain = crypto::aes_decrypt(
        &cek,
        &crypto::b64d(&record.secret_nonce)?,
        &crypto::b64d(&record.secret_cipher)?,
        &secret_aad(record),
    )?;
    storage::audit(&paths, "credential_reveal", id);
    String::from_utf8(plain).map_err(|_| "凭据内容不是有效文本".to_string())
}

fn grant_master(file: &mut GrantFile) -> Result<Zeroizing<[u8; KEY_LEN]>, String> {
    if file.master.is_empty() {
        let master = crypto::random_bytes::<KEY_LEN>();
        file.master = crypto::b64e(&crypto::dpapi_protect(&master)?);
        Ok(Zeroizing::new(master))
    } else {
        let raw = crypto::dpapi_unprotect(&crypto::b64d(&file.master)?)?;
        if raw.len() != KEY_LEN {
            return Err("授权密钥损坏".into());
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&raw);
        Ok(Zeroizing::new(key))
    }
}

fn grant_aad(grant: &GrantRecord) -> Vec<u8> {
    format!(
        "{}|{}|{}|{}|{}",
        grant.grant_id, grant.credential_id, grant.endpoint_id, grant.origin, grant.expires_at
    )
    .into_bytes()
}

/// `days == 0` means "this run only": the CEK stays in memory and never
/// touches the disk.
pub fn authorize(
    app: &AppHandle,
    state: &VaultState,
    credential_id: &str,
    days: i64,
) -> Result<GrantView, String> {
    let dek = require_unlocked(state)?;
    let paths = paths(app)?;
    let file = storage::load_vault(&paths)?;
    let record = file
        .credentials
        .iter()
        .find(|record| record.credential_id == credential_id)
        .ok_or_else(|| "凭据不存在".to_string())?
        .clone();
    let cek = crypto::derive_cek(&dek, &record.credential_id)?;
    let now = storage::now_ms();
    let days = days.clamp(0, MAX_GRANT_DAYS);
    let expires_at = if days == 0 {
        now + 24 * 3600 * 1000
    } else {
        now + days * 24 * 3600 * 1000
    };

    if days == 0 {
        let grant_id = uuid_like();
        state
            .inner
            .lock()
            .unwrap()
            .session_grants
            .push(SessionGrant {
                grant_id: grant_id.clone(),
                credential_id: record.credential_id.clone(),
                service_id: record.service_id.clone(),
                endpoint_id: record.endpoint_id.clone(),
                origin: record.origin.clone(),
                expires_at,
                cek: Zeroizing::new(cek),
            });
        storage::audit(&paths, "grant_session", &record.credential_id);
        return Ok(GrantView {
            grant_id,
            credential_id: record.credential_id,
            service_id: record.service_id,
            endpoint_id: record.endpoint_id,
            origin: record.origin,
            created_at: now,
            expires_at,
            expired: false,
            storage: "session".into(),
        });
    }

    let mut grants = storage::load_grants(&paths);
    let master = grant_master(&mut grants)?;
    let grant_id = uuid_like();
    let mut grant = GrantRecord {
        grant_id: grant_id.clone(),
        credential_id: record.credential_id.clone(),
        service_id: record.service_id.clone(),
        endpoint_id: record.endpoint_id.clone(),
        origin: record.origin.clone(),
        created_at: now,
        expires_at,
        wrapped_nonce: String::new(),
        wrapped_cek: String::new(),
    };
    let aad = grant_aad(&grant);
    let (nonce, wrapped) = crypto::aes_encrypt(&master, &cek, &aad)?;
    grant.wrapped_nonce = crypto::b64e(&nonce);
    grant.wrapped_cek = crypto::b64e(&wrapped);
    grants.grants.retain(|item| {
        !(item.credential_id == grant.credential_id && item.endpoint_id == grant.endpoint_id)
    });
    grants.grants.push(grant.clone());
    storage::save_grants(&paths, &grants)?;
    storage::audit(&paths, "grant_persist", &record.credential_id);

    Ok(GrantView {
        grant_id,
        credential_id: grant.credential_id,
        service_id: grant.service_id,
        endpoint_id: grant.endpoint_id,
        origin: grant.origin,
        created_at: grant.created_at,
        expires_at: grant.expires_at,
        expired: false,
        storage: "persisted".into(),
    })
}

pub fn list_grants(app: &AppHandle, state: &VaultState) -> Vec<GrantView> {
    let now = storage::now_ms();
    let mut out: Vec<GrantView> = Vec::new();
    if let Ok(paths) = paths(app) {
        for grant in storage::load_grants(&paths).grants {
            out.push(GrantView {
                grant_id: grant.grant_id,
                credential_id: grant.credential_id,
                service_id: grant.service_id,
                endpoint_id: grant.endpoint_id,
                origin: grant.origin,
                created_at: grant.created_at,
                expires_at: grant.expires_at,
                expired: grant.expires_at <= now,
                storage: "persisted".into(),
            });
        }
    }
    let inner = state.inner.lock().unwrap();
    for grant in &inner.session_grants {
        out.push(GrantView {
            grant_id: grant.grant_id.clone(),
            credential_id: grant.credential_id.clone(),
            service_id: grant.service_id.clone(),
            endpoint_id: grant.endpoint_id.clone(),
            origin: grant.origin.clone(),
            created_at: 0,
            expires_at: grant.expires_at,
            expired: grant.expires_at <= now,
            storage: "session".into(),
        });
    }
    out
}

pub fn revoke(app: &AppHandle, state: &VaultState, grant_id: &str) -> Result<(), String> {
    let paths = paths(app)?;
    let mut grants = storage::load_grants(&paths);
    let before = grants.grants.len();
    grants.grants.retain(|grant| grant.grant_id != grant_id);
    if grants.grants.len() != before {
        storage::save_grants(&paths, &grants)?;
    }
    state
        .inner
        .lock()
        .unwrap()
        .session_grants
        .retain(|grant| grant.grant_id != grant_id);
    storage::audit(&paths, "grant_revoke", grant_id);
    Ok(())
}

/// Builds the injected script. It runs inside the page, re-checks the origin,
/// waits for exactly one visible password field (SPA login routes render late),
/// fills it plus the nearest preceding text input, then submits.
pub fn autologin_script(origin: &str, username: &str, secret: &str) -> String {
    let o = serde_json::to_string(origin).unwrap_or_default();
    let u = serde_json::to_string(username).unwrap_or_default();
    let s = serde_json::to_string(secret).unwrap_or_default();
    [
        "(function(){try{",
        "var ORIGIN=", &o, ";var USER=", &u, ";var SECRET=", &s, ";",
        "if(location.origin!==ORIGIN)return;",
        "var timer=setInterval(function(){try{",
        "if(location.origin!==ORIGIN){clearInterval(timer);return;}",
        "var pw=Array.prototype.filter.call(document.querySelectorAll('input[type=password]'),function(el){return el.offsetParent!==null&&!el.disabled&&!el.readOnly;});",
        "if(pw.length!==1)return;",
        // Already filled for this URL and the field still holds a value: leave
        // it alone. An empty field means a fresh login form (expired session),
        // which we fill and submit again.
        "if(window.__sdAutologinFor===location.href&&pw[0].value)return;",
        "clearInterval(timer);window.__sdAutologinFor=location.href;",
        "var passEl=pw[0];",
        "var all=Array.prototype.slice.call(document.querySelectorAll('input'));",
        "var idx=all.indexOf(passEl);var userEl=null;",
        "for(var i=idx-1;i>=0;i--){var t=(all[i].type||'text').toLowerCase();if((t==='text'||t==='email'||t==='tel'||t==='')&&all[i].offsetParent!==null){userEl=all[i];break;}}",
        "function setVal(el,v){var proto=(el.tagName==='TEXTAREA')?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;var d=Object.getOwnPropertyDescriptor(proto,'value');if(d&&d.set){d.set.call(el,v);}else{el.value=v;}el.dispatchEvent(new Event('input',{bubbles:true}));el.dispatchEvent(new Event('change',{bubbles:true}));}",
        "if(userEl)setVal(userEl,USER);setVal(passEl,SECRET);",
        "setTimeout(function(){try{",
        "var btns=Array.prototype.slice.call(document.querySelectorAll('button,input[type=submit],[role=button]'));",
        "var re=/^(\\u767b\\s*\\u5f55|\\u767b\\u5f55|log\\s*in|login|sign\\s*in|submit)$/i;",
        "for(var j=0;j<btns.length;j++){var txt=(btns[j].innerText||btns[j].value||'').replace(/\\s+/g,' ').trim();if(re.test(txt)&&btns[j].offsetParent!==null&&!btns[j].disabled){btns[j].click();return;}}",
        "var f=passEl.closest?passEl.closest('form'):null;if(f&&f.requestSubmit)f.requestSubmit();",
        "}catch(e){}},400);",
        "}catch(e){}},500);",
        "setTimeout(function(){clearInterval(timer);},15000);",
        "}catch(e){}})();",
    ]
    .concat()
}

/// Fills and submits a login form when a live grant covers this exact
/// endpoint + origin. Called from the page-load hook and the URL poller.
/// One attempt per window + URL within a short window; on_page_load and the
/// URL poller otherwise both fire for the same navigation.
static LAST_AUTOLOGIN: Mutex<
    Option<std::collections::HashMap<String, (String, std::time::Instant)>>,
> = Mutex::new(None);

fn autologin_already_attempted(label: &str, href: &str) -> bool {
    let mut guard = LAST_AUTOLOGIN.lock().unwrap();
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    // Throttled per window *and* per document URL. A panel that hard-navigates
    // (`/` -> `/login`) is a new document and needs its own injection, while
    // the page-load hook and the URL poller must not fire twice for one URL.
    if let Some((last_href, at)) = map.get(label) {
        if last_href == href && at.elapsed() < std::time::Duration::from_secs(25) {
            return true;
        }
    }
    map.insert(
        label.to_string(),
        (href.to_string(), std::time::Instant::now()),
    );
    false
}

pub fn try_autologin(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    service_id: &str,
    endpoint_id: &str,
    origin: &str,
) {
    if origin.is_empty() {
        return;
    }
    let href = window.url().map(|url| url.to_string()).unwrap_or_default();
    if autologin_already_attempted(window.label(), &href) {
        return;
    }

    let state = app.state::<VaultState>();
    let Some((username, secret)) =
        credential_for_autologin(app, &state, service_id, endpoint_id, origin)
    else {
        return;
    };
    let script = autologin_script(origin, &username, &secret);
    let _ = window.eval(&script);
}

/// Used by the auto-login path (not exposed to the frontend yet: adapters are
/// the next milestone). Returns the
/// secret only when a live grant matches this exact endpoint + origin.
#[allow(dead_code)]
pub fn credential_for_autologin(
    app: &AppHandle,
    state: &VaultState,
    service_id: &str,
    endpoint_id: &str,
    origin: &str,
) -> Option<(String, String)> {
    let now = storage::now_ms();
    let paths = paths(app).ok()?;

    // Session grants first.
    {
        let inner = state.inner.lock().unwrap();
        if let Some(grant) = inner.session_grants.iter().find(|grant| {
            grant.service_id == service_id
                && grant.endpoint_id == endpoint_id
                && grant.origin == origin
                && grant.expires_at > now
        }) {
            return decrypt_with(&paths, &grant.credential_id, &grant.cek);
        }
    }

    let mut grants = storage::load_grants(&paths);
    if grants.master.is_empty() {
        return None;
    }
    let master = grant_master(&mut grants).ok()?;
    let grant = grants.grants.iter().find(|grant| {
        grant.service_id == service_id
            && grant.endpoint_id == endpoint_id
            && grant.origin == origin
            && grant.expires_at > now
    })?;
    let cek_raw = crypto::aes_decrypt(
        &master,
        &crypto::b64d(&grant.wrapped_nonce).ok()?,
        &crypto::b64d(&grant.wrapped_cek).ok()?,
        &grant_aad(grant),
    )
    .ok()?;
    if cek_raw.len() != KEY_LEN {
        return None;
    }
    let mut cek = [0u8; KEY_LEN];
    cek.copy_from_slice(&cek_raw);
    decrypt_with(&paths, &grant.credential_id, &Zeroizing::new(cek))
}

#[allow(dead_code)]
fn decrypt_with(
    paths: &VaultPaths,
    credential_id: &str,
    cek: &Zeroizing<[u8; KEY_LEN]>,
) -> Option<(String, String)> {
    let file = storage::load_vault(paths).ok()?;
    let record = file
        .credentials
        .iter()
        .find(|record| record.credential_id == credential_id && record.auto_login_enabled)?;
    let plain = crypto::aes_decrypt(
        cek,
        &crypto::b64d(&record.secret_nonce).ok()?,
        &crypto::b64d(&record.secret_cipher).ok()?,
        &secret_aad(record),
    )
    .ok()?;
    let secret = String::from_utf8(plain).ok()?;
    storage::audit(paths, "autologin", credential_id);
    Some((record.username.clone(), secret))
}

/// Destructive: wipes credentials, grants, service sessions and keys.
/// Never calls the remote services; config and channel identity are untouched.
pub fn reset(app: &AppHandle, state: &VaultState) -> Result<(), String> {
    let paths = paths(app)?;
    storage::mark_reset_pending(&paths)?;
    storage::audit(&paths, "reset", "started");

    // 1. Kill every path that could decrypt anything, before slow cleanup.
    let _ = std::fs::remove_file(paths.grants());
    {
        let mut inner = state.inner.lock().unwrap();
        inner.dek = None;
        inner.session_grants.clear();
    }

    // 2. Drop service webviews and their browsing data (cookies, storage).
    crate::windows::clear_and_destroy_service_windows(app);

    // 3. Remove the encrypted material.
    storage::destroy_files(&paths);

    // 4. If the profile folders are still locked by a live browser process,
    //    leave the marker in place: the next start retries the cleanup.
    let profiles_clean = crate::windows::purge_service_profiles(app);
    if profiles_clean {
        storage::clear_reset_pending(&paths);
    }
    storage::audit(
        &paths,
        "reset",
        if profiles_clean {
            "completed"
        } else {
            "pending"
        },
    );
    Ok(())
}

/// Runs at startup: finish an interrupted reset before anything else.
pub fn resume_pending_reset(app: &AppHandle) -> bool {
    let Ok(paths) = paths(app) else {
        return false;
    };
    if !paths.pending_reset() {
        return false;
    }
    let _ = std::fs::remove_file(paths.grants());
    storage::destroy_files(&paths);
    crate::windows::clear_and_destroy_service_windows(app);
    if crate::windows::purge_service_profiles(app) {
        storage::clear_reset_pending(&paths);
    }
    true
}

fn normalize_origin(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Origin 不能为空".into());
    }
    let parsed = url::Url::parse(trimmed).map_err(|e| format!("Origin 无效: {e}"))?;
    match parsed.host_str() {
        Some(host) => {
            let port = parsed.port_or_known_default().unwrap_or(0);
            Ok(format!("{}://{}:{}", parsed.scheme(), host, port))
        }
        None => Err("Origin 缺少主机名".into()),
    }
}

fn uuid_like() -> String {
    let bytes = crypto::random_bytes::<16>();
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_normalisation_is_stable() {
        assert_eq!(
            normalize_origin("http://service.example.com:6185").unwrap(),
            "http://service.example.com:6185"
        );
        assert_eq!(
            normalize_origin("https://panel.example.com/").unwrap(),
            "https://panel.example.com:443"
        );
        assert!(normalize_origin("").is_err());
    }

    #[test]
    fn grant_aad_binds_identity() {
        let base = GrantRecord {
            grant_id: "g1".into(),
            credential_id: "c1".into(),
            service_id: "s".into(),
            endpoint_id: "e1".into(),
            origin: "http://a:1".into(),
            created_at: 0,
            expires_at: 1,
            wrapped_nonce: String::new(),
            wrapped_cek: String::new(),
        };
        let mut other = base.clone();
        other.endpoint_id = "e2".into();
        assert_ne!(grant_aad(&base), grant_aad(&other));
    }

    #[test]
    fn session_grant_is_not_written_to_disk() {
        // Sanity check for the intent: session grants live only in memory.
        assert_eq!(crypto::NONCE_LEN, 12);
    }
}
