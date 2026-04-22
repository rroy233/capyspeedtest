//! VMess 变体格式解析器（Shadowrocket、Kitsunebi、Quan、标准 AEAD）

use std::collections::HashMap;

use regex::Regex;
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use super::super::types::ProxyPayload;
use super::super::utils::decode_base64_flexible;

/// 解析 VMess 标准 AEAD URL 格式
/// 格式: vmess+tcp+tls:uuid-aaaa-bbbb-cccc-dddddddddddd-0@example.com:443?host=xxx
pub fn try_parse_vmess_aead_url(raw: &str) -> Option<Vec<crate::models::NodeInfo>> {
    let body = raw.strip_prefix("vmess://")?;
    // 标准 AEAD 格式: vmess+tcp+tls:uuid-aid@host:port?...
    let re = Regex::new(
        r#"^vmess(?:\+([a-z]+))?(?:\+([a-z]+))?:([\da-f]{8}-[\da-f]{4}-[\da-f]{4}-[\da-f]{4}-[\da-f]{12})-(\d+)@(.+):(\d+)(?:/?\?(.*))?$"#,
    )
    .ok()?;

    if let Some(caps) = re.captures(body) {
        let net = caps.get(1).map(|m| m.as_str()).unwrap_or("tcp");
        let tls = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let id = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        let aid = caps.get(4).map(|m| m.as_str()).unwrap_or("0");
        let host = caps.get(5).map(|m| m.as_str()).unwrap_or("");
        let port: u16 = caps
            .get(6)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(443);
        let query_str = caps.get(7).map(|m| m.as_str()).unwrap_or("");

        let mut payload = ProxyPayload::new();
        payload.insert(
            "name".into(),
            JsonValue::String(format!("{}:{}", host, port)),
        );
        payload.insert("type".into(), JsonValue::String("vmess".to_string()));
        payload.insert("server".into(), JsonValue::String(host.to_string()));
        payload.insert("port".into(), JsonValue::from(port));
        payload.insert("uuid".into(), JsonValue::String(id.to_string()));
        payload.insert(
            "alterId".into(),
            JsonValue::from(aid.parse::<u16>().unwrap_or(0)),
        );
        payload.insert("cipher".into(), JsonValue::String("auto".to_string()));
        payload.insert("udp".into(), JsonValue::Bool(true));
        payload.insert("network".into(), JsonValue::String(net.to_string()));

        if !tls.is_empty() && tls != "none" {
            payload.insert("tls".into(), JsonValue::Bool(true));
        }

        // 解析查询参数
        let query_map: HashMap<String, String> = url::form_urlencoded::parse(query_str.as_bytes())
            .into_owned()
            .collect();

        if let Some(host_val) = query_map.get("host") {
            payload.insert("servername".into(), JsonValue::String(host_val.clone()));
        }
        if let Some(path_val) = query_map.get("path") {
            payload.insert("path".into(), JsonValue::String(path_val.clone()));
        }

        let mut names = HashMap::new();
        let node = super::super::build_node_from_payload(
            &mut payload,
            Some(raw.to_string()),
            "vmess-aead",
            &mut names,
            1,
        )?;
        return Some(vec![node]);
    }

    None
}

