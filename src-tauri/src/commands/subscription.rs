//! 订阅相关命令：解析节点、远程获取订阅。

use crate::models::{NodeFilter, NodeInfo, SubscriptionFetchProgressEvent};
use crate::services;
use tauri::Emitter;
use tracing::{error, info};

/// 解析订阅节点并按过滤器筛选（纯 CPU 操作，同步执行即可）。
#[tauri::command]
pub fn parse_subscription_nodes(
    raw_input: String,
    filter: Option<NodeFilter>,
) -> Result<Vec<NodeInfo>, String> {
    info!(
        "[命令] parse_subscription_nodes 输入长度={} chars, filter={:?}",
        raw_input.len(),
        filter.is_some()
    );
    let nodes = services::parse_subscription_nodes(&raw_input);
    info!(
        "[命令] parse_subscription_nodes 解析出 {} 个节点",
        nodes.len()
    );
    if let Some(current_filter) = filter {
        let filtered = services::filter_nodes(&nodes, &current_filter)?;
        info!(
            "[命令] parse_subscription_nodes 过滤后剩 {} 个节点",
            filtered.len()
        );
        Ok(filtered)
    } else {
        Ok(nodes)
    }
}

/// 从远程订阅 URL 获取并解析节点（异步）。
#[tauri::command]
pub async fn fetch_subscription_from_url(
    window: tauri::Window,
    url: String,
) -> Result<Vec<NodeInfo>, String> {
    info!("[命令] fetch_subscription_from_url url={}", url);

    let _ = window.emit(
        "subscription://fetch/progress",
        &SubscriptionFetchProgressEvent {
            stage: "fetching".to_string(),
            progress: 20.0,
            message: "正在获取订阅内容...".to_string(),
            nodes_count: None,
        },
    );

    let result = services::fetch_subscription_from_url(&url).await;

    match &result {
        Ok(nodes) => {
            let _ = window.emit(
                "subscription://fetch/progress",
                &SubscriptionFetchProgressEvent {
                    stage: "parsing".to_string(),
                    progress: 80.0,
                    message: "正在解析节点...".to_string(),
                    nodes_count: None,
                },
            );
            info!(
                "[命令] fetch_subscription_from_url 成功获取 {} 个节点",
                nodes.len()
            );
        }
        Err(e) => {
            let _ = window.emit(
                "subscription://fetch/progress",
                &SubscriptionFetchProgressEvent {
                    stage: "error".to_string(),
                    progress: 0.0,
                    message: format!("获取失败: {}", e),
                    nodes_count: None,
                },
            );
            error!("[命令] fetch_subscription_from_url 失败: {}", e);
        }
    }
    result
}
