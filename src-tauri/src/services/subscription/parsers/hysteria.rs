//! Hysteria 协议解析器

use serde_json::{json, Value as JsonValue};
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{parse_share_name, parse_bool_like, put_non_empty_string, query_map, split_csv};

/// 解析 Hysteria URL
/// 格式: hysteria://example.com:443?peer=cdn.example.com&obfs=foo&auth=bar&up=10&down=20&insecure=1#hy
pub fn parse_hysteria_line(raw: &str) -> Option<ProxyPayload> {
    let url = Url::parse(raw).ok()?;
    let query = query_map(&url);
    let mut payload = ProxyPayload::new();
    payload.insert(
        "name".into(),
        JsonValue::String(parse_share_name(
            &url,
            &format!("{}:{}", url.host_str()?, url.port().unwrap_or(443)),
        )),
    );
    payload.insert("type".into(), JsonValue::String("hysteria".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));
    put_non_empty_string(&mut payload, "sni", query.get("peer"));
    put_non_empty_string(&mut payload, "obfs", query.get("obfs"));
    put_non_empty_string(&mut payload, "auth_str", query.get("auth"));
    put_non_empty_string(&mut payload, "protocol", query.get("protocol"));
    if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
        payload.insert(
            "alpn".into(),
            JsonValue::Array(split_csv(alpn).into_iter().map(JsonValue::String).collect()),
        );
    }
    let up = query.get("up").or_else(|| query.get("upmbps"));
    let down = query.get("down").or_else(|| query.get("downmbps"));
    put_non_empty_string(&mut payload, "up", up);
    put_non_empty_string(&mut payload, "down", down);
    if parse_bool_like(query.get("insecure")) {
        payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    }
    Some(payload)
}