/// 解析 Shadowrocket 风格 VMess URL
/// 格式: vmess://[base64(cipher:uuid@server:port)]?remarks=xxx&obfs=websocket&obfsParam=xxx&path=xxx&tls=1
pub fn try_parse_shadowrocket_vmess(raw: &str) -> Option<Vec<crate::models::NodeInfo>> {
    let body = raw.strip_prefix("vmess://")?;
    if !body.contains("?remarks=") {
        return None;
    }

    // Shadowrocket 格式用 ? 分隔，前面是 base64 编码的 userinfo
    let parts: Vec<&str> = body.splitn(2, '?').collect();
    if parts.len() < 2 {
        return None;
    }

    let userinfo_b64 = parts[0];
    let query_str = parts[1];

    let decoded = decode_base64_flexible(userinfo_b64)?;
    let userinfo = String::from_utf8(decoded).ok()?;

    // userinfo 格式: cipher:uuid@server:port
    let user_parts: Vec<&str> = userinfo.splitn(2, ':').collect();
    if user_parts.len() < 2 {
        return None;
    }
    let cipher = user_parts[0];
    let rest = user_parts[1];

    let rest_parts: Vec<&str> = rest.rsplitn(2, '@').collect();
    if rest_parts.len() < 2 {
        return None;
    }
    let server_port = rest_parts[0];
    let uuid = rest_parts[1];

    let sp: Vec<&str> = server_port.rsplitn(2, ':').collect();
    if sp.len() < 2 {
        return None;
    }
    let server = sp[1];
    let port: u16 = sp[0].parse().ok().unwrap_or(443);

    let query_map: HashMap<String, String> = url::form_urlencoded::parse(query_str.as_bytes())
        .into_owned()
        .collect();

    let remarks = query_map
        .get("remarks")
        .cloned()
        .unwrap_or_else(|| format!("{}:{}", server, port));
    let obfs = query_map.get("obfs").cloned().unwrap_or_default();
    let obfs_param = query_map.get("obfsParam").cloned().unwrap_or_default();
    let path = query_map.get("path").cloned().unwrap_or_default();
    let network = query_map.get("network").cloned().unwrap_or_else(|| {
        if obfs == "websocket" {
            "ws".to_string()
        } else {
            "tcp".to_string()
        }
    });
    let tls = query_map.get("tls").map(|v| v == "1").unwrap_or(false);

    let mut payload = ProxyPayload::new();
    payload.insert("name".into(), JsonValue::String(remarks));
    payload.insert("type".into(), JsonValue::String("vmess".to_string()));
    payload.insert("server".into(), JsonValue::String(server.to_string()));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("uuid".into(), JsonValue::String(uuid.to_string()));
    payload.insert("alterId".into(), JsonValue::from(0));
    payload.insert("cipher".into(), JsonValue::String(cipher.to_string()));
    payload.insert("udp".into(), JsonValue::Bool(true));
    payload.insert("network".into(), JsonValue::String(network.clone()));

    if tls {
        payload.insert("tls".into(), JsonValue::Bool(true));
    }

    if network == "ws" && !path.is_empty() {
        let mut ws_opts = JsonMap::new();
        ws_opts.insert("path".to_string(), JsonValue::String(path.clone()));
        if !obfs_param.is_empty() {
            let mut headers = JsonMap::new();
            headers.insert("Host".to_string(), JsonValue::String(obfs_param));
            ws_opts.insert("headers".to_string(), JsonValue::Object(headers));
        }
        payload.insert("ws-opts".into(), JsonValue::Object(ws_opts));
    }

    let mut names = HashMap::new();
    let node = super::super::build_node_from_payload(
        &mut payload,
        Some(raw.to_string()),
        "vmess-shadowrocket",
        &mut names,
        1,
    )?;
    Some(vec![node])
}

/// 解析 Kitsunebi 风格 VMess URL
/// 格式: vmess1://[base64(userinfo)]?network=ws&tls=true&ws.host=xxx
pub fn try_parse_kitsunebi_vmess(raw: &str) -> Option<Vec<crate::models::NodeInfo>> {
    let body = raw.strip_prefix("vmess1://")?;
    if !body.contains("?network=") && !body.contains("?tls=") {
        return None;
    }

    // 分离 userinfo 和 query
    let parts: Vec<&str> = body.splitn(2, '?').collect();
    if parts.len() < 2 {
        return None;
    }

    let userinfo_b64 = parts[0];
    let query_str = parts[1];

    let decoded = decode_base64_flexible(userinfo_b64)?;
    let userinfo = String::from_utf8(decoded).ok()?;

    // userinfo 格式: uuid@server:port 或 server:port
    let (uuid, server, port) = if userinfo.contains('@') {
        let at_pos = userinfo.find('@').unwrap();
        let uuid = &userinfo[..at_pos];
        let rest = &userinfo[at_pos + 1..];
        let sp: Vec<&str> = rest.rsplitn(2, ':').collect();
        if sp.len() < 2 {
            return None;
        }
        (uuid.to_string(), sp[1], sp[0].parse().unwrap_or(443))
    } else {
        return None;
    };

    let query_map: HashMap<String, String> = url::form_urlencoded::parse(query_str.as_bytes())
        .into_owned()
        .collect();

    let remarks = query_map
        .get("remarks")
        .cloned()
        .unwrap_or_else(|| format!("{}:{}", server, port));
    let network = query_map
        .get("network")
        .cloned()
        .unwrap_or("tcp".to_string());
    let tls = query_map.get("tls").map(|v| v == "true").unwrap_or(false);
    let ws_host = query_map.get("ws.host").cloned().unwrap_or_default();

    let mut payload = ProxyPayload::new();
    payload.insert("name".into(), JsonValue::String(remarks));
    payload.insert("type".into(), JsonValue::String("vmess".to_string()));
    payload.insert("server".into(), JsonValue::String(server.to_string()));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("uuid".into(), JsonValue::String(uuid));
    payload.insert("alterId".into(), JsonValue::from(0));
    payload.insert("cipher".into(), JsonValue::String("auto".to_string()));
    payload.insert("udp".into(), JsonValue::Bool(true));
    payload.insert("network".into(), JsonValue::String(network.clone()));

    if tls {
        payload.insert("tls".into(), JsonValue::Bool(true));
    }

    if network == "ws" && !ws_host.is_empty() {
        let mut ws_opts = JsonMap::new();
        ws_opts.insert("path".to_string(), JsonValue::String("/".to_string()));
        let mut headers = JsonMap::new();
        headers.insert("Host".to_string(), JsonValue::String(ws_host));
        ws_opts.insert("headers".to_string(), JsonValue::Object(headers));
        payload.insert("ws-opts".into(), JsonValue::Object(ws_opts));
    }

    let mut names = HashMap::new();
    let node = super::super::build_node_from_payload(
        &mut payload,
        Some(raw.to_string()),
        "vmess-kitsunebi",
        &mut names,
        1,
    )?;
    Some(vec![node])
}

