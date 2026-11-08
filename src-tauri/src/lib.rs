mod auth;
mod channel;
mod config;
mod health;
mod tray;
mod vault;
mod windows;

use tauri::{AppHandle, Emitter, Manager, State, Webview, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

use auth::{AuthState, AuthStatus, EnableResult};
use channel::ChannelState;
use config::AppConfig;
use health::HealthResult;
use vault::{CredentialInput, CredentialView, GrantView, VaultState, VaultStatus};
use windows::{Rect, WinState};

#[tauri::command]
fn get_config(app: AppHandle) -> AppConfig {
    config::load(&app)
}

#[tauri::command]
async fn save_config(app: AppHandle, config: AppConfig) -> Result<(), String> {
    gate(&app)?;
    config::save(&app, &config)?;
    tray::refresh(&app);

    // Keep the built-in channel in sync with the settings.
    let state = app.state::<ChannelState>();
    if config.settings.channel_enabled {
        let snapshot = state.snapshot();
        let hostname_changed = snapshot
            .hostname
            .map(|h| h != config.settings.channel_hostname)
            .unwrap_or(false);
        let tags_changed = snapshot
            .tags
            .map(|t| t != config.settings.channel_tags)
            .unwrap_or(false);
        if snapshot.phase == "stopped" || hostname_changed || tags_changed {
            let _ = channel::start(&app);
        } else if channel::sync_allowlist(&app, &config).is_err() {
            // A broken pipe means the sidecar has exited. Restart with the
            // current allowlist instead of leaving an old policy in place.
            channel::start(&app)?;
        }
    } else if state.snapshot().phase != "stopped" {
        channel::stop(&app);
    }
    Ok(())
}

#[tauri::command]
fn channel_status(state: State<ChannelState>) -> channel::ChannelStatus {
    state.snapshot()
}

#[tauri::command]
async fn channel_start(app: AppHandle) -> Result<channel::ChannelStatus, String> {
    gate(&app)?;
    channel::start(&app)
}

#[tauri::command]
fn channel_stop(app: AppHandle) -> Result<(), String> {
    gate(&app)?;
    channel::stop(&app);
    Ok(())
}

#[tauri::command]
async fn channel_logout(app: AppHandle) -> Result<(), String> {
    gate(&app)?;
    channel::logout(&app)
}

#[tauri::command]
fn channel_open_login(app: AppHandle, url: String) -> Result<(), String> {
    gate(&app)?;
    let parsed = url::Url::parse(url.trim()).map_err(|e| format!("登录地址无效: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("登录地址必须是 https".into());
    }
    app.opener()
        .open_url(parsed.to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn config_path(app: AppHandle) -> String {
    config::config_path(&app)
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

/// Rejects sensitive IPC while the shell is locked.
fn gate(app: &AppHandle) -> Result<(), String> {
    auth::require(app, &app.state::<AuthState>())
}

#[tauri::command]
fn auth_get_status(
    webview: Webview,
    app: AppHandle,
    state: State<AuthState>,
) -> Result<AuthStatus, String> {
    vault::ensure_caller(&webview)?;
    auth::tick(&app, &state);
    Ok(auth::status(&app, &state))
}

#[tauri::command]
async fn auth_initialize(
    webview: Webview,
    app: AppHandle,
    state: State<'_, AuthState>,
    username: String,
    password: String,
    confirm: String,
) -> Result<EnableResult, String> {
    vault::ensure_caller(&webview)?;
    auth::enable(&app, &state, &username, &password, &confirm)
}

/// Forgot-password path, non-destructive: the recovery code signs you in.
#[tauri::command]
async fn auth_recover_with_code(
    webview: Webview,
    app: AppHandle,
    state: State<'_, AuthState>,
    code: String,
) -> Result<AuthStatus, String> {
    vault::ensure_caller(&webview)?;
    auth::recover_with_code(&app, &state, &code)
}

/// Forgot-password path: a one-time recovery code disables the panel lock and
/// clears the administrator account. Vault and service config are untouched.
#[tauri::command]
async fn auth_reset_with_recovery(
    webview: Webview,
    app: AppHandle,
    state: State<'_, AuthState>,
    code: String,
) -> Result<(), String> {
    vault::ensure_caller(&webview)?;
    auth::reset_with_recovery(&app, &state, &code)
}

#[tauri::command]
async fn auth_factory_reset(
    webview: Webview,
    app: AppHandle,
    state: State<'_, AuthState>,
) -> Result<(), String> {
    vault::ensure_caller(&webview)?;
    auth::factory_reset(&app, &state)
}

#[tauri::command]
async fn auth_login(
    webview: Webview,
    app: AppHandle,
    state: State<'_, AuthState>,
    username: String,
    password: String,
) -> Result<AuthStatus, String> {
    vault::ensure_caller(&webview)?;
    auth::login(&app, &state, &username, &password)
}

#[tauri::command]
fn auth_logout(webview: Webview, app: AppHandle, state: State<AuthState>) -> Result<(), String> {
    vault::ensure_caller(&webview)?;
    auth::logout(&app, &state);
    Ok(())
}

#[tauri::command]
fn auth_lock(webview: Webview, app: AppHandle, state: State<AuthState>) -> Result<(), String> {
    vault::ensure_caller(&webview)?;
    auth::lock(&app, &state);
    Ok(())
}

#[tauri::command]
fn auth_verify_password(
    webview: Webview,
    app: AppHandle,
    state: State<AuthState>,
    password: String,
) -> Result<(), String> {
    vault::ensure_caller(&webview)?;
    auth::verify(&app, &state, &password)
}

#[tauri::command]
fn auth_change_password(
    webview: Webview,
    app: AppHandle,
    state: State<AuthState>,
    current: String,
    next: String,
    confirm: String,
) -> Result<(), String> {
    vault::ensure_caller(&webview)?;
    auth::change_password(&app, &state, &current, &next, &confirm)
}

#[tauri::command]
fn auth_disable(
    webview: Webview,
    app: AppHandle,
    state: State<AuthState>,
    password: String,
) -> Result<(), String> {
    vault::ensure_caller(&webview)?;
    auth::disable(&app, &state, &password)
}

#[tauri::command]
fn auth_update_lock_settings(
    webview: Webview,
    app: AppHandle,
    state: State<AuthState>,
    minutes: u32,
) -> Result<AuthStatus, String> {
    vault::ensure_caller(&webview)?;
    auth::update_lock_settings(&app, &state, minutes)
}

#[tauri::command]
fn auth_get_current_user(
    webview: Webview,
    app: AppHandle,
    state: State<AuthState>,
) -> Result<Option<String>, String> {
    vault::ensure_caller(&webview)?;
    Ok(auth::status(&app, &state).username)
}

#[tauri::command]
fn vault_status(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
) -> Result<VaultStatus, String> {
    vault::ensure_caller(&webview)?;
    vault::check_idle(&state);
    Ok(vault::status(&app, &state))
}

#[tauri::command]
async fn vault_initialize(
    webview: Webview,
    app: AppHandle,
    state: State<'_, VaultState>,
    password: String,
) -> Result<(), String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::initialize(&app, &state, &password)
}

#[tauri::command]
async fn vault_unlock(
    webview: Webview,
    app: AppHandle,
    state: State<'_, VaultState>,
    password: String,
) -> Result<(), String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::unlock(&app, &state, &password)
}

#[tauri::command]
fn vault_lock(webview: Webview, state: State<VaultState>) -> Result<(), String> {
    gate(webview.app_handle())?;
    vault::ensure_caller(&webview)?;
    vault::lock(&state);
    Ok(())
}

#[tauri::command]
fn vault_list_credentials(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
) -> Result<Vec<CredentialView>, String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::list_credentials(&app, &state)
}

#[tauri::command]
fn vault_add_credential(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
    input: CredentialInput,
) -> Result<CredentialView, String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    if input.credential_id.is_some() {
        return Err("新增凭据不应带 credential_id".into());
    }
    vault::save_credential(&app, &state, input)
}

#[tauri::command]
fn vault_update_credential(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
    input: CredentialInput,
) -> Result<CredentialView, String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    if input.credential_id.is_none() {
        return Err("更新凭据需要 credential_id".into());
    }
    vault::save_credential(&app, &state, input)
}

