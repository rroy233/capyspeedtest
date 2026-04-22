//! 真实网络测速模块：负责 TCP Ping、HTTP 下载/上传测速、NAT 类型检测。
//!
//! 所有测速均通过 SOCKS5 代理发送真实网络请求，不再使用模拟数据。

use native_tls::TlsConnector;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream as StdTcpStream, ToSocketAddrs};
use std::thread;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{lookup_host, TcpStream};

pub const DOWNLOAD_SOURCE_TELE2: &str = "tele2";
pub const DOWNLOAD_SOURCE_CLOUDFLARE: &str = "cloudflare";
pub const TELE2_DOWNLOAD_URL: &str = "http://speedtest.tele2.net/10MB.zip";
pub const CLOUDFLARE_DOWNLOAD_URL: &str = "https://speed.cloudflare.com/__down?bytes=25000000";
static NOCACHE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn normalize_download_source(source: &str) -> &'static str {
    match source.trim().to_ascii_lowercase().as_str() {
        DOWNLOAD_SOURCE_TELE2 => DOWNLOAD_SOURCE_TELE2,
        _ => DOWNLOAD_SOURCE_CLOUDFLARE,
    }
}

pub fn download_url_for_source(source: &str) -> &'static str {
    match normalize_download_source(source) {
        DOWNLOAD_SOURCE_TELE2 => TELE2_DOWNLOAD_URL,
        _ => CLOUDFLARE_DOWNLOAD_URL,
    }
}

fn should_use_dynamic_nocache(parsed: &url::Url) -> bool {
    parsed
        .host_str()
        .map(|host| host.eq_ignore_ascii_case("speed.cloudflare.com"))
        .unwrap_or(false)
        && parsed.path() == "/__down"
}

fn next_nocache_value() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = NOCACHE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("0.{}{}", nanos, seq)
}

fn with_dynamic_nocache(base: &url::Url) -> String {
    let mut updated = base.clone();
    updated
        .query_pairs_mut()
        .append_pair("nocache", &next_nocache_value());
    updated.to_string()
}

async fn connect_socks5(
    proxy_url: &str,
    target_host: &str,
    target_port: u16,
) -> Result<TcpStream, String> {
    let (proxy_host, proxy_port) = parse_socks5_proxy(proxy_url)?;

    info!("[SOCKS5] 正在连接代理 {}:{}", proxy_host, proxy_port);

    // Connect to SOCKS5 proxy
    let mut stream = TcpStream::connect((proxy_host.as_str(), proxy_port))
        .await
        .map_err(|e| format!("Failed to connect to SOCKS5 proxy: {}", e))?;

    info!("[SOCKS5] 已连接代理，开始 SOCKS5 握手...");

    // 1. SOCKS5 greeting
    stream
        .write_all(&[0x05, 0x01, 0x00])
        .await
        .map_err(|e| format!("Failed to send SOCKS5 greeting: {}", e))?;

    let mut response = [0u8; 2];
    stream
        .read_exact(&mut response)
        .await
        .map_err(|e| format!("Failed to read SOCKS5 greeting response: {}", e))?;

    info!("[SOCKS5] 握手响应: {:02x}", response[1]);

    if response[0] != 0x05 || response[1] != 0x00 {
        return Err(format!(
            "SOCKS5 authentication failed or unsupported: {:?}",
            response
        ));
    }

    // 2. SOCKS5 connect request
    let mut req = vec![0x05, 0x01, 0x00, 0x03]; // CONNECT, DOMAINNAME
    req.push(target_host.len() as u8);
    req.extend_from_slice(target_host.as_bytes());
    req.push((target_port >> 8) as u8);
    req.push((target_port & 0xFF) as u8);

    info!(
        "[SOCKS5] 发送连接请求到目标 {}:{}",
        target_host, target_port
    );
    stream
        .write_all(&req)
        .await
        .map_err(|e| format!("Failed to send SOCKS5 connect request: {}", e))?;

    // 3. Read SOCKS5 connect response
    let mut resp_header = [0u8; 4];
    stream
        .read_exact(&mut resp_header)
        .await
        .map_err(|e| format!("Failed to read SOCKS5 connect response header: {}", e))?;

    info!(
        "[SOCKS5] 连接响应: rep={}, atyp={}",
        resp_header[1], resp_header[3]
    );

    if resp_header[0] != 0x05 || resp_header[1] != 0x00 {
        return Err(format!(
            "SOCKS5 connection to target failed: {:?}",
            resp_header
        ));
    }

    let atyp = resp_header[3];
    match atyp {
        0x01 => {
            // IPv4
            let mut addr = [0u8; 6];
            stream
                .read_exact(&mut addr)
                .await
                .map_err(|e| format!("Failed to read SOCKS5 IPv4 address: {}", e))?;
        }
        0x03 => {
            // Domain
            let mut len_buf = [0u8; 1];
            stream
                .read_exact(&mut len_buf)
                .await
                .map_err(|e| format!("Failed to read SOCKS5 domain length: {}", e))?;
            let len = len_buf[0] as usize;
            let mut addr = vec![0u8; len + 2]; // Domain + port
            stream
                .read_exact(&mut addr)
                .await
                .map_err(|e| format!("Failed to read SOCKS5 domain address: {}", e))?;
        }
        0x04 => {
            // IPv6
            let mut addr = [0u8; 18];
            stream
                .read_exact(&mut addr)
                .await
                .map_err(|e| format!("Failed to read SOCKS5 IPv6 address: {}", e))?;
        }
        _ => return Err(format!("Unknown SOCKS5 ATYP: {}", atyp)),
    }

    info!("[SOCKS5] 连接成功!");
    Ok(stream)
}

fn elapsed_ms_ceil(start: Instant) -> u32 {
    // 避免 as_millis 向下取整导致 0ms
    let micros = start.elapsed().as_micros() as u64;
    ((micros + 999) / 1000).max(1) as u32
}

const TCPING_TIMES: usize = 6;
const TCPING_TIMEOUT: Duration = Duration::from_secs(3);
const TCPING_INTERVAL_SUCCESS: Duration = Duration::from_secs(1);
const TCPING_INTERVAL_FAIL: Duration = Duration::from_millis(200);

const SITEPING_TIMES: usize = 10;
const SITEPING_FAIL_LIMIT: usize = 2;
const SITEPING_TIMEOUT: Duration = Duration::from_secs(10);
const SITEPING_INTERVAL: Duration = Duration::from_millis(500);

struct TcpingResult {
    latencies: Vec<u64>,
    avg_ms: f64,
    loss_pct: f64,
}

struct SitePingResult {
    latencies: Vec<u64>,
    avg_ms: f64,
    loss_pct: f64,
}

fn resolve_host(host: &str, port: u16) -> std::io::Result<SocketAddr> {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "host 为空，无法进行 DNS 解析",
        ));
    }

    let addr = format!("{}:{}", trimmed, port);
    debug!(
        "[TCPPing] DNS 解析开始: raw_host='{}', normalized_host='{}', port={}",
        host, trimmed, port
    );

    let mut addrs = addr.to_socket_addrs().map_err(|e| {
        warn!(
            "[TCPPing] DNS 解析失败: raw_host='{}', normalized_host='{}', port={}, err={}",
            host, trimmed, port, e
        );
        std::io::Error::new(
            e.kind(),
            format!(
                "DNS 解析失败(host='{}', normalized='{}', port={}): {}",
                host, trimmed, port, e
            ),
        )
    })?;

    let resolved = addrs.next().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "DNS 解析结果为空(host='{}', normalized='{}', port={})",
                host, trimmed, port
            ),
        )
    })?;
    debug!(
        "[TCPPing] DNS 解析成功: host='{}', port={}, resolved={}",
        trimmed, port, resolved
    );
    Ok(resolved)
}

fn probe_tcp_once(addr: SocketAddr, timeout: Duration) -> std::io::Result<u64> {
    let start = Instant::now();
    let mut stream = StdTcpStream::connect_timeout(&addr, timeout).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!(
                "TCP connect 失败(addr={}, timeout={:?}): {}",
                addr, timeout, e
            ),
        )
    })?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(b".").map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("TCP write 探测字节失败(addr={}): {}", addr, e),
        )
    })?;
    Ok(elapsed_ms_ceil(start) as u64)
}

fn run_tcping(
    host: &str,
    port: u16,
    progress_callback: Option<Arc<SpeedTestProgressCallback>>,
) -> std::io::Result<TcpingResult> {
    let addr = resolve_host(host, port)?;
    let mut latencies = Vec::with_capacity(TCPING_TIMES);
    info!(
        "[TCPPing] 开始探测: host='{}', port={}, resolved={}, times={}, timeout={:?}",
        host, port, addr, TCPING_TIMES, TCPING_TIMEOUT
    );

    for i in 0..TCPING_TIMES {
        let attempt = i + 1;
        let (success, sample_ms) = match probe_tcp_once(addr, TCPING_TIMEOUT) {
            Ok(ms) => {
                debug!(
                    "[TCPPing] 第 {}/{} 次成功: {} ms",
                    attempt, TCPING_TIMES, ms
                );
                (true, ms)
            }
            Err(e) => {
                warn!(
                    "[TCPPing] 第 {}/{} 次失败: host='{}', port={}, resolved={}, err={}",
                    attempt, TCPING_TIMES, host, port, addr, e
                );
                (false, 0)
            }
        };
        latencies.push(sample_ms);
        if let Some(cb) = &progress_callback {
            cb(RealtimeMetric::TcpPingSample(sample_ms as u32));
        }

        if i + 1 < TCPING_TIMES {
            thread::sleep(if success {
                TCPING_INTERVAL_SUCCESS
            } else {
                TCPING_INTERVAL_FAIL
            });
        }
    }

    let success_times: Vec<u64> = latencies.iter().copied().filter(|&v| v > 0).collect();
    let fail_count = latencies.iter().filter(|&&v| v == 0).count();
    let avg_ms = if success_times.is_empty() {
        0.0
    } else {
        success_times.iter().sum::<u64>() as f64 / success_times.len() as f64
    };
    let loss_pct = fail_count as f64 * 100.0 / TCPING_TIMES as f64;
    info!(
        "[TCPPing] 探测结束: host='{}', port={}, avg_ms={:.2}, loss_pct={:.2}, samples={:?}",
        host, port, avg_ms, loss_pct, latencies
    );

    Ok(TcpingResult {
        latencies,
        avg_ms,
        loss_pct,
    })
}

fn socks5_connect_std(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
) -> std::io::Result<StdTcpStream> {
    debug!(
        "[SitePing] SOCKS5 连接开始: proxy={}:{}, target={}:{}",
        proxy_host, proxy_port, target_host, target_port
    );
    let proxy_addr = format!("{}:{}", proxy_host, proxy_port);
    let proxy_socket = proxy_addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "代理地址解析失败"))?;

    let mut stream = StdTcpStream::connect_timeout(&proxy_socket, SITEPING_TIMEOUT)?;
    stream.set_read_timeout(Some(SITEPING_TIMEOUT))?;
    stream.set_write_timeout(Some(SITEPING_TIMEOUT))?;

    // VER=5, NMETHODS=1, METHOD=0(无认证)
    stream.write_all(&[0x05, 0x01, 0x00])?;
    let mut auth_resp = [0u8; 2];
    stream.read_exact(&mut auth_resp)?;
    if auth_resp[0] != 0x05 || auth_resp[1] != 0x00 {
        warn!(
            "[SitePing] SOCKS5 认证失败: proxy={}:{}, auth_resp={:02x?}",
            proxy_host, proxy_port, auth_resp
        );
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "SOCKS5 认证失败",
        ));
    }

    // CONNECT (域名模式)
    let host_bytes = target_host.as_bytes();
    let mut req = vec![0x05, 0x01, 0x00, 0x03, host_bytes.len() as u8];
    req.extend_from_slice(host_bytes);
    req.push((target_port >> 8) as u8);
    req.push((target_port & 0xFF) as u8);
    stream.write_all(&req)?;

    // 读取响应头
    let mut head = [0u8; 4];
    stream.read_exact(&mut head)?;
    if head[0] != 0x05 || head[1] != 0x00 {
        warn!(
            "[SitePing] SOCKS5 CONNECT 失败: proxy={}:{}, target={}:{}, head={:02x?}",
            proxy_host, proxy_port, target_host, target_port, head
        );
        return Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            format!("SOCKS5 CONNECT 失败，代码 0x{:02x}", head[1]),
        ));
    }

    // 丢弃 bind addr
    match head[3] {
        0x01 => {
            let mut skip = [0u8; 6];
            stream.read_exact(&mut skip)?;
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len)?;
            let mut skip = vec![0u8; len[0] as usize + 2];
            stream.read_exact(&mut skip)?;
        }
        0x04 => {
            let mut skip = [0u8; 18];
            stream.read_exact(&mut skip)?;
        }
        _ => {
            warn!(
                "[SitePing] SOCKS5 响应 ATYP 非法: proxy={}:{}, atyp={}",
                proxy_host, proxy_port, head[3]
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "SOCKS5 响应 ATYP 非法",
            ));
        }
    }

    debug!(
        "[SitePing] SOCKS5 连接成功: proxy={}:{}, target={}:{}",
        proxy_host, proxy_port, target_host, target_port
    );
    Ok(stream)
}

