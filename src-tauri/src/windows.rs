use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{
    webview::PageLoadEvent, AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::config::{self, AppConfig, EndpointKind};

const INIT_SCRIPT: &str = r#"
(function () {
  try {
    var nativeOpen = window.open;
    window.open = function (url, target, features) {
      try {
        if (!url) return null;
        var abs = new URL(String(url), location.href);
        if (abs.protocol === 'http:' || abs.protocol === 'https:') {
          location.href = abs.href;
          return null;
        }
      } catch (e) {}
      return nativeOpen ? nativeOpen.call(window, url, target, features) : null;
    };
  } catch (e) {}
})();
"#;

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// WebView2 browser arguments belong to the environment, and the environment
/// is identified by its user data folder — so "proxied" and "direct" traffic
/// cannot share one profile.
pub const PROFILE_TAILNET: &str = "services-webview";
pub const PROFILE_PUBLIC: &str = "services-webview-public";

/// What a service window was built for. A window whose network path no longer
/// matches the service's active endpoint has to be recreated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowMeta {
    pub endpoint_id: String,
    pub profile: String,
    pub proxy_port: Option<u16>,
}

/// Shared state for the embedded service layer:
/// - `rect` is the content area (in CSS px) reported by the Vue shell.
/// - `active` is the label of the currently visible service window.
/// - `meta` records the network path each service window was created with.
/// - `suspended` is set while the shell is minimized or hidden to tray; service
///   windows must stay invisible then, even if a background event re-opens them.
#[derive(Default)]
pub struct WinState {
    pub rect: Mutex<Rect>,
    pub active: Mutex<Option<String>>,
    pub meta: Mutex<HashMap<String, WindowMeta>>,
    pub suspended: Mutex<bool>,
}

impl WinState {
    pub fn is_suspended(&self) -> bool {
        *self.suspended.lock().unwrap()
    }
}

/// Hides service windows and blocks them from being shown until the shell is
/// visible again. `active` is kept so the window can be restored.
pub fn suspend(app: &AppHandle, state: &WinState) {
    *state.suspended.lock().unwrap() = true;
    for (label, win) in app.webview_windows() {
        if label.starts_with("svc-") {
            let _ = win.hide();
        }
    }
}

/// Shell is visible again: bring the active service window back.
pub fn resume(app: &AppHandle, state: &WinState) {
    *state.suspended.lock().unwrap() = false;
    let rect = *state.rect.lock().unwrap();
    if let Some(label) = state.active.lock().unwrap().clone() {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = apply_geometry(app, rect, &win);
            let _ = win.show();
        }
    }
}

/// True when the shell can carry a visible service window right now.
/// Deliberately derived from the live window state: a startup focus event can
/// arrive before the shell is mapped, and latching that as "suspended" would
/// keep every service window hidden forever.
pub fn shell_can_show(app: &AppHandle, _state: &WinState) -> bool {
    match app.get_webview_window("main") {
        Some(main) => !main.is_minimized().unwrap_or(false) && main.is_visible().unwrap_or(true),
        None => false,
    }
}

pub fn label_for(id: &str) -> String {
    format!("svc-{id}")
}

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())
}

/// Hosts the user has explicitly configured (in any endpoint) plus loopback
/// are the only origins allowed to keep rendering inside the app.
fn allowed_hosts(cfg: &AppConfig) -> Vec<String> {
    let mut hosts: Vec<String> = cfg
        .services
        .iter()
        .flat_map(|service| service.endpoints.iter())
        .filter_map(|endpoint| Url::parse(endpoint.url.trim()).ok())
        .filter_map(|url| {
            url.host_str()
                .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
        })
        .collect();
    for extra in ["localhost", "127.0.0.1", "::1", "0.0.0.0"] {
        hosts.push(extra.to_string());
    }
    hosts.sort();
    hosts.dedup();
    hosts
}

fn host_allowed(url: &Url, hosts: &[String]) -> bool {
    match url.scheme() {
        "about" | "data" | "blob" => true,
        "http" | "https" => {
            let Some(host) = url.host_str() else {
                return false;
            };
            let host = host.trim_end_matches('.').to_ascii_lowercase();
            hosts.iter().any(|h| h == &host)
        }
        _ => false,
    }
}

fn requires_http_confirmation(endpoint: &config::Endpoint) -> bool {
    endpoint.kind == EndpointKind::Public
        && endpoint.url.trim().starts_with("http://")
        && (endpoint.http_ack_at.is_none()
            || endpoint.http_ack_url.as_deref() != Some(endpoint.url.trim()))
}

