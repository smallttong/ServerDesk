use serde::Serialize;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::{self, AppConfig, EndpointKind};

/// What the UI needs to know about the built-in tailnet channel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    /// stopped | starting | login | up | error
    pub phase: String,
    pub socks_port: Option<u16>,
    pub login_url: Option<String>,
    pub ip: Option<String>,
    pub message: Option<String>,
    pub hostname: Option<String>,
    pub tags: Option<String>,
}

impl Default for ChannelStatus {
    fn default() -> Self {
        Self {
            phase: "stopped".into(),
            socks_port: None,
            login_url: None,
            ip: None,
            message: None,
            hostname: None,
            tags: None,
        }
    }
}

#[derive(Default)]
pub struct ChannelState {
    child: Mutex<Option<Child>>,
    status: Mutex<ChannelStatus>,
    allowed_targets: Mutex<Vec<String>>,
    generation: AtomicU64,
}

/// The sidecar accepts only targets from enabled private endpoints. Keep the
/// same canonical host:port spelling for startup and live updates.
pub fn allowed_targets(cfg: &AppConfig) -> Vec<String> {
    let mut targets: Vec<String> = cfg
        .services
        .iter()
        .filter(|service| service.enabled)
        .flat_map(|service| service.endpoints.iter())
        .filter(|endpoint| endpoint.enabled && endpoint.kind == EndpointKind::Tailnet)
        .filter_map(|endpoint| url::Url::parse(endpoint.url.trim()).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .filter_map(|url| {
            let host = url
                .host_str()?
                .trim_matches(&['[', ']'][..])
                .trim_end_matches('.')
                .to_ascii_lowercase();
            let port = url.port_or_known_default()?;
            Some(if host.contains(':') {
                format!("[{host}]:{port}")
            } else {
                format!("{host}:{port}")
            })
        })
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

pub fn sync_allowlist(app: &AppHandle, cfg: &AppConfig) -> Result<(), String> {
    let state = app.state::<ChannelState>();
    let targets = allowed_targets(cfg);
    if *state.allowed_targets.lock().unwrap() == targets {
        return Ok(());
    }
    let message = serde_json::json!({ "allowedTargets": targets }).to_string() + "\n";
    let mut child = state.child.lock().unwrap();
    let stdin = child
        .as_mut()
        .and_then(|child| child.stdin.as_mut())
        .ok_or("通道进程未运行")?;
    stdin
        .write_all(message.as_bytes())
        .map_err(|e| format!("更新目标白名单失败: {e}"))?;
    *state.allowed_targets.lock().unwrap() = targets;
    Ok(())
}

impl ChannelState {
    pub fn snapshot(&self) -> ChannelStatus {
        self.status.lock().unwrap().clone()
    }

    /// "127.0.0.1:PORT" while the tunnel is running.
    pub fn proxy_addr(&self) -> Option<String> {
        let status = self.status.lock().unwrap();
        if status.phase != "up" {
            return None;
        }
        status.socks_port.map(|port| format!("127.0.0.1:{port}"))
    }

    /// "socks5://127.0.0.1:PORT" for WebView2's --proxy-server.
    pub fn proxy_url(&self) -> Option<String> {
        let status = self.status.lock().unwrap();
        if status.phase != "up" {
            return None;
        }
        status
            .socks_port
            .map(|port| format!("socks5://127.0.0.1:{port}"))
    }
}

pub fn channel_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法定位配置目录: {e}"))?
        .join("channel-state");
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建通道目录: {e}"))?;
    Ok(dir)
}

/// The Go sidecar ships next to the app executable; in `tauri dev` it comes
/// from src-tauri/binaries.
fn resolve_sidecar() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let mut candidates = vec![dir.join("serverdesk-channel.exe")];
    if let Some(src_tauri) = dir.parent().and_then(|p| p.parent()) {
        candidates.push(
            src_tauri
                .join("binaries")
                .join("serverdesk-channel-x86_64-pc-windows-msvc.exe"),
        );
    }
    candidates.into_iter().find(|path| path.exists())
}

