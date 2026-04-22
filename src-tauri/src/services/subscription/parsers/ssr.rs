//! ShadowsocksR 协议解析器

use serde_json::Value as JsonValue;
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::{decode_base64_flexible, parse_share_name, put_non_empty_string};

/// 解析 SSR URL
/// 格式: ssr://[base64(host:port:protocol:method:obfs:password/?obfsparam=xxx&protoparam=xxx&remarks=xxx)]
pub fn parse_ssr_line(raw: &str) -> Option<ProxyPayload> {
    let body = raw.strip_prefix("ssr://")?;
    let decoded = decode_base64_flexible(body)?;
    let decoded_text = String::from_utf8(decoded).ok()?;
    let (before, after) = decoded_text.split_once("/?")?;
    let parts = before.split(':').collect::<Vec<_>>();
    if parts.len() != 6 {
        return None;
    }

    let host = parts[0];
    let port = parts[1].parse::<u16>().ok().unwrap_or(443);
    let protocol = parts[2];
    let method = parts[3];
    let obfs = parts[4];
    let password = String::from_utf8(decode_base64_flexible(parts[5])?).ok()?;

    let query = url::form_urlencoded::parse(after.as_bytes())
        .into_owned()
        .collect::<HashMap<String, String>>();
    let remarks = query
        .get("remarks")
        .and_then(|v| decode_base64_flexible(v))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_else(|| format!("{host}:{port}"));
    let obfs_param = query
        .get("obfsparam")
        .and_then(|v| decode_base64_flexible(v))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();
    let protocol_param = query
        .get("protoparam")
        .and_then(|v| decode_base64_flexible(v))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default();

    let mut payload = ProxyPayload::new();
    payload.insert("name".into(), JsonValue::String(remarks));
    payload.insert("type".into(), JsonValue::String("ssr".to_string()));
    payload.insert("server".into(), JsonValue::String(host.to_string()));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("cipher".into(), JsonValue::String(method.to_string()));
    payload.insert("password".into(), JsonValue::String(password));
    payload.insert("obfs".into(), JsonValue::String(obfs.to_string()));
    payload.insert("protocol".into(), JsonValue::String(protocol.to_string()));
    payload.insert("udp".into(), JsonValue::Bool(true));
    if !obfs_param.is_empty() {
        payload.insert("obfs-param".into(), JsonValue::String(obfs_param));
    }
    if !protocol_param.is_empty() {
        payload.insert("protocol-param".into(), JsonValue::String(protocol_param));
    }
    Some(payload)
}

use std::collections::HashMap;
