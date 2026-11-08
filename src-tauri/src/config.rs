use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn default_true() -> bool {
    true
}

fn default_start_page() -> String {
    "home".to_string()
}

fn default_hostname() -> String {
    "serverdesk".to_string()
}

fn default_mode() -> ConnectionMode {
    ConnectionMode::Tailnet
}

/// How a service is reached. The window layer probes the tailnet endpoint
/// before opening a Failover service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionMode {
    Tailnet,
    Public,
    Failover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndpointKind {
    Tailnet,
    Public,
}

/// One way of reaching a service. `endpoint_id` is stable and is what future
/// credential authorizations will bind to (never the URL string).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub endpoint_id: String,
    pub kind: EndpointKind,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub expected_origin: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub http_ack_at: Option<i64>,
    #[serde(default)]
    pub http_ack_origin: Option<String>,
    #[serde(default)]
    pub http_ack_url: Option<String>,
}

impl Endpoint {
    pub fn new(endpoint_id: String, kind: EndpointKind, url: &str) -> Self {
        Self {
            endpoint_id,
            kind,
            url: url.trim().to_string(),
            expected_origin: origin_of(url),
            enabled: true,
            http_ack_at: None,
            http_ack_origin: None,
            http_ack_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "default_true")]
    pub health_check: bool,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_mode")]
    pub connection_mode: ConnectionMode,
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    /// Only read during the v1 -> v2 migration; never written back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Service {
    fn new(id: &str, name: &str, category: &str, url: &str, icon: &str, sort_order: i32) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            category: category.to_string(),
            icon: icon.to_string(),
            health_check: true,
            sort_order,
            enabled: true,
            connection_mode: ConnectionMode::Tailnet,
            endpoints: vec![Endpoint::new(
                format!("ep-{id}-tailnet"),
                EndpointKind::Tailnet,
                url,
            )],
            url: None,
        }
    }

    pub fn endpoint(&self, kind: EndpointKind) -> Option<&Endpoint> {
        self.endpoints.iter().find(|endpoint| {
            endpoint.kind == kind && endpoint.enabled && !endpoint.url.trim().is_empty()
        })
    }

    /// Which endpoint the service currently uses.
    /// Static preference used by the configuration UI. The window layer
    /// probes and selects the live endpoint for Failover at open time.
    pub fn active_endpoint(&self) -> Option<&Endpoint> {
        match self.connection_mode {
            ConnectionMode::Tailnet => self.endpoint(EndpointKind::Tailnet),
            ConnectionMode::Public => self.endpoint(EndpointKind::Public),
            ConnectionMode::Failover => self
                .endpoint(EndpointKind::Tailnet)
                .or_else(|| self.endpoint(EndpointKind::Public)),
        }
    }
}

fn origin_of(url: &str) -> String {
    match url::Url::parse(url.trim()) {
        Ok(parsed) => match (parsed.scheme(), parsed.host_str()) {
            (scheme, Some(host)) => match parsed.port_or_known_default() {
                Some(port) => format!("{scheme}://{host}:{port}"),
                None => format!("{scheme}://{host}"),
            },
            _ => String::new(),
        },
        Err(_) => String::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    #[serde(default = "default_start_page")]
    pub start_page: String,
    /// Built-in tailnet channel (tsnet sidecar).
    #[serde(default = "default_true")]
    pub channel_enabled: bool,
    #[serde(default = "default_hostname")]
    pub channel_hostname: String,
    /// Optional comma separated tailnet tags, e.g. "tag:serverdesk".
    #[serde(default)]
    pub channel_tags: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            autostart: false,
            close_to_tray: true,
            start_page: default_start_page(),
            channel_enabled: true,
            channel_hostname: default_hostname(),
            channel_tags: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub services: Vec<Service>,
    #[serde(default)]
    pub settings: Settings,
    /// Keys written by a newer ServerDesk are kept as-is instead of being
    /// dropped on the next save.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

pub const CURRENT_VERSION: u32 = 2;

fn default_version() -> u32 {
    CURRENT_VERSION
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            services: vec![
                Service::new(
                    "astrbot",
                    "AstrBot",
                    "AI",
                    "http://YOUR-SERVER:6185",
                    "bot",
                    10,
                ),
                Service::new(
                    "napcat",
                    "NapCat",
                    "AI",
                    "http://YOUR-SERVER:6099",
                    "chat",
                    20,
                ),
                Service::new(
                    "1panel",
                    "1Panel",
                    "服务器",
                    "http://YOUR-SERVER:10086",
                    "dashboard",
                    30,
                ),
                Service::new(
                    "nezha",
                    "哪吒监控",
                    "服务器",
                    "http://YOUR-SERVER:8008",
                    "activity",
                    40,
                ),
            ],
            settings: Settings::default(),
            extra: serde_json::Map::new(),
        }
    }
}

