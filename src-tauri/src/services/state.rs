//! 状态持久化模块：负责运行状态的读写（内核版本、IP库版本等）。
//!
//! 状态以 JSON 格式保存在 `state.json` 中，位于应用数据目录下。

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::models::ClientUpdateStatus;

/// 持久化的完整运行状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub kernel_version: String,
    pub installed_kernel_versions: Vec<String>,
    pub ip_database_version: String,
    #[serde(default)]
    pub cached_kernel_versions: Vec<String>,
    #[serde(default)]
    pub kernel_last_checked_at: String,
    #[serde(default)]
    pub geoip_last_checked_at: String,
    #[serde(default)]
    pub client_update_last_checked_at: String,
    #[serde(default)]
    pub client_update_cache: Option<ClientUpdateStatus>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            kernel_version: super::kernel::DEFAULT_KERNEL_VERSIONS[0].to_string(),
            installed_kernel_versions: vec![super::kernel::DEFAULT_KERNEL_VERSIONS[0].to_string()],
            ip_database_version: super::geoip::default_ip_database_version(),
            cached_kernel_versions: super::kernel::DEFAULT_KERNEL_VERSIONS
                .iter()
                .map(|item| item.to_string())
                .collect(),
            kernel_last_checked_at: "0".to_string(),
            geoip_last_checked_at: "0".to_string(),
            client_update_last_checked_at: "0".to_string(),
            client_update_cache: None,
        }
    }
}

/// 返回应用数据根目录。
pub fn app_data_root() -> Result<PathBuf, String> {
    if let Ok(override_dir) = env::var("CAPYSPEEDTEST_DATA_DIR") {
        let path = PathBuf::from(override_dir);
        fs::create_dir_all(&path).map_err(|error| format!("创建数据目录失败: {error}"))?;
        return Ok(path);
    }
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| "无法定位本地数据目录".to_string())?;
    let dir = base.join("capyspeedtest");
    fs::create_dir_all(&dir).map_err(|error| format!("创建数据目录失败: {error}"))?;
    Ok(dir)
}

/// 状态文件路径。
pub fn state_file_path() -> Result<PathBuf, String> {
    Ok(app_data_root()?.join("state.json"))
}

/// 加载持久化的运行状态。
pub fn load_persisted_state() -> Result<PersistedState, String> {
    let path = state_file_path()?;
    if !path.exists() {
        return Ok(PersistedState::default());
    }
    let content =
        fs::read_to_string(&path).map_err(|error| format!("读取状态文件失败: {error}"))?;
    serde_json::from_str::<PersistedState>(&content)
        .map_err(|error| format!("解析状态文件失败: {error}"))
}

/// 保存运行状态。
pub fn save_persisted_state(state: &PersistedState) -> Result<(), String> {
    let path = state_file_path()?;
    let content =
        serde_json::to_string_pretty(state).map_err(|error| format!("序列化状态失败: {error}"))?;
    let parent = path
        .parent()
        .ok_or_else(|| "无法获取状态文件目录".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("创建状态目录失败: {error}"))?;
    fs::write(path, content).map_err(|error| format!("写入状态文件失败: {error}"))
}

/// 从磁盘加载运行状态；用于应用启动恢复内核与 GeoIP 版本。
pub fn load_runtime_state() -> PersistedState {
    load_persisted_state().unwrap_or_default()
}

/// 写入运行状态。
pub fn persist_runtime_state(
    kernel_version: &str,
    installed_kernel_versions: &[String],
    ip_database_version: &str,
) -> Result<(), String> {
    let mut state = load_persisted_state().unwrap_or_default();
    state.kernel_version = kernel_version.to_string();
    state.installed_kernel_versions = installed_kernel_versions.to_vec();
    state.ip_database_version = ip_database_version.to_string();
    save_persisted_state(&state)
}

/// 更新并持久化状态（保留其余字段）。
pub fn update_persisted_state(updater: impl FnOnce(&mut PersistedState)) -> Result<(), String> {
    let mut state = load_persisted_state().unwrap_or_default();
    updater(&mut state);
    save_persisted_state(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn 持久化状态应可回读() {
        let guard = ENV_LOCK.lock().expect("环境锁失败");
        let temp_root = env::temp_dir().join(format!(
            "capyspeedtest-test-{}",
            super::super::current_timestamp()
        ));
        let _ = fs::remove_dir_all(&temp_root);
        fs::create_dir_all(&temp_root).expect("创建临时目录失败");
        env::set_var("CAPYSPEEDTEST_DATA_DIR", &temp_root);

        let versions = vec!["v1.19.1".to_string(), "v1.19.0".to_string()];
        persist_runtime_state("v1.19.0", &versions, "2026.04.15").expect("写入状态失败");
        let loaded = load_runtime_state();
        assert_eq!(loaded.kernel_version, "v1.19.0");
        assert_eq!(loaded.installed_kernel_versions.len(), 2);
        assert_eq!(loaded.ip_database_version, "2026.04.15");

        env::remove_var("CAPYSPEEDTEST_DATA_DIR");
        let _ = fs::remove_dir_all(&temp_root);
        drop(guard);
    }
}
