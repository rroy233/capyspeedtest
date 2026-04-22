//! V2Ray 协议共享解析逻辑（VMess/VLESS）

use base64::Engine;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::collections::HashMap;
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{parse_share_name, put_non_empty_string, query_map, split_csv};

/// 处理 V 协议共享链接的公共部分
pub fn handle_v_share_link(url: &Url, scheme: &str) -> Option<ProxyPayload> {
    let query = query_map(url);
    let mut payload = ProxyPayload::new();
    payload.insert(
        "name".into(),
        JsonValue::String(parse_share_name(
            url,
            &format!("{}:{}", url.host_str()?, url.port().unwrap_or(443)),
        )),
    );
    payload.insert("type".into(), JsonValue::String(scheme.to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port()?));
    payload.insert("uuid".into(), JsonValue::String(url.username().to_string()));
    payload.insert("udp".into(), JsonValue::Bool(true));

    let security = query
        .get("security")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let tls_enabled = security.ends_with("tls") || security == "reality";
    if tls_enabled {
        payload.insert("tls".into(), JsonValue::Bool(true));
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
        if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
            payload.insert(
                "alpn".into(),
                JsonValue::Array(split_csv(alpn).into_iter().map(JsonValue::String).collect()),
            );
        }
        put_non_empty_string(&mut payload, "fingerprint", query.get("pcs"));
    }
    put_non_empty_string(&mut payload, "servername", query.get("sni"));
    if let Some(pbk) = query.get("pbk").filter(|v| !v.is_empty()) {
        payload.insert(
            "reality-opts".into(),
            json!({
                "public-key": pbk,
                "short-id": query.get("sid").cloned().unwrap_or_default()
            }),
        );
    }

    match query.get("packetEncoding").map(String::as_str) {
        Some("none") => {}
        Some("packet") => {
            payload.insert("packet-addr".into(), JsonValue::Bool(true));
        }
        _ => {
            payload.insert("xudp".into(), JsonValue::Bool(true));
        }
    }

    let mut network = query
        .get("type")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".to_string());
    let fake_type = query
        .get("headerType")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if fake_type == "http" {
        network = "http".to_string();
    } else if network == "http" {
        network = "h2".to_string();
    }
    payload.insert("network".into(), JsonValue::String(network.clone()));
    append_v_share_transport_fields(&query, &mut payload, &network, &fake_type);

    Some(payload)
}

/// 添加 V 协议传输层字段
pub fn append_v_share_transport_fields(
    query: &HashMap<String, String>,
    payload: &mut ProxyPayload,
    network: &str,
    fake_type: &str,
) {
    match network {
        "tcp" => {
            if !fake_type.is_empty() && fake_type != "none" {
                let mut headers = ProxyPayload::new();
                if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                    headers.insert(
                        "Host".to_string(),
                        JsonValue::Array(vec![JsonValue::String(host.to_string())]),
                    );
                }
                let http_opts = json!({
                    "path": [query.get("path").cloned().unwrap_or_else(|| "/".to_string())],
                    "method": query.get("method").cloned().unwrap_or_default(),
                    "headers": headers
                });
                payload.insert("http-opts".into(), http_opts);
            }
        }
        "http" => {
            let mut h2_opts = ProxyPayload::new();
            h2_opts.insert(
                "path".into(),
                JsonValue::Array(vec![JsonValue::String(
                    query
                        .get("path")
                        .cloned()
                        .unwrap_or_else(|| "/".to_string()),
                )]),
            );
            if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                h2_opts.insert(
                    "host".into(),
                    JsonValue::Array(split_csv(host).into_iter().map(JsonValue::String).collect()),
                );
            }
            h2_opts.insert("headers".into(), JsonValue::Object(ProxyPayload::new()));
            payload.insert("h2-opts".into(), JsonValue::Object(h2_opts));
        }
        "ws" | "httpupgrade" => {
            let mut headers = ProxyPayload::new();
            headers.insert(
                "User-Agent".to_string(),
                JsonValue::String("Mozilla/5.0".to_string()),
            );
            if let Some(host) = query.get("host").filter(|v| !v.is_empty()) {
                headers.insert("Host".to_string(), JsonValue::String(host.to_string()));
            }
            let mut ws_opts = ProxyPayload::new();
            ws_opts.insert(
                "path".into(),
                JsonValue::String(query.get("path").cloned().unwrap_or_default()),
            );
            ws_opts.insert("headers".into(), JsonValue::Object(headers));
            if let Some(early_data) = query.get("ed").and_then(|s| s.parse::<u32>().ok()) {
                if network == "ws" {
                    ws_opts.insert("max-early-data".into(), JsonValue::from(early_data));
                    ws_opts.insert(
                        "early-data-header-name".into(),
                        JsonValue::String("Sec-WebSocket-Protocol".to_string()),
                    );
                } else {
                    ws_opts.insert("v2ray-http-upgrade-fast-open".into(), JsonValue::Bool(true));
                }
            }
            if let Some(eh) = query.get("eh").filter(|v| !v.is_empty()) {
                ws_opts.insert(
                    "early-data-header-name".into(),
                    JsonValue::String(eh.to_string()),
                );
            }
            payload.insert("ws-opts".into(), JsonValue::Object(ws_opts));
        }
        "grpc" => {
            payload.insert(
                "grpc-opts".into(),
                json!({
                    "grpc-service-name": query.get("serviceName").cloned().unwrap_or_default()
                }),
            );
        }
        "xhttp" => {
            let mut xhttp_opts = ProxyPayload::new();
            put_non_empty_string_map(&mut xhttp_opts, "path", query.get("path"));
            put_non_empty_string_map(&mut xhttp_opts, "host", query.get("host"));
            put_non_empty_string_map(&mut xhttp_opts, "mode", query.get("mode"));
            if !xhttp_opts.is_empty() {
                payload.insert("xhttp-opts".into(), JsonValue::Object(xhttp_opts));
            }
        }
        _ => {}
    }
}

fn put_non_empty_string_map(payload: &mut ProxyPayload, key: &str, value: Option<&String>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        payload.insert(key.to_string(), JsonValue::String(v.to_string()));
    }
}
