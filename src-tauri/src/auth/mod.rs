//! ServerDesk Auth: an optional, local, single-administrator gate for the
//! shell. It protects the console, never the remote services' own auth, and it
//! never touches vault keys.
//!
//! The security-relevant logic works on a directory + in-memory state, which
//! keeps it testable without a running Tauri runtime; the small wrappers at the
//! bottom bind it to `AppHandle`.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

use crate::vault::crypto;

const AUTH_VERSION: u32 = 1;
const MIN_PASSWORD_LEN: usize = 8;
const MAX_FAILURES_BEFORE_DELAY: u32 = 5;
const MAX_LOCK_MINUTES: u32 = 1440;

fn hasher() -> argon2::Argon2<'static> {
    argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(65536, 3, 1, None).expect("argon2 params"),
    )
}

pub fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    let salt = SaltString::generate(&mut OsRng);
    hasher()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| format!("密码哈希失败: {e}"))
}

pub fn verify_password(password: &str, phc: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    match PasswordHash::new(phc) {
        Ok(parsed) => hasher()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub password_hash: String,
    #[serde(default)]
    pub owner_id: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_role")]
    pub role: String,
    /// Argon2id hash of the one-time recovery code. The code itself is never stored.
    #[serde(default)]
    pub recovery_hash: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_role() -> String {
    "administrator".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub user: Option<AuthUser>,
    /// 0 disables idle auto-lock.
    #[serde(default = "default_lock_minutes")]
    pub auto_lock_minutes: u32,
}

fn default_version() -> u32 {
    AUTH_VERSION
}

fn default_lock_minutes() -> u32 {
    15
}

impl Default for AuthFile {
    fn default() -> Self {
        Self {
            version: AUTH_VERSION,
            enabled: false,
            user: None,
            auto_lock_minutes: default_lock_minutes(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // session_id / authenticated_at belong to the session model
struct Session {
    session_id: String,
    user_id: String,
    username: String,
    authenticated_at: i64,
    last_activity: Instant,
}

#[derive(Default)]
struct Inner {
    session: Option<Session>,
    failures: u32,
    blocked_until_ms: i64,
    idle_notified: bool,
}

#[derive(Default)]
pub struct AuthState {
    inner: Mutex<Inner>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableResult {
    pub status: AuthStatus,
    /// Shown exactly once, right after the account is created.
    pub recovery_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub enabled: bool,
    pub registered: bool,
    pub authenticated: bool,
    pub locked: bool,
    pub username: Option<String>,
    pub user_id: Option<String>,
    pub auto_lock_minutes: u32,
    pub error: Option<String>,
}

// ---------------------------------------------------------------- storage --

fn file_at(dir: &Path) -> std::path::PathBuf {
    dir.join("auth.json")
}

fn backup_at(dir: &Path) -> std::path::PathBuf {
    dir.join("auth.backup.json")
}

fn state_marker_at(dir: &Path) -> std::path::PathBuf {
    dir.join("auth.state")
}

fn factory_reset_marker_at(dir: &Path) -> std::path::PathBuf {
    dir.join("auth-reset.pending")
}

fn write_marker(path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("无法写入认证状态: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("无法持久化认证状态: {e}"))
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("写入认证文件失败: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("替换认证文件失败: {e}"))
}

fn has_prior_data_at(dir: &Path) -> bool {
    [
        "config.json",
        "channel-state",
        "vault",
        "auth.backup.json",
        "auth.state",
        "sessions.pending",
    ]
    .iter()
    .any(|name| dir.join(name).exists())
}

/// Must run before config::load creates the first-run config file.
pub fn bootstrap_at(dir: &Path, has_service_profiles: bool) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("无法创建配置目录: {e}"))?;
    if file_at(dir).exists() {
        write_marker(&state_marker_at(dir))?;
        if let Ok(file) = load_at(dir) {
            let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
            write_atomic(&backup_at(dir), &text)?;
        }
        return Ok(());
    }
    if has_service_profiles || has_prior_data_at(dir) {
        write_marker(&state_marker_at(dir))?;
        return Ok(());
    }
    save_at(dir, &AuthFile::default())
}

pub fn load_at(dir: &Path) -> Result<AuthFile, String> {
    if factory_reset_marker_at(dir).exists() {
        return Err("本机认证重置尚未完成，请重新启动程序".into());
    }
    let path = file_at(dir);
    if !path.exists() {
        return if has_prior_data_at(dir) {
            Err("auth.json 丢失；请用恢复码恢复，或执行本机重置".into())
        } else {
            Ok(AuthFile::default())
        };
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("读取 auth.json 失败: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析 auth.json 失败: {e}"))
}

fn save_at(dir: &Path, file: &AuthFile) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("无法创建配置目录: {e}"))?;
    write_marker(&state_marker_at(dir))?;
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    write_atomic(&backup_at(dir), &text)?;
    write_atomic(&file_at(dir), &text)?;
    Ok(())
}

fn recoverable_file_at(dir: &Path) -> Result<AuthFile, String> {
    if factory_reset_marker_at(dir).exists() {
        return Err("本机认证重置尚未完成".into());
    }
    match load_at(dir) {
        Ok(file) => Ok(file),
        Err(_) => {
            let text = std::fs::read_to_string(backup_at(dir))
                .map_err(|_| "认证文件及其备份都不可用".to_string())?;
            serde_json::from_str(&text).map_err(|_| "认证备份已损坏".to_string())
        }
    }
}

// ------------------------------------------------------------------ logic --

pub fn status_at(dir: &Path, state: &AuthState) -> AuthStatus {
    let (file, error) = match load_at(dir) {
        Ok(file) => (file, None),
        // Unknown state must fail closed: keep protection on.
        Err(err) => (AuthFile::default(), Some(err)),
    };
    let mut inner = state.inner.lock().unwrap();
    if error.is_some() {
        inner.session = None;
    }
    let enabled = if error.is_some() { true } else { file.enabled };
    if let Some(session) = inner.session.as_ref() {
        if enabled
            && file.auto_lock_minutes > 0
            && session.last_activity.elapsed()
                >= Duration::from_secs(file.auto_lock_minutes as u64 * 60)
        {
            inner.session = None;
        }
    }
    let authenticated = if !enabled {
        true
    } else {
        inner.session.is_some()
    };
    AuthStatus {
        enabled,
        registered: file.user.is_some(),
        authenticated,
        locked: !authenticated,
        username: inner
            .session
            .as_ref()
            .map(|session| session.username.clone())
            .or_else(|| file.user.as_ref().map(|user| user.username.clone())),
        user_id: inner
            .session
            .as_ref()
            .map(|session| session.user_id.clone())
            .or_else(|| file.user.as_ref().map(|user| user.user_id.clone())),
        auto_lock_minutes: file.auto_lock_minutes,
        error,
    }
}

pub fn require_at(dir: &Path, state: &AuthState) -> Result<(), String> {
    let info = status_at(dir, state);
    if info.authenticated {
        let mut inner = state.inner.lock().unwrap();
        if let Some(session) = inner.session.as_mut() {
            session.last_activity = Instant::now();
        }
        Ok(())
    } else if let Some(error) = info.error {
        Err(error)
    } else {
        Err("需要先登录 ServerDesk".into())
    }
}

/// 16 chars from a 32-symbol alphabet (no look-alikes) = 80 bits.
fn generate_recovery_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
    let bytes = crypto::random_bytes::<16>();
    let mut raw = String::with_capacity(20);
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 && index % 4 == 0 {
            raw.push('-');
        }
        raw.push(ALPHABET[(*byte as usize) % ALPHABET.len()] as char);
    }
    raw
}

fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

fn note_failure(state: &AuthState) {
    let mut inner = state.inner.lock().unwrap();
    inner.failures += 1;
    if inner.failures >= MAX_FAILURES_BEFORE_DELAY {
        let excess = (inner.failures - MAX_FAILURES_BEFORE_DELAY).min(8) as i64;
        let delay = (30_000i64 * (1 << excess)).min(15 * 60 * 1000);
        inner.blocked_until_ms = crate::vault::storage::now_ms() + delay;
    }
}

fn assert_not_blocked(state: &AuthState) -> Result<(), String> {
    let inner = state.inner.lock().unwrap();
    if inner.blocked_until_ms > crate::vault::storage::now_ms() {
        return Err("尝试过于频繁，请稍后再试".into());
    }
    Ok(())
}

fn start_session(state: &AuthState, user: &AuthUser) -> String {
    let session_id = uuid_like();
    let mut inner = state.inner.lock().unwrap();
    inner.session = Some(Session {
        session_id: session_id.clone(),
        user_id: user.user_id.clone(),
        username: user.username.clone(),
        authenticated_at: crate::vault::storage::now_ms(),
        last_activity: Instant::now(),
    });
    inner.failures = 0;
    inner.idle_notified = false;
    session_id
}

pub fn enable_at(
    dir: &Path,
    state: &AuthState,
    username: &str,
    password: &str,
    confirm: &str,
) -> Result<(AuthStatus, Option<String>), String> {
    if password != confirm {
        return Err("两次输入的密码不一致".into());
    }
    let mut file = load_at(dir)?;

    let mut issued_code: Option<String> = None;

    if let Some(existing) = file.user.clone() {
        // Re-enabling never bypasses the existing password.
        if !verify_password(password, &existing.password_hash) {
            return Err("管理员密码错误".into());
        }
    } else {
        let name = username.trim();
        if name.is_empty() {
            return Err("请填写管理员用户名".into());
        }
        if password.chars().count() < MIN_PASSWORD_LEN {
            return Err(format!("密码至少 {MIN_PASSWORD_LEN} 个字符"));
        }
        let now = crate::vault::storage::now_ms();
        let code = generate_recovery_code();
        file.user = Some(AuthUser {
            user_id: uuid_like(),
            username: name.to_string(),
            password_hash: hash_password(password)?,
            owner_id: String::new(),
            created_at: now,
            updated_at: now,
            enabled: true,
            role: "administrator".into(),
            recovery_hash: Some(hash_password(&normalize_code(&code))?),
        });
        issued_code = Some(code);
    }

    let user = file.user.clone().ok_or("管理员账号创建失败")?;
    file.enabled = true;
    if file.auto_lock_minutes == 0 {
        file.auto_lock_minutes = default_lock_minutes();
    }
    // Account and enabled flag land in one atomic write.
    save_at(dir, &file)?;
    start_session(state, &user);
    Ok((status_at(dir, state), issued_code))
}

pub fn login_at(
    dir: &Path,
    state: &AuthState,
    username: &str,
    password: &str,
) -> Result<AuthStatus, String> {
    let file = load_at(dir)?;
    if !file.enabled {
        return Err("登录保护未启用".into());
    }
    let user = file.user.clone().ok_or("管理员账号不存在")?;
    if !user.enabled {
        return Err("管理员账号已停用".into());
    }
    assert_not_blocked(state)?;
    let name_ok = user.username.eq_ignore_ascii_case(username.trim());
    if !name_ok || !verify_password(password, &user.password_hash) {
        note_failure(state);
        return Err("用户名或密码错误".into());
    }
    start_session(state, &user);
    Ok(status_at(dir, state))
}

fn check_recovery_code(dir: &Path, state: &AuthState, code: &str) -> Result<AuthUser, String> {
    let file = recoverable_file_at(dir)?;
    if !file.enabled {
        return Err("登录保护未启用".into());
    }
    let Some(user) = file.user.clone() else {
        return Err("管理员账号不存在".into());
    };
    let Some(recovery_hash) = user.recovery_hash.clone() else {
        return Err("该账号没有可用的恢复码".into());
    };
    assert_not_blocked(state)?;
    let normalized = normalize_code(code);
    if normalized.len() < 8 || !verify_password(&normalized, &recovery_hash) {
        note_failure(state);
        return Err("恢复码不正确".into());
    }
    Ok(user)
}

/// "恢复": a valid recovery code signs the administrator in. The account and
/// its password stay exactly as they were; the user can change the password
/// afterwards from Settings.
pub fn recover_with_code_at(
    dir: &Path,
    state: &AuthState,
    code: &str,
) -> Result<AuthStatus, String> {
    let user = check_recovery_code(dir, state, code)?;
    if load_at(dir).is_err() {
        let file = recoverable_file_at(dir)?;
        save_at(dir, &file)?;
    }
    start_session(state, &user);
    Ok(status_at(dir, state))
}

/// Recovery path: a valid one-time code disables the panel lock and removes
/// the account, so the next enable creates a fresh password and recovery code.
/// This storage-only helper leaves WebView2 sessions untouched. The app-level
/// wrapper below clears them after the account change.
pub fn reset_with_recovery_at(dir: &Path, state: &AuthState, code: &str) -> Result<(), String> {
    check_recovery_code(dir, state, code)?;
    let mut file = recoverable_file_at(dir)?;
    file.enabled = false;
    file.user = None;
    save_at(dir, &file)?;
    lock_session(state);
    Ok(())
}

/// Clears the session. Returns true when something was actually locked.
pub fn lock_session(state: &AuthState) -> bool {
    let mut inner = state.inner.lock().unwrap();
    let had = inner.session.is_some();
    inner.session = None;
    inner.idle_notified = true;
    had
}

pub fn verify_at(dir: &Path, state: &AuthState, password: &str) -> Result<(), String> {
    require_at(dir, state)?;
    let file = load_at(dir)?;
    let user = file.user.as_ref().ok_or("管理员账号不存在")?;
    if verify_password(password, &user.password_hash) {
        Ok(())
    } else {
        Err("管理员密码错误".into())
    }
}

pub fn change_password_at(
    dir: &Path,
    state: &AuthState,
    current: &str,
    next: &str,
    confirm: &str,
) -> Result<(), String> {
    require_at(dir, state)?;
    if next != confirm {
        return Err("两次输入的新密码不一致".into());
    }
    if next.chars().count() < MIN_PASSWORD_LEN {
        return Err(format!("新密码至少 {MIN_PASSWORD_LEN} 个字符"));
    }
    let mut file = load_at(dir)?;
    let user = file.user.as_mut().ok_or("管理员账号不存在")?;
    if !verify_password(current, &user.password_hash) {
        return Err("当前密码错误".into());
    }
    user.password_hash = hash_password(next)?;
    user.updated_at = crate::vault::storage::now_ms();
    let snapshot = user.clone();
    save_at(dir, &file)?;
    // Old sessions are invalid: start a fresh one.
    start_session(state, &snapshot);
    Ok(())
}

pub fn disable_at(dir: &Path, state: &AuthState, password: &str) -> Result<(), String> {
    require_at(dir, state)?;
    let mut file = load_at(dir)?;
    let user = file.user.as_ref().ok_or("管理员账号不存在")?;
    if !verify_password(password, &user.password_hash) {
        return Err("管理员密码错误".into());
    }
    file.enabled = false;
    save_at(dir, &file)?;
    state.inner.lock().unwrap().session = None;
    Ok(())
}

pub fn update_lock_settings_at(
    dir: &Path,
    state: &AuthState,
    minutes: u32,
) -> Result<AuthStatus, String> {
    require_at(dir, state)?;
    let mut file = load_at(dir)?;
    file.auto_lock_minutes = minutes.min(MAX_LOCK_MINUTES);
    save_at(dir, &file)?;
    Ok(status_at(dir, state))
}

/// Used by the idle timer: returns true when it just locked.
pub fn tick_at(dir: &Path, state: &AuthState) -> bool {
    let info = status_at(dir, state);
    if !info.enabled || info.authenticated {
        state.inner.lock().unwrap().idle_notified = false;
        return false;
    }
    let mut inner = state.inner.lock().unwrap();
    if inner.idle_notified {
        return false;
    }
    inner.idle_notified = true;
    true
}

// ----------------------------------------------------------------- wrappers --

fn config_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法定位配置目录: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录: {e}"))?;
    Ok(dir)
}

pub fn bootstrap(app: &AppHandle) -> Result<(), String> {
    let has_service_profiles = app
        .path()
        .app_local_data_dir()
        .map(|base| {
            base.join(crate::windows::PROFILE_TAILNET).exists()
                || base.join(crate::windows::PROFILE_PUBLIC).exists()
        })
        .unwrap_or(true);
    bootstrap_at(&config_dir(app)?, has_service_profiles)
}

pub fn factory_reset_pending(app: &AppHandle) -> bool {
    config_dir(app)
        .map(|dir| factory_reset_marker_at(&dir).exists())
        .unwrap_or(true)
}

fn complete_factory_reset(app: &AppHandle, state: &AuthState) -> Result<(), String> {
    let dir = config_dir(app)?;
    if !factory_reset_marker_at(&dir).exists() {
        return Ok(());
    }
    crate::channel::stop(app);
    crate::vault::revoke_all_grants(app)?;
    crate::windows::clear_and_destroy_service_windows(app);
    if !crate::windows::purge_service_profiles(app) {
        return Err("服务会话仍被占用，请关闭程序后重试".into());
    }
    let identity = dir.join("channel-state");
    if identity.exists() {
        std::fs::remove_dir_all(&identity).map_err(|e| format!("清除通道身份失败: {e}"))?;
    }
    save_at(&dir, &AuthFile::default())?;
    std::fs::remove_file(factory_reset_marker_at(&dir))
        .map_err(|e| format!("完成认证重置失败: {e}"))?;
    lock_session(state);
    Ok(())
}

pub fn resume_pending_factory_reset(app: &AppHandle) -> Result<(), String> {
    complete_factory_reset(app, &app.state::<AuthState>())
}

/// Explicit destructive recovery when both the auth file and backup are lost.
pub fn factory_reset(app: &AppHandle, state: &AuthState) -> Result<(), String> {
    let dir = config_dir(app)?;
    if load_at(&dir).is_ok() {
        return Err("认证文件正常，不能使用无恢复码重置".into());
    }
    write_marker(&factory_reset_marker_at(&dir))?;
    complete_factory_reset(app, state)?;
    if crate::config::load(app).settings.channel_enabled {
        let _ = crate::channel::start(app);
    }
    emit_status(app, state);
    Ok(())
}

fn emit_status(app: &AppHandle, state: &AuthState) {
    if let Ok(dir) = config_dir(app) {
        let _ = app.emit("auth-status", status_at(&dir, state));
    }
}

pub fn load(app: &AppHandle) -> Result<AuthFile, String> {
    load_at(&config_dir(app)?)
}

pub fn status(app: &AppHandle, state: &AuthState) -> AuthStatus {
    match config_dir(app) {
        Ok(dir) => status_at(&dir, state),
        Err(err) => AuthStatus {
            enabled: true,
            registered: false,
            authenticated: false,
            locked: true,
            username: None,
            user_id: None,
            auto_lock_minutes: default_lock_minutes(),
            error: Some(err),
        },
    }
}

pub fn is_locked(app: &AppHandle, state: &AuthState) -> bool {
    let info = status(app, state);
    info.enabled && !info.authenticated
}

pub fn require(app: &AppHandle, state: &AuthState) -> Result<(), String> {
    require_at(&config_dir(app)?, state)
}

pub fn enable(
    app: &AppHandle,
    state: &AuthState,
    username: &str,
    password: &str,
    confirm: &str,
) -> Result<EnableResult, String> {
    let (status, recovery_code) = enable_at(&config_dir(app)?, state, username, password, confirm)?;
    emit_status(app, state);
    Ok(EnableResult {
        status,
        recovery_code,
    })
}

pub fn recover_with_code(
    app: &AppHandle,
    state: &AuthState,
    code: &str,
) -> Result<AuthStatus, String> {
    let status = recover_with_code_at(&config_dir(app)?, state, code)?;
    if crate::config::load(app).settings.channel_enabled
        && app.state::<crate::channel::ChannelState>().snapshot().phase == "stopped"
    {
        let _ = crate::channel::start(app);
    }
    emit_status(app, state);
    Ok(status)
}

/// Marker that survives a crash between "account removed" and "sessions wiped",
/// so the wipe is always finished before any service can be opened again.
fn session_wipe_marker(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("sessions.pending"))
}

/// A recovery-code reset removes the account *and* every local service session:
/// cookies, localStorage and IndexedDB for both WebView2 profiles. Service
/// configuration, the tailnet identity and the vault are untouched.
pub fn reset_with_recovery(app: &AppHandle, state: &AuthState, code: &str) -> Result<(), String> {
    let dir = config_dir(app)?;
    check_recovery_code(&dir, state, code)?;
    let marker = session_wipe_marker(app).ok_or("无法定位会话清理标记")?;
    let marker_file =
        std::fs::File::create(&marker).map_err(|e| format!("无法创建会话清理标记: {e}"))?;
    marker_file
        .sync_all()
        .map_err(|e| format!("无法持久化会话清理标记: {e}"))?;
    if let Err(err) = reset_with_recovery_at(&dir, state, code) {
        let _ = std::fs::remove_file(&marker);
        return Err(err);
    }

    crate::windows::clear_and_destroy_service_windows(app);
    if crate::windows::purge_service_profiles(app) {
        if let Some(marker) = session_wipe_marker(app) {
            let _ = std::fs::remove_file(marker);
        }
    }

    emit_status(app, state);
    Ok(())
}

/// Finishes an interrupted session wipe at startup, before the UI can open
/// any service window.
pub fn resume_pending_session_wipe(app: &AppHandle) {
    let Some(marker) = session_wipe_marker(app) else {
        return;
    };
    if !marker.exists() {
        return;
    }
    // A crash before the account update must not turn a failed reset into a
    // session wipe. A disabled account means the update committed.
    if load(app).map(|file| file.enabled).unwrap_or(true) {
        let _ = std::fs::remove_file(&marker);
        return;
    }
    crate::windows::clear_and_destroy_service_windows(app);
    if crate::windows::purge_service_profiles(app) {
        let _ = std::fs::remove_file(&marker);
    }
}

pub fn login(
    app: &AppHandle,
    state: &AuthState,
    username: &str,
    password: &str,
) -> Result<AuthStatus, String> {
    let result = login_at(&config_dir(app)?, state, username, password)?;
    emit_status(app, state);
    Ok(result)
}

pub fn lock(app: &AppHandle, state: &AuthState) {
    lock_session(state);
    // Never leave a protected window reachable while locked.
    crate::windows::hide_all(app, &app.state::<crate::windows::WinState>());
    emit_status(app, state);
}

pub fn logout(app: &AppHandle, state: &AuthState) {
    lock(app, state);
}

pub fn verify(app: &AppHandle, state: &AuthState, password: &str) -> Result<(), String> {
    verify_at(&config_dir(app)?, state, password)
}

pub fn change_password(
    app: &AppHandle,
    state: &AuthState,
    current: &str,
    next: &str,
    confirm: &str,
) -> Result<(), String> {
    change_password_at(&config_dir(app)?, state, current, next, confirm)?;
    emit_status(app, state);
    Ok(())
}

pub fn disable(app: &AppHandle, state: &AuthState, password: &str) -> Result<(), String> {
    disable_at(&config_dir(app)?, state, password)?;
    emit_status(app, state);
    Ok(())
}

pub fn update_lock_settings(
    app: &AppHandle,
    state: &AuthState,
    minutes: u32,
) -> Result<AuthStatus, String> {
    let result = update_lock_settings_at(&config_dir(app)?, state, minutes)?;
    emit_status(app, state);
    Ok(result)
}

pub fn tick(app: &AppHandle, state: &AuthState) {
    let Ok(dir) = config_dir(app) else {
        return;
    };
    if tick_at(&dir, state) {
        crate::windows::hide_all(app, &app.state::<crate::windows::WinState>());
        emit_status(app, state);
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

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("serverdesk-auth-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn password_hash_roundtrip() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong", &hash));
        assert!(!hash.contains("correct horse"));
    }

    #[test]
    fn admin_password_accepts_eight_characters() {
        let dir = temp_dir("eight-character-password");
        let state = AuthState::default();
        assert!(enable_at(&dir, &state, "admin", "1234567", "1234567").is_err());
        enable_at(&dir, &state, "admin", "12345678", "12345678").unwrap();
        assert!(change_password_at(&dir, &state, "12345678", "abcdefg", "abcdefg").is_err());
        change_password_at(&dir, &state, "12345678", "abcdefgh", "abcdefgh").unwrap();
    }

    #[test]
    fn hash_is_salted() {
        let a = hash_password("same-password-value").unwrap();
        let b = hash_password("same-password-value").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn enable_stores_hash_only_and_cold_start_locks() {
        let dir = temp_dir("enable");
        let state = AuthState::default();
        let (status, code) = enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        assert!(status.enabled && status.authenticated);
        let code = code.expect("first enable must issue a recovery code");
        assert!(code.len() >= 16, "recovery code should be long enough");

        let text = std::fs::read_to_string(file_at(&dir)).unwrap();
        assert!(
            !text.contains("ServerDesk-Panel-2026"),
            "password must not be stored in clear"
        );
        assert!(text.contains("$argon2id$"));
        assert!(text.contains("\"enabled\": true"));

        // A fresh process has no session: it starts locked.
        let cold = AuthState::default();
        assert!(status_at(&dir, &cold).locked);
        assert!(require_at(&dir, &cold).is_err());
    }

    #[test]
    fn wrong_password_is_rejected_and_rate_limited() {
        let dir = temp_dir("login");
        let state = AuthState::default();
        enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        lock_session(&state);

        for _ in 0..5 {
            assert!(login_at(&dir, &state, "admin", "wrong").is_err());
        }
        let limited = login_at(&dir, &state, "admin", "ServerDesk-Panel-2026");
        assert!(limited.is_err());
        assert!(limited.unwrap_err().contains("频繁"));

        state.inner.lock().unwrap().blocked_until_ms = 0;
        login_at(&dir, &state, "admin", "ServerDesk-Panel-2026").unwrap();
        assert!(require_at(&dir, &state).is_ok());
    }

    #[test]
    fn change_password_invalidates_old_one() {
        let dir = temp_dir("change");
        let state = AuthState::default();
        enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        change_password_at(
            &dir,
            &state,
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2027",
            "ServerDesk-Panel-2027",
        )
        .unwrap();
        lock_session(&state);
        assert!(login_at(&dir, &state, "admin", "ServerDesk-Panel-2026").is_err());
        assert!(login_at(&dir, &state, "admin", "ServerDesk-Panel-2027").is_ok());
    }

    #[test]
    fn disable_requires_password_and_re_enable_needs_it_again() {
        let dir = temp_dir("disable");
        let state = AuthState::default();
        enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();

        assert!(disable_at(&dir, &state, "nope").is_err());
        disable_at(&dir, &state, "ServerDesk-Panel-2026").unwrap();
        assert!(!status_at(&dir, &state).locked);

        let file = load_at(&dir).unwrap();
        assert!(file.user.is_some(), "account must survive disabling");
        assert!(!file.enabled);

        // Re-enabling still verifies the original password.
        assert!(enable_at(&dir, &state, "admin", "wrong", "wrong").is_err());
        assert!(enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026"
        )
        .is_ok());
    }

    #[test]
    fn corrupt_file_fails_closed() {
        let dir = temp_dir("corrupt");
        std::fs::write(file_at(&dir), b"{not json").unwrap();
        let state = AuthState::default();
        let info = status_at(&dir, &state);
        assert!(info.enabled, "unknown auth state must stay enabled");
        assert!(info.locked);
        assert!(require_at(&dir, &state).is_err());
    }

    #[test]
    fn fresh_bootstrap_persists_explicit_disabled_state() {
        let dir = temp_dir("fresh-bootstrap");
        bootstrap_at(&dir, false).unwrap();
        assert!(file_at(&dir).exists());
        assert!(backup_at(&dir).exists());
        assert!(state_marker_at(&dir).exists());
        let info = status_at(&dir, &AuthState::default());
        assert!(!info.enabled);
        assert!(info.authenticated);
    }

    #[test]
    fn missing_auth_with_existing_data_fails_closed() {
        let dir = temp_dir("missing-existing");
        std::fs::write(dir.join("config.json"), b"{}").unwrap();
        bootstrap_at(&dir, false).unwrap();
        assert!(!file_at(&dir).exists());
        let info = status_at(&dir, &AuthState::default());
        assert!(info.locked);
        assert!(info.error.is_some());
    }

    #[test]
    fn recovery_restores_missing_primary_without_clearing_sessions() {
        let dir = temp_dir("missing-recover");
        let state = AuthState::default();
        let (_, issued) = enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        let code = issued.unwrap();
        let session = dir.join("service-session.marker");
        std::fs::write(&session, b"keep").unwrap();
        std::fs::remove_file(file_at(&dir)).unwrap();
        lock_session(&state);
        assert!(status_at(&dir, &state).locked);
        assert!(recover_with_code_at(&dir, &state, "wrong-code").is_err());
        assert!(!file_at(&dir).exists());
        recover_with_code_at(&dir, &state, &code).unwrap();
        assert!(file_at(&dir).exists());
        assert_eq!(std::fs::read(&session).unwrap(), b"keep");
        lock_session(&state);
        assert!(login_at(&dir, &state, "admin", "ServerDesk-Panel-2026").is_ok());
    }

    #[test]
    fn losing_both_auth_files_stays_locked() {
        let dir = temp_dir("both-missing");
        let state = AuthState::default();
        enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        std::fs::remove_file(file_at(&dir)).unwrap();
        std::fs::remove_file(backup_at(&dir)).unwrap();
        lock_session(&state);
        assert!(status_at(&dir, &state).locked);
        assert!(recover_with_code_at(&dir, &state, "any-code").is_err());
    }

    #[test]
    fn deleting_auth_during_an_active_session_revokes_access() {
        let dir = temp_dir("missing-during-session");
        let state = AuthState::default();
        enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        assert!(require_at(&dir, &state).is_ok());
        std::fs::remove_file(file_at(&dir)).unwrap();
        assert!(status_at(&dir, &state).locked);
        assert!(require_at(&dir, &state).is_err());
    }

    #[test]
    fn pending_factory_reset_never_exposes_a_disabled_auth_file() {
        let dir = temp_dir("pending-factory-reset");
        bootstrap_at(&dir, false).unwrap();
        write_marker(&factory_reset_marker_at(&dir)).unwrap();
        let info = status_at(&dir, &AuthState::default());
        assert!(info.locked);
        assert!(info.error.is_some());
        std::fs::remove_file(factory_reset_marker_at(&dir)).unwrap();
        assert!(!status_at(&dir, &AuthState::default()).locked);
    }

    #[test]
    fn backup_tracks_password_changes_before_recovery() {
        let dir = temp_dir("backup-password-change");
        let state = AuthState::default();
        let (_, code) = enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        change_password_at(
            &dir,
            &state,
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2027",
            "ServerDesk-Panel-2027",
        )
        .unwrap();
        std::fs::remove_file(file_at(&dir)).unwrap();
        lock_session(&state);
        recover_with_code_at(&dir, &state, &code.unwrap()).unwrap();
        lock_session(&state);
        assert!(login_at(&dir, &state, "admin", "ServerDesk-Panel-2026").is_err());
        assert!(login_at(&dir, &state, "admin", "ServerDesk-Panel-2027").is_ok());
    }

    #[test]
    fn recovery_code_recovers_and_resets() {
        let dir = temp_dir("recovery");
        let state = AuthState::default();
        let (_, issued) = enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        let code = issued.expect("recovery code issued");
        let stored = std::fs::read_to_string(file_at(&dir)).unwrap();
        assert!(
            !stored.contains(&code),
            "recovery code must be stored hashed only"
        );
        assert!(stored.contains("recoveryHash"));

        // Wrong code is rejected and does not sign anyone in.
        lock_session(&state);
        assert!(recover_with_code_at(&dir, &state, "AAAA-BBBB-CCCC-DDDD").is_err());
        assert!(status_at(&dir, &state).locked);

        // Right code signs in without touching the password.
        recover_with_code_at(&dir, &state, &code.to_lowercase()).unwrap();
        assert!(!status_at(&dir, &state).locked);

        // Reset with the same code disables protection and drops the account.
        lock_session(&state);
        reset_with_recovery_at(&dir, &state, &code).unwrap();
        let info = status_at(&dir, &state);
        assert!(!info.enabled);
        assert!(!info.registered);
        assert!(!info.locked, "no protection means no lock screen");
        let file = load_at(&dir).unwrap();
        assert!(file.user.is_none());
        assert!(!file.enabled);
    }

    #[test]
    fn idle_lock_triggers_once() {
        let dir = temp_dir("idle");
        let state = AuthState::default();
        enable_at(
            &dir,
            &state,
            "admin",
            "ServerDesk-Panel-2026",
            "ServerDesk-Panel-2026",
        )
        .unwrap();
        {
            let mut file = load_at(&dir).unwrap();
            file.auto_lock_minutes = 1;
            save_at(&dir, &file).unwrap();
        }
        {
            let mut inner = state.inner.lock().unwrap();
            if let Some(session) = inner.session.as_mut() {
                session.last_activity = Instant::now() - Duration::from_secs(120);
            }
        }
        assert!(tick_at(&dir, &state));
        assert!(!tick_at(&dir, &state), "must warn only once per lock");
        assert!(status_at(&dir, &state).locked);
    }
}
