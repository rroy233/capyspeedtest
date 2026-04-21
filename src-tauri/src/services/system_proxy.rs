//! System proxy detection across all platforms.
//!
//! Provides a unified interface for detecting system proxy settings on
//! Windows (registry), Linux (environment variables), and macOS (scutil).

use tracing::{debug, info};

/// System proxy configuration detected from system settings.
#[derive(Debug, Clone, PartialEq)]
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

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        SystemProxyConfig {
            enabled: false,
            proxy_url: None,
        }
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

    if let Some(ref url) = proxy_url {
        info!("[系统代理] 检测到已启用的系统代理: {}", url);
    }

    SystemProxyConfig {
        enabled: proxy_url.is_some(),
        proxy_url,
    }
}

/// Parses the Windows `ProxyServer` registry value into a canonical proxy URL.
///
/// The value can be either:
/// - A plain `host:port` string applying to all protocols.
/// - A semicolon-separated list of `protocol=host:port` pairs, e.g.
///   `"http=proxy:8080;socks=proxy:1080"`.
///
/// Priority: SOCKS > HTTP/HTTPS (SOCKS is protocol-agnostic and generally
/// preferred when both are present).
#[cfg(target_os = "windows")]
fn parse_windows_proxy_server(server: &str) -> Option<String> {
    let server = server.trim();
    if server.is_empty() {
        return None;
    }

    if server.contains('=') {
        // Per-protocol format: "http=proxy:8080;socks=proxy:1080"
        let mut http_proxy: Option<String> = None;
        let mut socks_proxy: Option<String> = None;

        for part in server.split(';') {
            let part = part.trim();
            if let Some((proto, address)) = part.split_once('=') {
                let address = address.trim();
                if address.is_empty() {
                    continue;
                }
                match proto.trim().to_lowercase().as_str() {
                    "http" | "https" if http_proxy.is_none() => {
                        http_proxy = Some(ensure_http_scheme(address));
                    }
                    "socks" | "socks4" | "socks5" if socks_proxy.is_none() => {
                        socks_proxy = Some(ensure_socks_scheme(address));
                    }
                    _ => {}
                }
            }
        }
        // SOCKS preferred over HTTP when both are present
        socks_proxy.or(http_proxy)
    } else {
        // Global format: "proxy.example.com:8080"
        // The value may already have a scheme in rare cases; normalise it.
        Some(ensure_http_scheme(server))
    }
}

#[cfg(target_os = "windows")]
fn ensure_http_scheme(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{}", addr)
    }
}

#[cfg(target_os = "windows")]
fn ensure_socks_scheme(addr: &str) -> String {
    if addr.starts_with("socks4://")
        || addr.starts_with("socks5://")
        || addr.starts_with("socks://")
    {
        addr.to_string()
    } else {
        format!("socks5://{}", addr)
    }
}

// =============================================================================
// Linux implementation
// =============================================================================
#[cfg(target_os = "linux")]
fn get_linux_proxy() -> SystemProxyConfig {
    use std::env;

    debug!("[系统代理] 检测 Linux 系统代理设置");

    // Read no_proxy early so we can use it to skip local-only entries.
    let no_proxy = env::var("no_proxy")
        .or_else(|_| env::var("NO_PROXY"))
        .unwrap_or_default();

    // If no_proxy is "*", every host is excluded — proxy effectively disabled.
    if no_proxy.trim() == "*" {
        debug!("[系统代理] no_proxy=* 覆盖所有代理，视为未启用");
        return SystemProxyConfig {
            enabled: false,
            proxy_url: None,
        };
    }

    // Check environment variables in priority order.
    // lowercase variants take precedence over uppercase (curl / wget convention).
    let proxy_vars = [
        "all_proxy",
        "ALL_PROXY",
        "https_proxy",
        "HTTPS_PROXY",
        "http_proxy",
        "HTTP_PROXY",
    ];

    for var in &proxy_vars {
        if let Ok(value) = env::var(var) {
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            if is_local_address(&value) {
                debug!("[系统代理] 跳过本地地址 ({}: {})", var, value);
                continue;
            }
            match normalize_proxy_url(&value) {
                Some(proxy_url) => {
                    info!("[系统代理] 检测到系统代理 ({}): {}", var, proxy_url);
                    return SystemProxyConfig {
                        enabled: true,
                        proxy_url: Some(proxy_url),
                    };
                }
                None => continue,
            }
        }
    }

    debug!("[系统代理] 未检测到已启用的系统代理");
    SystemProxyConfig {
        enabled: false,
        proxy_url: None,
    }
}

