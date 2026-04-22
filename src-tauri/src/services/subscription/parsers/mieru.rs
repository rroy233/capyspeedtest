//! Mieru 协议解析器

use serde_json::{Value as JsonValue};
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{parse_share_name, query_vec_map};

/// 解析 Mieru URL
/// 格式: mierus://user:pass@1.2.3.4?profile=default&port=6666&port=9998-9999&protocol=TCP&protocol=UDP
pub fn parse_mierus_line(raw: &str) -> Vec<ProxyPayload> {
    let Some(url) = Url::parse(raw).ok() else {
        return Vec::new();
    };
    let Some(server) = url.host_str() else {
        return Vec::new();
    };
    let username = url.username().to_string();
    let password = url.password().unwrap_or_default().to_string();
    let query_map_vec = query_vec_map(&url);
    let port_list = query_map_vec.get("port").cloned().unwrap_or_default();
    let protocol_list = query_map_vec.get("protocol").cloned().unwrap_or_default();
    if port_list.is_empty() || port_list.len() != protocol_list.len() {
        return Vec::new();
    }

    let base_name = parse_share_name(
        &url,
        query_map_vec
            .get("profile")
            .and_then(|v| v.first())
            .map(String::as_str)
            .unwrap_or(server),
    );
    let multiplexing = query_map_vec
        .get("multiplexing")
        .and_then(|v| v.first())
        .cloned();
    let handshake_mode = query_map_vec
        .get("handshake-mode")
        .and_then(|v| v.first())
        .cloned();
    let traffic_pattern = query_map_vec
        .get("traffic-pattern")
        .and_then(|v| v.first())
        .cloned();

    let mut result = Vec::new();
    for (idx, port) in port_list.iter().enumerate() {
        let protocol = protocol_list[idx].clone();
        let mut payload = ProxyPayload::new();
        payload.insert(
            "name".into(),
            JsonValue::String(format!("{base_name}:{port}/{protocol}")),
        );
        payload.insert("type".into(), JsonValue::String("mieru".to_string()));
        payload.insert("server".into(), JsonValue::String(server.to_string()));
        payload.insert("transport".into(), JsonValue::String(protocol));
        payload.insert("udp".into(), JsonValue::Bool(true));
        payload.insert("username".into(), JsonValue::String(username.clone()));
        payload.insert("password".into(), JsonValue::String(password.clone()));
        if port.contains('-') {
            payload.insert("port-range".into(), JsonValue::String(port.clone()));
        } else if let Ok(p) = port.parse::<u16>() {
            payload.insert("port".into(), JsonValue::from(p));
        } else {
            continue;
        }
        if let Some(v) = multiplexing.clone().filter(|v| !v.is_empty()) {
            payload.insert("multiplexing".into(), JsonValue::String(v));
        }
        if let Some(v) = handshake_mode.clone().filter(|v| !v.is_empty()) {
            payload.insert("handshake-mode".into(), JsonValue::String(v));
        }
        if let Some(v) = traffic_pattern.clone().filter(|v| !v.is_empty()) {
            payload.insert("traffic-pattern".into(), JsonValue::String(v));
        }
        result.push(payload);
    }
    result
}
