//! Hysteria2 协议解析器

use serde_json::{json, Value as JsonValue};
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{extract_userinfo_from_raw, parse_share_name, parse_bool_like, put_non_empty_string, query_map, split_csv};

/// 解析 Hysteria2 URL
/// 格式: hy2://letmein@example.com:8443/?insecure=1&obfs=salamander&obfs-password=gawrgura&pinSHA256=deadbeef&sni=real.example.com&up=114&down=514&alpn=h3,h4#hy2test
pub fn parse_hysteria2_line(raw: &str) -> Option<ProxyPayload> {
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
    payload.insert("type".into(), JsonValue::String("hysteria2".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));
    put_non_empty_string(&mut payload, "obfs", query.get("obfs"));
    put_non_empty_string(&mut payload, "obfs-password", query.get("obfs-password"));
    put_non_empty_string(&mut payload, "sni", query.get("sni"));
    if parse_bool_like(query.get("insecure")) {
        payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    }
    if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
        payload.insert(
            "alpn".into(),
            JsonValue::Array(split_csv(alpn).into_iter().map(JsonValue::String).collect()),
        );
    }
    if let Some(userinfo) = extract_userinfo_from_raw(raw).filter(|v| !v.is_empty()) {
        payload.insert("password".into(), JsonValue::String(userinfo));
    } else if !url.username().is_empty() {
        payload.insert(
            "password".into(),
            JsonValue::String(url.username().to_string()),
        );
    }
    put_non_empty_string(&mut payload, "fingerprint", query.get("pinSHA256"));
    put_non_empty_string(&mut payload, "up", query.get("up"));
    put_non_empty_string(&mut payload, "down", query.get("down"));
    Some(payload)
}
