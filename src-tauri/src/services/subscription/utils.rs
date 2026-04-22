//! 公共工具函数

use base64::Engine;
use std::collections::HashMap;
use serde_json::{Map as JsonMap, Value as JsonValue};
use url::Url;
use urlencoding::decode as url_decode;

use super::types::ProxyPayload;

/// 从 URL 中提取 userinfo 部分（不依赖 rust-url 的解析）
pub fn extract_userinfo_from_raw(raw: &str) -> Option<String> {
    let (_, rest) = raw.split_once("://")?;
    let authority = rest.split('/').next().unwrap_or(rest);
    let (userinfo, _) = authority.rsplit_once('@')?;
    Some(
        url_decode(userinfo)
            .ok()
            .map(|s| s.into_owned())
            .unwrap_or_else(|| userinfo.to_string()),
    )
}

/// 解析查询字符串为 HashMap
pub fn query_map(url: &Url) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in url.query_pairs() {
        map.insert(k.to_string(), v.to_string());
    }
    map
}

/// 解析查询字符串为 HashMap（支持同一 key 多个值）
pub fn query_vec_map(url: &Url) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::<String, Vec<String>>::new();
    for (k, v) in url.query_pairs() {
        map.entry(k.to_string()).or_default().push(v.to_string());
    }
    map
}

/// 分割逗号分隔的字符串
pub fn split_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// 添加非空字符串到 payload
pub fn put_non_empty_string(payload: &mut ProxyPayload, key: &str, value: Option<&String>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        payload.insert(key.to_string(), JsonValue::String(v.to_string()));
    }
}

/// 添加非空字符串到 payload（Map 版本）
pub fn put_non_empty_string_map(payload: &mut ProxyPayload, key: &str, value: Option<&String>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        payload.insert(key.to_string(), JsonValue::String(v.to_string()));
    }
}

/// 解析布尔值-like 字符串
pub fn parse_bool_like(value: Option<&String>) -> bool {
    value
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// 从 URL fragment 或默认值获取分享名称
pub fn parse_share_name(url: &Url, fallback: &str) -> String {
    url.fragment()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|encoded| url_decode(encoded).ok().map(|s| s.into_owned()))
        .unwrap_or_else(|| fallback.to_string())
}

/// 规范化协议名称
pub fn normalize_protocol(protocol: &str) -> String {
    match protocol.to_ascii_lowercase().as_str() {
        "hy2" => "hysteria2".to_string(),
        "socks" | "socks5" | "socks5h" => "socks5".to_string(),
        other => other.to_string(),
    }
}

/// 从 JsonValue 中提取 u16
pub fn extract_json_u16(value: Option<&JsonValue>) -> Option<u16> {
    let value = value?;
    if let Some(v) = value.as_u64() {
        return u16::try_from(v).ok();
    }
    if let Some(v) = value.as_i64() {
        return u16::try_from(v).ok();
    }
    value.as_str()?.parse::<u16>().ok()
}

/// 从 JsonValue 中提取 u16（别名）
pub fn extract_u16(value: Option<&JsonValue>) -> Option<u16> {
    extract_json_u16(value)
}

/// 灵活的 Base64 解码（支持标准、URL-safe、URL-safe-no-pad）
pub fn decode_base64_flexible(content: &str) -> Option<Vec<u8>> {
    let normalized = content.replace('-', "+").replace('_', "/");
    let mut with_padding = normalized.clone();
    while with_padding.len() % 4 != 0 {
        with_padding.push('=');
    }
    base64::engine::general_purpose::STANDARD
        .decode(&with_padding)
        .ok()
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE
                .decode(content)
                .ok()
        })
        .or_else(|| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(content)
                .ok()
        })
}

/// 唯一名称生成
pub fn unique_name(names: &mut HashMap<String, usize>, name: &str) -> String {
    if let Some(index) = names.get_mut(name) {
        *index += 1;
        format!("{name}-{:02}", *index)
    } else {
        names.insert(name.to_string(), 0);
        name.to_string()
    }
}