#[tauri::command]
fn vault_delete_credential(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
    credential_id: String,
) -> Result<(), String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::delete_credential(&app, &state, &credential_id)
}

#[tauri::command]
fn vault_reveal_credential(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
    credential_id: String,
) -> Result<String, String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::reveal_credential(&app, &state, &credential_id)
}

#[tauri::command]
fn vault_authorize_service(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
    credential_id: String,
    days: i64,
) -> Result<GrantView, String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::authorize(&app, &state, &credential_id, days)
}

#[tauri::command]
fn vault_renew_authorization(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
    credential_id: String,
    days: i64,
) -> Result<GrantView, String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::authorize(&app, &state, &credential_id, days)
}

#[tauri::command]
fn vault_list_authorizations(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
) -> Result<Vec<GrantView>, String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    Ok(vault::list_grants(&app, &state))
}

#[tauri::command]
fn vault_revoke_authorization(
    webview: Webview,
    app: AppHandle,
    state: State<VaultState>,
    grant_id: String,
) -> Result<(), String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::revoke(&app, &state, &grant_id)
}

#[tauri::command]
async fn vault_reset(
    webview: Webview,
    app: AppHandle,
    state: State<'_, VaultState>,
) -> Result<(), String> {
    gate(&app)?;
    vault::ensure_caller(&webview)?;
    vault::reset(&app, &state)
}

