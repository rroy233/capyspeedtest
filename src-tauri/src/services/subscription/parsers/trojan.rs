//! Trojan 协议解析器

use serde_json::{json, Value as JsonValue};
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{parse_bool_like, parse_share_name, put_non_empty_string, query_map};

/// 解析 Trojan URL
/// 格式: trojan://pass@example.com:443?type=ws&path=%2Fws#t1
pub fn parse_trojan_line(raw: &str) -> Option<ProxyPayload> {
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
    payload.insert("type".into(), JsonValue::String("trojan".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));
    if url.username().is_empty() {
        return None;
    }
    payload.insert(
        "password".into(),
        JsonValue::String(url.username().to_string()),
    );
    payload.insert("udp".into(), JsonValue::Bool(true));
    if parse_bool_like(query.get("allowInsecure")) || parse_bool_like(query.get("insecure")) {
        payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    }
    put_non_empty_string(&mut payload, "sni", query.get("sni"));
    if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
        payload.insert(
            "alpn".into(),
            JsonValue::Array(
                alpn.split(',')
                    .map(|s| JsonValue::String(s.trim().to_string()))
                    .collect(),
            ),
        );
    }
    let network = query
        .get("type")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if !network.is_empty() {
        payload.insert("network".into(), JsonValue::String(network.clone()));
        match network.as_str() {
            "ws" => {
                let ws_opts = json!({
                    "path": query.get("path").cloned().unwrap_or_default(),
                    "headers": {"User-Agent": "Mozilla/5.0"}
                });
                payload.insert("ws-opts".into(), ws_opts);
            }
            "grpc" => {
                let grpc_opts = json!({
                    "grpc-service-name": query.get("serviceName").cloned().unwrap_or_default()
                });
                payload.insert("grpc-opts".into(), grpc_opts);
            }
            _ => {}
        }
    }
    payload.insert(
        "client-fingerprint".into(),
        JsonValue::String(
            query
                .get("fp")
                .cloned()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "chrome".to_string()),
        ),
    );
    put_non_empty_string(&mut payload, "fingerprint", query.get("pcs"));
    Some(payload)
}