fn stop_child(state: &ChannelState) {
    if let Some(mut child) = state.child.lock().unwrap().take() {
        // Dropping stdin makes the sidecar exit on its own; kill as a fallback.
        drop(child.stdin.take());
        std::thread::sleep(std::time::Duration::from_millis(120));
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn stop(app: &AppHandle) {
    let state = app.state::<ChannelState>();
    state.generation.fetch_add(1, Ordering::SeqCst);
    stop_child(&state);
    state.allowed_targets.lock().unwrap().clear();
    {
        let mut status = state.status.lock().unwrap();
        *status = ChannelStatus {
            hostname: status.hostname.clone(),
            ..Default::default()
        };
    }
    let _ = app.emit("channel-status", state.snapshot());
}

pub fn start(app: &AppHandle) -> Result<ChannelStatus, String> {
    let cfg = config::load(app);
    let state = app.state::<ChannelState>();
    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    stop_child(&state);
    let targets = allowed_targets(&cfg);

    let sidecar =
        resolve_sidecar().ok_or_else(|| "找不到通道程序 serverdesk-channel.exe".to_string())?;
    let dir = channel_dir(app)?;
    let hostname = if cfg.settings.channel_hostname.trim().is_empty() {
        "serverdesk".to_string()
    } else {
        cfg.settings.channel_hostname.trim().to_string()
    };

    let mut command = Command::new(&sidecar);
    command
        .arg("--state-dir")
        .arg(&dir)
        .arg("--hostname")
        .arg(&hostname)
        .arg("--allow-targets")
        .arg(serde_json::to_string(&targets).map_err(|e| e.to_string())?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    if !cfg.settings.channel_tags.trim().is_empty() {
        command.arg("--tags").arg(cfg.settings.channel_tags.trim());
    }

    let mut child = command.spawn().map_err(|e| format!("启动通道失败: {e}"))?;
    let stdout = child.stdout.take().ok_or("无法读取通道输出")?;

    *state.child.lock().unwrap() = Some(child);
    *state.allowed_targets.lock().unwrap() = targets;
    {
        let mut status = state.status.lock().unwrap();
        *status = ChannelStatus {
            phase: "starting".into(),
            hostname: Some(hostname),
            tags: Some(cfg.settings.channel_tags.clone()),
            ..Default::default()
        };
    }
    let _ = app.emit("channel-status", state.snapshot());

    let handle = app.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                handle_event(&handle, generation, value);
            }
        }
        // The sidecar exited (crash or logout): reflect that in the UI.
        let state = handle.state::<ChannelState>();
        let mut status = state.status.lock().unwrap();
        if state.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        if status.phase != "stopped" {
            status.phase = "stopped".into();
            status.message = Some("通道已停止".into());
        }
        drop(status);
        let snapshot = state.snapshot();
        drop(state);
        let _ = handle.emit("channel-status", snapshot);
    });

    Ok(state.snapshot())
}

fn handle_event(app: &AppHandle, generation: u64, value: serde_json::Value) {
    let event = value.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let state = app.state::<ChannelState>();
    let mut became_up = false;
    {
        let mut status = state.status.lock().unwrap();
        if state.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        match event {
            "listening" => {
                status.socks_port = value.get("port").and_then(|v| v.as_u64()).map(|p| p as u16);
            }
            "login" => {
                status.login_url = value
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                status.phase = "login".into();
                status.message = Some("需要在浏览器中完成 Tailscale 授权".into());
            }
            "status" => {
                let backend = value.get("backend").and_then(|v| v.as_str()).unwrap_or("");
                status.ip = value
                    .get("ip")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                if backend == "Running" {
                    if status.phase != "up" {
                        became_up = true;
                    }
                    status.phase = "up".into();
                    status.login_url = None;
                    status.message = None;
                } else if matches!(backend, "NeedsLogin" | "NoState" | "Starting") {
                    if status.phase != "login" {
                        status.phase = "starting".into();
                    }
                } else if !backend.is_empty() {
                    status.message = Some(format!("通道状态: {backend}"));
                }
            }
            "error" => {
                status.phase = "error".into();
                status.message = value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            _ => {}
        }
    }
    let snapshot = state.snapshot();
    drop(state);
    let _ = app.emit("channel-status", snapshot.clone());

    // Windows created before the tunnel came up were loaded without a proxy.
    // Recreating them is cheap here and avoids a silently broken page.
    if became_up && snapshot.phase == "up" {
        crate::windows::reset_service_windows(app);
    }
}

pub fn logout(app: &AppHandle) -> Result<(), String> {
    stop(app);
    let dir = channel_dir(app)?;
    std::fs::remove_dir_all(&dir).map_err(|e| format!("清除通道身份失败: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("重建通道目录失败: {e}"))?;
    let cfg = config::load(app);
    if cfg.settings.channel_enabled {
        start(app)?;
    }
    Ok(())
}

#[cfg(test)]
mod allowlist_tests {
    use super::*;

    #[test]
    fn only_enabled_private_endpoints_enter_the_allowlist() {
        let mut cfg = AppConfig::default();
        cfg.services.truncate(1);
        cfg.services[0].endpoints[0].url = "http://100.64.1.2:6185/path".into();
        cfg.services[0].endpoints.push(config::Endpoint::new(
            "public".into(),
            EndpointKind::Public,
            "https://panel.example.com",
        ));
        assert_eq!(allowed_targets(&cfg), vec!["100.64.1.2:6185"]);
        cfg.services[0].enabled = false;
        assert!(allowed_targets(&cfg).is_empty());
    }
}
