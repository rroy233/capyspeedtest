//! VMess 协议解析器

use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{decode_base64_flexible, query_map, split_csv};
use super::v2::{append_v_share_transport_fields, handle_v_share_link};

/// 解析 VMess URL（支持 JSON 和 AEAD 格式）
/// 格式: vmess://[base64-json] 或 vmess://uuid@host:port?...
pub fn parse_vmess_line(raw: &str) -> Option<ProxyPayload> {
    let body = raw.strip_prefix("vmess://")?;
    if let Some(decoded) =
        decode_base64_flexible(body).and_then(|bytes| String::from_utf8(bytes).ok())
    {
        if let Ok(values) = serde_json::from_str::<JsonValue>(&decoded) {
            return build_vmess_payload_from_json(&values);
        }
    }

    let url = Url::parse(raw).ok()?;
    let mut payload = handle_v_share_link(&url, "vmess")?;
    let query = query_map(&url);
    payload.insert("alterId".into(), JsonValue::from(0));
    payload.insert(
        "cipher".into(),
        JsonValue::String(
            query
                .get("encryption")
                .filter(|v| !v.is_empty())
                .cloned()
                .unwrap_or_else(|| "auto".to_string()),
        ),
    );
    Some(payload)
}

/// 从 VMess JSON 对象构建 payload
pub fn build_vmess_payload_from_json(values: &JsonValue) -> Option<ProxyPayload> {
    let obj = values.as_object()?;
    let server = obj.get("add")?.as_str()?.to_string();
    let uuid = obj.get("id")?.as_str()?.to_string();
    let port = extract_json_u16(obj.get("port"))?;
    let name = obj
        .get("ps")
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or("vmess")
        .to_string();

    let mut payload = ProxyPayload::new();
    payload.insert("name".into(), JsonValue::String(name));
    payload.insert("type".into(), JsonValue::String("vmess".to_string()));
    payload.insert("server".into(), JsonValue::String(server));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("uuid".into(), JsonValue::String(uuid));
    payload.insert(
        "alterId".into(),
        JsonValue::from(extract_json_u16(obj.get("aid")).unwrap_or(0)),
    );
    payload.insert("udp".into(), JsonValue::Bool(true));
    payload.insert("xudp".into(), JsonValue::Bool(true));
    payload.insert("tls".into(), JsonValue::Bool(false));
    payload.insert("skip-cert-verify".into(), JsonValue::Bool(false));
    payload.insert(
        "cipher".into(),
        JsonValue::String(
            obj.get("scy")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .unwrap_or("auto")
                .to_string(),
        ),
    );

    if let Some(sni) = obj
        .get("sni")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
    {
        payload.insert("servername".into(), JsonValue::String(sni.to_string()));
    }

    let mut network = obj
        .get("net")
        .and_then(|v| v.as_str())
        .unwrap_or("tcp")
        .to_ascii_lowercase();
    if obj.get("type").and_then(|v| v.as_str()) == Some("http") {
        network = "http".to_string();
    } else if network == "http" {
        network = "h2".to_string();
    }
    payload.insert("network".into(), JsonValue::String(network.clone()));

    if let Some(tls) = obj.get("tls").and_then(|v| v.as_str()) {
        if tls.to_ascii_lowercase().ends_with("tls") {
            payload.insert("tls".into(), JsonValue::Bool(true));
        }
        if let Some(alpn) = obj
            .get("alpn")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
        {
            payload.insert(
                "alpn".into(),
                JsonValue::Array(split_csv(alpn).into_iter().map(JsonValue::String).collect()),
            );
        }
    }

    match network.as_str() {
        "http" => {
            let host = obj
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = obj
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("/")
                .to_string();
            payload.insert(
                "http-opts".into(),
                json!({
                    "path": [path],
                    "headers": if host.is_empty() { json!({}) } else { json!({"Host": [host]}) }
                }),
            );
        }
        "h2" => {
            let host = obj
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = obj
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            payload.insert(
                "h2-opts".into(),
                json!({
                    "path": path,
                    "headers": if host.is_empty() { json!({}) } else { json!({"Host": [host]}) }
                }),
            );
        }
        "ws" | "httpupgrade" => {
            let host = obj
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let path = obj
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("/")
                .to_string();
            payload.insert(
                "ws-opts".into(),
                json!({
                    "path": path,
                    "headers": if host.is_empty() { json!({"User-Agent":"Mozilla/5.0"}) } else { json!({"User-Agent":"Mozilla/5.0","Host":host}) }
                }),
            );
        }
        "grpc" => {
            payload.insert(
                "grpc-opts".into(),
                json!({
                    "grpc-service-name": obj.get("path").and_then(|v| v.as_str()).unwrap_or("")
                }),
            );
        }
        _ => {}
    }
    Some(payload)
}

fn extract_json_u16(value: Option<&JsonValue>) -> Option<u16> {
    let value = value?;
    if let Some(v) = value.as_u64() {
        return u16::try_from(v).ok();
    }
    if let Some(v) = value.as_i64() {
        return u16::try_from(v).ok();
    }
    value.as_str()?.parse::<u16>().ok()
}
