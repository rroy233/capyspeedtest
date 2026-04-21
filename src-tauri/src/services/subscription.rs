//! 订阅解析模块：统一处理 YAML/URI/Base64 订阅并输出可直接用于测速的节点信息。

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::models::{NodeConnectInfo, NodeFilter, NodeInfo};
use base64::Engine;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use tracing::info;
use url::Url;
use urlencoding::decode as url_decode;

/// 默认下载测速服务器（不经过代理，直连测速用）。
const DEFAULT_TEST_FILE: &str = "http://speedtest.tele2.net/10MB.zip";
/// 默认上传测速服务器。
const DEFAULT_UPLOAD_TARGET: &str = "http://httpbin.org/post";
/// 内部行格式前缀：用于把 YAML 节点重新编码成单行，兼容前端“按 raw 回填再解析”流程。
const INTERNAL_PROXY_PREFIX: &str = "proxycfg://";

type ProxyPayload = JsonMap<String, JsonValue>;

#[derive(Debug, Deserialize)]
struct ProxySubscriptionYaml {
    #[serde(default)]
    proxies: Vec<serde_yaml::Value>,
}

/// 解析订阅文本中的节点链接，并提取基础属性。
pub fn parse_subscription_nodes(raw_input: &str) -> Vec<NodeInfo> {
    let normalized = normalize_input(raw_input);
    if normalized.is_empty() {
        return Vec::new();
    }

    let decoded = decode_base64_subscription_if_needed(&normalized).unwrap_or(normalized);

    if let Some(nodes) = parse_yaml_subscription_nodes(&decoded) {
        if !nodes.is_empty() {
            return nodes;
        }
    }

    parse_uri_subscription_nodes(&decoded)
}

/// 从远程 URL 获取订阅内容并解析为节点列表（异步）。
pub async fn fetch_subscription_from_url(url: &str) -> Result<Vec<NodeInfo>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("capyspeedtest/0.1")
        .build()
        .map_err(|e| format!("初始化客户端失败: {e}"))?;
    let response = client
        .get(url)
        .header("User-Agent", "capyspeedtest/0.1")
        .send()
        .await
        .map_err(|e| format!("获取订阅失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("订阅响应异常: {e}"))?;
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取订阅内容失败: {e}"))?;
    Ok(parse_subscription_nodes(&body))
}

/// 按过滤器筛选节点。
pub fn filter_nodes(nodes: &[NodeInfo], filter: &NodeFilter) -> Result<Vec<NodeInfo>, String> {
    let name_regex = if let Some(pattern) = &filter.name_regex {
        if pattern.trim().is_empty() {
            None
        } else {
            Some(Regex::new(pattern).map_err(|error| format!("无效正则表达式: {error}"))?)
        }
    } else {
        None
    };

    let country_set = filter.countries.as_ref().map(|list| {
        list.iter()
            .map(|item| item.to_ascii_uppercase())
            .collect::<Vec<_>>()
    });

    let mut result = nodes
        .iter()
        .filter(|node| {
            if let Some(regex) = &name_regex {
                regex.is_match(&node.name)
            } else {
                true
            }
        })
        .filter(|node| {
            if let Some(countries) = &country_set {
                countries.is_empty() || countries.contains(&node.country.to_ascii_uppercase())
            } else {
                true
            }
        })
        .cloned()
        .collect::<Vec<_>>();

    if let (Some(countries), Some(limit_per_country)) =
        (&filter.countries, filter.limit_per_country)
    {
        let country_set: HashSet<String> =
            countries.iter().map(|c| c.to_ascii_uppercase()).collect();
        let mut per_country_count: HashMap<String, usize> = HashMap::new();
        result.retain(|node| {
            let upper_country = node.country.to_ascii_uppercase();
            if !country_set.contains(&upper_country) {
                return true;
            }
            let count = per_country_count.entry(upper_country).or_insert(0);
            if *count < limit_per_country {
                *count += 1;
                return true;
            }
            false
        });
    } else if let Some(limit) = filter.limit {
        result.truncate(limit);
    }

    Ok(result)
}

fn parse_yaml_subscription_nodes(raw_input: &str) -> Option<Vec<NodeInfo>> {
    if !raw_input.contains("proxies:") {
        return None;
    }
    let parsed: ProxySubscriptionYaml = serde_yaml::from_str(raw_input).ok()?;
    if parsed.proxies.is_empty() {
        return None;
    }

    let mut names = HashMap::new();
    let mut nodes = Vec::with_capacity(parsed.proxies.len());
    for (index, value) in parsed.proxies.into_iter().enumerate() {
        let mut payload = serde_json::to_value(value).ok()?.as_object()?.clone();
        let protocol = payload
            .get("type")
            .and_then(|v| v.as_str())
            .map(normalize_protocol)
            .unwrap_or_default();
        if protocol.is_empty() {
            continue;
        }
        payload.insert("type".to_string(), JsonValue::String(protocol.clone()));
        let fallback_name = format!("{protocol}-{}", index + 1);
        let node =
            build_node_from_payload(&mut payload, None, &fallback_name, &mut names, index + 1)?;
        nodes.push(node);
    }
    Some(nodes)
}

fn parse_uri_subscription_nodes(raw_input: &str) -> Vec<NodeInfo> {
    let mut nodes = Vec::new();
    let mut names = HashMap::new();
    let mut logical_index = 0usize;
    for line in raw_input.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        logical_index += 1;
        nodes.extend(parse_subscription_line(line, logical_index, &mut names));
    }
    nodes
}

