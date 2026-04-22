//! SOCKS/HTTP 代理协议解析器

use serde_json::Value as JsonValue;
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{decode_base64_flexible, extract_userinfo_from_raw, parse_share_name};

/// 解析 SOCKS/HTTP URL
/// 格式: socks5://user:pass@server:port#name 或 http://user:pass@server:port#name
pub fn parse_socks_like_line(raw: &str) -> Option<ProxyPayload> {
    let url = Url::parse(raw).ok()?;
    let scheme = url.scheme().to_ascii_lowercase();
    let server = url.host_str()?.to_string();
    let port = url.port()?;
    let mut username = String::new();
    let mut password = String::new();
    if let Some(encoded_user) = extract_userinfo_from_raw(raw).filter(|v| !v.is_empty()) {
        if let Some(decoded) =
            decode_base64_flexible(&encoded_user).and_then(|bytes| String::from_utf8(bytes).ok())
        {
            if let Some((u, p)) = decoded.split_once(':') {
                username = u.to_string();
                password = p.to_string();
            } else {
                username = decoded;
            }
        } else {
            username = url.username().to_string();
            password = url.password().unwrap_or_default().to_string();
        }
    }
    let mut payload = ProxyPayload::new();
    payload.insert(
        "name".into(),
        JsonValue::String(parse_share_name(&url, &format!("{server}:{port}"))),
    );
    let mapped_type = match scheme.as_str() {
        "socks" | "socks5" | "socks5h" => "socks5",
        "http" | "https" => "http",
        _ => scheme.as_str(),
    };
    payload.insert("type".into(), JsonValue::String(mapped_type.to_string()));
    payload.insert("server".into(), JsonValue::String(server));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("username".into(), JsonValue::String(username));
    payload.insert("password".into(), JsonValue::String(password));
    payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    if scheme == "https" {
        payload.insert("tls".into(), JsonValue::Bool(true));
    }
    Some(payload)
}