#[cfg(test)]
mod http_confirmation_tests {
    use super::*;

    #[test]
    fn confirmation_is_bound_to_the_full_public_http_address() {
        let mut endpoint =
            config::Endpoint::new("ep".into(), EndpointKind::Public, "http://panel.test/a");
        assert!(requires_http_confirmation(&endpoint));
        endpoint.http_ack_at = Some(1);
        endpoint.http_ack_url = Some(endpoint.url.clone());
        assert!(!requires_http_confirmation(&endpoint));
        endpoint.url = "http://panel.test/b".into();
        assert!(requires_http_confirmation(&endpoint));
        endpoint.url = "https://panel.test/b".into();
        assert!(!requires_http_confirmation(&endpoint));
    }
}

/// Breathing room around the embedded page so it reads as a card inside the
/// shell instead of a pane bleeding to every edge.
const SERVICE_INSET: f64 = 6.0;

fn apply_geometry(app: &AppHandle, rect: Rect, win: &WebviewWindow) -> Result<(), String> {
    if rect.width < 40.0 || rect.height < 40.0 {
        return Ok(());
    }
    let rect = Rect {
        x: rect.x + SERVICE_INSET,
        y: rect.y + SERVICE_INSET,
        width: (rect.width - SERVICE_INSET * 2.0).max(80.0),
        height: (rect.height - SERVICE_INSET * 2.0).max(80.0),
    };
    let main = main_window(app)?;
    let scale = main.scale_factor().map_err(|e| e.to_string())?;
    let inner = main.inner_position().map_err(|e| e.to_string())?;

    let x = inner.x + (rect.x * scale).round() as i32;
    let y = inner.y + (rect.y * scale).round() as i32;

    // The WebView2 viewport is a few pixels wider than the visible client area
    // (it is sized from the window rect, not the client rect), so the rect
    // reported by the page can overhang the shell. Clamp to the client area so
    // the service window never spills past the shell's border.
    let client = main.inner_size().map_err(|e| e.to_string())?;
    let avail_w = client.width as i64 - (rect.x * scale).round() as i64;
    let avail_h = client.height as i64 - (rect.y * scale).round() as i64;
    let width = ((rect.width * scale).round() as i64).min(avail_w).max(80) as u32;
    let height = ((rect.height * scale).round() as i64).min(avail_h).max(80) as u32;

    let _ = win.set_size(PhysicalSize::new(width, height));
    let _ = win.set_position(PhysicalPosition::new(x, y));
    Ok(())
}

pub fn set_rect(app: &AppHandle, state: &WinState, rect: Rect) {
    *state.rect.lock().unwrap() = rect;
    if let Some(label) = state.active.lock().unwrap().clone() {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = apply_geometry(app, rect, &win);
        }
    }
}

/// Re-align the service window after the main window moves or resizes.
pub fn sync_glue(app: &AppHandle) {
    let rect = {
        let state = app.state::<WinState>();
        let guard = state.rect.lock().unwrap();
        *guard
    };
    let label = {
        let state = app.state::<WinState>();
        let guard = state.active.lock().unwrap();
        guard.clone()
    };
    if let Some(label) = label {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = apply_geometry(app, rect, &win);
        }
    }
}