/// Returns `true` when `value` refers to a loopback / unspecified address
/// and should therefore be skipped (it is not a "real" proxy).
#[cfg(target_os = "linux")]
fn is_local_address(value: &str) -> bool {
    // Strip scheme for matching so that "http://127.0.0.1:…" is caught as well.
    let host_part = value
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("socks5://")
        .trim_start_matches("socks4://")
        .trim_start_matches("socks://");

    host_part.starts_with("localhost")
        || host_part.starts_with("127.")
        || host_part.starts_with("[::1]")
        || host_part.starts_with("::1")
        || host_part.starts_with("0.0.0.0")
}

/// Normalises a raw proxy value into a URL with an explicit scheme.
///
/// Returns `None` only when `value` is empty after trimming.
#[cfg(target_os = "linux")]
fn normalize_proxy_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("socks5://")
        || value.starts_with("socks4://")
        || value.starts_with("socks://")
    {
        Some(value.to_string())
    } else {
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

    let output = match Command::new("scutil").args(["--proxy"]).output() {
        Ok(o) => o,
        Err(e) => {
            debug!("[系统代理] scutil 执行失败: {}", e);
            return SystemProxyConfig {
                enabled: false,
                proxy_url: None,
            };
        }
    };

    if !output.status.success() {
        debug!("[系统代理] scutil 返回非零退出码");
        return SystemProxyConfig {
            enabled: false,
            proxy_url: None,
        };
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_scutil_output(&stdout)
}

/// Pure function that parses `scutil --proxy` text output.
/// Extracted so it can be unit-tested without spawning a process.
#[cfg(target_os = "macos")]
pub(crate) fn parse_scutil_output(text: &str) -> SystemProxyConfig {
    let mut http_enabled = false;
    let mut http_host: Option<String> = None;
    let mut http_port: Option<u16> = None;

    let mut https_enabled = false;
    let mut https_host: Option<String> = None;
    let mut https_port: Option<u16> = None;

    let mut socks_enabled = false;
    let mut socks_host: Option<String> = None;
    let mut socks_port: Option<u16> = None;

    for line in text.lines() {
        // Each line looks like: "  KeyName : value"
        // Split on the first ':' only.
        let line = line.trim();
        let (key, val) = match line.split_once(':') {
            Some(pair) => (pair.0.trim(), pair.1.trim()),
            None => continue,
        };

        match key {
            "HTTPEnable" => http_enabled = val == "1",
            "HTTPProxy" => http_host = Some(val.to_string()),
            "HTTPPort" => http_port = val.parse().ok(),
            "HTTPSEnable" => https_enabled = val == "1",
            "HTTPSProxy" => https_host = Some(val.to_string()),
            "HTTPSPort" => https_port = val.parse().ok(),
            "SOCKSEnable" => socks_enabled = val == "1",
            "SOCKSProxy" => socks_host = Some(val.to_string()),
            "SOCKSPort" => socks_port = val.parse().ok(),
            _ => {}
        }
    }

    // Build the proxy URL, preferring SOCKS > HTTPS > HTTP.
    let proxy_url = if socks_enabled {
        socks_host.map(|h| {
            let port = socks_port.unwrap_or(1080);
            format!("socks5://{}:{}", h, port)
        })
    } else if https_enabled {
        https_host.map(|h| {
            let port = https_port.unwrap_or(8080);
            format!("https://{}:{}", h, port)
        })
    } else if http_enabled {
        http_host.map(|h| {
            let port = http_port.unwrap_or(8080);
            format!("http://{}:{}", h, port)
        })
    } else {
        None
    };

    if let Some(ref url) = proxy_url {
        info!("[系统代理] 检测到已启用的系统代理: {}", url);
    } else {
        debug!("[系统代理] 未检测到已启用的系统代理");
    }

    SystemProxyConfig {
        enabled: proxy_url.is_some(),
        proxy_url,
    }
}

// =============================================================================
// Tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Windows tests
    // -------------------------------------------------------------------------
    #[cfg(target_os = "windows")]
    mod windows {
        use super::*;

        // -- parse_windows_proxy_server --

        #[test]
        fn single_host_port() {
            assert_eq!(
                parse_windows_proxy_server("proxy.example.com:8080"),
                Some("http://proxy.example.com:8080".to_string())
            );
        }

        #[test]
        fn single_host_no_port() {
            // Host without port is still a valid proxy address.
            assert_eq!(
                parse_windows_proxy_server("proxy.example.com"),
                Some("http://proxy.example.com".to_string())
            );
        }

        #[test]
        fn empty_string_returns_none() {
            assert_eq!(parse_windows_proxy_server(""), None);
        }

        #[test]
        fn whitespace_only_returns_none() {
            assert_eq!(parse_windows_proxy_server("   "), None);
        }

        #[test]
        fn per_protocol_http_only() {
            assert_eq!(
                parse_windows_proxy_server("http=proxy:8080"),
                Some("http://proxy:8080".to_string())
            );
        }

        #[test]
        fn per_protocol_https_only() {
            assert_eq!(
                parse_windows_proxy_server("https=proxy:8443"),
                Some("http://proxy:8443".to_string())
            );
        }

        #[test]
        fn per_protocol_http_and_https_picks_http() {
            // Both map to http scheme; the first matched (http=) wins.
            assert_eq!(
                parse_windows_proxy_server("http=proxy:8080;https=proxy:8443"),
                Some("http://proxy:8080".to_string())
            );
        }

        #[test]
        fn per_protocol_socks_preferred_over_http() {
            assert_eq!(
                parse_windows_proxy_server("http=proxy:8080;socks=proxy:1080"),
                Some("socks5://proxy:1080".to_string())
            );
        }

        #[test]
        fn per_protocol_socks5_explicit() {
            assert_eq!(
                parse_windows_proxy_server("socks5=proxy:1080"),
                Some("socks5://proxy:1080".to_string())
            );
        }

        #[test]
        fn per_protocol_unknown_keys_ignored() {
            assert_eq!(parse_windows_proxy_server("ftp=proxy:21"), None);
        }

        #[test]
        fn per_protocol_empty_address_skipped() {
            // "http=" with no address should not produce a result.
            assert_eq!(
                parse_windows_proxy_server("http=;socks=proxy:1080"),
                Some("socks5://proxy:1080".to_string())
            );
        }

        #[test]
        fn value_with_existing_http_scheme_preserved() {
            assert_eq!(
                parse_windows_proxy_server("http://proxy.example.com:8080"),
                Some("http://proxy.example.com:8080".to_string())
            );
        }

        #[test]
        fn per_protocol_extra_whitespace_tolerated() {
            assert_eq!(
                parse_windows_proxy_server(" http = proxy:8080 "),
                Some("http://proxy:8080".to_string())
            );
        }
    }

    // -------------------------------------------------------------------------
    // Linux tests
    // -------------------------------------------------------------------------
    #[cfg(target_os = "linux")]
    mod linux {
        use super::*;

        // -- normalize_proxy_url --

        #[test]
        fn normalize_empty_returns_none() {
            assert_eq!(normalize_proxy_url(""), None);
        }

        #[test]
        fn normalize_whitespace_returns_none() {
            assert_eq!(normalize_proxy_url("   "), None);
        }

        #[test]
        fn normalize_bare_host_port_adds_http() {
            assert_eq!(
                normalize_proxy_url("proxy:8080"),
                Some("http://proxy:8080".to_string())
            );
        }

        #[test]
        fn normalize_http_scheme_preserved() {
            assert_eq!(
                normalize_proxy_url("http://proxy:8080"),
                Some("http://proxy:8080".to_string())
            );
        }

        #[test]
        fn normalize_https_scheme_preserved() {
            assert_eq!(
                normalize_proxy_url("https://proxy:8443"),
                Some("https://proxy:8443".to_string())
            );
        }

        #[test]
        fn normalize_socks5_scheme_preserved() {
            assert_eq!(
                normalize_proxy_url("socks5://proxy:1080"),
                Some("socks5://proxy:1080".to_string())
            );
        }

        #[test]
        fn normalize_socks4_scheme_preserved() {
            assert_eq!(
                normalize_proxy_url("socks4://proxy:1080"),
                Some("socks4://proxy:1080".to_string())
            );
        }

        // -- is_local_address --

        #[test]
        fn local_localhost() {
            assert!(is_local_address("localhost:3128"));
        }

        #[test]
        fn local_127() {
            assert!(is_local_address("127.0.0.1:3128"));
        }

        #[test]
        fn local_127_with_http_scheme() {
            assert!(is_local_address("http://127.0.0.1:3128"));
        }

        #[test]
        fn local_ipv6_loopback() {
            assert!(is_local_address("[::1]:3128"));
        }

        #[test]
        fn local_ipv6_loopback_no_brackets() {
            assert!(is_local_address("::1"));
        }

        #[test]
        fn local_unspecified() {
            assert!(is_local_address("0.0.0.0:3128"));
        }

        #[test]
        fn remote_address_not_local() {
            assert!(!is_local_address("proxy.example.com:8080"));
        }

        #[test]
        fn remote_ip_not_local() {
            assert!(!is_local_address("10.0.0.1:3128"));
        }

        // -- get_linux_proxy (env-var integration) --

        use std::env;
        use std::sync::Mutex;

        // Env-var tests mutate process-wide state, so serialize them.
        static ENV_LOCK: Mutex<()> = Mutex::new(());

        fn clear_proxy_env() {
            for v in &[
                "http_proxy",
                "HTTP_PROXY",
                "https_proxy",
                "HTTPS_PROXY",
                "all_proxy",
                "ALL_PROXY",
                "no_proxy",
                "NO_PROXY",
            ] {
                env::remove_var(v);
            }
        }

        #[test]
        fn no_env_vars_returns_disabled() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            let cfg = get_linux_proxy();
            assert!(!cfg.enabled);
            assert!(cfg.proxy_url.is_none());
        }

        #[test]
        fn http_proxy_env_var() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            env::set_var("http_proxy", "http://proxy.corp:3128");
            let cfg = get_linux_proxy();
            assert!(cfg.enabled);
            assert_eq!(cfg.proxy_url, Some("http://proxy.corp:3128".to_string()));
        }

        #[test]
        fn https_proxy_takes_priority_over_http() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            env::set_var("http_proxy", "http://http-proxy:3128");
            env::set_var("https_proxy", "https://https-proxy:3128");
            // all_proxy is highest priority and not set, so https_proxy wins
            let cfg = get_linux_proxy();
            assert!(cfg.enabled);
            assert_eq!(cfg.proxy_url, Some("https://https-proxy:3128".to_string()));
        }

        #[test]
        fn all_proxy_takes_highest_priority() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            env::set_var("all_proxy", "socks5://socks-proxy:1080");
            env::set_var("http_proxy", "http://http-proxy:3128");
            let cfg = get_linux_proxy();
            assert!(cfg.enabled);
            assert_eq!(cfg.proxy_url, Some("socks5://socks-proxy:1080".to_string()));
        }

        #[test]
        fn no_proxy_wildcard_disables_all() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            env::set_var("http_proxy", "http://proxy.corp:3128");
            env::set_var("no_proxy", "*");
            let cfg = get_linux_proxy();
            assert!(!cfg.enabled);
            assert!(cfg.proxy_url.is_none());
        }

        #[test]
        fn localhost_proxy_is_skipped() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            env::set_var("http_proxy", "http://127.0.0.1:3128");
            let cfg = get_linux_proxy();
            assert!(!cfg.enabled);
        }

        #[test]
        fn bare_host_port_gets_http_scheme() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            env::set_var("http_proxy", "proxy.corp:3128");
            let cfg = get_linux_proxy();
            assert!(cfg.enabled);
            assert_eq!(cfg.proxy_url, Some("http://proxy.corp:3128".to_string()));
        }

        #[test]
        fn uppercase_env_var_fallback() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            env::set_var("HTTP_PROXY", "http://proxy.corp:3128");
            let cfg = get_linux_proxy();
            assert!(cfg.enabled);
            assert_eq!(cfg.proxy_url, Some("http://proxy.corp:3128".to_string()));
        }

        #[test]
        fn empty_env_var_ignored() {
            let _g = ENV_LOCK.lock().unwrap();
            clear_proxy_env();
            env::set_var("http_proxy", "");
            let cfg = get_linux_proxy();
            assert!(!cfg.enabled);
        }
    }

    // -------------------------------------------------------------------------
    // macOS tests
    // -------------------------------------------------------------------------
    #[cfg(target_os = "macos")]
    mod macos {
        use super::*;

        // -- parse_scutil_output --

        #[test]
        fn empty_output_returns_disabled() {
            let cfg = parse_scutil_output("");
            assert!(!cfg.enabled);
            assert!(cfg.proxy_url.is_none());
        }

        #[test]
        fn no_enable_flags_returns_disabled() {
            let input = "<dictionary> {\n  HTTPProxy : proxy.example.com\n  HTTPPort : 8080\n}";
            let cfg = parse_scutil_output(input);
            assert!(!cfg.enabled);
        }

        #[test]
        fn http_proxy_enabled() {
            let input = "\
                HTTPEnable : 1\n\
                HTTPProxy : proxy.example.com\n\
                HTTPPort : 8080\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(
                cfg.proxy_url,
                Some("http://proxy.example.com:8080".to_string())
            );
        }

        #[test]
        fn https_proxy_enabled() {
            let input = "\
                HTTPSEnable : 1\n\
                HTTPSProxy : proxy.example.com\n\
                HTTPSPort : 8443\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(
                cfg.proxy_url,
                Some("https://proxy.example.com:8443".to_string())
            );
        }

        #[test]
        fn socks_proxy_enabled() {
            let input = "\
                SOCKSEnable : 1\n\
                SOCKSProxy : socks.example.com\n\
                SOCKSPort : 1080\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(
                cfg.proxy_url,
                Some("socks5://socks.example.com:1080".to_string())
            );
        }

        #[test]
        fn socks_preferred_over_https_and_http() {
            let input = "\
                HTTPEnable : 1\n\
                HTTPProxy : http-proxy.example.com\n\
                HTTPPort : 8080\n\
                HTTPSEnable : 1\n\
                HTTPSProxy : https-proxy.example.com\n\
                HTTPSPort : 8443\n\
                SOCKSEnable : 1\n\
                SOCKSProxy : socks.example.com\n\
                SOCKSPort : 1080\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(
                cfg.proxy_url,
                Some("socks5://socks.example.com:1080".to_string())
            );
        }

        #[test]
        fn https_preferred_over_http_when_no_socks() {
            let input = "\
                HTTPEnable : 1\n\
                HTTPProxy : http-proxy.example.com\n\
                HTTPPort : 8080\n\
                HTTPSEnable : 1\n\
                HTTPSProxy : https-proxy.example.com\n\
                HTTPSPort : 8443\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(
                cfg.proxy_url,
                Some("https://https-proxy.example.com:8443".to_string())
            );
        }

        #[test]
        fn http_enable_zero_returns_disabled() {
            let input = "\
                HTTPEnable : 0\n\
                HTTPProxy : proxy.example.com\n\
                HTTPPort : 8080\n";
            let cfg = parse_scutil_output(input);
            assert!(!cfg.enabled);
            assert!(cfg.proxy_url.is_none());
        }

        #[test]
        fn missing_port_uses_default_http() {
            let input = "\
                HTTPEnable : 1\n\
                HTTPProxy : proxy.example.com\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(
                cfg.proxy_url,
                Some("http://proxy.example.com:8080".to_string())
            );
        }

        #[test]
        fn missing_port_uses_default_socks() {
            let input = "\
                SOCKSEnable : 1\n\
                SOCKSProxy : proxy.example.com\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(
                cfg.proxy_url,
                Some("socks5://proxy.example.com:1080".to_string())
            );
        }

        #[test]
        fn enabled_but_missing_host_returns_disabled() {
            // Flag set but no host recorded — cannot build a usable URL.
            let input = "HTTPEnable : 1\n";
            let cfg = parse_scutil_output(input);
            assert!(!cfg.enabled);
            assert!(cfg.proxy_url.is_none());
        }

        #[test]
        fn extra_dictionary_wrapper_lines_ignored() {
            // Real scutil output wraps values in "<dictionary> { … }"
            let input = "\
                <dictionary> {\n\
                  HTTPEnable : 1\n\
                  HTTPProxy : proxy.example.com\n\
                  HTTPPort : 3128\n\
                }\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(
                cfg.proxy_url,
                Some("http://proxy.example.com:3128".to_string())
            );
        }

        #[test]
        fn proxy_host_with_colon_in_ipv6() {
            // IPv6 addresses contain ':' — only the first ':' in the line is the key/value delimiter.
            // scutil typically wraps IPv6 in brackets; ensure the parser doesn't break.
            let input = "\
                SOCKSEnable : 1\n\
                SOCKSProxy : [::1]\n\
                SOCKSPort : 1080\n";
            let cfg = parse_scutil_output(input);
            assert!(cfg.enabled);
            assert_eq!(cfg.proxy_url, Some("socks5://[::1]:1080".to_string()));
        }
    }
}