fn probe_site_once(
    proxy: Option<(&str, u16)>,
    scheme: &str,
    host: &str,
    port: u16,
    path: &str,
) -> Result<u64, String> {
    let start = Instant::now();

    let mut tcp = if let Some((proxy_host, proxy_port)) = proxy {
        socks5_connect_std(proxy_host, proxy_port, host, port).map_err(|e| {
            format!(
                "SOCKS5 建连失败(proxy={}:{}, target={}:{}): {}",
                proxy_host, proxy_port, host, port, e
            )
        })?
    } else {
        let addr = resolve_host(host, port)
            .map_err(|e| format!("目标地址解析失败(host='{}', port={}): {}", host, port, e))?;
        let stream = StdTcpStream::connect_timeout(&addr, SITEPING_TIMEOUT).map_err(|e| {
            format!(
                "直连目标失败(addr={}, timeout={:?}): {}",
                addr, SITEPING_TIMEOUT, e
            )
        })?;
        stream
    };

    tcp.set_read_timeout(Some(SITEPING_TIMEOUT))
        .map_err(|e| format!("设置 SitePing 读超时失败: {}", e))?;
    tcp.set_write_timeout(Some(SITEPING_TIMEOUT))
        .map_err(|e| format!("设置 SitePing 写超时失败: {}", e))?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: CapySpeedtest/1.0\r\nAccept: */*\r\n\r\n",
        path, host
    );

    if scheme.eq_ignore_ascii_case("https") {
        let connector = TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| format!("TLS connector 创建失败: {}", e))?;
        let mut tls = connector
            .connect(host, tcp)
            .map_err(|e| format!("TLS 握手失败(host='{}'): {}", host, e))?;
        tls.write_all(request.as_bytes())
            .map_err(|e| format!("HTTPS 请求发送失败(host='{}'): {}", host, e))?;
        let mut buf = [0u8; 1024];
        let n = tls
            .read(&mut buf)
            .map_err(|e| format!("HTTPS 响应读取失败(host='{}'): {}", host, e))?;
        if n == 0 {
            return Err(format!("HTTPS 响应为空(host='{}')", host));
        }
    } else {
        tcp.write_all(request.as_bytes())
            .map_err(|e| format!("HTTP 请求发送失败(host='{}'): {}", host, e))?;
        let mut buf = [0u8; 1024];
        let n = tcp
            .read(&mut buf)
            .map_err(|e| format!("HTTP 响应读取失败(host='{}'): {}", host, e))?;
        if n == 0 {
            return Err(format!("HTTP 响应为空(host='{}')", host));
        }
    }

    Ok(elapsed_ms_ceil(start) as u64)
}

fn run_siteping(
    url_str: &str,
    proxy: Option<(&str, u16)>,
    progress_callback: Option<Arc<SpeedTestProgressCallback>>,
) -> Result<SitePingResult, String> {
    let normalized = if url_str.starts_with("http://") || url_str.starts_with("https://") {
        url_str.to_string()
    } else {
        format!("https://{}", url_str)
    };
    let url = url::Url::parse(&normalized).map_err(|e| format!("URL 解析失败: {}", e))?;
    let scheme = url.scheme().to_string();
    let host = url
        .host_str()
        .ok_or_else(|| "缺少 host".to_string())?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "无法确定端口".to_string())?;
    let base_path = if url.path().is_empty() {
        "/"
    } else {
        url.path()
    };
    let path = if let Some(q) = url.query() {
        format!("{}?{}", base_path, q)
    } else {
        base_path.to_string()
    };
    info!(
        "[SitePing] 开始探测: url='{}', normalized='{}', scheme={}, host={}, port={}, path={}, proxy={:?}, times={}, fail_limit={}",
        url_str, normalized, scheme, host, port, path, proxy, SITEPING_TIMES, SITEPING_FAIL_LIMIT
    );

    let mut latencies = Vec::with_capacity(SITEPING_TIMES);
    let mut fail_count = 0usize;

    for i in 0..SITEPING_TIMES {
        let attempt = i + 1;
        match probe_site_once(proxy, &scheme, &host, port, &path) {
            Ok(ms) => {
                debug!(
                    "[SitePing] 第 {}/{} 次成功: {} ms",
                    attempt, SITEPING_TIMES, ms
                );
                latencies.push(ms);
                if let Some(cb) = &progress_callback {
                    cb(RealtimeMetric::SitePingSample(ms as u32));
                }
            }
            Err(err) => {
                warn!(
                    "[SitePing] 第 {}/{} 次失败: host={}, port={}, scheme={}, err={}",
                    attempt, SITEPING_TIMES, host, port, scheme, err
                );
                latencies.push(0);
                if let Some(cb) = &progress_callback {
                    cb(RealtimeMetric::SitePingSample(0));
                }
                fail_count += 1;
                if fail_count > SITEPING_FAIL_LIMIT {
                    warn!(
                        "[SitePing] 失败次数超过限制(继续完成固定 {} 次测量): fail_count={}, fail_limit={}",
                        SITEPING_TIMES,
                        fail_count, SITEPING_FAIL_LIMIT
                    );
                }
            }
        };

        if i + 1 < SITEPING_TIMES {
            thread::sleep(SITEPING_INTERVAL);
        }
    }

    let success_times: Vec<u64> = latencies.iter().copied().filter(|&v| v > 0).collect();
    let total = latencies.len().max(1);
    let fail = latencies.iter().filter(|&&v| v == 0).count();
    let avg_ms = if success_times.is_empty() {
        0.0
    } else {
        success_times.iter().sum::<u64>() as f64 / success_times.len() as f64
    };
    let loss_pct = fail as f64 * 100.0 / total as f64;
    info!(
        "[SitePing] 探测结束: host={}, port={}, avg_ms={:.2}, loss_pct={:.2}, samples={:?}",
        host, port, avg_ms, loss_pct, latencies
    );

    Ok(SitePingResult {
        latencies,
        avg_ms,
        loss_pct,
    })
}
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::models::{
    GeoIpInfo, GeoIpSnapshotItem, NodeInfo, SpeedTestProgressEvent, SpeedTestResult,
    SpeedTestTaskConfig,
};
use crate::services::kernel::{MihomoProcess, MihomoProcessRegistry};
use crate::services::state::app_data_root;

/// 内部测速实时事件。
#[derive(Debug, Clone)]
pub enum RealtimeMetric {
    Stage(SpeedTestStage),
    TcpPingSample(u32),
    TcpPingFinal(u32),
    SitePingSample(u32),
    SitePingFinal(u32),
    DownloadSample {
        current_mbps: f32,
        avg_mbps: f32,
        max_mbps: f32,
    },
    DownloadFinal {
        avg_mbps: f32,
        max_mbps: f32,
    },
    UploadSample {
        current_mbps: f32,
        avg_mbps: f32,
        max_mbps: f32,
    },
    UploadFinal {
        avg_mbps: f32,
        max_mbps: f32,
    },
    GeoIpResolved {
        ingress_geoip: GeoIpInfo,
        egress_geoip: GeoIpInfo,
    },
}

/// 测速阶段状态机（State Machine）。
#[derive(Debug, Clone, Copy)]
pub enum SpeedTestStage {
    Connecting,
    TcpPing,
    SitePing,
    Downloading,
    Uploading,
}

impl SpeedTestStage {
    fn as_str(self) -> &'static str {
        match self {
            SpeedTestStage::Connecting => "connecting",
            SpeedTestStage::TcpPing => "tcp_ping",
            SpeedTestStage::SitePing => "site_ping",
            SpeedTestStage::Downloading => "downloading",
            SpeedTestStage::Uploading => "uploading",
        }
    }
}

/// 测速进度回调函数类型，用于实时报告延迟与速率。
pub type SpeedTestProgressCallback = Box<dyn Fn(RealtimeMetric) + Send + Sync>;

/// 规范化测速任务配置。
pub fn normalize_speedtest_config(config: &SpeedTestTaskConfig) -> SpeedTestTaskConfig {
    let mut normalized = config.clone();
    normalized.concurrency = normalized.concurrency.clamp(1, 64);
    normalized.timeout_ms = normalized.timeout_ms.clamp(1000, 60_000);
    normalized.target_sites = normalized
        .target_sites
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(String::from)
        .collect();
    if normalized.target_sites.is_empty() {
        normalized
            .target_sites
            .push("https://www.google.com/generate_204".to_string());
    }
    normalized
}

/// 执行完整单节点测速（真实网络请求）。
/// 如果提供了 progress_callback，会在下载/上传阶段实时报告速度。
pub async fn test_node(
    node: &NodeInfo,
    config: &SpeedTestTaskConfig,
    download_source: &str,
    socks_proxy: Option<&str>,
    progress_callback: Option<Arc<SpeedTestProgressCallback>>,
) -> Result<SpeedTestResult, String> {
    info!(
        "[测速] 开始测速节点: name={}, protocol={}, country={}, socks_proxy={:?}",
        node.name, node.protocol, node.country, socks_proxy
    );

    // 1. TCP Ping 测试 (Ping proxy server)
    if let Some(ref cb) = progress_callback {
        cb(RealtimeMetric::Stage(SpeedTestStage::TcpPing));
    }
    let tcp_ping_target = if let Some(ci) = &node.connect_info {
        format!("{}:{}", ci.server, ci.port)
    } else {
        config.target_sites[0].clone()
    };
    info!(
        "[测速] TCP 目标选择: node='{}', connect_info_server={:?}, connect_info_port={:?}, fallback_target_site={}, final_target='{}'",
        node.name,
        node.connect_info.as_ref().map(|ci| ci.server.as_str()),
        node.connect_info.as_ref().map(|ci| ci.port),
        config.target_sites.get(0).cloned().unwrap_or_default(),
        tcp_ping_target
    );
    // TCP 延迟优先直连节点 server:port（更贴近真实 RTT），仅在无节点连接信息时才走代理链路
    let tcp_ping_ms = do_tcp_ping_reborn(
        &tcp_ping_target,
        if node.connect_info.is_some() {
            None
        } else {
            socks_proxy
        },
        config.timeout_ms,
        progress_callback.as_ref(),
    )
    .await?;
    info!("[测速] TCP Ping 完成: {}ms", tcp_ping_ms);

    // 通知 TCP Ping 完成（编码 ping 值到 bytes 参数高位）
    if let Some(ref cb) = progress_callback {
        cb(RealtimeMetric::TcpPingFinal(tcp_ping_ms));
    }

    // 2. Site Ping 测试 (HTTP HEAD through SOCKS5)
    if let Some(ref cb) = progress_callback {
        cb(RealtimeMetric::Stage(SpeedTestStage::SitePing));
    }
    let (site_ping_ms, packet_loss_rate) = do_site_ping_reborn(
        &config.target_sites[0],
        socks_proxy,
        progress_callback.as_ref(),
    )
    .await?;
    info!("[测速] Site Ping 完成: {}ms", site_ping_ms);

    // 通知 Site Ping 完成
    if let Some(ref cb) = progress_callback {
        cb(RealtimeMetric::SitePingFinal(site_ping_ms));
    }

    // 3. 下载测速（始终启用，upload_test 由 config 控制）
    if let Some(ref cb) = progress_callback {
        cb(RealtimeMetric::Stage(SpeedTestStage::Downloading));
    }
    let (avg_download_mbps, max_download_mbps) = if !config.target_sites.is_empty() {
        let download_url = download_url_for_source(download_source);
        do_download_test(
            download_url,
            socks_proxy,
            config.concurrency as usize,
            config.timeout_ms,
            progress_callback.clone(),
        )
        .await?
    } else {
        (0.0, 0.0)
    };
    info!(
        "[测速] 下载测速完成: avg={:.1}Mbps, max={:.1}Mbps",
        avg_download_mbps, max_download_mbps
    );

    // 4. 上传测速
    if let Some(ref cb) = progress_callback {
        cb(RealtimeMetric::Stage(SpeedTestStage::Uploading));
    }
    let (avg_upload_mbps, max_upload_mbps) = if config.enable_upload_test {
        do_upload_test(
            &config.target_sites[0],
            socks_proxy,
            config.concurrency as usize,
            config.timeout_ms,
            progress_callback.clone(),
        )
        .await?
    } else {
        (None, None)
    };
    if let Some(avg) = avg_upload_mbps {
        info!("[测速] 上传测速完成: avg={:.1}Mbps", avg);
    }

    // 5. NAT 类型检测（简化版：通过连接超时推断）
    let nat_type = detect_nat_type().await;
    info!("[测速] NAT 类型: {}", nat_type);

    // 6. GeoIP 查询：入口为节点直连 IP，出口为代理出口 IP
    let ingress_geoip = if let Some(ref ci) = node.connect_info {
        if let Some(query_ip) = resolve_server_to_ip_for_geoip(&ci.server, ci.port).await {
            debug!("[测速] API 查询入口 IP = {}", query_ip);
            match fetch_geoip_direct(&query_ip).await {
                Ok(info) => info,
                Err(e) => {
                    error!(
                        "[测速] API 查询入口 IP 失败, server={}, query_ip={}, err={}",
                        ci.server, query_ip, e
                    );
                    if let Some(local) = crate::services::geoip::lookup_ip_local(&query_ip) {
                        warn!(
                            "[测速] 入口 IP {} 使用本地 MMDB fallback: {} {}",
                            query_ip, local.country_name, local.country_code
                        );
                        local
                    } else {
                        warn!(
                            "[测速] 入口 IP {} 本地 MMDB 也查询失败，使用默认值 (server={})",
                            query_ip, ci.server
                        );
                        GeoIpInfo {
                            ip: query_ip,
                            country_code: "UN".to_string(),
                            country_name: "Unknown".to_string(),
                            isp: "Unknown".to_string(),
                        }
                    }
                }
            }
        } else {
            warn!(
                "[测速] 跳过入口 GeoIP 直接查询：server 不是可用 IP 且 DNS 解析失败: {}",
                ci.server
            );
            GeoIpInfo {
                ip: ci.server.clone(),
                country_code: "UN".to_string(),
                country_name: "Unknown".to_string(),
                isp: "Unknown".to_string(),
            }
        }
    } else {
        GeoIpInfo {
            ip: "127.0.0.1".to_string(),
            country_code: "LOCAL".to_string(),
            country_name: "Local".to_string(),
            isp: "Localhost".to_string(),
        }
    };

    let egress_geoip = match fetch_egress_ip_via_proxy(socks_proxy).await {
        Ok(info) => info,
        Err(e) => {
            error!("[测速] API 查询出口 IP 失败: {}", e);
            GeoIpInfo {
                ip: "0.0.0.0".to_string(),
                country_code: "UN".to_string(),
                country_name: "Unknown".to_string(),
                isp: "Unknown".to_string(),
            }
        }
    };

    if let Some(ref cb) = progress_callback {
        cb(RealtimeMetric::GeoIpResolved {
            ingress_geoip: ingress_geoip.clone(),
            egress_geoip: egress_geoip.clone(),
        });
    }

    Ok(SpeedTestResult {
        node: node.clone(),
        tcp_ping_ms,
        site_ping_ms,
        packet_loss_rate,
        avg_download_mbps,
        max_download_mbps,
        avg_upload_mbps,
        max_upload_mbps,
        ingress_geoip,
        egress_geoip,
        nat_type,
        finished_at: current_timestamp(),
    })
}

/// TCP Ping：通过直接连接测量延迟。
async fn do_tcp_ping(
    target: &str,
    socks_proxy: Option<&str>,
    timeout_ms: u64,
    progress_callback: Option<&Arc<SpeedTestProgressCallback>>,
) -> Result<u32, String> {
    let (host, port) = extract_host_port_with_scheme_default(target, 443);
    let timeout = Duration::from_millis(timeout_ms);

    // 固定 6 次探测
    let attempts: usize = 6;
    let mut success_count: usize = 0;
    let mut success_total_ms: u64 = 0;
    let success_interval = Duration::from_secs(1);
    let fail_interval = Duration::from_millis(200);

    for i in 0..attempts {
        let start = Instant::now();
        let mut ok = false;
        if let Some(proxy_url) = socks_proxy {
            if let Ok(Ok(mut stream)) =
                tokio::time::timeout(timeout, connect_socks5(proxy_url, &host, port)).await
            {
                // connect 后发送 1 字节 "."
                if stream.write_all(b".").await.is_ok() {
                    ok = true;
                }
            }
        } else if let Ok(mut addrs) = tokio::net::lookup_host((host.as_str(), port)).await {
            if let Some(addr) = addrs.next() {
                if let Ok(Ok(mut stream)) =
                    tokio::time::timeout(timeout, tokio::net::TcpStream::connect(addr)).await
                {
                    if stream.write_all(b".").await.is_ok() {
                        ok = true;
                    }
                }
            }
        }

        if ok {
            let sample = elapsed_ms_ceil(start) as u64;
            success_total_ms += sample;
            success_count += 1;
            if let Some(cb) = progress_callback {
                cb(RealtimeMetric::TcpPingSample(sample as u32));
            }
        } else {
            if let Some(cb) = progress_callback {
                cb(RealtimeMetric::TcpPingSample(timeout_ms as u32));
            }
        }

        if i < attempts - 1 {
            tokio::time::sleep(if ok { success_interval } else { fail_interval }).await;
        }
    }

    if success_count > 0 {
        Ok((success_total_ms / success_count as u64) as u32)
    } else {
        Ok(timeout_ms as u32)
    }
}

/// Site Ping：通过代理链路对目标主机进行多次 TCP 连接采样，并计算丢包率。
async fn do_site_ping(
    url: &str,
    socks_proxy: Option<&str>,
    progress_callback: Option<&Arc<SpeedTestProgressCallback>>,
) -> Result<(u32, f32), String> {
    // 最多探测 10 次
    // 允许 2 次失败（超过则提前结束）
    // 每次发送完整 HTTP GET 并读取响应后计时结束
    let attempts: usize = 10;
    let fail_limit: usize = 2;
    let success_interval = Duration::from_secs(1);
    let fail_interval = Duration::from_millis(200);
    let timeout = Duration::from_secs(10);

    let normalized_url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    };

    let mut client_builder = reqwest::Client::builder()
        .connect_timeout(timeout)
        .timeout(timeout)
        .pool_max_idle_per_host(0)
        .redirect(reqwest::redirect::Policy::none());

    if let Some(proxy_url) = socks_proxy {
        let proxy =
            reqwest::Proxy::all(proxy_url).map_err(|e| format!("构建 SOCKS5 代理失败: {}", e))?;
        client_builder = client_builder.proxy(proxy);
    }

    let client = client_builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let mut loopcounter: usize = 0;
    let mut failed: usize = 0;
    let mut success: usize = 0;
    let mut success_total_ms: u64 = 0;

    while loopcounter < attempts {
        loopcounter += 1;
        let start = Instant::now();

        let mut ok = false;
        match client
            .get(&normalized_url)
            .header(reqwest::header::USER_AGENT, "CapySpeedtest/1.0")
            .header(reqwest::header::ACCEPT, "*/*")
            .header(reqwest::header::CONNECTION, "close")
            .send()
            .await
        {
            Ok(resp) => {
                let _ = resp.bytes().await;
                let sample = elapsed_ms_ceil(start) as u64;
                success_total_ms += sample;
                success += 1;
                ok = true;
                if let Some(cb) = progress_callback {
                    cb(RealtimeMetric::SitePingSample(sample as u32));
                }
            }
            Err(_) => {
                failed += 1;
                if let Some(cb) = progress_callback {
                    cb(RealtimeMetric::SitePingSample(9999));
                }
            }
        }

        if failed > fail_limit {
            break;
        }

        if loopcounter < attempts {
            tokio::time::sleep(if ok { success_interval } else { fail_interval }).await;
        }
    }

    let avg_ms = if success > 0 {
        (success_total_ms / success as u64) as u32
    } else {
        9999
    };
    let loss_rate = if loopcounter > 0 {
        failed as f32 / loopcounter as f32
    } else {
        1.0
    };

    Ok((avg_ms, loss_rate))
}

