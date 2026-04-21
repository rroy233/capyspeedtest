//! 数据库相关命令：批次CRUD、散点图数据。

use crate::database::batch;
use crate::models::{BatchSummary, ScatterPoint, SpeedTestResult, SpeedTestTaskConfig};
use crate::services;
use tracing::info;

/// 保存测速批次到 SQLite 数据库
#[tauri::command]
pub fn db_save_batch(
    subscription_text: String,
    config_json: String,
    results: Vec<SpeedTestResult>,
) -> Result<i64, String> {
    info!(
        "[命令] db_save_batch called, results count={}",
        results.len()
    );
    let config: SpeedTestTaskConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("解析配置失败: {}", e))?;
    let now = services::current_timestamp()
        .parse::<i64>()
        .unwrap_or_else(|_| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });
    batch::save_batch(now, &subscription_text, &config, &results)
}

/// 分页获取批次列表
#[tauri::command]
pub fn db_get_batches(
    from_timestamp: Option<i64>,
    to_timestamp: Option<i64>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<BatchSummary>, String> {
    info!(
        "[命令] db_get_batches from={:?}, to={:?}, limit={:?}, offset={:?}",
        from_timestamp, to_timestamp, limit, offset
    );
    batch::get_batches(
        from_timestamp,
        to_timestamp,
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )
}

/// 获取指定批次的全部测速结果
#[tauri::command]
pub fn db_get_batch_results(batch_id: i64) -> Result<Vec<SpeedTestResult>, String> {
    info!("[命令] db_get_batch_results batch_id={}", batch_id);
    batch::get_batch_results(batch_id)
}

/// 删除指定批次
#[tauri::command]
pub fn db_delete_batches(batch_ids: Vec<i64>) -> Result<usize, String> {
    info!("[命令] db_delete_batches batch_ids={:?}", batch_ids);
    batch::delete_batches(&batch_ids)
}

/// 删除 N 个月前的所有批次
#[tauri::command]
pub fn db_delete_batches_older_than(months: i64) -> Result<usize, String> {
    info!("[命令] db_delete_batches_older_than months={}", months);
    let seconds_per_month: i64 = 30 * 24 * 60 * 60;
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 - months * seconds_per_month)
        .unwrap_or(0);
    batch::delete_batches_older_than(cutoff)
}

/// 清空所有历史记录
#[tauri::command]
pub fn db_clear_all_batches() -> Result<usize, String> {
    info!("[命令] db_clear_all_batches");
    batch::clear_all_batches()
}

/// 获取散点图数据
#[tauri::command]
pub fn db_get_scatter_data(
    from_timestamp: Option<i64>,
    to_timestamp: Option<i64>,
) -> Result<Vec<ScatterPoint>, String> {
    info!(
        "[命令] db_get_scatter_data from={:?}, to={:?}",
        from_timestamp, to_timestamp
    );
    batch::get_scatter_data(from_timestamp, to_timestamp)
}

/// 获取所有出现过的地区代码
#[tauri::command]
pub fn db_get_all_countries() -> Result<Vec<String>, String> {
    info!("[命令] db_get_all_countries");
    batch::get_all_countries()
}