pub fn open(app: &AppHandle, state: &WinState, id: &str) -> Result<(), String> {
    let cfg = config::load(app);
    let service = cfg
        .services
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or_else(|| "服务不存在".to_string())?;

    let endpoint = match service.connection_mode {
        config::ConnectionMode::Tailnet => service.endpoint(EndpointKind::Tailnet),
        config::ConnectionMode::Public => service.endpoint(EndpointKind::Public),
        config::ConnectionMode::Failover => {
            let private = service.endpoint(EndpointKind::Tailnet);
            let reachable = private
                .and_then(|ep| {
                    app.state::<crate::channel::ChannelState>()
                        .proxy_addr()
                        .map(|proxy| (ep, proxy))
                })
                .map(|(ep, proxy)| crate::health::probe_via_socks(&proxy, &ep.url).online)
                .unwrap_or(false);
            if reachable {
                private
            } else {
                service.endpoint(EndpointKind::Public)
            }
        }
    }
    .cloned()
    .ok_or_else(|| {
        format!(
            "「{}」没有可用的访问入口",
            if service.name.is_empty() {
                &service.id
            } else {
                &service.name
            }
        )
    })?;

    if requires_http_confirmation(&endpoint) {
        return Err("HTTP_CONFIRM_REQUIRED".into());
    }

    let is_tailnet = endpoint.kind == EndpointKind::Tailnet;
    let proxy_url = if is_tailnet {
        app.state::<crate::channel::ChannelState>()
            .proxy_url()
            .ok_or_else(|| "私网通道未就绪：请先在设置里完成 Tailscale 授权".to_string())?
            .into()
    } else {
        None
    };
    let proxy_parsed = proxy_url.as_deref().and_then(|raw| Url::parse(raw).ok());
    let profile = if is_tailnet {
        PROFILE_TAILNET
    } else {
        PROFILE_PUBLIC
    };
    let desired = WindowMeta {
        endpoint_id: endpoint.endpoint_id.clone(),
        profile: profile.to_string(),
        proxy_port: proxy_parsed.as_ref().and_then(|url| url.port()),
    };

    let label = label_for(id);
    let rect = *state.rect.lock().unwrap();
    let previous = state.active.lock().unwrap().clone();

    if let Some(win) = app.get_webview_window(&label) {
        let same_path = state
            .meta
            .lock()
            .unwrap()
            .get(&label)
            .map(|meta| *meta == desired)
            .unwrap_or(false);
        if same_path {
            apply_geometry(app, rect, &win)?;
            // While the shell is minimized or hidden to tray, opening a service
            // must not pop a window onto the desktop.
            if shell_can_show(app, state) {
                win.show().map_err(|e| format!("显示服务窗口失败: {e}"))?;
                let _ = win.set_focus();
            }
            // No reload happens for a window that kept its network path, so the
            // page-load hook never fires. The page may still be sitting on a
            // login form (session expired), so re-inject here; the throttle
            // keyed on window + URL keeps this from spamming the page.
            crate::vault::try_autologin(
                app,
                &win,
                &service.id,
                &endpoint.endpoint_id,
                &endpoint.expected_origin,
            );
            hide_previous(app, previous.as_deref(), &label);
            *state.active.lock().unwrap() = Some(label);
            return Ok(());
        }
        // Network path changed (mode switch, channel came up, endpoint edited):
        // the WebView2 environment cannot be reconfigured in place.
        let _ = win.destroy();
        state.meta.lock().unwrap().remove(&label);
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    let url = Url::parse(endpoint.url.trim()).map_err(|e| format!("地址无效: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("只支持 http / https 地址".into());
    }

    let main = main_window(app)?;
    let hosts = allowed_hosts(&cfg);
    let nav_app = app.clone();

    // Auto-login context: the adapter only ever acts for this exact
    // service / endpoint / origin triple.
    let login_service = service.id.clone();
    let login_origin = endpoint.expected_origin.clone();
    let login_endpoint = endpoint.endpoint_id.clone();
    let login_on_load = login_service.clone();
    let login_origin_on_load = login_origin.clone();
    let login_endpoint_on_load = login_endpoint.clone();

    let mut builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(if service.name.is_empty() {
            "服务"
        } else {
            &service.name
        })
        .decorations(false)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(true)
        .shadow(false)
        .visible(false)
        .initialization_script(INIT_SCRIPT)
        .on_page_load(move |window, payload| {
            // Only after the document finished loading. `Started` fires at
            // ContentLoading, and panels that hard-navigate (`/` -> `/login`,
            // as fnOS does) throw that document away, taking the injected
            // polling script with it. The Finished event of the replacement
            // document gets its own injection.
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            crate::vault::try_autologin(
                window.app_handle(),
                &window,
                &login_on_load,
                &login_endpoint_on_load,
                &login_origin_on_load,
            );
        })
        .on_navigation(move |target| {
            if host_allowed(target, &hosts) {
                true
            } else {
                let target = target.to_string();
                let handle = nav_app.clone();
                std::thread::spawn(move || {
                    let _ = handle.opener().open_url(target, None::<&str>);
                });
                false
            }
        });

    // One WebView2 environment per network path: tailnet traffic is proxied
    // through the sidecar, public traffic is not, and browser arguments cannot
    // be shared between the two.
    if let Ok(base) = app.path().app_local_data_dir() {
        let data_dir = base.join(profile);
        let _ = std::fs::create_dir_all(&data_dir);
        builder = builder.data_directory(data_dir);
    }
    if let Some(proxy) = proxy_parsed {
        builder = builder.proxy_url(proxy);
    }

    // Owned by the main window so it always sits above the shell.
    builder = builder
        .parent(&main)
        .map_err(|e| format!("设置窗口归属失败: {e}"))?;

    if rect.width > 40.0 && rect.height > 40.0 {
        builder = builder.inner_size(rect.width, rect.height);
    }

    let win = builder
        .build()
        .map_err(|e| format!("创建服务窗口失败: {e}"))?;

    /// Windows 11 rounds decorated windows but not borderless ones, so the page
    /// corner looks square against the shell. Ask DWM for rounded corners.
    #[cfg(windows)]
    fn round_corners(win: &WebviewWindow) {
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;

        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
        const DWMWCP_ROUND: i32 = 2;

        if let Ok(hwnd) = win.hwnd() {
            let preference = DWMWCP_ROUND;
            unsafe {
                DwmSetWindowAttribute(
                    hwnd.0 as *mut core::ffi::c_void,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &preference as *const i32 as *const core::ffi::c_void,
                    std::mem::size_of::<i32>() as u32,
                );
            }
        }
    }

    #[cfg(not(windows))]
    fn round_corners(_win: &WebviewWindow) {}

    state.meta.lock().unwrap().insert(label.clone(), desired);
    hide_previous(app, previous.as_deref(), &label);
    *state.active.lock().unwrap() = Some(label.clone());
    round_corners(&win);
    apply_geometry(app, rect, &win)?;
    if shell_can_show(app, state) {
        win.show().map_err(|e| format!("显示服务窗口失败: {e}"))?;
        let _ = win.set_focus();
    }

    // SPA logins (hash routes) don't fire another page load, so watch the URL
    // and re-run the adapter when the app lands on a login route.
    {
        let handle = app.clone();
        let poll_label = label.clone();
        std::thread::spawn(move || {
            let mut last = String::new();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1200));
                let Some(window) = handle.get_webview_window(&poll_label) else {
                    break;
                };
                let Ok(url) = window.url() else { continue };
                let current = url.to_string();
                if current == last {
                    continue;
                }
                last = current;
                if looks_like_login(&url) {
                    crate::vault::try_autologin(
                        &handle,
                        &window,
                        &login_service,
                        &login_endpoint,
                        &login_origin,
                    );
                }
            }
        });
    }

    Ok(())
}

