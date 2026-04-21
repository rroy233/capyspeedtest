//! 测速断点保存模块
//!
//! 在批量测速过程中，每完成一个节点后将进度序列化写入 checkpoint 文件。
//! 应用重启后可从 checkpoint 恢复测速。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::models::SpeedTestTaskConfig;
use crate::services::state::app_data_root;
use tracing::info;

/// 断点状态结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedtestCheckpoint {
    /// 任务 ID
    pub task_id: String,
    /// 节点总数
    pub total: usize,
    /// 已完成数
    pub completed: usize,
    /// 节点名称列表（按顺序）
    pub node_names: Vec<String>,
    /// 每个节点的测速结果快照
    pub node_results: Vec<Option<NodeResultSnapshot>>,
    /// 原始输入文本
    pub raw_input: String,
    /// 测速任务配置（用于恢复）
    #[serde(default)]
    pub config: Option<SpeedTestTaskConfig>,
    /// 保存时间戳（毫秒）
    pub saved_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResultSnapshot {
    pub tcp_ping_ms: Option<u32>,
    pub site_ping_ms: Option<u32>,
    pub avg_download_mbps: Option<f32>,
    pub max_download_mbps: Option<f32>,
    pub avg_upload_mbps: Option<f32>,
    pub max_upload_mbps: Option<f32>,
    pub status: String,
    pub ingress_geoip: Option<crate::models::GeoIpInfo>,
    pub egress_geoip: Option<crate::models::GeoIpInfo>,
}

/// checkpoint 文件路径
pub fn checkpoint_path() -> Result<PathBuf, String> {
    Ok(app_data_root()?.join("speedtest_checkpoint.json"))
}

/// 保存 checkpoint 到磁盘
pub fn save_checkpoint(checkpoint: &SpeedtestCheckpoint) -> Result<(), String> {
    let path = checkpoint_path()?;
    let content = serde_json::to_string_pretty(checkpoint)
        .map_err(|e| format!("序列化 checkpoint 失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("写入 checkpoint 失败: {}", e))?;
    info!(
        "[Checkpoint] 已保存断点: completed={}/{}",
        checkpoint.completed, checkpoint.total
    );
    Ok(())
}

/// 从磁盘加载 checkpoint
pub fn load_checkpoint() -> Result<Option<SpeedtestCheckpoint>, String> {
    let path = checkpoint_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("读取 checkpoint 失败: {}", e))?;
    let checkpoint: SpeedtestCheckpoint =
        serde_json::from_str(&content).map_err(|e| format!("解析 checkpoint 失败: {}", e))?;
    Ok(Some(checkpoint))
}

/// 清除 checkpoint 文件
pub fn clear_checkpoint() -> Result<(), String> {
    let path = checkpoint_path()?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("删除 checkpoint 失败: {}", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_序列化和反序列化() {
        let checkpoint = SpeedtestCheckpoint {
            task_id: "test-task".to_string(),
            total: 10,
            completed: 3,
            node_names: vec!["node-1".to_string(), "node-2".to_string()],
            node_results: vec![
                Some(NodeResultSnapshot {
                    tcp_ping_ms: Some(50),
                    site_ping_ms: Some(100),
                    avg_download_mbps: Some(100.0),
                    max_download_mbps: Some(150.0),
                    avg_upload_mbps: None,
                    max_upload_mbps: None,
                    status: "completed".to_string(),
                    ingress_geoip: None,
                    egress_geoip: None,
                }),
                None,
            ],
            raw_input: "vless://test".to_string(),
            config: Some(SpeedTestTaskConfig::default()),
            saved_at: 1234567890,
        };

        let json = serde_json::to_string_pretty(&checkpoint).unwrap();
        let loaded: SpeedtestCheckpoint = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.task_id, "test-task");
        assert_eq!(loaded.total, 10);
        assert_eq!(loaded.completed, 3);
        assert_eq!(
            loaded.node_results[0].as_ref().unwrap().tcp_ping_ms,
            Some(50)
        );
    }
}
