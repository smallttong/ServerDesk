use serde::Serialize;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};
use url::Url;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResult {
    pub id: String,
    pub online: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// Lightweight liveness probe: TCP connect to the service's host/port.
/// This intentionally avoids HTTP parsing so that redirects, auth walls,
/// self-signed certificates or CORS never produce false negatives.
pub fn probe(url: &str) -> HealthResult {
    let parsed = match Url::parse(url.trim()) {
        Ok(u) => u,
        Err(e) => {
            return HealthResult {
                id: String::new(),
                online: false,
                latency_ms: None,
                error: Some(format!("地址无效: {e}")),
            }
        }
    };

    if !matches!(parsed.scheme(), "http" | "https") {
        return HealthResult {
            id: String::new(),
            online: false,
            latency_ms: None,
            error: Some("仅支持 http / https 地址".into()),
        };
    }

    let Some(host) = parsed.host_str().map(|h| h.to_string()) else {
        return HealthResult {
            id: String::new(),
            online: false,
            latency_ms: None,
            error: Some("地址缺少主机名".into()),
        };
    };
    let port = parsed.port_or_known_default().unwrap_or(80);
    let timeout = Duration::from_millis(1800);

    let addrs: Vec<_> = match (host.as_str(), port).to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => {
            return HealthResult {
                id: String::new(),
                online: false,
                latency_ms: None,
                error: Some(format!("域名解析失败: {e}")),
            }
        }
    };

    if addrs.is_empty() {
        return HealthResult {
            id: String::new(),
            online: false,
            latency_ms: None,
            error: Some("域名解析失败".into()),
        };
    }

    let start = Instant::now();
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, timeout).is_ok() {
            return HealthResult {
                id: String::new(),
                online: true,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                error: None,
            };
        }
    }

    HealthResult {
        id: String::new(),
        online: false,
        latency_ms: None,
        error: Some("连接超时或被拒绝".into()),
    }
}

/// Probe through the built-in channel: a SOCKS5 CONNECT to the service's
/// host/port, exactly the path the WebView will take.
pub fn probe_via_socks(proxy: &str, url: &str) -> HealthResult {
    let parsed = match Url::parse(url.trim()) {
        Ok(u) => u,
        Err(e) => {
            return HealthResult {
                id: String::new(),
                online: false,
                latency_ms: None,
                error: Some(format!("地址无效: {e}")),
            }
        }
    };

    let Some(host) = parsed.host_str().map(|h| h.to_string()) else {
        return HealthResult {
            id: String::new(),
            online: false,
            latency_ms: None,
            error: Some("地址缺少主机名".into()),
        };
    };
    let port = parsed.port_or_known_default().unwrap_or(80);
    let timeout = Duration::from_millis(3000);
    let start = Instant::now();

    match socks5_connect(proxy, &host, port, timeout) {
        Ok(_) => HealthResult {
            id: String::new(),
            online: true,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
        Err(err) => HealthResult {
            id: String::new(),
            online: false,
            latency_ms: None,
            error: Some(err),
        },
    }
}

fn socks5_connect(proxy: &str, host: &str, port: u16, timeout: Duration) -> Result<(), String> {
    let proxy_addr = proxy
        .parse::<std::net::SocketAddr>()
        .map_err(|e| format!("代理地址无效: {e}"))?;
    let mut stream = TcpStream::connect_timeout(&proxy_addr, timeout)
        .map_err(|e| format!("连接通道失败: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;

    // Greeting: SOCKS5, no auth.
    stream
        .write_all(&[5, 1, 0])
        .map_err(|e| format!("通道握手失败: {e}"))?;
    let mut reply = [0u8; 2];
    stream
        .read_exact(&mut reply)
        .map_err(|e| format!("通道握手失败: {e}"))?;
    if reply[0] != 5 || reply[1] != 0 {
        return Err("通道拒绝了连接请求".into());
    }

    // CONNECT request, with the hostname resolved inside the tailnet.
    let mut request = vec![5u8, 1, 0];
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            request.push(1);
            request.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            request.push(4);
            request.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            if host.len() > 255 {
                return Err("主机名过长".into());
            }
            request.push(3);
            request.push(host.len() as u8);
            request.extend_from_slice(host.as_bytes());
        }
    }
    request.extend_from_slice(&port.to_be_bytes());
    stream
        .write_all(&request)
        .map_err(|e| format!("发送连接请求失败: {e}"))?;

    let mut head = [0u8; 4];
    stream
        .read_exact(&mut head)
        .map_err(|e| format!("通道无响应: {e}"))?;
    if head[0] != 5 {
        return Err("通道响应格式错误".into());
    }
    if head[1] != 0 {
        return Err(match head[1] {
            1 => "通道内部错误".into(),
            2 => "通道规则禁止访问".into(),
            3 => "网络不可达".into(),
            4 => "主机不可达".into(),
            5 => "连接被拒绝".into(),
            6 => "TTL 过期".into(),
            _ => format!("通道错误码 {}", head[1]),
        });
    }

    // Drain the bound address so the socket is left in a clean state.
    let skip = match head[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut len = [0u8; 1];
            let _ = stream.read_exact(&mut len);
            len[0] as usize
        }
        _ => 0,
    };
    let mut bound = vec![0u8; skip + 2];
    let _ = stream.read_exact(&mut bound);
    Ok(())
}