/// 解析 Quan 风格 VMess 配置
/// 格式: vmess=xxx=vmess,[server],[port],[cipher],[uuid] group=xxx obfs-path=xxx obfs-header=...
pub fn try_parse_quan_vmess(raw: &str) -> Option<Vec<crate::models::NodeInfo>> {
    if !raw.contains("vmess=") || !raw.contains("=vmess,") {
        return None;
    }

    let trimmed = raw.trim();
    let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
    if parts.len() < 2 {
        return None;
    }

    let config_part = parts[1];
    let config_segments: Vec<&str> = config_part.splitn(6, ',').collect();
    if config_segments.len() < 5 {
        return None;
    }

    let remarks = parts[0].trim();
    let server = config_segments[0].trim();
    let port: u16 = config_segments[1].trim().parse().ok().unwrap_or(443);
    let cipher = config_segments[2].trim();
    let uuid = config_segments[3].trim().trim_matches('"');

    // 解析额外参数
    let extra_str = if config_segments.len() > 5 {
        config_segments[5]
    } else {
        ""
    };

    let query_map: HashMap<String, String> = url::form_urlencoded::parse(extra_str.as_bytes())
        .into_owned()
        .collect();

    let obfs = query_map.get("obfs").cloned().unwrap_or_default();
    let obfs_path = query_map.get("obfs-path").cloned().unwrap_or_default();
    let obfs_header = query_map.get("obfs-header").cloned().unwrap_or_default();
    let tls = query_map
        .get("over-tls")
        .map(|v| v == "true")
        .unwrap_or(false);

    let mut network = "tcp".to_string();
    if obfs == "ws" {
        network = "ws".to_string();
    }

    let mut payload = ProxyPayload::new();
    payload.insert("name".into(), JsonValue::String(remarks.to_string()));
    payload.insert("type".into(), JsonValue::String("vmess".to_string()));
    payload.insert("server".into(), JsonValue::String(server.to_string()));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("uuid".into(), JsonValue::String(uuid.to_string()));
    payload.insert("alterId".into(), JsonValue::from(0));
    payload.insert("cipher".into(), JsonValue::String(cipher.to_string()));
    payload.insert("udp".into(), JsonValue::Bool(true));
    payload.insert("network".into(), JsonValue::String(network.clone()));

    if tls {
        payload.insert("tls".into(), JsonValue::Bool(true));
    }

    if network == "ws" && !obfs_path.is_empty() {
        let mut ws_opts = JsonMap::new();
        ws_opts.insert("path".to_string(), JsonValue::String(obfs_path));
        if !obfs_header.is_empty() {
            let mut headers = JsonMap::new();
            // 解析 obfs-header 格式: "Host: xxx\r\n"
            for line in obfs_header.lines() {
                if line.to_lowercase().starts_with("host:") {
                    let host_val = line.trim_start_matches("Host:").trim();
                    headers.insert("Host".to_string(), JsonValue::String(host_val.to_string()));
                    break;
                }
            }
            ws_opts.insert("headers".to_string(), JsonValue::Object(headers));
        }
        payload.insert("ws-opts".into(), JsonValue::Object(ws_opts));
    }

    let mut names = HashMap::new();
    let node = super::super::build_node_from_payload(
        &mut payload,
        Some(raw.to_string()),
        "vmess-quan",
        &mut names,
        1,
    )?;
    Some(vec![node])
}
