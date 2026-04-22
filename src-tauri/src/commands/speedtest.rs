//! 测速命令：批量执行测速。

use crate::commands::AppState;
use crate::models::{
    GeoIpInfo, KernelDownloadProgressEvent, NodeFilter, NodeInfo, SpeedTestProgressEvent,
    SpeedTestResult, SpeedTestTaskConfig,
};
use crate::services;
use std::path::PathBuf;
use tauri::Emitter;
use tracing::{info, warn};

async fn resolve_kernel_path(
    window: &tauri::Window,
    state: &tauri::State<'_, AppState>,
) -> Result<PathBuf, String> {
    let kernel_version = state.kernel_version.lock().unwrap().clone();
    let platform = services::detect_platform();

    let kernel_exists = services::kernel::kernel_binary_exists(&platform, &kernel_version)
        .map_err(|e| format!("检查内核存在性失败: {}", e))?;

    if kernel_exists {
        return services::kernel::kernel_binary_path(&platform, &kernel_version)
            .map_err(|e| format!("获取内核路径失败: {}", e));
    }

    info!("[命令] 内核 {} 不存在，开始下载...", kernel_version);

    let _ = window.emit(
        "kernel://download/progress",
        &KernelDownloadProgressEvent {
            version: kernel_version.clone(),
            stage: "downloading".to_string(),
            progress: 0.0,
            message: "开始下载内核...".to_string(),
        },
    );

    let downloaded_path = services::kernel::download_kernel_version_with_progress(
        &platform,
        &kernel_version,
        |event| {
            let _ = window.emit("kernel://download/progress", &event);
        },
    )
    .await
    .map_err(|e| format!("下载内核失败: {}", e))?;

    Ok(PathBuf::from(downloaded_path))
}

fn unknown_geoip() -> GeoIpInfo {
    GeoIpInfo {
        ip: "0.0.0.0".to_string(),
        country_code: "UN".to_string(),
        country_name: "Unknown".to_string(),
        isp: "Unknown".to_string(),
    }
}

fn rebuild_result_from_snapshot(
    node: NodeInfo,
    snapshot: Option<&services::checkpoint::NodeResultSnapshot>,
) -> SpeedTestResult {
    let is_error = snapshot
        .map(|s| s.status.eq_ignore_ascii_case("error"))
        .unwrap_or(true);

    SpeedTestResult {
        node,
        tcp_ping_ms: snapshot.and_then(|s| s.tcp_ping_ms).unwrap_or(9999),
        site_ping_ms: snapshot.and_then(|s| s.site_ping_ms).unwrap_or(9999),
        packet_loss_rate: if is_error { 1.0 } else { 0.0 },
        avg_download_mbps: snapshot.and_then(|s| s.avg_download_mbps).unwrap_or(0.0),
        max_download_mbps: snapshot.and_then(|s| s.max_download_mbps).unwrap_or(0.0),
        avg_upload_mbps: snapshot.and_then(|s| s.avg_upload_mbps),
        max_upload_mbps: snapshot.and_then(|s| s.max_upload_mbps),
        ingress_geoip: snapshot
            .and_then(|s| s.ingress_geoip.clone())
            .unwrap_or_else(unknown_geoip),
        egress_geoip: snapshot
            .and_then(|s| s.egress_geoip.clone())
            .unwrap_or_else(unknown_geoip),
        nat_type: "Unknown".to_string(),
        finished_at: services::current_timestamp(),
    }
}