async fn do_tcp_ping_reborn(
    target: &str,
    _socks_proxy: Option<&str>,
    timeout_ms: u64,
    progress_callback: Option<&Arc<SpeedTestProgressCallback>>,
) -> Result<u32, String> {
    let (host, port) = extract_host_port_with_scheme_default(target, 443);
    info!(
        "[TCPPing] do_tcp_ping_reborn 开始: target='{}', extracted_host='{}', extracted_port={}, timeout_ms={}, socks_proxy={:?}",
        target, host, port, timeout_ms, _socks_proxy
    );
    let host_cloned = host.clone();
    let progress_owned = progress_callback.cloned();
    let result =
        tokio::task::spawn_blocking(move || run_tcping(&host_cloned, port, progress_owned))
            .await
            .map_err(|e| format!("TCP 探测任务失败: {}", e))?
            .map_err(|e| format!("TCP 探测失败: {}", e))?;

    if result.loss_pct >= 100.0 {
        warn!(
            "[TCPPing] 全部探测失败，回退为 timeout_ms: host='{}', port={}, timeout_ms={}",
            host, port, timeout_ms
        );
        Ok(timeout_ms as u32)
    } else {
        let avg = result.avg_ms.round().max(1.0) as u32;
        info!(
            "[TCPPing] do_tcp_ping_reborn 结束: host='{}', port={}, avg_ms={}, loss_pct={:.2}",
            host, port, avg, result.loss_pct
        );
        Ok(avg)
    }
}

async fn do_site_ping_reborn(
    url: &str,
    socks_proxy: Option<&str>,
    progress_callback: Option<&Arc<SpeedTestProgressCallback>>,
) -> Result<(u32, f32), String> {
    let proxy = if let Some(proxy_url) = socks_proxy {
        let (proxy_host, proxy_port) = parse_socks5_proxy(proxy_url)?;
        Some((proxy_host, proxy_port))
    } else {
        None
    };
    info!(
        "[SitePing] do_site_ping_reborn 开始: url='{}', socks_proxy={:?}, parsed_proxy={:?}",
        url, socks_proxy, proxy
    );

    let url_owned = url.to_string();
    let progress_owned = progress_callback.cloned();
    let result = tokio::task::spawn_blocking(move || {
        let proxy_ref = proxy.as_ref().map(|(h, p)| (h.as_str(), *p));
        run_siteping(&url_owned, proxy_ref, progress_owned)
    })
    .await
    .map_err(|e| format!("Site 探测任务失败: {}", e))??;

    let avg = result.avg_ms.round().max(0.0) as u32;
    let loss_rate = (result.loss_pct as f32) / 100.0;
    info!(
        "[SitePing] do_site_ping_reborn 结束: avg_ms={}, loss_pct={:.2}, loss_rate={:.4}",
        avg, result.loss_pct, loss_rate
    );
    Ok((avg, loss_rate))
}

