//! AnyTLS 协议解析器

use serde_json::Value as JsonValue;
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{parse_share_name, put_non_empty_string, query_map};

/// 解析 AnyTLS URL
/// 格式: anytls://user:pass@example.com:443?sni=example.com&hpkp=fingerprint&insecure=1#at
pub fn parse_anytls_line(raw: &str) -> Option<ProxyPayload> {
    let url = Url::parse(raw).ok()?;
    let query = query_map(&url);
    let server = url.host_str()?.to_string();
    let port = url.port()?;
    let username = url.username().to_string();
    let password = url.password().unwrap_or(&username).to_string();
    let mut payload = ProxyPayload::new();
    payload.insert(
        "name".into(),
        JsonValue::String(parse_share_name(&url, &format!("{server}:{port}"))),
    );
    payload.insert("type".into(), JsonValue::String("anytls".to_string()));
    payload.insert("server".into(), JsonValue::String(server));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("username".into(), JsonValue::String(username));
    payload.insert("password".into(), JsonValue::String(password));
    put_non_empty_string(&mut payload, "sni", query.get("sni"));
    put_non_empty_string(&mut payload, "fingerprint", query.get("hpkp"));
    if query.get("insecure").map(|v| v == "1").unwrap_or(false) {
        payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    }
    payload.insert("udp".into(), JsonValue::Bool(true));
    Some(payload)
}
