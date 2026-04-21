//! services 模块：核心业务逻辑，按职责拆分为独立子模块。
//!
//! | 子模块 | 职责 |
//! |---|---|
//! | `state` | 运行状态持久化（JSON 文件读写） |
//! | `checkpoint` | 测速断点持久化（JSON 文件读写） |
//! | `subscription` | 订阅解析（Base64 解码、URI 解析、节点过滤） |
//! | `kernel` | Mihomo 内核下载、Spawn、配置生成、生命周期管理 |
//! | `geoip` | MMDB 数据库读取、真实 IP→地理位置查询 |
//! | `speedtest` | 真实网络测速（TCP Ping、HTTP 下载/上传、NAT 检测） |
//! | `updater` | GitHub 版本检查、客户端更新包下载 |
//! | `system_proxy` | Windows 系统代理自动检测 |
//! | `http_client` | 非测速场景全局 HTTP 单例客户端（含系统代理） |

pub mod checkpoint;
pub mod geoip;
pub mod http_client;
pub mod kernel;
pub mod speedtest;
pub mod state;
pub mod subscription;
pub mod system_proxy;
pub mod updater;

// Re-export commonly used types.
pub use crate::models::{
    ClientUpdateDownloadResult, ClientUpdateStatus, GeoIpInfo, KernelStatus, NodeFilter, NodeInfo,
    SettingsSnapshot, SpeedTestResult, SpeedTestTaskConfig,
};

// Re-export functions from submodules.
pub use geoip::{
    default_ip_database_version, download_geoip_database, geoip_database_exists,
    geoip_database_path, infer_country_from_name, latest_ip_database_version, lookup_ip_local,
};
pub use kernel::{
    detect_platform, download_kernel_version, kernel_binary_exists, kernel_binary_path,
    list_kernel_versions, list_local_kernel_versions, DEFAULT_KERNEL_VERSIONS,
};
pub use speedtest::{current_timestamp, run_batch_speedtest};
pub use state::{
    app_data_root as state_app_data_root, load_runtime_state, persist_runtime_state,
    update_persisted_state,
};
pub use subscription::{fetch_subscription_from_url, filter_nodes, parse_subscription_nodes};
pub use updater::{download_client_update_package, try_check_client_update};