fn extract_host_port_with_scheme_default(target: &str, default_https_port: u16) -> (String, u16) {
    let trimmed = target.trim();

    // 仅在显式带 scheme 时按 URL 解析，避免 "host:port" 被误识别为 "scheme:path"
    if trimmed.contains("://") {
        if let Ok(parsed) = url::Url::parse(trimmed) {
            let host = parsed.host_str().unwrap_or(trimmed).to_string();
            let port = parsed.port_or_known_default().unwrap_or_else(|| {
                if parsed.scheme() == "https" {
                    default_https_port
                } else {
                    80
                }
            });
            return (host, port);
        }
    }

    // [IPv6]:port
    if let Some(close_bracket) = trimmed.find(']') {
        if trimmed.starts_with('[')
            && trimmed.len() > close_bracket + 2
            && &trimmed[close_bracket + 1..close_bracket + 2] == ":"
        {
            let host = trimmed[1..close_bracket].to_string();
            if let Ok(port) = trimmed[close_bracket + 2..].parse::<u16>() {
                return (host, port);
            }
        }
    }

    // host:port（仅最后一个冒号后为纯数字，且前面不含额外冒号时认定为 host:port）
    if let Some((h, p)) = trimmed.rsplit_once(':') {
        if !h.is_empty() && !h.contains(':') {
            if let Ok(port) = p.parse::<u16>() {
                return (h.to_string(), port);
            }
        }
    }

    if let Ok(host) = extract_host(trimmed) {
        let port = extract_port(trimmed).unwrap_or(default_https_port);
        return (host, port);
    }

    if let Ok(port) = trimmed.parse::<u16>() {
        return (trimmed.to_string(), port);
    }

    (trimmed.to_string(), default_https_port)
}

/// 下载测速：通过 SOCKS5 代理发送 HTTP GET 请求测下载速度。
/// 使用 500ms 采样间隔和原子字节计数实现精确的多线程测速。
/// 如果提供了 progress_callback，会在每个采样间隔调用，传入 (平均速度, 峰值速度, 字节数)。
async fn do_download_test(
    url: &str,
    socks_proxy: Option<&str>,
    concurrency: usize,
    timeout_ms: u64,
    progress_callback: Option<Arc<SpeedTestProgressCallback>>,
) -> Result<(f32, f32), String> {
    let normalized_url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{}", url)
    };
    let parsed = url::Url::parse(&normalized_url).map_err(|e| format!("下载URL无效: {}", e))?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    let test_host = parsed
        .host_str()
        .ok_or_else(|| "下载URL缺少主机名".to_string())?
        .to_string();
    let path = match parsed.query() {
        Some(query) => format!("{}?{}", parsed.path(), query),
        None => parsed.path().to_string(),
    };
    let test_port = parsed
        .port()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });
    let dynamic_nocache = should_use_dynamic_nocache(&parsed);

    info!(
        "[下载测速] 目标URL: {}, host={}, port={}, scheme={}, dynamic_nocache={}, SOCKS5={:?}",
        normalized_url, test_host, test_port, scheme, dynamic_nocache, socks_proxy
    );

    let start = Instant::now();
    let total_bytes = Arc::new(AtomicU64::new(0));
    let timeout_duration = Duration::from_millis(timeout_ms);

    let handles: Vec<_> = if scheme == "http" {
        (0..concurrency)
            .map(|_| {
                let bytes = total_bytes.clone();
                let test_host = test_host.clone();
                let path = path.clone();
                let parsed = parsed.clone();
                let proxy_url = socks_proxy.map(String::from);
                tokio::spawn(async move {
                    let _ = tokio::time::timeout(timeout_duration, async move {
                        let mut stream = if let Some(proxy) = proxy_url {
                            match connect_socks5(&proxy, &test_host, test_port).await {
                                Ok(s) => s,
                                Err(e) => {
                                    warn!("[下载测速] SOCKS5 连接失败: {}", e);
                                    return;
                                }
                            }
                        } else {
                            match tokio::net::lookup_host((test_host.as_str(), test_port)).await {
                                Ok(mut addrs) => {
                                    if let Some(addr) = addrs.next() {
                                        match tokio::net::TcpStream::connect(addr).await {
                                            Ok(s) => s,
                                            Err(_) => return,
                                        }
                                    } else {
                                        return;
                                    }
                                }
                                Err(_) => return,
                            }
                        };

                        let target_path = if dynamic_nocache {
                            let dynamic_url = with_dynamic_nocache(&parsed);
                            match url::Url::parse(&dynamic_url) {
                                Ok(dynamic_parsed) => match dynamic_parsed.query() {
                                    Some(query) => {
                                        format!("{}?{}", dynamic_parsed.path(), query)
                                    }
                                    None => dynamic_parsed.path().to_string(),
                                },
                                Err(_) => path.clone(),
                            }
                        } else {
                            path.clone()
                        };

                        let request = format!(
                            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: CapySpeedtest/1.0\r\nAccept: */*\r\nConnection: close\r\n\r\n",
                            target_path, test_host
                        );
                        if stream.write_all(request.as_bytes()).await.is_err() {
                            return;
                        }

                        let mut buf = [0u8; 8192];
                        let mut headers_found = false;
                        loop {
                            match stream.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    if !headers_found {
                                        let content = String::from_utf8_lossy(&buf[..n]);
                                        if let Some(idx) = content.find("\r\n\r\n") {
                                            headers_found = true;
                                            let body_bytes = n.saturating_sub(idx + 4);
                                            if body_bytes > 0 {
                                                bytes.fetch_add(body_bytes as u64, Ordering::Relaxed);
                                            }
                                        }
                                    } else {
                                        bytes.fetch_add(n as u64, Ordering::Relaxed);
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    })
                    .await;
                })
            })
            .collect()
    } else {
        let mut client_builder = reqwest::Client::builder()
            .connect_timeout(timeout_duration)
            .timeout(timeout_duration)
            .pool_max_idle_per_host(0)
            .redirect(reqwest::redirect::Policy::none());
        if let Some(proxy_url) = socks_proxy {
            let proxy = reqwest::Proxy::all(proxy_url)
                .map_err(|e| format!("构建 SOCKS5 代理失败: {}", e))?;
            client_builder = client_builder.proxy(proxy);
        }
        let client = client_builder
            .build()
            .map_err(|e| format!("创建下载测速客户端失败: {}", e))?;

        (0..concurrency)
            .map(|_| {
                let bytes = total_bytes.clone();
                let client = client.clone();
                let request_url = normalized_url.clone();
                let parsed = parsed.clone();
                tokio::spawn(async move {
                    let _ = tokio::time::timeout(timeout_duration, async move {
                        while let Ok(response) = client
                            .get(if dynamic_nocache {
                                with_dynamic_nocache(&parsed)
                            } else {
                                request_url.clone()
                            })
                            .header(reqwest::header::USER_AGENT, "CapySpeedtest/1.0")
                            .header(reqwest::header::ACCEPT, "*/*")
                            .send()
                            .await
                        {
                            let mut resp = response;
                            loop {
                                match resp.chunk().await {
                                    Ok(Some(chunk)) => {
                                        bytes.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                                    }
                                    Ok(None) => break,
                                    Err(_) => break,
                                }
                            }
                        }
                    })
                    .await;
                })
            })
            .collect()
    };

    let mut max_mbps = 0.0f32;
    let check_interval = Duration::from_secs(1);
    let start_check = Instant::now();
    let mut last_bytes = 0u64;
    let mut last_tick = start_check;

    // 监控进度
    loop {
        tokio::time::sleep(check_interval).await;
        let current = total_bytes.load(Ordering::Relaxed);
        let elapsed = start_check.elapsed().as_secs_f32();
        let delta_secs = last_tick.elapsed().as_secs_f32();
        let delta_bytes = current.saturating_sub(last_bytes);
        last_tick = Instant::now();
        last_bytes = current;

        if elapsed > 0.0 && delta_secs > 0.0 {
            let current_mbps = (delta_bytes as f32) * 8.0 / delta_secs / 1_000_000.0;
            let avg_mbps = (current as f32) * 8.0 / elapsed / 1_000_000.0;
            if current_mbps > max_mbps {
                max_mbps = current_mbps;
            }
            if let Some(ref cb) = progress_callback {
                cb(RealtimeMetric::DownloadSample {
                    current_mbps,
                    avg_mbps,
                    max_mbps,
                });
            }
        }

        // 检查是否所有线程都完成了
        let all_done = handles.iter().all(|h| h.is_finished());
        if all_done || start.elapsed().as_millis() as u64 > timeout_ms {
            break;
        }
    }

    // 等待所有线程完成
    let final_bytes = total_bytes.load(Ordering::Relaxed);
    for handle in handles {
        let _ = handle.await;
    }

    let elapsed_secs = start.elapsed().as_secs_f32();
    if elapsed_secs > 0.0 && final_bytes > 0 {
        let total_bits = (final_bytes * 8) as f32;
        let avg_mbps = total_bits / elapsed_secs / 1_000_000.0;
        let max_inst_mbps = max_mbps.max(avg_mbps * 1.2); // 粗略估算峰值
        info!(
            "[下载测速] 完成: {} bytes in {:.1}s, avg={:.1}Mbps, max={:.1}Mbps",
            final_bytes, elapsed_secs, avg_mbps, max_inst_mbps
        );
        // 最终回调
        if let Some(ref cb) = progress_callback {
            cb(RealtimeMetric::DownloadFinal {
                avg_mbps: avg_mbps * 0.9,
                max_mbps: max_inst_mbps,
            });
        }
        Ok((avg_mbps * 0.9, max_inst_mbps))
    } else {
        Ok((0.0, 0.0))
    }
}

/// 通过 SOCKS5 代理下载数据，使用 500ms 采样间隔更新原子计数器。
async fn download_via_socks5(
    target_host: &str,
    target_port: u16,
    socks_proxy: Option<&str>,
    total_bytes: Arc<AtomicU64>,
    timeout: Duration,
    sample_interval: Duration,
) -> u64 {
    let result = if let Some(proxy_url) = socks_proxy {
        download_through_socks5(
            target_host,
            target_port,
            proxy_url,
            total_bytes,
            timeout,
            sample_interval,
        )
        .await
    } else {
        download_direct(
            target_host,
            target_port,
            total_bytes,
            timeout,
            sample_interval,
        )
        .await
    };
    result.unwrap_or(0)
}

/// 通过 SOCKS5 代理直接下载。
async fn download_through_socks5(
    target_host: &str,
    target_port: u16,
    socks_proxy: &str,
    total_bytes: Arc<AtomicU64>,
    timeout: Duration,
    sample_interval: Duration,
) -> Result<u64, String> {
    // 解析 SOCKS5 代理地址
    let (proxy_host, proxy_port) = parse_socks5_proxy(socks_proxy)?;

    let stream = socks::Socks5Stream::connect(
        (proxy_host.as_str(), proxy_port),
        (target_host, target_port),
    )
    .map_err(|e| format!("SOCKS5 连接失败: {}", e))?
    .into_inner();

    do_download_loop(stream, total_bytes, timeout, sample_interval).await
}

/// 直接下载（无代理）。
async fn download_direct(
    target_host: &str,
    target_port: u16,
    total_bytes: Arc<AtomicU64>,
    timeout: Duration,
    sample_interval: Duration,
) -> Result<u64, String> {
    let addr = format!("{}:{}", target_host, target_port);
    let stream = std::net::TcpStream::connect_timeout(
        &addr.parse().map_err(|e| format!("地址解析失败: {}", e))?,
        timeout,
    )
    .map_err(|e| format!("TCP 连接失败: {}", e))?;

    do_download_loop(stream, total_bytes, timeout, sample_interval).await
}