pub fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("无法定位配置目录: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建配置目录: {e}"))?;
    Ok(dir.join("config.json"))
}

pub fn load(app: &AppHandle) -> AppConfig {
    let Ok(path) = config_path(app) else {
        return AppConfig::default();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        let cfg = AppConfig::default();
        let _ = save(app, &cfg);
        return cfg;
    };
    match serde_json::from_str::<AppConfig>(&text) {
        Ok(cfg) => {
            let (migrated, changed) = migrate(cfg);
            if changed {
                // Keep the pre-migration file: a failed upgrade must never
                // leave the user without their original configuration.
                let _ = fs::copy(&path, path.with_extension("json.v1.bak"));
                if let Err(err) = save(app, &migrated) {
                    eprintln!("配置迁移写盘失败，本次仅使用内存中的迁移结果: {err}");
                }
            }
            migrated
        }
        Err(err) => {
            eprintln!("config.json 解析失败，已备份并使用默认配置: {err}");
            let _ = fs::copy(&path, path.with_extension("json.bak"));
            let cfg = AppConfig::default();
            let _ = save(app, &cfg);
            cfg
        }
    }
}

/// v1 -> v2: every service gets an explicit connection mode and endpoint list.
/// Old services become `tailnet` with their existing URL as the tailnet
/// endpoint; nothing is silently turned into a public endpoint.
pub fn migrate(cfg: AppConfig) -> (AppConfig, bool) {
    if cfg.version >= CURRENT_VERSION {
        return (cfg, false);
    }
    let mut cfg = cfg;
    for svc in &mut cfg.services {
        if svc.endpoints.is_empty() {
            if let Some(url) = svc.url.take() {
                if !url.trim().is_empty() {
                    svc.endpoints.push(Endpoint::new(
                        format!("ep-{}-tailnet", svc.id),
                        EndpointKind::Tailnet,
                        &url,
                    ));
                }
            }
        }
        svc.connection_mode = ConnectionMode::Tailnet;
        svc.url = None;
    }
    cfg.version = CURRENT_VERSION;
    (cfg, true)
}

pub fn validate(cfg: &AppConfig) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    let mut endpoints_seen = std::collections::HashSet::new();
    for svc in &cfg.services {
        if svc.id.trim().is_empty() {
            return Err("服务 ID 不能为空".into());
        }
        if !svc
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!("服务 ID 含有非法字符: {}", svc.id));
        }
        if !seen.insert(svc.id.clone()) {
            return Err(format!("服务 ID 重复: {}", svc.id));
        }
        for endpoint in &svc.endpoints {
            if endpoint.endpoint_id.trim().is_empty() {
                return Err(format!("服务 {} 的入口 ID 不能为空", svc.name));
            }
            if !endpoints_seen.insert(endpoint.endpoint_id.clone()) {
                return Err(format!("入口 ID 重复: {}", endpoint.endpoint_id));
            }
            let url = endpoint.url.trim();
            if url.is_empty() {
                return Err(format!(
                    "服务「{}」有一个入口没有填写地址",
                    if svc.name.is_empty() {
                        &svc.id
                    } else {
                        &svc.name
                    }
                ));
            }
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(format!("入口地址必须以 http:// 或 https:// 开头: {url}"));
            }
        }
        if svc.enabled && svc.endpoints.is_empty() {
            return Err(format!(
                "服务「{}」没有配置任何访问入口",
                if svc.name.is_empty() {
                    &svc.id
                } else {
                    &svc.name
                }
            ));
        }
    }
    Ok(())
}

pub fn save(app: &AppHandle, cfg: &AppConfig) -> Result<(), String> {
    validate(cfg)?;
    let path = config_path(app)?;
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("写入配置失败: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("替换配置失败: {e}"))?;
    Ok(())
}