fn parse_subscription_line(
    raw: &str,
    index: usize,
    names: &mut HashMap<String, usize>,
) -> Vec<NodeInfo> {
    if let Some(mut payload) = parse_internal_proxy_line(raw) {
        let protocol = payload
            .get("type")
            .and_then(|v| v.as_str())
            .map(normalize_protocol)
            .unwrap_or_default();
        if !protocol.is_empty() {
            payload.insert("type".to_string(), JsonValue::String(protocol.clone()));
            let fallback_name = format!("{protocol}-{index}");
            if let Some(node) = build_node_from_payload(
                &mut payload,
                Some(raw.to_string()),
                &fallback_name,
                names,
                index,
            ) {
                return vec![node];
            }
        }
        return Vec::new();
    }

    let scheme = raw
        .split_once("://")
        .map(|(s, _)| s.to_ascii_lowercase())
        .unwrap_or_default();
    let payloads = match scheme.as_str() {
        "hysteria" => parse_hysteria_line(raw).into_iter().collect::<Vec<_>>(),
        "hysteria2" | "hy2" => parse_hysteria2_line(raw).into_iter().collect::<Vec<_>>(),
        "tuic" => parse_tuic_line(raw).into_iter().collect::<Vec<_>>(),
        "trojan" => parse_trojan_line(raw).into_iter().collect::<Vec<_>>(),
        "vless" => parse_vless_line(raw).into_iter().collect::<Vec<_>>(),
        "vmess" => parse_vmess_line(raw).into_iter().collect::<Vec<_>>(),
        "ss" => parse_ss_line(raw).into_iter().collect::<Vec<_>>(),
        "ssr" => parse_ssr_line(raw).into_iter().collect::<Vec<_>>(),
        "socks" | "socks5" | "socks5h" | "http" | "https" => {
            parse_socks_like_line(raw).into_iter().collect::<Vec<_>>()
        }
        "anytls" => parse_anytls_line(raw).into_iter().collect::<Vec<_>>(),
        "mierus" => parse_mierus_line(raw),
        _ => Vec::new(),
    };

    let mut nodes = Vec::new();
    for (offset, mut payload) in payloads.into_iter().enumerate() {
        let protocol = payload
            .get("type")
            .and_then(|v| v.as_str())
            .map(normalize_protocol)
            .unwrap_or_else(|| scheme.clone());
        if protocol.is_empty() {
            continue;
        }
        payload.insert("type".to_string(), JsonValue::String(protocol.clone()));
        let fallback_name = format!("{protocol}-{}", index + offset);
        if let Some(node) = build_node_from_payload(
            &mut payload,
            Some(raw.to_string()),
            &fallback_name,
            names,
            index + offset,
        ) {
            nodes.push(node);
        }
    }
    nodes
}

fn build_node_from_payload(
    payload: &mut ProxyPayload,
    raw_override: Option<String>,
    fallback_name: &str,
    names: &mut HashMap<String, usize>,
    index: usize,
) -> Option<NodeInfo> {
    let protocol = payload
        .get("type")
        .and_then(|v| v.as_str())
        .map(normalize_protocol)
        .unwrap_or_default();
    if protocol.is_empty() {
        return None;
    }
    payload.insert("type".to_string(), JsonValue::String(protocol.clone()));

    let base_name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("{fallback_name}-{index}"));
    let unique = unique_name(names, &base_name);
    payload.insert("name".to_string(), JsonValue::String(unique.clone()));

    let country = crate::services::geoip::infer_country_from_name(&unique);
    info!("infer_country_from_name {} -> {}", unique, country);
    let connect_info = payload_to_connect_info(&protocol, payload);
    let payload_text = serde_json::to_string(payload).ok();
    let raw = raw_override.unwrap_or_else(|| encode_internal_proxy_line(payload));

    Some(NodeInfo {
        name: unique,
        protocol,
        country,
        raw,
        parsed_proxy_payload: payload_text,
        connect_info,
        test_file: Some(DEFAULT_TEST_FILE.to_string()),
        upload_target: Some(DEFAULT_UPLOAD_TARGET.to_string()),
    })
}