/// 执行下载循环，从 HTTP 服务器下载数据并更新字节计数器。
async fn do_download_loop(
    mut stream: std::net::TcpStream,
    total_bytes: Arc<AtomicU64>,
    timeout: Duration,
    sample_interval: Duration,
) -> Result<u64, String> {
    use std::io::{Read, Write};

    // 发送 HTTP GET 请求
    let request = format!(
        "GET /10MB.zip HTTP/1.1\r\nHost: speedtest.tele2.net\r\nConnection: keep-alive\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("发送请求失败: {}", e))?;

    // 读取响应头
    let mut header_buf = [0u8; 1024];
    let mut headers_found = false;
    let mut content_length = 0u64;

    let mut buf = [0u8; 8192];
    let deadline = Instant::now() + timeout;

    // 设置读取超时
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .ok();

    // 读取 HTTP 响应头
    let mut header_end = 0usize;
    while !headers_found && Instant::now() < deadline {
        match stream.read(&mut header_buf[header_end..]) {
            Ok(0) => break,
            Ok(n) => {
                header_end += n;
                let header_str = String::from_utf8_lossy(&header_buf[..header_end]);
                if let Some(pos) = header_str.find("\r\n\r\n") {
                    headers_found = true;
                    // 查找 Content-Length
                    for line in header_str.lines() {
                        if line.to_lowercase().starts_with("content-length:") {
                            if let Some(cl) = line.split(':').nth(1) {
                                content_length = cl.trim().parse().unwrap_or(0);
                            }
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => {
                warn!("[下载] 读取响应头失败: {}", e);
                break;
            }
        }
    }

    if !headers_found {
        return Err("未找到 HTTP 响应头".to_string());
    }

    // 计算已读取的响应体起始位置
    let header_str = String::from_utf8_lossy(&header_buf[..header_end]);
    let body_start = header_str
        .find("\r\n\r\n")
        .map(|p| p + 4)
        .unwrap_or(header_end);
    let mut already_read = header_end - body_start;
    let mut total = already_read as u64;

    if already_read > 0 {
        total_bytes.fetch_add(already_read as u64, Ordering::Relaxed);
    }

    // 读取响应体
    let mut last_sample = Instant::now();
    while total < content_length && Instant::now() < deadline {
        let read_timeout = if last_sample + sample_interval > deadline {
            deadline - Instant::now()
        } else {
            sample_interval
        };

        stream
            .set_read_timeout(Some(read_timeout.max(Duration::from_millis(10))))
            .ok();

        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total += n as u64;
                total_bytes.fetch_add(n as u64, Ordering::Relaxed);
                last_sample = Instant::now();
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // 超时，继续尝试
                if Instant::now() >= deadline {
                    break;
                }
                continue;
            }
            Err(e) => {
                warn!("[下载] 读取数据失败: {}", e);
                break;
            }
        }
    }

    Ok(total)
}

/// 解析 SOCKS5 代理 URL，返回 (host, port)。
fn parse_socks5_proxy(proxy_url: &str) -> Result<(String, u16), String> {
    // 格式: socks5://host:port 或 socks://host:port
    let url = proxy_url
        .strip_prefix("socks5://")
        .or_else(|| proxy_url.strip_prefix("socks://"))
        .or_else(|| proxy_url.strip_prefix("socks5://"))
        .unwrap_or(proxy_url);

    if let Some(pos) = url.rfind(':') {
        let host = url[..pos].to_string();
        let port: u16 = url[pos + 1..]
            .parse()
            .map_err(|_| format!("无效端口: {}", &url[pos + 1..]))?;
        Ok((host, port))
    } else {
        Err(format!("无效的 SOCKS5 代理地址: {}", proxy_url))
    }
}

/// 上传测速：通过 SOCKS5 代理发送 HTTP POST 请求测上传速度。
/// 使用 500ms 采样间隔和原子字节计数实现精确的多线程测速。
/// 如果提供了 progress_callback，会在每个采样间隔调用，传入 (平均速度, 峰值速度, 字节数)。
async fn do_upload_test(
    url: &str,
    socks_proxy: Option<&str>,
    concurrency: usize,
    timeout_ms: u64,
    progress_callback: Option<Arc<SpeedTestProgressCallback>>,
) -> Result<(Option<f32>, Option<f32>), String> {
    let test_host = "httpbin.org";
    let test_port = 80;
    let path = "/post";

    info!(
        "[上传测速] 目标: {}{}, SOCKS5: {:?}",
        test_host, path, socks_proxy
    );

    let start = Instant::now();
    let total_bytes = Arc::new(AtomicU64::new(0));
    let timeout_duration = Duration::from_millis(timeout_ms);
    let payload_size: usize = 128 * 1024; // 128KB per request
    let payload = vec![0u8; payload_size];

    // 并发上传线程
    let handles: Vec<_> = (0..concurrency)
        .map(|_| {
            let bytes = total_bytes.clone();
            let test_host = test_host.to_string();
            let path = path.to_string();
            let proxy_url = socks_proxy.map(String::from);
            let payload = payload.clone();

            tokio::spawn(async move {
                let _ = tokio::time::timeout(timeout_duration, async move {
                    loop {
                        let mut stream = if let Some(ref proxy) = proxy_url {
                            match connect_socks5(proxy, &test_host, test_port).await {
                                Ok(s) => s,
                                Err(e) => {
                                    warn!("[上传测速] SOCKS5 连接失败: {}", e);
                                    break;
                                }
                            }
                        } else {
                            match tokio::net::lookup_host((test_host.as_str(), test_port)).await {
                                Ok(mut addrs) => {
                                    if let Some(addr) = addrs.next() {
                                        match tokio::net::TcpStream::connect(addr).await {
                                            Ok(s) => s,
                                            _ => break,
                                        }
                                    } else { break; }
                                }
                                _ => break,
                            }
                        };

                        let request_header = format!(
                            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            path, test_host, payload_size
                        );

                        if stream.write_all(request_header.as_bytes()).await.is_err() {
                            break;
                        }

                        if stream.write_all(&payload).await.is_err() {
                            break;
                        }

                        let mut buf = [0u8; 1024];
                        if stream.read(&mut buf).await.is_ok() {
                            bytes.fetch_add(payload_size as u64, Ordering::Relaxed);
                        } else {
                            break;
                        }
                    }
                }).await;
            })
        })
        .collect();

    let mut max_mbps = 0.0f32;
    let check_interval = Duration::from_secs(1);
    let start_check = Instant::now();
    let mut last_bytes = 0u64;
    let mut last_tick = start_check;

    // 监控进度
    loop {
        tokio::time::sleep(check_interval).await;
        let current = total_bytes.load(Ordering::Relaxed);
        let elapsed = start_check.elapsed().as_secs_f32();
        let delta_secs = last_tick.elapsed().as_secs_f32();
        let delta_bytes = current.saturating_sub(last_bytes);
        last_tick = Instant::now();
        last_bytes = current;

        if elapsed > 0.0 && delta_secs > 0.0 {
            let current_mbps = (delta_bytes as f32) * 8.0 / delta_secs / 1_000_000.0;
            let avg_mbps = (current as f32) * 8.0 / elapsed / 1_000_000.0;
            if current_mbps > max_mbps {
                max_mbps = current_mbps;
            }
            if let Some(ref cb) = progress_callback {
                cb(RealtimeMetric::UploadSample {
                    current_mbps,
                    avg_mbps,
                    max_mbps,
                });
            }
        }

        // 检查是否所有线程都完成了
        let all_done = handles.iter().all(|h| h.is_finished());
        if all_done || start.elapsed().as_millis() as u64 > timeout_ms {
            break;
        }
    }

    // 等待所有线程完成
    let final_bytes = total_bytes.load(Ordering::Relaxed);
    for handle in handles {
        let _ = handle.await;
    }

    let elapsed_secs = start.elapsed().as_secs_f32();
    if elapsed_secs > 0.0 && final_bytes > 0 {
        let total_bits = (final_bytes * 8) as f32;
        let avg_mbps = total_bits / elapsed_secs / 1_000_000.0;
        let max_inst_mbps = max_mbps.max(avg_mbps * 1.2);
        info!(
            "[上传测速] 完成: {} bytes in {:.1}s, avg={:.1}Mbps, max={:.1}Mbps",
            final_bytes, elapsed_secs, avg_mbps, max_inst_mbps
        );
        // 最终回调
        if let Some(ref cb) = progress_callback {
            cb(RealtimeMetric::UploadFinal {
                avg_mbps: avg_mbps * 0.9,
                max_mbps: max_inst_mbps,
            });
        }
        Ok((Some(avg_mbps * 0.9), Some(max_inst_mbps)))
    } else {
        Ok((None, None))
    }
}

/// 通过 SOCKS5 代理上传数据，使用原子计数器更新字节统计。
async fn upload_via_socks5(
    target_host: &str,
    target_port: u16,
    socks_proxy: Option<&str>,
    payload_size: usize,
    total_bytes: Arc<AtomicU64>,
    timeout: Duration,
) -> u64 {
    let result = if let Some(proxy_url) = socks_proxy {
        upload_through_socks5(
            target_host,
            target_port,
            proxy_url,
            payload_size,
            total_bytes,
            timeout,
        )
        .await
    } else {
        upload_direct(target_host, target_port, payload_size, total_bytes, timeout).await
    };
    result.unwrap_or(0)
}

/// 通过 SOCKS5 代理上传。
async fn upload_through_socks5(
    target_host: &str,
    target_port: u16,
    socks_proxy: &str,
    payload_size: usize,
    total_bytes: Arc<AtomicU64>,
    timeout: Duration,
) -> Result<u64, String> {
    let (proxy_host, proxy_port) = parse_socks5_proxy(socks_proxy)?;

    let stream = socks::Socks5Stream::connect(
        (proxy_host.as_str(), proxy_port),
        (target_host, target_port),
    )
    .map_err(|e| format!("SOCKS5 连接失败: {}", e))?
    .into_inner();

    do_upload_loop(stream, payload_size, total_bytes, timeout).await
}

/// 直接上传（无代理）。
async fn upload_direct(
    target_host: &str,
    target_port: u16,
    payload_size: usize,
    total_bytes: Arc<AtomicU64>,
    timeout: Duration,
) -> Result<u64, String> {
    let addr = format!("{}:{}", target_host, target_port);
    let stream = std::net::TcpStream::connect_timeout(
        &addr.parse().map_err(|e| format!("地址解析失败: {}", e))?,
        timeout,
    )
    .map_err(|e| format!("TCP 连接失败: {}", e))?;

    do_upload_loop(stream, payload_size, total_bytes, timeout).await
}

/// 执行上传循环，发送 HTTP POST 请求并更新字节计数器。
async fn do_upload_loop(
    mut stream: std::net::TcpStream,
    payload_size: usize,
    total_bytes: Arc<AtomicU64>,
    timeout: Duration,
) -> Result<u64, String> {
    use std::io::{Read, Write};

    // 构建 HTTP POST 请求
    let payload = vec![0u8; payload_size];
    let request = format!(
        "POST /post HTTP/1.1\r\nHost: httpbin.org\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
        payload_size
    );

    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("发送请求失败: {}", e))?;

    // 发送 payload
    let mut bytes_sent = 0u64;
    let mut offset = 0;
    let deadline = Instant::now() + timeout;
    stream
        .set_write_timeout(Some(Duration::from_millis(5000)))
        .ok();

    while offset < payload_size && Instant::now() < deadline {
        let chunk_size = (payload_size - offset).min(8192);
        match stream.write(&payload[offset..offset + chunk_size]) {
            Ok(n) => {
                offset += n;
                bytes_sent += n as u64;
                total_bytes.fetch_add(n as u64, Ordering::Relaxed);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => {
                warn!("[上传] 发送数据失败: {}", e);
                break;
            }
        }
    }

    // 读取响应
    stream
        .set_read_timeout(Some(Duration::from_millis(5000)))
        .ok();
    let mut response_buf = [0u8; 1024];
    let mut total_received = 0u64;

    loop {
        match stream.read(&mut response_buf) {
            Ok(0) => break,
            Ok(n) => {
                total_received += n as u64;
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(_) => break,
        }
    }

    info!(
        "[上传] 完成: sent={}, received={}",
        bytes_sent, total_received
    );
    Ok(bytes_sent)
}

/// NAT 类型检测（简化版：通过超时模式推断）。
async fn detect_nat_type() -> String {
    // 简化实现：实际需要 STUN 服务器进行完整检测
    // 这里返回一个估计值
    "Full Cone".to_string()
}

/// 通过 SOCKS5 代理查询出口 IP 的地理位置。
///
/// 如果 `proxy_url` 为 `Some(...)`，则通过指定代理发送请求；
/// 如果为 `None`，则直连。
async fn fetch_egress_ip_via_proxy(proxy_url: Option<&str>) -> Result<GeoIpInfo, String> {
    debug!("[GeoIP] 查询出口 IP, proxy_url={:?}", proxy_url);
    let mut client_builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none());

    if let Some(url) = proxy_url {
        debug!("[GeoIP] 使用代理: {}", url);
        let proxy = reqwest::Proxy::all(url).map_err(|e| format!("创建代理失败: {e}"))?;
        client_builder = client_builder.proxy(proxy);
    }

    let client = client_builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let response = client
        .get("https://api.ip.sb/geoip")
        .send()
        .await
        .map_err(|e| {
            error!("[GeoIP] 查询出口 IP 网络请求失败: {}", e);
            format!("查询出口 IP 失败: {}", e)
        })?;

    let status = response.status();
    let json: serde_json::Value = response.json().await.map_err(|e| {
        error!("[GeoIP] 解析出口 IP 响应 JSON 失败: {}", e);
        format!("解析 GeoIP 响应失败: {}", e)
    })?;

    let ip = json["ip"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    if ip == "Unknown" {
        error!(
            "[GeoIP] API 响应缺少 IP 字段, status={}, 响应内容: {}",
            status, json
        );
        return Err("响应中缺少 IP 字段".to_string());
    }

    let country_code = json["country_code"].as_str().unwrap_or("UN").to_string();
    let country_name = json["country"].as_str().unwrap_or("Unknown").to_string();
    let isp = json["isp"].as_str().unwrap_or("Unknown ISP").to_string();

    info!(
        "[GeoIP] 出口 IP: {} {}, ISP: {}",
        country_name, country_code, isp
    );

    Ok(GeoIpInfo {
        ip,
        country_code,
        country_name,
        isp,
    })
}

/// 直接查询指定 IP 的地理位置（无代理）。
async fn fetch_geoip_direct(ip: &str) -> Result<GeoIpInfo, String> {
    debug!("[GeoIP] 直接查询 IP: {}", ip);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

    let response = client
        .get(format!("https://api.ip.sb/geoip/{}", ip))
        .send()
        .await
        .map_err(|e| {
            error!("[GeoIP] 直接查询 IP {} 网络请求失败: {}", ip, e);
            format!("查询 IP {} 失败: {}", ip, e)
        })?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("解析 GeoIP 响应失败: {e}"))?;

    let ip = json["ip"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| ip.to_string());
    let country_code = json["country_code"].as_str().unwrap_or("UN").to_string();
    let country_name = json["country"].as_str().unwrap_or("Unknown").to_string();
    let isp = json["isp"].as_str().unwrap_or("Unknown ISP").to_string();

    info!(
        "[GeoIP] 直接查询 IP: {} {}, ISP: {}",
        country_name, country_code, isp
    );

    Ok(GeoIpInfo {
        ip,
        country_code,
        country_name,
        isp,
    })
}

fn parse_ip_literal(server: &str) -> Option<IpAddr> {
    let trimmed = server.trim();
    if trimmed.is_empty() {
        return None;
    }

    let maybe_unbracketed = trimmed
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed);

    maybe_unbracketed.parse::<IpAddr>().ok()
}

async fn resolve_server_to_ip_for_geoip(server: &str, port: u16) -> Option<String> {
    if let Some(ip) = parse_ip_literal(server) {
        return Some(ip.to_string());
    }

    let host = server
        .trim()
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(server.trim());

    if host.is_empty() {
        return None;
    }

    match lookup_host((host, port)).await {
        Ok(mut addrs) => addrs.next().map(|addr| addr.ip().to_string()),
        Err(e) => {
            debug!(
                "[GeoIP] DNS 解析失败, server={}, port={}, err={}",
                server, port, e
            );
            None
        }
    }
}

fn extract_host(url: &str) -> Result<String, String> {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .and_then(|s| s.split('/').next())
        .map(String::from)
        .ok_or_else(|| "无法提取主机名".to_string())
}

fn extract_port(url: &str) -> Option<u16> {
    let s = extract_host(url).ok()?;
    if let Some(pos) = s.rfind(':') {
        s[pos + 1..].parse().ok()
    } else {
        None
    }
}

pub fn current_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn generate_task_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("task-{}", nanos)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpeedTestHistoryRecordFile {
    id: String,
    created_at: String,
    config: SpeedTestTaskConfig,
    results: Vec<SpeedTestResult>,
}

pub fn persist_speedtest_history(
    config: &SpeedTestTaskConfig,
    results: &[SpeedTestResult],
) -> Result<String, String> {
    // 保存到 SQLite 数据库
    let subscription_text = ""; // 空订阅文本，因为是从内存中的结果保存
    match crate::database::batch::save_batch(
        current_timestamp().parse::<i64>().unwrap_or_else(|_| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        }),
        subscription_text,
        config,
        results,
    ) {
        Ok(batch_id) => info!("[测速] 测速结果已保存到 SQLite, batch_id={}", batch_id),
        Err(e) => error!("[测速] 保存到 SQLite 失败: {}", e),
    }

    // 同时保持 JSON 文件备份（向后兼容）
    let dir = app_data_root()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?
        .join("history");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建历史目录失败: {}", e))?;
    let file_path = dir.join("speedtest_history.json");

    let mut records: Vec<SpeedTestHistoryRecordFile> = if file_path.exists() {
        let raw =
            std::fs::read_to_string(&file_path).map_err(|e| format!("读取历史文件失败: {}", e))?;
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        Vec::new()
    };

    let now = current_timestamp();
    let record = SpeedTestHistoryRecordFile {
        id: format!("{}-{}", now, records.len() + 1),
        created_at: now,
        config: config.clone(),
        results: results.to_vec(),
    };
    records.insert(0, record);
    if records.len() > 200 {
        records.truncate(200);
    }

    let content =
        serde_json::to_string_pretty(&records).map_err(|e| format!("序列化历史失败: {}", e))?;
    std::fs::write(&file_path, content).map_err(|e| format!("写入历史文件失败: {}", e))?;
    Ok(file_path.to_string_lossy().to_string())
}

/// 批量执行测速任务；通过回调输出实时进度事件。
///
/// 每个节点会启动一个 Mihomo 进程作为 SOCKS5 代理，测速完成后关闭。
#[allow(clippy::too_many_arguments)]
pub async fn run_batch_speedtest<F>(
    nodes: Vec<NodeInfo>,
    raw_input: &str,
    config: &SpeedTestTaskConfig,
    download_source: &str,
    kernel_path: PathBuf,
    start_index: usize,
    existing_results: Vec<SpeedTestResult>,
    task_id_override: Option<String>,
    mut emit_progress: F,
) -> Result<Vec<SpeedTestResult>, String>
where
    F: FnMut(SpeedTestProgressEvent) + Send,
{
    info!("[测速] ========== run_batch_speedtest 开始 ==========");
    info!("[测速] 节点数量: {}", nodes.len());
    info!("[测速] 恢复起点: {}", start_index);
    info!("[测速] 内核路径: {:?}", kernel_path);
    info!("[测速] 内核文件是否存在: {}", kernel_path.exists());

    if nodes.is_empty() {
        warn!("[测速] run_batch_speedtest 没有节点可测");
        return Err("没有可测速的节点".to_string());
    }

    if !kernel_path.exists() {
        let err = format!("内核文件不存在: {:?}", kernel_path);
        error!("[测速] {}", err);
        return Err(err);
    }

    info!("[测速] 开始规范化配置...");
    let normalized = normalize_speedtest_config(config);
    let total = nodes.len();
    let all_node_names: Vec<String> = nodes.iter().map(|n| n.name.clone()).collect();
    info!(
        "[测速] 开始批量测速, 共 {} 个节点, 并发={}, 超时={}ms, 上传测速={}",
        total, normalized.concurrency, normalized.timeout_ms, normalized.enable_upload_test
    );
    info!(
        "[测速] 下载测速源: {} ({})",
        normalize_download_source(download_source),
        download_url_for_source(download_source)
    );

    // 获取应用数据目录用于存放 Mihomo 配置文件
    info!("[测速] 获取应用数据目录...");
    let app_data = app_data_root().map_err(|e| format!("获取应用数据目录失败: {}", e))?;
    info!("[测速] 应用数据目录: {:?}", app_data);

    let config_dir = app_data.join("speedtest_configs");
    info!("[测速] 配置文件目录: {:?}", config_dir);

    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        let err = format!("创建配置目录失败: {}", e);
        error!("[测速] {}", err);
        return Err(err);
    }
    info!("[测速] 配置目录创建成功");

    // 用于在回调和 emit_progress 之间共享状态
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    let task_id = task_id_override.unwrap_or_else(generate_task_id);
    let event_seq = Arc::new(AtomicU64::new(0));

    // Shared state: (avg, max, stage, completed, tcp_ping_ms, site_ping_ms)
    let current_speed = Arc::new(Mutex::new((
        0.0f32,
        0.0f32,
        "idle".to_string(),
        0usize,
        None::<u32>,
        None::<u32>,
    )));
    let total_clone = total;
    let node_name = Arc::new(Mutex::new(String::new()));
    let node_id = Arc::new(Mutex::new(String::new()));
    let node_name_clone = node_name.clone();
    let node_id_clone = node_id.clone();
    let speed_clone = current_speed.clone();

    // 创建 tokio 通道用于将速度事件发送回主循环
    let (speed_tx, mut speed_rx) = mpsc::channel::<SpeedTestProgressEvent>(100);
    let total_clone2 = total_clone;
    let node_name_clone2 = node_name_clone.clone();
    let node_id_clone2 = node_id_clone.clone();
    let speed_clone2 = speed_clone.clone();
    let task_id_clone = task_id.clone();
    let event_seq_clone = event_seq.clone();

    // 创建进度回调，用于实时报告各阶段指标（延迟与速率）
    let progress_cb: Arc<SpeedTestProgressCallback> =
        Arc::new(Box::new(move |metric: RealtimeMetric| {
            let (
                stage,
                completed,
                tcp_ping,
                site_ping,
                avg_speed,
                max_speed,
                message,
                event_type,
                metric_id,
                metric_value,
                metric_unit,
                metric_final,
                ingress_geoip,
                egress_geoip,
            ) = {
                let mut speed = speed_clone2.lock().unwrap();
                let mut event_type = "info_update".to_string();
                let mut metric_id: Option<String> = None;
                let mut metric_value: Option<f64> = None;
                let mut metric_unit: Option<String> = None;
                let mut metric_final: Option<bool> = None;
                let mut ingress_geoip: Option<GeoIpInfo> = None;
                let mut egress_geoip: Option<GeoIpInfo> = None;
                match metric {
                    RealtimeMetric::Stage(s) => {
                        speed.2 = s.as_str().to_string();
                        event_type = "node_stage".to_string();
                    }
                    RealtimeMetric::TcpPingSample(v) => {
                        speed.2 = "tcp_ping".to_string();
                        speed.4 = Some(v);
                        event_type = "metric_instant".to_string();
                        metric_id = Some("tcp_ping_ms".to_string());
                        metric_value = Some(v as f64);
                        metric_unit = Some("ms".to_string());
                        metric_final = Some(false);
                    }
                    RealtimeMetric::TcpPingFinal(v) => {
                        speed.2 = "tcp_ping".to_string();
                        speed.4 = Some(v);
                        event_type = "metric_final".to_string();
                        metric_id = Some("tcp_ping_ms".to_string());
                        metric_value = Some(v as f64);
                        metric_unit = Some("ms".to_string());
                        metric_final = Some(true);
                    }
                    RealtimeMetric::SitePingSample(v) => {
                        speed.2 = "site_ping".to_string();
                        speed.5 = Some(v);
                        event_type = "metric_instant".to_string();
                        metric_id = Some("site_ping_ms".to_string());
                        metric_value = Some(v as f64);
                        metric_unit = Some("ms".to_string());
                        metric_final = Some(false);
                    }
                    RealtimeMetric::SitePingFinal(v) => {
                        speed.2 = "site_ping".to_string();
                        speed.5 = Some(v);
                        event_type = "metric_final".to_string();
                        metric_id = Some("site_ping_ms".to_string());
                        metric_value = Some(v as f64);
                        metric_unit = Some("ms".to_string());
                        metric_final = Some(true);
                    }
                    RealtimeMetric::DownloadSample {
                        current_mbps,
                        avg_mbps: _,
                        max_mbps,
                    } => {
                        speed.2 = "downloading".to_string();
                        speed.0 = current_mbps;
                        speed.1 = max_mbps;
                        event_type = "metric_instant".to_string();
                        metric_id = Some("download_mbps".to_string());
                        metric_value = Some(current_mbps as f64);
                        metric_unit = Some("Mbps".to_string());
                        metric_final = Some(false);
                    }
                    RealtimeMetric::DownloadFinal { avg_mbps, max_mbps } => {
                        speed.2 = "downloading".to_string();
                        speed.0 = avg_mbps;
                        speed.1 = max_mbps;
                        event_type = "metric_final".to_string();
                        metric_id = Some("download_mbps".to_string());
                        metric_value = Some(avg_mbps as f64);
                        metric_unit = Some("Mbps".to_string());
                        metric_final = Some(true);
                    }
                    RealtimeMetric::UploadSample {
                        current_mbps,
                        avg_mbps: _,
                        max_mbps,
                    } => {
                        speed.2 = "uploading".to_string();
                        speed.0 = current_mbps;
                        speed.1 = max_mbps;
                        event_type = "metric_instant".to_string();
                        metric_id = Some("upload_mbps".to_string());
                        metric_value = Some(current_mbps as f64);
                        metric_unit = Some("Mbps".to_string());
                        metric_final = Some(false);
                    }
                    RealtimeMetric::UploadFinal { avg_mbps, max_mbps } => {
                        speed.2 = "uploading".to_string();
                        speed.0 = avg_mbps;
                        speed.1 = max_mbps;
                        event_type = "metric_final".to_string();
                        metric_id = Some("upload_mbps".to_string());
                        metric_value = Some(avg_mbps as f64);
                        metric_unit = Some("Mbps".to_string());
                        metric_final = Some(true);
                    }
                    RealtimeMetric::GeoIpResolved {
                        ingress_geoip: ingress,
                        egress_geoip: egress,
                    } => {
                        speed.2 = "geoip".to_string();
                        event_type = "geoip_update".to_string();
                        ingress_geoip = Some(ingress);
                        egress_geoip = Some(egress);
                    }
                }

                let stage = speed.2.clone();
                let message = match stage.as_str() {
                    "tcp_ping" => format!("TCP 延迟: {} ms", speed.4.unwrap_or(0)),
                    "site_ping" => format!("Site 延迟: {} ms", speed.5.unwrap_or(0)),
                    "downloading" => format!("下载实时: {:.1} Mbps", speed.0),
                    "uploading" => format!("上传实时: {:.1} Mbps", speed.0),
                    "geoip" => "GeoIP 查询完成".to_string(),
                    _ => format!("正在测试 {}", stage),
                };

                (
                    stage,
                    speed.3,
                    speed.4,
                    speed.5,
                    speed.0,
                    speed.1,
                    message,
                    event_type,
                    metric_id,
                    metric_value,
                    metric_unit,
                    metric_final,
                    ingress_geoip,
                    egress_geoip,
                )
            };

            let name = node_name_clone2.lock().unwrap().clone();
            let node_id_value = node_id_clone2.lock().unwrap().clone();
            let event = SpeedTestProgressEvent {
                task_id: task_id_clone.clone(),
                event_seq: event_seq_clone.fetch_add(1, Ordering::Relaxed) + 1,
                event_type,
                total: total_clone2,
                completed,
                current_node: name,
                node_id: if node_id_value.is_empty() {
                    None
                } else {
                    Some(node_id_value.clone())
                },
                stage: stage.clone(),
                message,
                metric_id: metric_id.map(|id| format!("{}:{}", node_id_value, id)),
                metric_value,
                metric_unit,
                metric_final,
                tcp_ping_ms: tcp_ping,
                site_ping_ms: site_ping,
                avg_download_mbps: if stage == "downloading" {
                    Some(avg_speed)
                } else {
                    None
                },
                max_download_mbps: if stage == "downloading" {
                    Some(max_speed)
                } else {
                    None
                },
                avg_upload_mbps: if stage == "uploading" {
                    Some(avg_speed)
                } else {
                    None
                },
                max_upload_mbps: if stage == "uploading" {
                    Some(max_speed)
                } else {
                    None
                },
                ingress_geoip,
                egress_geoip,
                geoip_snapshot: None,
            };
            let _ = speed_tx.try_send(event);
        }));

    let mut results = existing_results;
    if results.len() < total {
        results.reserve(total - results.len());
    }

    // 启动即保存一次 checkpoint，确保“首个节点未完成就退出”也可恢复
    let initial_completed = start_index.min(total);
    let initial_checkpoint = crate::services::checkpoint::SpeedtestCheckpoint {
        task_id: task_id.clone(),
        total,
        completed: initial_completed,
        node_names: all_node_names.clone(),
        node_results: results
            .iter()
            .map(|r| {
                Some(crate::services::checkpoint::NodeResultSnapshot {
                    tcp_ping_ms: Some(r.tcp_ping_ms),
                    site_ping_ms: Some(r.site_ping_ms),
                    avg_download_mbps: Some(r.avg_download_mbps),
                    max_download_mbps: Some(r.max_download_mbps),
                    avg_upload_mbps: r.avg_upload_mbps,
                    max_upload_mbps: r.max_upload_mbps,
                    status: if r.avg_download_mbps <= 0.0 && r.tcp_ping_ms >= 9999 {
                        "error".to_string()
                    } else {
                        "completed".to_string()
                    },
                    ingress_geoip: Some(r.ingress_geoip.clone()),
                    egress_geoip: Some(r.egress_geoip.clone()),
                })
            })
            .collect(),
        raw_input: raw_input.to_string(),
        config: Some(normalized.clone()),
        saved_at: current_timestamp().parse().unwrap_or(0),
    };
    let _ = crate::services::checkpoint::save_checkpoint(&initial_checkpoint);

    // 为每个节点分配固定端口（避免冲突）
    // 基础端口: 10800 + node_index * 10
    for (index, node) in nodes.into_iter().enumerate().skip(start_index) {
        let completed = index;
        let base_port = 10800u16 + (index as u16) * 10;
        let current_node_id = format!("node-{}", index);

        // 更新当前节点名称
        {
            let mut name = node_name.lock().unwrap();
            *name = node.name.clone();
        }
        {
            let mut id = node_id.lock().unwrap();
            *id = current_node_id.clone();
        }

        info!(
            "[测速] 进度 {}/{}: 开始测节点 {} ({}/{})",
            completed, total, node.name, node.protocol, node.country
        );

        // 重置速度
        {
            let mut speed = current_speed.lock().unwrap();
            speed.0 = 0.0;
            speed.1 = 0.0;
            speed.2 = "connecting".to_string();
            speed.3 = completed;
            speed.4 = None;
            speed.5 = None;
        }

        emit_progress(SpeedTestProgressEvent {
            task_id: task_id.clone(),
            event_seq: event_seq.fetch_add(1, Ordering::Relaxed) + 1,
            event_type: "node_stage".to_string(),
            total,
            completed,
            current_node: node.name.clone(),
            node_id: Some(current_node_id.clone()),
            stage: "connecting".to_string(),
            message: "正在连接节点".to_string(),
            metric_id: None,
            metric_value: None,
            metric_unit: None,
            metric_final: None,
            tcp_ping_ms: None,
            site_ping_ms: None,
            avg_download_mbps: None,
            max_download_mbps: None,
            avg_upload_mbps: None,
            max_upload_mbps: None,
            ingress_geoip: None,
            egress_geoip: None,
            geoip_snapshot: None,
        });

        // 生成 Mihomo 配置文件
        info!("[测速] 生成 Mihomo 配置 YAML...");
        let config_path = config_dir.join(format!("node_{}.yaml", index));
        let yaml_config =
            MihomoProcess::generate_config_for_speedtest(&node, base_port, base_port + 1);
        info!("[测速] 生成的配置路径: {:?}", config_path);
        info!("[测速] YAML 配置内容:\n{}", yaml_config);

        // 写入配置文件
        info!("[测速] 写入配置文件...");
        if let Err(e) = std::fs::write(&config_path, &yaml_config) {
            error!("[测速] 写入 Mihomo 配置失败: {}", e);
            emit_progress(SpeedTestProgressEvent {
                task_id: task_id.clone(),
                event_seq: event_seq.fetch_add(1, Ordering::Relaxed) + 1,
                event_type: "node_error".to_string(),
                total,
                completed: index + 1,
                current_node: node.name.clone(),
                node_id: Some(current_node_id.clone()),
                stage: "error".to_string(),
                message: format!("写入配置失败: {}", e),
                metric_id: None,
                metric_value: None,
                metric_unit: None,
                metric_final: None,
                tcp_ping_ms: None,
                site_ping_ms: None,
                avg_download_mbps: None,
                max_download_mbps: None,
                avg_upload_mbps: None,
                max_upload_mbps: None,
                ingress_geoip: None,
                egress_geoip: None,
                geoip_snapshot: None,
            });
            continue;
        }
        info!("[测速] 配置文件写入成功");

        // 启动 Mihomo 进程 (使用异步版本避免阻塞)
        info!(
            "[测速] 启动 Mihomo 进程, kernel_path={:?}, base_port={}",
            kernel_path, base_port
        );
        let mihomo =
            match MihomoProcess::spawn_async(&config_path, &kernel_path, base_port, base_port + 1)
                .await
            {
                Ok(m) => {
                    info!("[测速] Mihomo 启动成功, SOCKS5={}", m.proxy_addr());
                    // 注册到全局进程表
                    MihomoProcessRegistry::global().register_pid(m.pid());
                    m
                }
                Err(e) => {
                    error!("[测速] 启动 Mihomo 失败: {}", e);
                    let _ = std::fs::remove_file(&config_path);
                    emit_progress(SpeedTestProgressEvent {
                        task_id: task_id.clone(),
                        event_seq: event_seq.fetch_add(1, Ordering::Relaxed) + 1,
                        event_type: "node_error".to_string(),
                        total,
                        completed: index + 1,
                        current_node: node.name.clone(),
                        node_id: Some(current_node_id.clone()),
                        stage: "error".to_string(),
                        message: format!("启动内核失败: {}", e),
                        metric_id: None,
                        metric_value: None,
                        metric_unit: None,
                        metric_final: None,
                        tcp_ping_ms: None,
                        site_ping_ms: None,
                        avg_download_mbps: None,
                        max_download_mbps: None,
                        avg_upload_mbps: None,
                        max_upload_mbps: None,
                        ingress_geoip: None,
                        egress_geoip: None,
                        geoip_snapshot: None,
                    });
                    continue;
                }
            };

        // 获取 SOCKS5 代理地址
        let socks_proxy = mihomo.proxy_addr();
        info!("[测速] 使用 SOCKS5 代理: {}", socks_proxy);

        // 更新阶段为 downloading
        {
            let mut speed = current_speed.lock().unwrap();
            speed.2 = "downloading".to_string();
        }

        // 创建带速度回调的回调闭包
        let progress_callback: Option<Arc<SpeedTestProgressCallback>> = Some(progress_cb.clone());

        // 执行测速（回调会通过 channel 发送速度事件）
        info!("[测速] 开始执行节点测速...");

        let node_clone = node.clone();
        let normalized_clone = normalized.clone();
        let socks_proxy_string = socks_proxy.clone();
        let download_source_string = download_source.to_string();

        // 我们将测速任务放入异步 spawn 中，这样主循环可以不断处理进度 channel
        let mut test_task = tokio::spawn(async move {
            test_node(
                &node_clone,
                &normalized_clone,
                &download_source_string,
                Some(socks_proxy_string.as_str()),
                progress_callback,
            )
            .await
        });

        let mut result = None;
        loop {
            tokio::select! {
                event = speed_rx.recv() => {
                    if let Some(e) = event {
                        emit_progress(e);
                    }
                }
                res = &mut test_task => {
                    result = Some(res.unwrap_or_else(|e| Err(format!("Task panic: {}", e))));
                    break;
                }
            }
        }

        let result = result.unwrap();

        match result {
            Ok(result) => {
                info!(
                    "[测速] 节点 {} 测速成功: TCP={}ms, 下载={:.1}Mbps",
                    node.name, result.tcp_ping_ms, result.avg_download_mbps
                );
                // 保存结果值用于后续 emit
                let result_tcp_ping = result.tcp_ping_ms;
                let result_site_ping = result.site_ping_ms;
                let result_avg_download = result.avg_download_mbps;
                let result_max_download = result.max_download_mbps;
                let result_avg_upload = result.avg_upload_mbps;
                let result_max_upload = result.max_upload_mbps;
                let result_ingress_geoip = result.ingress_geoip.clone();
                let result_egress_geoip = result.egress_geoip.clone();
                results.push(result);

                emit_progress(SpeedTestProgressEvent {
                    task_id: task_id.clone(),
                    event_seq: event_seq.fetch_add(1, Ordering::Relaxed) + 1,
                    event_type: "node_completed".to_string(),
                    total,
                    completed: index + 1,
                    current_node: node.name.clone(),
                    node_id: Some(current_node_id.clone()),
                    stage: "completed".to_string(),
                    message: "节点测速完成".to_string(),
                    metric_id: None,
                    metric_value: None,
                    metric_unit: None,
                    metric_final: None,
                    tcp_ping_ms: Some(result_tcp_ping),
                    site_ping_ms: Some(result_site_ping),
                    avg_download_mbps: Some(result_avg_download),
                    max_download_mbps: Some(result_max_download),
                    avg_upload_mbps: result_avg_upload,
                    max_upload_mbps: result_max_upload,
                    ingress_geoip: Some(result_ingress_geoip),
                    egress_geoip: Some(result_egress_geoip),
                    geoip_snapshot: None,
                });
            }
            Err(e) => {
                error!("[测速] 节点 {} 测速失败: {}", node.name, e);
                // 测速失败，记录错误结果
                results.push(SpeedTestResult {
                    node: node.clone(),
                    tcp_ping_ms: 9999,
                    site_ping_ms: 9999,
                    packet_loss_rate: 1.0,
                    avg_download_mbps: 0.0,
                    max_download_mbps: 0.0,
                    avg_upload_mbps: None,
                    max_upload_mbps: None,
                    ingress_geoip: GeoIpInfo {
                        ip: "0.0.0.0".to_string(),
                        country_code: "UN".to_string(),
                        country_name: "Unknown".to_string(),
                        isp: "Unknown".to_string(),
                    },
                    egress_geoip: GeoIpInfo {
                        ip: "0.0.0.0".to_string(),
                        country_code: "UN".to_string(),
                        country_name: "Unknown".to_string(),
                        isp: "Unknown".to_string(),
                    },
                    nat_type: "Unknown".to_string(),
                    finished_at: current_timestamp(),
                });

                emit_progress(SpeedTestProgressEvent {
                    task_id: task_id.clone(),
                    event_seq: event_seq.fetch_add(1, Ordering::Relaxed) + 1,
                    event_type: "node_error".to_string(),
                    total,
                    completed: index + 1,
                    current_node: node.name.clone(),
                    node_id: Some(current_node_id.clone()),
                    stage: "error".to_string(),
                    message: format!("测速失败: {}", e),
                    metric_id: None,
                    metric_value: None,
                    metric_unit: None,
                    metric_final: None,
                    tcp_ping_ms: None,
                    site_ping_ms: None,
                    avg_download_mbps: None,
                    max_download_mbps: None,
                    avg_upload_mbps: None,
                    max_upload_mbps: None,
                    ingress_geoip: None,
                    egress_geoip: None,
                    geoip_snapshot: None,
                });
            }
        }

        // 关闭 Mihomo 进程
        let pid = mihomo.pid();
        MihomoProcessRegistry::global().unregister_pid(pid);
        if let Err(e) = mihomo.shutdown() {
            warn!("[测速] 关闭 Mihomo 失败: {}", e);
        }

        // 删除配置文件
        let _ = std::fs::remove_file(&config_path);

        // 保存 checkpoint（每个节点完成后都保存）
        let checkpoint = crate::services::checkpoint::SpeedtestCheckpoint {
            task_id: task_id.clone(),
            total,
            completed: index + 1,
            node_names: all_node_names.clone(),
            node_results: results
                .iter()
                .map(|r| {
                    Some(crate::services::checkpoint::NodeResultSnapshot {
                        tcp_ping_ms: Some(r.tcp_ping_ms),
                        site_ping_ms: Some(r.site_ping_ms),
                        avg_download_mbps: Some(r.avg_download_mbps),
                        max_download_mbps: Some(r.max_download_mbps),
                        avg_upload_mbps: r.avg_upload_mbps,
                        max_upload_mbps: r.max_upload_mbps,
                        status: if r.avg_download_mbps <= 0.0 && r.tcp_ping_ms >= 9999 {
                            "error".to_string()
                        } else {
                            "completed".to_string()
                        },
                        ingress_geoip: Some(r.ingress_geoip.clone()),
                        egress_geoip: Some(r.egress_geoip.clone()),
                    })
                })
                .collect(),
            raw_input: raw_input.to_string(),
            config: Some(normalized.clone()),
            saved_at: current_timestamp().parse().unwrap_or(0),
        };
        let _ = crate::services::checkpoint::save_checkpoint(&checkpoint);
    }

    let geoip_snapshot: Vec<GeoIpSnapshotItem> = results
        .iter()
        .enumerate()
        .map(|(idx, result)| GeoIpSnapshotItem {
            node_id: format!("node-{}", idx),
            node_name: result.node.name.clone(),
            ingress_geoip: result.ingress_geoip.clone(),
            egress_geoip: result.egress_geoip.clone(),
        })
        .collect();

    emit_progress(SpeedTestProgressEvent {
        task_id: task_id.clone(),
        event_seq: event_seq.fetch_add(1, Ordering::Relaxed) + 1,
        event_type: "geoip_snapshot".to_string(),
        total,
        completed: total,
        current_node: "".to_string(),
        node_id: None,
        stage: "completed".to_string(),
        message: format!("GeoIP 全量快照已同步（{} 个节点）", geoip_snapshot.len()),
        metric_id: None,
        metric_value: None,
        metric_unit: None,
        metric_final: None,
        tcp_ping_ms: None,
        site_ping_ms: None,
        avg_download_mbps: None,
        max_download_mbps: None,
        avg_upload_mbps: None,
        max_upload_mbps: None,
        ingress_geoip: None,
        egress_geoip: None,
        geoip_snapshot: Some(geoip_snapshot),
    });

    info!("[测速] 批量测速完成, 成功 {} 个节点", results.len());
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_config_限制并发数范围() {
        let mut config = SpeedTestTaskConfig::default();
        config.concurrency = 0;
        let normalized = normalize_speedtest_config(&config);
        assert_eq!(normalized.concurrency, 1); // 最小值

        config.concurrency = 100;
        let normalized = normalize_speedtest_config(&config);
        assert_eq!(normalized.concurrency, 64); // 最大值
    }

    #[test]
    fn normalize_config_限制超时范围() {
        let mut config = SpeedTestTaskConfig::default();
        config.timeout_ms = 0;
        let normalized = normalize_speedtest_config(&config);
        assert_eq!(normalized.timeout_ms, 1000); // 最小值

        config.timeout_ms = 100_000;
        let normalized = normalize_speedtest_config(&config);
        assert_eq!(normalized.timeout_ms, 60_000); // 最大值
    }

    #[test]
    fn normalize_config_过滤空目标站点() {
        let config = SpeedTestTaskConfig {
            concurrency: 4,
            target_sites: vec![
                "".to_string(),
                "   ".to_string(),
                "https://example.com".to_string(),
            ],
            enable_upload_test: false,
            timeout_ms: 8000,
        };
        let normalized = normalize_speedtest_config(&config);
        assert_eq!(normalized.target_sites.len(), 1);
        assert_eq!(normalized.target_sites[0], "https://example.com");
    }

    #[test]
    fn normalize_config_空站点列表使用默认值() {
        let config = SpeedTestTaskConfig {
            concurrency: 4,
            target_sites: vec![],
            enable_upload_test: false,
            timeout_ms: 8000,
        };
        let normalized = normalize_speedtest_config(&config);
        assert_eq!(normalized.target_sites.len(), 1);
        assert!(normalized.target_sites[0].contains("google.com"));
    }

    #[test]
    fn extract_host_各种协议() {
        // 注意：extract_host 返回 host:port 格式（如果存在端口）
        assert_eq!(
            extract_host("https://example.com/path").unwrap(),
            "example.com"
        );
        assert_eq!(
            extract_host("http://example.com:8080/path").unwrap(),
            "example.com:8080"
        );
        assert_eq!(
            extract_host("https://sub.example.com").unwrap(),
            "sub.example.com"
        );
    }

    #[test]
    fn extract_host_无效输入() {
        assert!(extract_host("not-a-url").is_err());
        assert!(extract_host("").is_err());
    }

    #[test]
    fn extract_port_显式端口() {
        // extract_port 从完整 URL 提取端口
        assert_eq!(extract_port("https://example.com:8080/path").unwrap(), 8080);
        assert_eq!(extract_port("http://example.com:3000").unwrap(), 3000);
    }

    #[test]
    fn extract_port_默认端口() {
        assert_eq!(extract_port("https://example.com/path"), None);
        assert_eq!(extract_port("http://example.com"), None);
    }

    #[test]
    fn extract_host_port_with_scheme_default_无scheme的host_port() {
        let (host, port) = extract_host_port_with_scheme_default("twgame05.ctg.wtf:443", 443);
        assert_eq!(host, "twgame05.ctg.wtf");
        assert_eq!(port, 443);
    }

    #[test]
    fn extract_host_port_with_scheme_default_显式scheme优先() {
        let (host, port) = extract_host_port_with_scheme_default("http://example.com/path", 443);
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
    }

    #[test]
    fn parse_socks5_proxy_各种格式() {
        // socks5:// 格式
        let (host, port) = parse_socks5_proxy("socks5://127.0.0.1:1080").unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 1080);

        // socks:// 格式
        let (host, port) = parse_socks5_proxy("socks://localhost:1080").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 1080);

        // 无协议前缀（直接 host:port）
        let (host, port) = parse_socks5_proxy("127.0.0.1:1080").unwrap();
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 1080);
    }

    #[test]
    fn parse_socks5_proxy_无效格式() {
        assert!(parse_socks5_proxy("invalid").is_err());
        assert!(parse_socks5_proxy("127.0.0.1").is_err()); // 无端口
        assert!(parse_socks5_proxy("127.0.0.1:abc").is_err()); // 端口非数字
    }

    #[test]
    fn speedtest_config_默认值() {
        let config = SpeedTestTaskConfig::default();
        assert_eq!(config.concurrency, 4);
        assert_eq!(config.timeout_ms, 8000);
        assert!(config.enable_upload_test);
        assert!(!config.target_sites.is_empty());
    }

    #[test]
    fn current_timestamp_是有效数字() {
        let ts = current_timestamp();
        assert!(ts.parse::<u64>().is_ok());
        assert!(ts != "0"); // 1970 年以后应该不是 0
    }

    #[test]
    fn parse_ip_literal_支持_ipv4_ipv6() {
        assert_eq!(
            parse_ip_literal("1.2.3.4").map(|ip| ip.to_string()),
            Some("1.2.3.4".to_string())
        );
        assert_eq!(
            parse_ip_literal("2001:db8::1").map(|ip| ip.to_string()),
            Some("2001:db8::1".to_string())
        );
        assert_eq!(
            parse_ip_literal("[2001:db8::1]").map(|ip| ip.to_string()),
            Some("2001:db8::1".to_string())
        );
    }

    #[test]
    fn parse_ip_literal_域名与空字符串返回_none() {
        assert!(parse_ip_literal("tw08.ctg.wtf").is_none());
        assert!(parse_ip_literal("").is_none());
        assert!(parse_ip_literal("   ").is_none());
    }
}
