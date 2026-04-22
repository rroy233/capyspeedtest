//! Netch 格式解析器

use serde_json::{Value as JsonValue};

use super::super::types::ProxyPayload;
use super::super::utils::decode_base64_flexible;

/// 解析 Netch 格式
/// 格式: Netch://[base64(JSON)]
pub fn parse_netch_line(raw: &str) -> Vec<ProxyPayload> {
    let body = match raw.strip_prefix("Netch://") {
        Some(b) => b,
        None => return Vec::new(),
    };
    let decoded = match decode_base64_flexible(body) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let decoded_text = match String::from_utf8(decoded) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let json: serde_json::Value = match serde_json::from_str(&decoded_text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    // Netch JSON 格式
    let server_array = json.get("Server").and_then(|v| v.as_array());
    let mut result = Vec::new();

    if let Some(servers) = server_array {
        for server_val in servers {
            let mut payload = ProxyPayload::new();
            let server = server_val.get("Hostname").and_then(|v| v.as_str()).unwrap_or("");
            let remarks = server_val.get("Remark").and_then(|v| v.as_str()).unwrap_or(server);
            let port = server_val.get("Port").and_then(|v| v.as_u64()).unwrap_or(443) as u16;
            let protocol_type = server_val.get("Type").and_then(|v| v.as_str()).unwrap_or("");

            payload.insert("name".into(), JsonValue::String(remarks.to_string()));
            payload.insert("server".into(), JsonValue::String(server.to_string()));
            payload.insert("port".into(), JsonValue::from(port));

            match protocol_type {
                "SS" => {
                    payload.insert("type".into(), JsonValue::String("ss".to_string()));
                    if let Some(method) = server_val.get("EncryptMethod").and_then(|v| v.as_str()) {
                        payload.insert("cipher".into(), JsonValue::String(method.to_string()));
                    }
                    if let Some(password) = server_val.get("Password").and_then(|v| v.as_str()) {
                        payload.insert("password".into(), JsonValue::String(password.to_string()));
                    }
                    payload.insert("udp".into(), JsonValue::Bool(true));
                }
                "SSR" => {
                    payload.insert("type".into(), JsonValue::String("ssr".to_string()));
                    if let Some(method) = server_val.get("EncryptMethod").and_then(|v| v.as_str()) {
                        payload.insert("cipher".into(), JsonValue::String(method.to_string()));
                    }
                    if let Some(password) = server_val.get("Password").and_then(|v| v.as_str()) {
                        payload.insert("password".into(), JsonValue::String(password.to_string()));
                    }
                    if let Some(protocol) = server_val.get("Protocol").and_then(|v| v.as_str()) {
                        payload.insert("protocol".into(), JsonValue::String(protocol.to_string()));
                    }
                    if let Some(obfs) = server_val.get("OBFS").and_then(|v| v.as_str()) {
                        payload.insert("obfs".into(), JsonValue::String(obfs.to_string()));
                    }
                    payload.insert("udp".into(), JsonValue::Bool(true));
                }
                "VMess" => {
                    payload.insert("type".into(), JsonValue::String("vmess".to_string()));
                    if let Some(id) = server_val.get("UserID").and_then(|v| v.as_str()) {
                        payload.insert("uuid".into(), JsonValue::String(id.to_string()));
                    }
                    if let Some(aid) = server_val.get("AlterID").and_then(|v| v.as_u64()) {
                        payload.insert("alterId".into(), JsonValue::from(aid as u16));
                    }
                    if let Some(method) = server_val.get("EncryptMethod").and_then(|v| v.as_str()) {
                        payload.insert("cipher".into(), JsonValue::String(method.to_string()));
                    } else {
                        payload.insert("cipher".into(), JsonValue::String("auto".to_string()));
                    }
                    let transprot = server_val.get("TransferProtocol").and_then(|v| v.as_str()).unwrap_or("tcp");
                    payload.insert("network".into(), JsonValue::String(transprot.to_string()));
                    payload.insert("udp".into(), JsonValue::Bool(true));
                }
                "Socks5" => {
                    payload.insert("type".into(), JsonValue::String("socks5".to_string()));
                    if let Some(username) = server_val.get("Username").and_then(|v| v.as_str()) {
                        payload.insert("username".into(), JsonValue::String(username.to_string()));
                    }
                    if let Some(password) = server_val.get("Password").and_then(|v| v.as_str()) {
                        payload.insert("password".into(), JsonValue::String(password.to_string()));
                    }
                }
                "Trojan" => {
                    payload.insert("type".into(), JsonValue::String("trojan".to_string()));
                    if let Some(password) = server_val.get("Password").and_then(|v| v.as_str()) {
                        payload.insert("password".into(), JsonValue::String(password.to_string()));
                    }
                    payload.insert("udp".into(), JsonValue::Bool(true));
                }
                _ => continue,
            }

            result.push(payload);
        }
    }

    result
}