fn parse_internal_proxy_line(raw: &str) -> Option<ProxyPayload> {
    let body = raw.strip_prefix(INTERNAL_PROXY_PREFIX)?;
    let bytes = decode_base64_flexible(body)?;
    let value: JsonValue = serde_json::from_slice(&bytes).ok()?;
    value.as_object().cloned()
}

fn encode_internal_proxy_line(payload: &ProxyPayload) -> String {
    let json = serde_json::to_vec(payload).unwrap_or_default();
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json);
    format!("{INTERNAL_PROXY_PREFIX}{encoded}")
}

fn parse_hysteria_line(raw: &str) -> Option<ProxyPayload> {
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
    payload.insert("type".into(), JsonValue::String("hysteria".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));
    put_non_empty_string(&mut payload, "sni", query.get("peer"));
    put_non_empty_string(&mut payload, "obfs", query.get("obfs"));
    put_non_empty_string(&mut payload, "auth_str", query.get("auth"));
    put_non_empty_string(&mut payload, "protocol", query.get("protocol"));
    if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
        payload.insert(
            "alpn".into(),
            JsonValue::Array(split_csv(alpn).into_iter().map(JsonValue::String).collect()),
        );
    }
    let up = query.get("up").or_else(|| query.get("upmbps"));
    let down = query.get("down").or_else(|| query.get("downmbps"));
    put_non_empty_string(&mut payload, "up", up);
    put_non_empty_string(&mut payload, "down", down);
    if parse_bool_like(query.get("insecure")) {
        payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    }
    Some(payload)
}

fn parse_hysteria2_line(raw: &str) -> Option<ProxyPayload> {
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

fn parse_tuic_line(raw: &str) -> Option<ProxyPayload> {
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
    payload.insert("type".into(), JsonValue::String("tuic".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));
    payload.insert("udp".into(), JsonValue::Bool(true));
    if let Some(password) = url.password() {
        payload.insert("uuid".into(), JsonValue::String(url.username().to_string()));
        payload.insert("password".into(), JsonValue::String(password.to_string()));
    } else if !url.username().is_empty() {
        payload.insert(
            "token".into(),
            JsonValue::String(url.username().to_string()),
        );
    }
    put_non_empty_string(
        &mut payload,
        "congestion-controller",
        query.get("congestion_control"),
    );
    put_non_empty_string(&mut payload, "sni", query.get("sni"));
    if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
        payload.insert(
            "alpn".into(),
            JsonValue::Array(split_csv(alpn).into_iter().map(JsonValue::String).collect()),
        );
    }
    if query.get("disable_sni").map(|v| v == "1").unwrap_or(false) {
        payload.insert("disable-sni".into(), JsonValue::Bool(true));
    }
    put_non_empty_string(&mut payload, "udp-relay-mode", query.get("udp_relay_mode"));
    Some(payload)
}

fn parse_trojan_line(raw: &str) -> Option<ProxyPayload> {
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
    payload.insert("type".into(), JsonValue::String("trojan".to_string()));
    payload.insert(
        "server".into(),
        JsonValue::String(url.host_str()?.to_string()),
    );
    payload.insert("port".into(), JsonValue::from(url.port().unwrap_or(443)));
    if url.username().is_empty() {
        return None;
    }
    payload.insert(
        "password".into(),
        JsonValue::String(url.username().to_string()),
    );
    payload.insert("udp".into(), JsonValue::Bool(true));
    if parse_bool_like(query.get("allowInsecure")) || parse_bool_like(query.get("insecure")) {
        payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    }
    put_non_empty_string(&mut payload, "sni", query.get("sni"));
    if let Some(alpn) = query.get("alpn").filter(|v| !v.is_empty()) {
        payload.insert(
            "alpn".into(),
            JsonValue::Array(split_csv(alpn).into_iter().map(JsonValue::String).collect()),
        );
    }
    let network = query
        .get("type")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if !network.is_empty() {
        payload.insert("network".into(), JsonValue::String(network.clone()));
        match network.as_str() {
            "ws" => {
                let ws_opts = json!({
                    "path": query.get("path").cloned().unwrap_or_default(),
                    "headers": {"User-Agent": "Mozilla/5.0"}
                });
                payload.insert("ws-opts".into(), ws_opts);
            }
            "grpc" => {
                let grpc_opts = json!({
                    "grpc-service-name": query.get("serviceName").cloned().unwrap_or_default()
                });
                payload.insert("grpc-opts".into(), grpc_opts);
            }
            _ => {}
        }
    }
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
    put_non_empty_string(&mut payload, "fingerprint", query.get("pcs"));
    Some(payload)
}

