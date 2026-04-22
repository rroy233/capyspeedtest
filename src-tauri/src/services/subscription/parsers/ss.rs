//! Shadowsocks 协议解析器

use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use url::Url;
use urlencoding::decode as url_decode;

use super::super::types::ProxyPayload;
use super::super::utils::{
    decode_base64_flexible, extract_userinfo_from_raw, parse_share_name, put_non_empty_string,
    query_map,
};

/// 解析 Shadowsocks URL
/// 格式: ss://[base64(userinfo)]@server:port#name 或 ss://method:password@server:port#name
pub fn parse_ss_line(raw: &str) -> Option<ProxyPayload> {
    let mut url = Url::parse(raw).ok()?;
    if url.port().is_none() {
        let host = url.host_str()?;
        let host = url_decode(host)
            .ok()
            .map(|s| s.into_owned())
            .unwrap_or_else(|| host.to_string());
        let decoded = decode_base64_flexible(&host)?;
        let decoded_text = String::from_utf8(decoded).ok()?;
        url = Url::parse(&format!("ss://{decoded_text}")).ok()?;
    }

    let mut cipher = if url.username().is_empty() {
        extract_userinfo_from_raw(raw)
            .and_then(|u| u.split(':').next().map(ToString::to_string))
            .unwrap_or_default()
    } else {
        url.username().to_string()
    };
    cipher = url_decode(&cipher)
        .ok()
        .map(|s| s.into_owned())
        .unwrap_or(cipher);
    let mut password = url.password().map(ToString::to_string);
    if password.is_none() {
        if let Some(decoded) =
            decode_base64_flexible(&cipher).and_then(|d| String::from_utf8(d).ok())
        {
            if let Some((decoded_cipher, decoded_password)) = decoded.split_once(':') {
                cipher = decoded_cipher.to_string();
                password = Some(decoded_password.to_string());
            }
        }
    }
    if password.is_none() {
        if let Some(userinfo) = extract_userinfo_from_raw(raw) {
            if let Some((plain_cipher, plain_password)) = userinfo.split_once(':') {
                cipher = plain_cipher.to_string();
                password = Some(plain_password.to_string());
            }
        }
    }
    if password.is_none() {
        return None;
    }

    let query = query_map(&url);
    let mut payload = ProxyPayload::new();
    payload.insert(
        "name".into(),
        JsonValue::String(parse_share_name(
            &url,
            &format!("{}:{}", url.host_str()?, url.port().unwrap_or(443)),
        )),
    );
    payload.insert("type".into(), JsonValue::String("ss".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));
    payload.insert("cipher".into(), JsonValue::String(cipher));
    payload.insert(
        "password".into(),
        JsonValue::String(password.unwrap_or_default()),
    );
    payload.insert("udp".into(), JsonValue::Bool(true));

    if query
        .get("udp-over-tcp")
        .map(|v| v == "true")
        .unwrap_or(false)
        || query.get("uot").map(|v| v == "1").unwrap_or(false)
    {
        payload.insert("udp-over-tcp".into(), JsonValue::Bool(true));
    }

    if let Some(plugin) = query.get("plugin").filter(|v| v.contains(';')) {
        let plugin_query = format!("pluginName={}", plugin.replace(';', "&"));
        let plugin_map = url::form_urlencoded::parse(plugin_query.as_bytes())
            .into_owned()
            .collect::<HashMap<String, String>>();
        if let Some(plugin_name) = plugin_map.get("pluginName") {
            if plugin_name.contains("obfs") {
                payload.insert("plugin".into(), JsonValue::String("obfs".to_string()));
                payload.insert(
                    "plugin-opts".into(),
                    json!({
                        "mode": plugin_map.get("obfs").cloned().unwrap_or_default(),
                        "host": plugin_map.get("obfs-host").cloned().unwrap_or_default()
                    }),
                );
            } else if plugin_name.contains("v2ray-plugin") {
                payload.insert(
                    "plugin".into(),
                    JsonValue::String("v2ray-plugin".to_string()),
                );
                payload.insert(
                    "plugin-opts".into(),
                    json!({
                        "mode": plugin_map.get("mode").cloned().unwrap_or_default(),
                        "host": plugin_map.get("host").cloned().unwrap_or_default(),
                        "path": plugin_map.get("path").cloned().unwrap_or_default(),
                        "tls": plugin.contains("tls")
                    }),
                );
            }
        }
    }
    Some(payload)
}
