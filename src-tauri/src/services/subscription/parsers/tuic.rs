//! TUIC 协议解析器

use serde_json::{json, Value as JsonValue};
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{parse_share_name, put_non_empty_string, query_map, split_csv};

/// 解析 TUIC URL
/// 格式: tuic://token@example.com:443?udp_relay_mode=native#tuic-v4
/// 或: tuic://uuid:pwd@example.com:443?congestion_control=bbr#tuic-v5
pub fn parse_tuic_line(raw: &str) -> Option<ProxyPayload> {
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
    payload.insert("type".into(), JsonValue::String("tuic".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));
    payload.insert("udp".into(), JsonValue::Bool(true));
    if let Some(password) = url.password() {
        payload.insert("uuid".into(), JsonValue::String(url.username().to_string()));
        payload.insert("password".into(), JsonValue::String(password.to_string()));
    } else if !url.username().is_empty() {
        payload.insert(
            "token".into(),
            JsonValue::String(url.username().to_string()),
        );
    }
    put_non_empty_string(
        &mut payload,
        "congestion-controller",
        query.get("congestion_control"),
    );
    put_non_empty_string(&mut payload, "sni", query.get("sni"));
    if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
        payload.insert(
            "alpn".into(),
            JsonValue::Array(split_csv(alpn).into_iter().map(JsonValue::String).collect()),
        );
    }
    if query.get("disable_sni").map(|v| v == "1").unwrap_or(false) {
        payload.insert("disable-sni".into(), JsonValue::Bool(true));
    }
    put_non_empty_string(&mut payload, "udp-relay-mode", query.get("udp_relay_mode"));
    Some(payload)
}
