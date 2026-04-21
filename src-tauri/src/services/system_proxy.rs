//! System proxy detection across all platforms.
//!
//! Provides a unified interface for detecting system proxy settings on
//! Windows (registry), Linux (environment variables), and macOS (scutil).

use tracing::{debug, info};

/// System proxy configuration detected from system settings.
#[derive(Debug, Clone)]
pub struct SystemProxyConfig {
    /// Whether system proxy is enabled.
    pub enabled: bool,
    /// The proxy URL if enabled (e.g., "http://proxy:8080" or "socks5://proxy:1080").
    pub proxy_url: Option<String>,
}

/// Detects the current system proxy settings across all platforms.
pub fn get_system_proxy() -> SystemProxyConfig {
    #[cfg(target_os = "windows")]
    {
        get_windows_proxy()
    }

    #[cfg(target_os = "linux")]
    {
        get_linux_proxy()
    }

    #[cfg(target_os = "macos")]
    {
        get_macos_proxy()
    }
}

// =============================================================================
// Windows implementation
// =============================================================================
#[cfg(target_os = "windows")]
fn get_windows_proxy() -> SystemProxyConfig {
    use winreg::enums::*;
    use winreg::RegKey;

    debug!("[系统代理] 检测 Windows 系统代理设置");
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let internet_settings =
        match hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings") {
            Ok(key) => key,
            Err(_) => {
                debug!("[系统代理] 无法打开注册表项");
                return SystemProxyConfig {
                    enabled: false,
                    proxy_url: None,
                };
            }
        };

    let proxy_enable: u32 = match internet_settings.get_value("ProxyEnable") {
        Ok(v) => v,
        Err(_) => {
            return SystemProxyConfig {
                enabled: false,
                proxy_url: None,
            }
        }
    };

    if proxy_enable != 1 {
        debug!("[系统代理] ProxyEnable={}, 未启用", proxy_enable);
        return SystemProxyConfig {
            enabled: false,
            proxy_url: None,
        };
    }

    let proxy_server: String = match internet_settings.get_value("ProxyServer") {
        Ok(v) => v,
        Err(_) => {
            return SystemProxyConfig {
                enabled: false,
                proxy_url: None,
            }
        }
    };

    let proxy_url = parse_windows_proxy_server(&proxy_server);

    if proxy_url.is_some() {
        info!("[系统代理] 检测到已启用的系统代理: {}", proxy_server);
    }

    SystemProxyConfig {
        enabled: true,
        proxy_url,
    }
}

#[cfg(target_os = "windows")]
fn parse_windows_proxy_server(server: &str) -> Option<String> {
    if server.contains('=') {
        let mut http_proxy = None;
        let mut socks_proxy = None;

        for part in server.split(';') {
            let part = part.trim();
            if let Some((proto, address)) = part.split_once('=') {
                match proto.to_lowercase().as_str() {
                    "http" | "https" if http_proxy.is_none() => {
                        http_proxy = Some(format!("http://{}", address));
                    }
                    "socks" | "socks4" | "socks5" if socks_proxy.is_none() => {
                        socks_proxy = Some(format!("socks5://{}", address));
                    }
                    _ => {}
                }
            }
        }
        socks_proxy.or(http_proxy)
    } else {
        if server.is_empty() {
            None
        } else {
            Some(format!("http://{}", server))
        }
    }
}

// =============================================================================
// Linux implementation
// =============================================================================
#[cfg(target_os = "linux")]
fn get_linux_proxy() -> SystemProxyConfig {
    use std::env;

    debug!("[系统代理] 检测 Linux 系统代理设置");

    // Check environment variables (case-insensitive, lowercase first)
    let proxy_vars = [
        "http_proxy",
        "HTTP_PROXY",
        "https_proxy",
        "HTTPS_PROXY",
        "all_proxy",
        "ALL_PROXY",
    ];

    for var in &proxy_vars {
        if let Ok(value) = env::var(var) {
            if !value.is_empty() && !value.starts_with("localhost") && !value.starts_with("127.") {
                let proxy_url = normalize_proxy_url(&value);
                if proxy_url.is_some() {
                    info!("[系统代理] 检测到系统代理 ({}): {}", var, value);
                    return SystemProxyConfig {
                        enabled: true,
                        proxy_url,
                    };
                }
            }
        }
    }

    // Also check no_proxy / NO_PROXY
    let no_proxy = env::var("no_proxy")
        .or_else(|_| env::var("NO_PROXY"))
        .unwrap_or_default();

    // If all_proxy is set but no_proxy matches everything, proxy is effectively disabled
    let all_proxy = env::var("all_proxy")
        .or_else(|_| env::var("ALL_PROXY"))
        .unwrap_or_default();

    if !all_proxy.is_empty() && no_proxy == "*" {
        return SystemProxyConfig {
            enabled: false,
            proxy_url: Some(format!("http://{}", all_proxy)),
        };
    }

    debug!("[系统代理] 未检测到已启用的系统代理");
    SystemProxyConfig {
        enabled: false,
        proxy_url: None,
    }
}