fn looks_like_login(url: &Url) -> bool {
    let mut text = String::from(url.path());
    if let Some(query) = url.query() {
        text.push('?');
        text.push_str(query);
    }
    if let Some(fragment) = url.fragment() {
        text.push('#');
        text.push_str(fragment);
    }
    let text = text.to_ascii_lowercase();
    ["login", "signin", "sign-in", "/auth"]
        .iter()
        .any(|key| text.contains(key))
}

/// Only one service window is visible at a time; the others stay alive so
/// switching back keeps their session, scroll position and WebSocket state.
fn hide_previous(app: &AppHandle, previous: Option<&str>, current: &str) {
    if let Some(prev) = previous {
        if prev != current {
            if let Some(win) = app.get_webview_window(prev) {
                let _ = win.hide();
            }
        }
    }
}

pub fn hide_all(app: &AppHandle, state: &WinState) {
    for (label, win) in app.webview_windows() {
        if label.starts_with("svc-") {
            let _ = win.hide();
        }
    }
    *state.active.lock().unwrap() = None;
}

pub fn close_one(app: &AppHandle, state: &WinState, id: &str) {
    let label = label_for(id);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.destroy();
    }
    state.meta.lock().unwrap().remove(&label);
    let mut active = state.active.lock().unwrap();
    if active.as_deref() == Some(label.as_str()) {
        *active = None;
    }
}

/// Called when the tunnel transitions to "up". A restarted sidecar has a new
/// proxy port, so stale tailnet windows must be recreated.
pub fn reset_service_windows(app: &AppHandle) {
    let state = app.state::<WinState>();
    let current_port = app
        .state::<crate::channel::ChannelState>()
        .snapshot()
        .socks_port;
    let stale: Vec<String> = {
        let meta = state.meta.lock().unwrap();
        meta.iter()
            .filter(|(_, m)| m.profile == PROFILE_TAILNET && m.proxy_port != current_port)
            .map(|(label, _)| label.clone())
            .collect()
    };
    for label in stale {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.destroy();
        }
        state.meta.lock().unwrap().remove(&label);
    }
    let mut active = state.active.lock().unwrap();
    if active
        .as_deref()
        .map(|label| !app.webview_windows().contains_key(label))
        .unwrap_or(false)
    {
        *active = None;
    }
}

