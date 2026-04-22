//! Snell 协议解析器

use serde_json::{Value as JsonValue};
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{extract_userinfo_from_raw, parse_share_name, put_non_empty_string, query_map};

/// 解析 Snell URL
/// 格式: snell://[version]:[password]@[server]:[port] 或 snell://[password]@[server]:[port] (默认 v2)
pub fn parse_snell_line(raw: &str) -> Option<ProxyPayload> {
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
    payload.insert("type".into(), JsonValue::String("snell".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));

    // Snell URL 格式: snell://[version]:[password]@[server]:[port]
    // 版本号默认为 2
    let userinfo = extract_userinfo_from_raw(raw)?;
    let parts: Vec<&str> = userinfo.split(':').collect();
    let mut version = 2u16;
    let password;

    if parts.len() >= 2 {
        // 第一个部分可能是版本号
        if let Ok(v) = parts[0].parse::<u16>() {
            version = v;
            password = parts[1..].join(":");
        } else {
            version = 2;
            password = userinfo;
        }
    } else {
        password = userinfo;
    }

    payload.insert("password".into(), JsonValue::String(password));
    payload.insert("version".into(), JsonValue::from(version));

    // obfs 参数
    put_non_empty_string(&mut payload, "obfs", query.get("obfs"));
    put_non_empty_string(&mut payload, "obfs-host", query.get("obfs-host"));

    Some(payload)
}