/// 批量执行测速任务并通过事件实时推送进度。
#[tauri::command]
pub async fn run_speedtest_batch(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    raw_input: String,
    filter: Option<NodeFilter>,
    config: Option<SpeedTestTaskConfig>,
) -> Result<Vec<SpeedTestResult>, String> {
    let nodes = services::parse_subscription_nodes(&raw_input);
    let filtered_nodes = if let Some(current_filter) = filter {
        services::filter_nodes(&nodes, &current_filter)?
    } else {
        nodes
    };
    let checkpoint_raw_input = filtered_nodes
        .iter()
        .map(|node| node.raw.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let task_config = config.unwrap_or_default();
    let download_source = state.speedtest_download_source.lock().unwrap().clone();

    let kernel_path = resolve_kernel_path(&window, &state).await?;

    info!("[命令] run_speedtest_batch 使用内核 at {:?}", kernel_path);

    let results = services::run_batch_speedtest(
        filtered_nodes,
        &checkpoint_raw_input,
        &task_config,
        &download_source,
        kernel_path,
        0,
        Vec::new(),
        None,
        |event| {
            info!(
                "[事件] task={}, seq={}, type={}, node_id={:?}, node={}, stage={}, metric_id={:?}, metric_value={:?}, final={:?}",
                event.task_id,
                event.event_seq,
                event.event_type,
                event.node_id,
                event.current_node,
                event.stage,
                event.metric_id,
                event.metric_value,
                event.metric_final
            );
            let _ = window.emit("speedtest://progress", &event);
        },
    )
    .await?;

    if let Err(e) = services::speedtest::persist_speedtest_history(&task_config, &results) {
        warn!("[命令] persist_speedtest_history 失败: {}", e);
    }

    // 测速成功后清除 checkpoint
    if let Err(e) = services::checkpoint::clear_checkpoint() {
        warn!("[命令] 清除 checkpoint 失败: {}", e);
    }

    Ok(results)
}

/// 从 checkpoint 恢复并继续测速。
#[tauri::command]
pub async fn resume_speedtest_from_checkpoint(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SpeedTestResult>, String> {
    let checkpoint = services::checkpoint::load_checkpoint()?
        .ok_or_else(|| "没有可恢复的测速任务".to_string())?;

    if checkpoint.raw_input.trim().is_empty() {
        return Err("断点缺少原始输入，无法恢复".to_string());
    }

    let nodes = services::parse_subscription_nodes(&checkpoint.raw_input);
    if nodes.is_empty() {
        return Err("断点节点为空，无法恢复".to_string());
    }

    let total = nodes.len();
    let resume_from = checkpoint.completed.min(total);

    let mut seed_results = Vec::with_capacity(resume_from);
    for (index, node) in nodes.iter().enumerate().take(resume_from) {
        let snapshot = checkpoint
            .node_results
            .get(index)
            .and_then(|item| item.as_ref());
        seed_results.push(rebuild_result_from_snapshot(node.clone(), snapshot));
    }

    let task_config = checkpoint.config.clone().unwrap_or_default();
    let download_source = state.speedtest_download_source.lock().unwrap().clone();

    if resume_from >= total {
        if let Err(e) = services::speedtest::persist_speedtest_history(&task_config, &seed_results)
        {
            warn!("[命令] persist_speedtest_history 失败: {}", e);
        }
        if let Err(e) = services::checkpoint::clear_checkpoint() {
            warn!("[命令] 清除 checkpoint 失败: {}", e);
        }
        return Ok(seed_results);
    }

    let kernel_path = resolve_kernel_path(&window, &state).await?;
    info!(
        "[命令] resume_speedtest_from_checkpoint 从 {}/{} 继续",
        resume_from, total
    );

    let results = services::run_batch_speedtest(
        nodes,
        &checkpoint.raw_input,
        &task_config,
        &download_source,
        kernel_path,
        resume_from,
        seed_results,
        Some(checkpoint.task_id.clone()),
        |event| {
            info!(
                "[恢复事件] task={}, seq={}, type={}, node_id={:?}, node={}, stage={}, metric_id={:?}, metric_value={:?}, final={:?}",
                event.task_id,
                event.event_seq,
                event.event_type,
                event.node_id,
                event.current_node,
                event.stage,
                event.metric_id,
                event.metric_value,
                event.metric_final
            );
            let _ = window.emit("speedtest://progress", &event);
        },
    )
    .await?;

    if let Err(e) = services::speedtest::persist_speedtest_history(&task_config, &results) {
        warn!("[命令] persist_speedtest_history 失败: {}", e);
    }

    if let Err(e) = services::checkpoint::clear_checkpoint() {
        warn!("[命令] 清除 checkpoint 失败: {}", e);
    }

    Ok(results)
}

/// 获取当前测速 checkpoint（如果有）
#[tauri::command]
pub fn get_speedtest_checkpoint(
) -> Result<Option<services::checkpoint::SpeedtestCheckpoint>, String> {
    services::checkpoint::load_checkpoint()
}

/// 清除测速 checkpoint
#[tauri::command]
pub fn clear_speedtest_checkpoint() -> Result<(), String> {
    services::checkpoint::clear_checkpoint()
}