fn parse_vless_line(raw: &str) -> Option<ProxyPayload> {
    let url = Url::parse(raw).ok()?;
    let mut payload = handle_v_share_link(&url, "vless")?;
    let query = query_map(&url);
    if let Some(flow) = query.get("flow").filter(|v| !v.is_empty()) {
        payload.insert(
            "flow".to_string(),
            JsonValue::String(flow.to_ascii_lowercase()),
        );
    }
    put_non_empty_string(&mut payload, "encryption", query.get("encryption"));
    Some(payload)
}

fn parse_vmess_line(raw: &str) -> Option<ProxyPayload> {
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

fn parse_ss_line(raw: &str) -> Option<ProxyPayload> {
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

fn parse_ssr_line(raw: &str) -> Option<ProxyPayload> {
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

fn parse_socks_like_line(raw: &str) -> Option<ProxyPayload> {
    let url = Url::parse(raw).ok()?;
    let scheme = url.scheme().to_ascii_lowercase();
    let server = url.host_str()?.to_string();
    let port = url.port()?;
    let mut username = String::new();
    let mut password = String::new();
    if let Some(encoded_user) = extract_userinfo_from_raw(raw).filter(|v| !v.is_empty()) {
        if let Some(decoded) =
            decode_base64_flexible(&encoded_user).and_then(|bytes| String::from_utf8(bytes).ok())
        {
            if let Some((u, p)) = decoded.split_once(':') {
                username = u.to_string();
                password = p.to_string();
            } else {
                username = decoded;
            }
        } else {
            username = url.username().to_string();
            password = url.password().unwrap_or_default().to_string();
        }
    }
    let mut payload = ProxyPayload::new();
    payload.insert(
        "name".into(),
        JsonValue::String(parse_share_name(&url, &format!("{server}:{port}"))),
    );
    let mapped_type = match scheme.as_str() {
        "socks" | "socks5" | "socks5h" => "socks5",
        "http" | "https" => "http",
        _ => scheme.as_str(),
    };
    payload.insert("type".into(), JsonValue::String(mapped_type.to_string()));
    payload.insert("server".into(), JsonValue::String(server));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("username".into(), JsonValue::String(username));
    payload.insert("password".into(), JsonValue::String(password));
    payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    if scheme == "https" {
        payload.insert("tls".into(), JsonValue::Bool(true));
    }
    Some(payload)
}

fn parse_anytls_line(raw: &str) -> Option<ProxyPayload> {
    let url = Url::parse(raw).ok()?;
    let query = query_map(&url);
    let server = url.host_str()?.to_string();
    let port = url.port()?;
    let username = url.username().to_string();
    let password = url.password().unwrap_or(&username).to_string();
    let mut payload = ProxyPayload::new();
    payload.insert(
        "name".into(),
        JsonValue::String(parse_share_name(&url, &format!("{server}:{port}"))),
    );
    payload.insert("type".into(), JsonValue::String("anytls".to_string()));
    payload.insert("server".into(), JsonValue::String(server));
    payload.insert("port".into(), JsonValue::from(port));
    payload.insert("username".into(), JsonValue::String(username));
    payload.insert("password".into(), JsonValue::String(password));
    put_non_empty_string(&mut payload, "sni", query.get("sni"));
    put_non_empty_string(&mut payload, "fingerprint", query.get("hpkp"));
    if query.get("insecure").map(|v| v == "1").unwrap_or(false) {
        payload.insert("skip-cert-verify".into(), JsonValue::Bool(true));
    }
    payload.insert("udp".into(), JsonValue::Bool(true));
    Some(payload)
}

fn parse_mierus_line(raw: &str) -> Vec<ProxyPayload> {
    let Some(url) = Url::parse(raw).ok() else {
        return Vec::new();
    };
    let Some(server) = url.host_str() else {
        return Vec::new();
    };
    let username = url.username().to_string();
    let password = url.password().unwrap_or_default().to_string();
    let query_map_vec = query_vec_map(&url);
    let port_list = query_map_vec.get("port").cloned().unwrap_or_default();
    let protocol_list = query_map_vec.get("protocol").cloned().unwrap_or_default();
    if port_list.is_empty() || port_list.len() != protocol_list.len() {
        return Vec::new();
    }

    let base_name = parse_share_name(
        &url,
        query_map_vec
            .get("profile")
            .and_then(|v| v.first())
            .map(String::as_str)
            .unwrap_or(server),
    );
    let multiplexing = query_map_vec
        .get("multiplexing")
        .and_then(|v| v.first())
        .cloned();
    let handshake_mode = query_map_vec
        .get("handshake-mode")
        .and_then(|v| v.first())
        .cloned();
    let traffic_pattern = query_map_vec
        .get("traffic-pattern")
        .and_then(|v| v.first())
        .cloned();

    let mut result = Vec::new();
    for (idx, port) in port_list.iter().enumerate() {
        let protocol = protocol_list[idx].clone();
        let mut payload = ProxyPayload::new();
        payload.insert(
            "name".into(),
            JsonValue::String(format!("{base_name}:{port}/{protocol}")),
        );
        payload.insert("type".into(), JsonValue::String("mieru".to_string()));
        payload.insert("server".into(), JsonValue::String(server.to_string()));
        payload.insert("transport".into(), JsonValue::String(protocol));
        payload.insert("udp".into(), JsonValue::Bool(true));
        payload.insert("username".into(), JsonValue::String(username.clone()));
        payload.insert("password".into(), JsonValue::String(password.clone()));
        if port.contains('-') {
            payload.insert("port-range".into(), JsonValue::String(port.clone()));
        } else if let Ok(p) = port.parse::<u16>() {
            payload.insert("port".into(), JsonValue::from(p));
        } else {
            continue;
        }
        if let Some(v) = multiplexing.clone().filter(|v| !v.is_empty()) {
            payload.insert("multiplexing".into(), JsonValue::String(v));
        }
        if let Some(v) = handshake_mode.clone().filter(|v| !v.is_empty()) {
            payload.insert("handshake-mode".into(), JsonValue::String(v));
        }
        if let Some(v) = traffic_pattern.clone().filter(|v| !v.is_empty()) {
            payload.insert("traffic-pattern".into(), JsonValue::String(v));
        }
        result.push(payload);
    }
    result
}

fn handle_v_share_link(url: &Url, scheme: &str) -> Option<ProxyPayload> {
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

fn append_v_share_transport_fields(
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

fn build_vmess_payload_from_json(values: &JsonValue) -> Option<ProxyPayload> {
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

fn payload_to_connect_info(protocol: &str, payload: &ProxyPayload) -> Option<NodeConnectInfo> {
    let server = payload.get("server")?.as_str()?.to_string();
    let port = extract_u16(payload.get("port"))?;
    let mut username = None;
    let mut password = None;

    match protocol {
        "vless" | "vmess" => {
            username = payload
                .get("uuid")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            if protocol == "vmess" {
                password = payload.get("alterId").map(|v| match v {
                    JsonValue::Number(n) => n.to_string(),
                    JsonValue::String(s) => s.clone(),
                    _ => "0".to_string(),
                });
            }
        }
        "trojan" => {
            password = payload
                .get("password")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
        }
        "ss" | "ssr" => {
            username = payload
                .get("cipher")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            password = payload
                .get("password")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
        }
        "hysteria" | "hysteria2" | "tuic" | "socks5" | "http" | "anytls" | "mieru" => {
            username = payload
                .get("username")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
            password = payload
                .get("password")
                .and_then(|v| v.as_str())
                .map(ToString::to_string);
        }
        _ => {}
    }

    Some(NodeConnectInfo {
        server,
        port,
        username,
        password,
    })
}

fn normalize_input(raw_input: &str) -> String {
    let trimmed = raw_input.trim();
    trimmed
        .strip_prefix('\u{feff}')
        .unwrap_or(trimmed)
        .to_string()
}

fn decode_base64_subscription_if_needed(raw_input: &str) -> Option<String> {
    let compact = raw_input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<String>();
    if compact.len() < 16 {
        return None;
    }
    if raw_input.contains("://") || raw_input.contains("proxies:") {
        return None;
    }
    decode_base64_to_utf8(&compact)
        .filter(|decoded| decoded.contains("://") || decoded.contains("proxies:"))
}

fn decode_base64_to_utf8(content: &str) -> Option<String> {
    let bytes = decode_base64_flexible(content)?;
    String::from_utf8(bytes).ok()
}

fn decode_base64_flexible(content: &str) -> Option<Vec<u8>> {
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

fn unique_name(names: &mut HashMap<String, usize>, name: &str) -> String {
    if let Some(index) = names.get_mut(name) {
        *index += 1;
        format!("{name}-{:02}", *index)
    } else {
        names.insert(name.to_string(), 0);
        name.to_string()
    }
}

fn parse_share_name(url: &Url, fallback: &str) -> String {
    url.fragment()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|encoded| url_decode(encoded).ok().map(|s| s.into_owned()))
        .unwrap_or_else(|| fallback.to_string())
}

fn normalize_protocol(protocol: &str) -> String {
    match protocol.to_ascii_lowercase().as_str() {
        "hy2" => "hysteria2".to_string(),
        "socks" | "socks5" | "socks5h" => "socks5".to_string(),
        other => other.to_string(),
    }
}

fn query_map(url: &Url) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in url.query_pairs() {
        map.insert(k.to_string(), v.to_string());
    }
    map
}

fn query_vec_map(url: &Url) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::<String, Vec<String>>::new();
    for (k, v) in url.query_pairs() {
        map.entry(k.to_string()).or_default().push(v.to_string());
    }
    map
}

fn split_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn put_non_empty_string(payload: &mut ProxyPayload, key: &str, value: Option<&String>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        payload.insert(key.to_string(), JsonValue::String(v.to_string()));
    }
}

fn put_non_empty_string_map(payload: &mut ProxyPayload, key: &str, value: Option<&String>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        payload.insert(key.to_string(), JsonValue::String(v.to_string()));
    }
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

fn extract_u16(value: Option<&JsonValue>) -> Option<u16> {
    extract_json_u16(value)
}

fn parse_bool_like(value: Option<&String>) -> bool {
    value
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn extract_userinfo_from_raw(raw: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn find_protocol<'a>(nodes: &'a [NodeInfo], protocol: &str) -> &'a NodeInfo {
        nodes
            .iter()
            .find(|n| n.protocol == protocol)
            .expect("protocol not found")
    }

    #[test]
    fn 解析_hysteria() {
        let raw = "hysteria://example.com:443?peer=cdn.example.com&obfs=foo&auth=bar&up=10&down=20&insecure=1#hy";
        let nodes = parse_subscription_nodes(&raw);
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert_eq!(node.protocol, "hysteria");
        assert_eq!(node.name, "hy");
        assert!(node
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("\"type\":\"hysteria\""));
    }

    #[test]
    fn 解析_hysteria2_hy2() {
        let raw = "hy2://letmein@example.com:8443/?insecure=1&obfs=salamander&obfs-password=gawrgura&pinSHA256=deadbeef&sni=real.example.com&up=114&down=514&alpn=h3,h4#hy2test";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert_eq!(node.protocol, "hysteria2");
        assert!(node
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("\"fingerprint\":\"deadbeef\""));
    }

    #[test]
    fn 解析_tuic_v4_v5() {
        let raw = "tuic://token@example.com:443?udp_relay_mode=native#tuic-v4\n\
                   tuic://uuid:pwd@example.com:443?congestion_control=bbr#tuic-v5";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].protocol, "tuic");
        assert!(nodes[0]
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("\"token\""));
        assert!(nodes[1]
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("\"uuid\""));
    }

    #[test]
    fn 解析_trojan_ws_grpc() {
        let raw = "trojan://pass@example.com:443?type=ws&path=%2Fws#t1\n\
                   trojan://pass2@example.com:443?type=grpc&serviceName=svc#t2";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 2);
        assert!(nodes[0]
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("ws-opts"));
        assert!(nodes[1]
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("grpc-opts"));
    }

    #[test]
    fn 解析_vless_reality_packet_xhttp() {
        let raw = "vless://uuid@example.com:443?type=xhttp&path=%2Fv&mode=auto&security=reality&sni=www.microsoft.com&fp=chrome&pbk=pubkey&sid=abcd&packetEncoding=packet#vless";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert_eq!(node.protocol, "vless");
        let payload = node.parsed_proxy_payload.as_ref().unwrap();
        assert!(payload.contains("reality-opts"));
        assert!(payload.contains("\"packet-addr\":true"));
        assert!(payload.contains("xhttp-opts"));
    }

    #[test]
    fn 解析_vmess_base64_json() {
        let json = r#"{"ps":"vmess-json","add":"example.com","port":"443","id":"uuid-1","aid":"0","net":"ws","path":"/ws","host":"h.example.com","tls":"tls"}"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(json);
        let raw = format!("vmess://{encoded}");
        let nodes = parse_subscription_nodes(&raw);
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert_eq!(node.protocol, "vmess");
        assert_eq!(node.name, "vmess-json");
        assert!(node
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("ws-opts"));
    }

    #[test]
    fn 解析_vmess_aead_url() {
        let raw = "vmess://uuid@example.com:443?type=grpc&serviceName=svc&security=tls&sni=example.com#vmess-aead";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0]
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("grpc-opts"));
    }

    #[test]
    fn 解析_ss_插件_uot() {
        let userinfo = base64::engine::general_purpose::STANDARD.encode("aes-256-gcm:pass");
        let plugin =
            urlencoding::encode("v2ray-plugin;mode=websocket;host=www.example.com;path=/ws;tls=1");
        let raw = format!("ss://{userinfo}@example.com:8388?plugin={plugin}&uot=1#ss");
        let nodes = parse_subscription_nodes(&raw);
        assert_eq!(nodes.len(), 1);
        let payload = nodes[0].parsed_proxy_payload.as_ref().unwrap();
        assert!(payload.contains("\"plugin\":\"v2ray-plugin\""));
        assert!(payload.contains("\"udp-over-tcp\":true"));
    }

    #[test]
    fn 解析_ssr_remarks_obfsparam_protoparam() {
        let password = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("pass");
        let obfsparam = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("host.example.com");
        let protoparam = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("proto-param");
        let remarks = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("SSR-NODE");
        let content = format!(
            "tw.example.com:443:origin:aes-256-cfb:plain:{password}/?obfsparam={obfsparam}&protoparam={protoparam}&remarks={remarks}"
        );
        let raw = format!(
            "ssr://{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(content)
        );
        let nodes = parse_subscription_nodes(&raw);
        assert_eq!(nodes.len(), 1);
        let payload = nodes[0].parsed_proxy_payload.as_ref().unwrap();
        assert!(payload.contains("\"type\":\"ssr\""));
        assert!(payload.contains("obfs-param"));
        assert!(payload.contains("protocol-param"));
    }

    #[test]
    fn 解析_socks_http_https() {
        let raw = "socks5://dXNlcjpwYXNz@example.com:1080#s1\n\
                   http://dXNlcjpwYXNz@example.com:8080#h1\n\
                   https://dXNlcjpwYXNz@example.com:8443#h2";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].protocol, "socks5");
        assert_eq!(nodes[1].protocol, "http");
        assert_eq!(nodes[2].protocol, "http");
        assert!(nodes[2]
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("\"tls\":true"));
    }

    #[test]
    fn 解析_anytls() {
        let raw =
            "anytls://user:pass@example.com:443?sni=example.com&hpkp=fingerprint&insecure=1#at";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 1);
        let payload = nodes[0].parsed_proxy_payload.as_ref().unwrap();
        assert!(payload.contains("\"type\":\"anytls\""));
        assert!(payload.contains("\"skip-cert-verify\":true"));
    }

    #[test]
    fn 解析_mierus_多端口展开() {
        let raw = "mierus://user:pass@1.2.3.4?profile=default&port=6666&port=9998-9999&protocol=TCP&protocol=UDP";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].protocol, "mieru");
        assert!(nodes[1]
            .parsed_proxy_payload
            .as_ref()
            .unwrap()
            .contains("port-range"));
    }

    #[test]
    fn 解析_yaml_proxies() {
        let raw = r#"
proxies:
  - name: yaml-vless
    type: vless
    server: example.com
    port: 443
    uuid: abc
"#;
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "yaml-vless");
        assert!(nodes[0].raw.starts_with(INTERNAL_PROXY_PREFIX));
    }

    #[test]
    fn 解析_base64_uri订阅() {
        let content = "vless://uuid@example.com:443#节点1\nvless://uuid2@example.com:443#节点2";
        let encoded = base64::engine::general_purpose::STANDARD.encode(content);
        let nodes = parse_subscription_nodes(&encoded);
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn 解析_urlsafe_base64_uri订阅() {
        let content = "vless://uuid@example.com:443#节点1";
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(content);
        let nodes = parse_subscription_nodes(&encoded);
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn 混合坏行会跳过() {
        let raw = "not-a-link\nvless://uuid@example.com:443#ok\ninvalid://x";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "ok");
    }

    #[test]
    fn 名称去重规则() {
        let raw = "vless://uuid@example.com:443#dup\nvless://uuid2@example.com:443#dup";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "dup");
        assert_eq!(nodes[1].name, "dup-01");
    }

    #[test]
    fn 国家推断回归() {
        let nodes = parse_subscription_nodes("vless://uuid@example.com:443#香港-HK-01");
        assert_eq!(nodes[0].country, "HK");
        let nodes = parse_subscription_nodes("vless://uuid@example.com:443#未知节点");
        assert_eq!(nodes[0].country, "UNKNOWN");
    }

    #[test]
    fn 过滤节点_正则与限制() {
        let nodes = vec![
            NodeInfo {
                name: "香港-HK-01".to_string(),
                protocol: "vless".to_string(),
                country: "HK".to_string(),
                raw: "".to_string(),
                parsed_proxy_payload: None,
                connect_info: None,
                test_file: None,
                upload_target: None,
            },
            NodeInfo {
                name: "香港-HK-02".to_string(),
                protocol: "vless".to_string(),
                country: "HK".to_string(),
                raw: "".to_string(),
                parsed_proxy_payload: None,
                connect_info: None,
                test_file: None,
                upload_target: None,
            },
            NodeInfo {
                name: "日本-JP-01".to_string(),
                protocol: "vless".to_string(),
                country: "JP".to_string(),
                raw: "".to_string(),
                parsed_proxy_payload: None,
                connect_info: None,
                test_file: None,
                upload_target: None,
            },
        ];
        let filter = NodeFilter {
            name_regex: Some("HK.*".to_string()),
            countries: None,
            limit: Some(1),
            limit_per_country: None,
        };
        let filtered = filter_nodes(&nodes, &filter).unwrap();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn 过滤节点_按国家每地区限制() {
        let nodes = vec![
            NodeInfo {
                name: "香港-HK-01".to_string(),
                protocol: "vless".to_string(),
                country: "HK".to_string(),
                raw: "".to_string(),
                parsed_proxy_payload: None,
                connect_info: None,
                test_file: None,
                upload_target: None,
            },
            NodeInfo {
                name: "香港-HK-02".to_string(),
                protocol: "vless".to_string(),
                country: "HK".to_string(),
                raw: "".to_string(),
                parsed_proxy_payload: None,
                connect_info: None,
                test_file: None,
                upload_target: None,
            },
            NodeInfo {
                name: "日本-JP-01".to_string(),
                protocol: "vless".to_string(),
                country: "JP".to_string(),
                raw: "".to_string(),
                parsed_proxy_payload: None,
                connect_info: None,
                test_file: None,
                upload_target: None,
            },
        ];
        let filter = NodeFilter {
            name_regex: None,
            countries: Some(vec!["HK".to_string(), "JP".to_string()]),
            limit: None,
            limit_per_country: Some(1),
        };
        let filtered = filter_nodes(&nodes, &filter).unwrap();
        assert_eq!(filtered.iter().filter(|n| n.country == "HK").count(), 1);
        assert_eq!(filtered.iter().filter(|n| n.country == "JP").count(), 1);
    }

    #[test]
    fn 内部raw可回解析() {
        let yaml = r#"
proxies:
  - name: x1
    type: socks5
    server: 1.1.1.1
    port: 1080
"#;
        let nodes = parse_subscription_nodes(yaml);
        assert_eq!(nodes.len(), 1);
        let roundtrip = parse_subscription_nodes(&nodes[0].raw);
        assert_eq!(roundtrip.len(), 1);
        assert_eq!(roundtrip[0].protocol, "socks5");
    }

    #[test]
    fn 默认测速字段存在() {
        let nodes = parse_subscription_nodes("vless://uuid@example.com:443#测试节点");
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].test_file.as_ref().unwrap().contains("speedtest"));
        assert!(nodes[0].upload_target.as_ref().unwrap().contains("httpbin"));
    }

    #[test]
    fn 关键协议可被检索() {
        let raw = "hysteria://a.com:443#h\nhy2://a.com:443#hy2\ntuic://t@a.com:443#t\nanytls://u:p@a.com:443#at";
        let nodes = parse_subscription_nodes(raw);
        assert_eq!(find_protocol(&nodes, "hysteria").protocol, "hysteria");
        assert_eq!(find_protocol(&nodes, "hysteria2").protocol, "hysteria2");
        assert_eq!(find_protocol(&nodes, "tuic").protocol, "tuic");
        assert_eq!(find_protocol(&nodes, "anytls").protocol, "anytls");
    }
}
