//! 全局共享 HTTP 客户端（非测速场景）。
//!
//! 目标：
//! 1. 使用单例，复用连接池，避免重复构建客户端；
//! 2. 自动接入系统代理（若已启用）。

use once_cell::sync::OnceCell;
use tracing::info;

use super::system_proxy::get_system_proxy;

const DEFAULT_HTTP_USER_AGENT: &str = "capyspeedtest/0.1";

static SHARED_HTTP_CLIENT: OnceCell<reqwest::Client> = OnceCell::new();

/// 获取全局共享的 HTTP 客户端（单例）。
pub fn shared_http_client() -> Result<&'static reqwest::Client, String> {
    SHARED_HTTP_CLIENT.get_or_try_init(|| {
        let proxy_config = get_system_proxy();
        let mut builder = reqwest::Client::builder().user_agent(DEFAULT_HTTP_USER_AGENT);

        if proxy_config.enabled {
            if let Some(proxy_url) = proxy_config.proxy_url.as_deref() {
                let proxy =
                    reqwest::Proxy::all(proxy_url).map_err(|e| format!("构建系统代理失败: {e}"))?;
                builder = builder.proxy(proxy);
                info!("[HTTP] 使用系统代理: {}", proxy_url);
            }
        }

        builder
            .build()
            .map_err(|e| format!("创建全局 HTTP 客户端失败: {e}"))
    })
}
