//! VLESS 协议解析器

use serde_json::Value as JsonValue;
use url::Url;

use super::super::types::ProxyPayload;
use super::super::utils::query_map;
use super::v2::handle_v_share_link;

/// 解析 VLESS URL
/// 格式: vless://uuid@example.com:443?type=ws&path=%2Fws&security=tls&sni=example.com#vless
pub fn parse_vless_line(raw: &str) -> Option<ProxyPayload> {
    let url = Url::parse(raw).ok()?;
    let mut payload = handle_v_share_link(&url, "vless")?;
    let query = query_map(&url);
    if let Some(flow) = query.get("flow").filter(|v| !v.is_empty()) {
        payload.insert(
            "flow".to_string(),
            JsonValue::String(flow.to_ascii_lowercase()),
        );
    }
    super::super::utils::put_non_empty_string(&mut payload, "encryption", query.get("encryption"));
    Some(payload)
}
