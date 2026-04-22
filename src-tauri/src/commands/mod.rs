//! commands 模块：Tauri 命令处理层，按职责拆分为独立子模块。
//!
//! | 子模块 | 职责 |
//! |---|---|
//! | `settings` | 设置相关命令：内核版本列表/切换、IP库刷新 |
//! | `subscription` | 订阅解析命令：解析节点、远程获取订阅 |
//! | `updates` | 更新检查命令：客户端更新、内核/GeoIP 检查、定时任务 |
//! | `speedtest` | 测速命令：批量执行测速 |
//! | `database_cmd` | 数据库命令：批次CRUD、散点图数据 |
//! | `data_mgmt` | 数据管理命令：目录信息、导出、清理 |
//! | `fs_utils` | 文件系统工具：目录统计、zip打包 |

pub mod data_mgmt;
pub mod database_cmd;
pub mod fs_utils;
pub mod settings;
pub mod speedtest;
pub mod subscription;
pub mod updates;

// Re-export all commands for main.rs compatibility.
// main.rs uses commands::function_name in generate_handler!.
pub use data_mgmt::*;
pub use database_cmd::*;
pub use settings::*;
pub use speedtest::*;
pub use subscription::*;
pub use updates::*;

// ============================================================================
// AppState - 应用运行时状态
// ============================================================================

use crate::services;
use std::sync::Mutex;

/// 应用运行时状态：当前选择的内核版本和 IP 库版本。
pub struct AppState {
    pub kernel_version: Mutex<String>,
    pub installed_kernel_versions: Mutex<Vec<String>>,
    pub ip_database_version: Mutex<String>,
    pub cached_kernel_versions: Mutex<Vec<String>>,
    pub kernel_last_checked_at: Mutex<String>,
    pub geoip_last_checked_at: Mutex<String>,
    pub client_update_last_checked_at: Mutex<String>,
    pub client_update_cache: Mutex<Option<crate::models::ClientUpdateStatus>>,
    pub receive_prerelease_updates: Mutex<bool>,
    pub speedtest_download_source: Mutex<String>,
}

impl Default for AppState {
    fn default() -> Self {
        let runtime_state = services::load_runtime_state();

        Self {
            kernel_version: Mutex::new(runtime_state.kernel_version),
            installed_kernel_versions: Mutex::new(runtime_state.installed_kernel_versions),
            ip_database_version: Mutex::new(runtime_state.ip_database_version),
            cached_kernel_versions: Mutex::new(runtime_state.cached_kernel_versions),
            kernel_last_checked_at: Mutex::new(runtime_state.kernel_last_checked_at),
            geoip_last_checked_at: Mutex::new(runtime_state.geoip_last_checked_at),
            client_update_last_checked_at: Mutex::new(runtime_state.client_update_last_checked_at),
            client_update_cache: Mutex::new(runtime_state.client_update_cache),
            receive_prerelease_updates: Mutex::new(runtime_state.receive_prerelease_updates),
            speedtest_download_source: Mutex::new(runtime_state.speedtest_download_source),
        }
    }
}

// ============================================================================
// Shared constants
// ============================================================================

const DAY_SECONDS: i64 = 24 * 60 * 60;
const WEEK_SECONDS: i64 = 7 * DAY_SECONDS;

// ============================================================================
// Shared helper functions
// ============================================================================

fn parse_ts_seconds(text: &str) -> i64 {
    text.parse::<i64>().unwrap_or(0)
}

fn runtime_app_version(app: &tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}