fn with_active<F>(app: &AppHandle, state: &WinState, f: F) -> Result<(), String>
where
    F: FnOnce(&WebviewWindow) -> Result<(), String>,
{
    let label = state
        .active
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "当前没有打开的服务".to_string())?;
    let win = app
        .get_webview_window(&label)
        .ok_or_else(|| "服务窗口不存在".to_string())?;
    f(&win)
}

pub fn nav_back(app: &AppHandle, state: &WinState) -> Result<(), String> {
    with_active(app, state, |w| {
        w.eval("history.back()").map_err(|e| e.to_string())
    })
}

pub fn nav_forward(app: &AppHandle, state: &WinState) -> Result<(), String> {
    with_active(app, state, |w| {
        w.eval("history.forward()").map_err(|e| e.to_string())
    })
}

pub fn nav_reload(app: &AppHandle, state: &WinState) -> Result<(), String> {
    with_active(app, state, |w| {
        w.eval("location.reload()").map_err(|e| e.to_string())
    })
}

pub fn nav_home(app: &AppHandle, state: &WinState) -> Result<(), String> {
    let cfg = config::load(app);
    let label = state
        .active
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "当前没有打开的服务".to_string())?;
    let service = cfg
        .services
        .iter()
        .find(|s| label_for(&s.id) == label)
        .ok_or_else(|| "服务配置不存在".to_string())?;
    let selected_id = state
        .meta
        .lock()
        .unwrap()
        .get(&label)
        .map(|meta| meta.endpoint_id.clone());
    let home = service
        .endpoints
        .iter()
        .find(|endpoint| Some(&endpoint.endpoint_id) == selected_id.as_ref())
        .map(|endpoint| endpoint.url.trim().to_string())
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "该服务没有可用的访问入口".to_string())?;
    let target = serde_json::to_string(&home).map_err(|e| e.to_string())?;
    let win = app
        .get_webview_window(&label)
        .ok_or_else(|| "服务窗口不存在".to_string())?;
    if shell_can_show(app, &app.state::<WinState>()) {
        let _ = win.show();
    }
    win.eval(&format!("location.href = {target};"))
        .map_err(|e| e.to_string())
}

pub fn active_url(app: &AppHandle, state: &WinState) -> Option<String> {
    let label = state.active.lock().unwrap().clone()?;
    let win = app.get_webview_window(&label)?;
    win.url().ok().map(|u| u.to_string())
}

/// Used by the vault reset: drop every service webview and its browsing data.
/// The deletion WebView2 performs is asynchronous, so we poll the cookie jar
/// before destroying the window that asked for it.
pub fn clear_and_destroy_service_windows(app: &AppHandle) {
    let state = app.state::<WinState>();
    let labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|label| label.starts_with("svc-"))
        .cloned()
        .collect();

    let mut probe: Option<String> = None;
    for label in &labels {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.clear_all_browsing_data();
            probe = Some(label.clone());
        }
    }

    if let Some(label) = probe {
        if let Some(win) = app.get_webview_window(&label) {
            for _ in 0..20 {
                std::thread::sleep(std::time::Duration::from_millis(150));
                if win
                    .cookies()
                    .map(|cookies| cookies.is_empty())
                    .unwrap_or(false)
                {
                    break;
                }
            }
        }
    }

    for label in labels {
        if let Some(win) = app.get_webview_window(&label) {
            let _ = win.destroy();
        }
        state.meta.lock().unwrap().remove(&label);
    }
    *state.active.lock().unwrap() = None;
}

/// Deletes the WebView2 profiles used by service windows. Returns false when a
/// browser process still holds the files; the caller keeps the reset pending.
pub fn purge_service_profiles(app: &AppHandle) -> bool {
    let Ok(base) = app.path().app_local_data_dir() else {
        return false;
    };
    let mut clean = true;
    for profile in [PROFILE_TAILNET, PROFILE_PUBLIC] {
        let dir = base.join(profile);
        if !dir.exists() {
            continue;
        }
        for _ in 0..10 {
            if std::fs::remove_dir_all(&dir).is_ok() || !dir.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        if dir.exists() {
            clean = false;
        }
    }
    clean
}