/// Async on purpose: creating a WebView2 window requires the main event loop
/// to keep running. Sync commands execute on the main thread and would deadlock.
#[tauri::command]
async fn open_service(
    app: AppHandle,
    state: State<'_, WinState>,
    id: String,
) -> Result<(), String> {
    gate(&app)?;
    windows::open(&app, &state, &id)
}

#[tauri::command]
fn hide_all_services(app: AppHandle, state: State<WinState>) -> Result<(), String> {
    gate(&app)?;
    windows::hide_all(&app, &state);
    Ok(())
}

/// Async for the same reason as `open_service`: destroying a webview window
/// must not run on the main thread.
#[tauri::command]
async fn close_service(
    app: AppHandle,
    state: State<'_, WinState>,
    id: String,
) -> Result<(), String> {
    gate(&app)?;
    windows::close_one(&app, &state, &id);
    Ok(())
}

#[tauri::command]
fn set_content_rect(app: AppHandle, state: State<WinState>, rect: Rect) {
    windows::set_rect(&app, &state, rect);
}

#[tauri::command]
fn nav_back(app: AppHandle, state: State<WinState>) -> Result<(), String> {
    gate(&app)?;
    windows::nav_back(&app, &state)
}

#[tauri::command]
fn nav_forward(app: AppHandle, state: State<WinState>) -> Result<(), String> {
    gate(&app)?;
    windows::nav_forward(&app, &state)
}

#[tauri::command]
fn nav_reload(app: AppHandle, state: State<WinState>) -> Result<(), String> {
    gate(&app)?;
    windows::nav_reload(&app, &state)
}

#[tauri::command]
fn nav_home(app: AppHandle, state: State<WinState>) -> Result<(), String> {
    gate(&app)?;
    windows::nav_home(&app, &state)
}

#[tauri::command]
fn active_url(app: AppHandle, state: State<WinState>) -> Result<Option<String>, String> {
    gate(&app)?;
    Ok(windows::active_url(&app, &state))
}

