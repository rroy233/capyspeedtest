//! SSD (Shadowsocks Android) 订阅格式解析器

use serde_json::{Value as JsonValue};

use super::super::types::ProxyPayload;
use super::super::utils::decode_base64_flexible;

/// 解析 SSD 订阅格式
/// 格式: ssd://[base64(JSON)]
pub fn parse_ssd_line(raw: &str) -> Vec<ProxyPayload> {
    let body = match raw.strip_prefix("ssd://") {
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

    if !json.get("servers").is_some() {
        return Vec::new();
    }

    let default_port = json.get("port").and_then(|v| v.as_u64()).unwrap_or(443) as u16;
    let default_method = json.get("encryption").and_then(|v| v.as_str()).unwrap_or("");
    let default_password = json.get("password").and_then(|v| v.as_str()).unwrap_or("");
    let default_plugin = json.get("plugin").and_then(|v| v.as_str()).unwrap_or("");
    let default_plugin_opts = json.get("plugin_options").and_then(|v| v.as_str()).unwrap_or("");

    let servers = json.get("servers").and_then(|v| v.as_array());
    let mut result = Vec::new();

    if let Some(servers_arr) = servers {
        for server in servers_arr {
            let mut payload = ProxyPayload::new();
            let server_str = server.get("server").and_then(|v| v.as_str()).unwrap_or("");
            let remarks = server.get("remarks").and_then(|v| v.as_str()).unwrap_or(server_str);
            let port = server.get("port").and_then(|v| v.as_u64()).unwrap_or(default_port as u64) as u16;
            let method = server.get("encryption").and_then(|v| v.as_str()).unwrap_or(default_method);
            let password = server.get("password").and_then(|v| v.as_str()).unwrap_or(default_password);
            let plugin = server.get("plugin").and_then(|v| v.as_str()).unwrap_or(default_plugin);
            let plugin_opts = server.get("plugin_options").and_then(|v| v.as_str()).unwrap_or(default_plugin_opts);

            payload.insert("name".into(), JsonValue::String(remarks.to_string()));
            payload.insert("type".into(), JsonValue::String("ss".to_string()));
            payload.insert("server".into(), JsonValue::String(server_str.to_string()));
            payload.insert("port".into(), JsonValue::from(port));
            payload.insert("cipher".into(), JsonValue::String(method.to_string()));
            payload.insert("password".into(), JsonValue::String(password.to_string()));
            payload.insert("udp".into(), JsonValue::Bool(true));

            if !plugin.is_empty() {
                payload.insert("plugin".into(), JsonValue::String(plugin.to_string()));
                payload.insert("plugin-opts".into(), JsonValue::String(plugin_opts.to_string()));
            }

            result.push(payload);
        }
    }

    result
}