#[cfg(target_os = "linux")]
fn normalize_proxy_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    // Already has scheme
    if value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("socks5://")
    {
        Some(value.to_string())
    } else {
        // Add http:// scheme
        Some(format!("http://{}", value))
    }
}

// =============================================================================
// macOS implementation
// =============================================================================
#[cfg(target_os = "macos")]
fn get_macos_proxy() -> SystemProxyConfig {
    use std::process::Command;

    debug!("[系统代理] 检测 macOS 系统代理设置");

    // Use scutil to get proxy settings
    let output = Command::new("scutil").args(["--proxy"]).output();

    match output {
        Ok(out) => {
            if !out.status.success() {
                return SystemProxyConfig {
                    enabled: false,
                    proxy_url: None,
                };
            }

            let stdout = String::from_utf8_lossy(&out.stdout);
            let mut http_proxy: Option<String> = None;
            let mut https_proxy: Option<String> = None;
            let mut socks_proxy: Option<String> = None;
            let mut proxy_enable = false;

            for line in stdout.lines() {
                let line = line.trim();
                if line.starts_with("ProxyEnable") {
                    if let Some(val) = line.split_whitespace().last() {
                        proxy_enable = val == "1";
                    }
                } else if line.starts_with("HTTPProxy") {
                    http_proxy = extract_proxy_value(line);
                } else if line.starts_with("HTTPSProxy") {
                    https_proxy = extract_proxy_value(line);
                } else if line.starts_with("SOCKSProxy") {
                    socks_proxy = extract_proxy_value(line);
                }
            }

            if proxy_enable {
                let proxy_url = socks_proxy.or(https_proxy).or(http_proxy).map(|addr| {
                    if addr.starts_with("http://")
                        || addr.starts_with("https://")
                        || addr.starts_with("socks5://")
                    {
                        addr
                    } else {
                        format!("http://{}", addr)
                    }
                });

                if let Some(ref url) = proxy_url {
                    info!("[系统代理] 检测到已启用的系统代理: {}", url);
                }

                return SystemProxyConfig {
                    enabled: true,
                    proxy_url,
                };
            }

            SystemProxyConfig {
                enabled: false,
                proxy_url: None,
            }
        }
        Err(e) => {
            debug!("[系统代理] scutil 执行失败: {}", e);
            SystemProxyConfig {
                enabled: false,
                proxy_url: None,
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn extract_proxy_value(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        let value = parts[1].trim_matches(':').trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

// =============================================================================
// Cross-platform tests
// =============================================================================
#[cfg(test)]
mod tests {
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_single_proxy() {
        let result = parse_windows_proxy_server("proxy.example.com:8080");
        assert_eq!(result, Some("http://proxy.example.com:8080".to_string()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_per_protocol() {
        let result = parse_windows_proxy_server("http=proxy:8080;https=proxy:8443");
        assert_eq!(result, Some("http://proxy:8080".to_string()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_with_socks() {
        let result = parse_windows_proxy_server("http=proxy:8080;socks=proxy:1080");
        assert_eq!(result, Some("socks5://proxy:1080".to_string()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_parse_empty() {
        let result = parse_windows_proxy_server("");
        assert_eq!(result, None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_normalize_proxy_url() {
        assert_eq!(
            normalize_proxy_url("http://proxy:8080"),
            Some("http://proxy:8080".to_string())
        );
        assert_eq!(
            normalize_proxy_url("socks5://proxy:1080"),
            Some("socks5://proxy:1080".to_string())
        );
        assert_eq!(
            normalize_proxy_url("proxy:8080"),
            Some("http://proxy:8080".to_string())
        );
        assert_eq!(normalize_proxy_url(""), None);
    }
}