#[tauri::command]
fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    gate(&app)?;
    let parsed = url::Url::parse(url.trim()).map_err(|e| format!("地址无效: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("只允许打开 http / https 地址".into());
    }
    app.opener()
        .open_url(parsed.to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_services(app: AppHandle, ids: Vec<String>) -> Result<Vec<HealthResult>, String> {
    gate(&app)?;
    let cfg = config::load(&app);
    let proxy = app.state::<ChannelState>().proxy_addr();
    // (id, primary URL, via channel, optional public fallback)
    let targets: Vec<(String, String, bool, Option<String>)> = ids
        .iter()
        .filter_map(|id| {
            cfg.services.iter().find(|s| &s.id == id).and_then(|s| {
                s.active_endpoint().map(|endpoint| {
                    (
                        s.id.clone(),
                        endpoint.url.clone(),
                        endpoint.kind == config::EndpointKind::Tailnet,
                        if s.connection_mode == config::ConnectionMode::Failover
                            && endpoint.kind == config::EndpointKind::Tailnet
                        {
                            s.endpoint(config::EndpointKind::Public)
                                .map(|ep| ep.url.clone())
                        } else {
                            None
                        },
                    )
                })
            })
        })
        .filter(|(_, url, _, _)| !url.trim().is_empty())
        .collect();

    tauri::async_runtime::spawn_blocking(move || {
        let mut threads = Vec::with_capacity(targets.len());
        for (id, url, via_channel, fallback) in targets {
            let proxy = proxy.clone();
            threads.push(std::thread::spawn(move || {
                let mut result = if via_channel {
                    match proxy {
                        Some(proxy) => health::probe_via_socks(&proxy, &url),
                        None => HealthResult {
                            id: String::new(),
                            online: false,
                            latency_ms: None,
                            error: Some("私网通道未就绪".into()),
                        },
                    }
                } else {
                    health::probe(&url)
                };
                if !result.online {
                    if let Some(public_url) = fallback {
                        result = health::probe(&public_url);
                    }
                }
                result.id = id;
                result
            }));
        }
        let mut out = Vec::new();
        for handle in threads {
            if let Ok(result) = handle.join() {
                out.push(result);
            }
        }
        out
    })
    .await
    .map_err(|e| format!("健康检查失败: {e}"))
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    gate(&app)?;
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| format!("设置开机自启失败: {e}"))
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        tray::show_main(app);
    }));

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(WinState::default())
        .manage(ChannelState::default())
        .manage(VaultState::default())
        .manage(AuthState::default())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            config_path,
            channel_status,
            channel_start,
            channel_stop,
            channel_logout,
            channel_open_login,
            vault_status,
            vault_initialize,
            vault_unlock,
            vault_lock,
            vault_list_credentials,
            vault_add_credential,
            vault_update_credential,
            vault_delete_credential,
            vault_reveal_credential,
            vault_authorize_service,
            vault_renew_authorization,
            vault_list_authorizations,
            vault_revoke_authorization,
            vault_reset,
            auth_get_status,
            auth_initialize,
            auth_login,
            auth_logout,
            auth_lock,
            auth_verify_password,
            auth_change_password,
            auth_disable,
            auth_update_lock_settings,
            auth_get_current_user,
            auth_reset_with_recovery,
            auth_factory_reset,
            auth_recover_with_code,
            open_service,
            hide_all_services,
            close_service,
            set_content_rect,
            nav_back,
            nav_forward,
            nav_reload,
            nav_home,
            active_url,
            open_external,
            check_services,
            set_autostart,
            quit_app,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Decide whether this is a fresh install before config::load
            // creates default data. Missing auth with existing data fails closed.
            if let Err(err) = auth::bootstrap(&handle) {
                eprintln!("认证状态初始化失败: {err}");
            }
            if let Err(err) = auth::resume_pending_factory_reset(&handle) {
                eprintln!("认证重置尚未完成: {err}");
            }

            // Make sure a config file exists before the UI asks for it.
            let mut cfg = config::load(&handle);
            let _ = config::save(&handle, &cfg);

            // Reconcile the autostart flag with the actual OS registration.
            if let Ok(enabled) = handle.autolaunch().is_enabled() {
                if cfg.settings.autostart != enabled {
                    cfg.settings.autostart = enabled;
                    let _ = config::save(&handle, &cfg);
                }
            }

            tray::build(&handle)?;

            // Bring the built-in tailnet channel up (it will emit a login URL
            // on first run instead of failing).
            if cfg.settings.channel_enabled
                && !auth::factory_reset_pending(&handle)
                && auth::load(&handle).is_ok()
            {
                if let Err(err) = channel::start(&handle) {
                    eprintln!("通道启动失败: {err}");
                    let _ = handle.emit(
                        "channel-status",
                        serde_json::json!({
                            "phase": "error",
                            "message": err,
                        }),
                    );
                }
            }

            // Finish an interrupted "reset clears service sessions" step first.
            auth::resume_pending_session_wipe(&handle);

            // Finish an interrupted vault reset before anything can use the
            // vault or reopen a service window.
            if vault::resume_pending_reset(&handle) {
                eprintln!("上次的保险箱重置未完成，已继续清理");
            }

            // Idle auto-lock for the vault (state lives in Rust, not in Vue).
            {
                let lock_handle = handle.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(15));
                    let state = lock_handle.state::<VaultState>();
                    if vault::check_idle(&state) {
                        drop(state);
                        let _ = lock_handle
                            .emit("vault-status", serde_json::json!({ "unlocked": false }));
                    }
                });
            }
            // Auth idle auto-lock (state lives in Rust).
            {
                let auth_handle = handle.clone();
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_secs(15));
                    let state = auth_handle.state::<AuthState>();
                    auth::tick(&auth_handle, &state);
                });
            }
            if let Some(main) = app.get_webview_window("main") {
                let handle = handle.clone();
                main.on_window_event(move |event| match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        let cfg = config::load(&handle);
                        if cfg.settings.close_to_tray {
                            api.prevent_close();
                            windows::suspend(&handle, &handle.state::<WinState>());
                            if let Some(win) = handle.get_webview_window("main") {
                                let _ = win.hide();
                            }
                        }
                    }
                    WindowEvent::Moved(_)
                    | WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::Focused(_) => {
                        let state = handle.state::<WinState>();
                        let Some(main) = handle.get_webview_window("main") else {
                            return;
                        };
                        let hidden = main.is_minimized().unwrap_or(false)
                            || !main.is_visible().unwrap_or(true);
                        if hidden {
                            if !state.is_suspended() {
                                windows::suspend(&handle, &state);
                            }
                        } else if state.is_suspended() {
                            windows::resume(&handle, &state);
                        } else {
                            windows::sync_glue(&handle);
                        }
                    }
                    _ => {}
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
