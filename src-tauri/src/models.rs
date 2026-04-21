use serde::{Deserialize, Serialize};

/// 应用设置页所需的内核运行状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KernelStatus {
    pub platform: String,
    pub current_version: String,
    pub installed_versions: Vec<String>,
    pub current_exists: bool,
    #[serde(default)]
    pub local_installed_versions: Vec<String>,
    pub last_checked_at: String,
}

/// IP 库状态信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpDatabaseStatus {
    pub current_version: String,
    #[serde(default)]
    pub current_exists: bool,
    #[serde(default)]
    pub latest_version: String,
    pub last_checked_at: String,
}

/// 客户端更新状态信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientUpdateStatus {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub download_url: String,
    pub release_notes: String,
}

/// 客户端更新包下载结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientUpdateDownloadResult {
    pub version: String,
    pub package_path: String,
    pub backup_path: Option<String>,
    pub rolled_back: bool,
}

/// 设置页聚合快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SettingsSnapshot {
    pub kernel: KernelStatus,
    pub ip_database: IpDatabaseStatus,
    pub client_update: ClientUpdateStatus,
}

/// 手动检查内核与 GeoIP 的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KernelGeoIpCheckResult {
    pub kernel: KernelStatus,
    pub ip_database: IpDatabaseStatus,
}

/// 启动后的定时后台检查结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduledUpdateCheckResult {
    pub client_update: Option<ClientUpdateStatus>,
}

/// 节点连接信息（用于 Mihomo 代理测试）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConnectInfo {
    pub server: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// 解析后的节点信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeInfo {
    pub name: String,
    pub protocol: String,
    pub country: String,
    pub raw: String,
    /// 解析后的 mihomo 代理配置（单节点 JSON），用于内核配置生成。
    #[serde(default)]
    pub parsed_proxy_payload: Option<String>,
    /// 连接信息（Mihomo 代理测速时使用）
    #[serde(default)]
    pub connect_info: Option<NodeConnectInfo>,
    /// 测试文件 URL（下载测速用）
    #[serde(default)]
    pub test_file: Option<String>,
    /// 上传目标 URL
    #[serde(default)]
    pub upload_target: Option<String>,
}

/// 节点过滤器；支持名称正则、地区列表和数量限制。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct NodeFilter {
    pub name_regex: Option<String>,
    pub countries: Option<Vec<String>>,
    pub limit: Option<usize>,
    pub limit_per_country: Option<usize>,
}

/// 单次测速任务配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeedTestTaskConfig {
    pub concurrency: u8,
    pub target_sites: Vec<String>,
    pub enable_upload_test: bool,
    pub timeout_ms: u64,
}

impl Default for SpeedTestTaskConfig {
    fn default() -> Self {
        Self {
            concurrency: 4,
            target_sites: vec!["https://www.google.com/generate_204".to_string()],
            enable_upload_test: true,
            timeout_ms: 8000,
        }
    }
}

/// GeoIP 信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeoIpInfo {
    pub ip: String,
    pub country_code: String,
    pub country_name: String,
    pub isp: String,
}

/// 全量 GeoIP 快照中的单节点项。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeoIpSnapshotItem {
    pub node_id: String,
    pub node_name: String,
    pub ingress_geoip: GeoIpInfo,
    pub egress_geoip: GeoIpInfo,
}

/// 单节点测速结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeedTestResult {
    pub node: NodeInfo,
    pub tcp_ping_ms: u32,
    pub site_ping_ms: u32,
    pub packet_loss_rate: f32,
    pub avg_download_mbps: f32,
    pub max_download_mbps: f32,
    pub avg_upload_mbps: Option<f32>,
    pub max_upload_mbps: Option<f32>,
    pub ingress_geoip: GeoIpInfo,
    pub egress_geoip: GeoIpInfo,
    pub nat_type: String,
    pub finished_at: String,
}

/// 批量测速进度事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeedTestProgressEvent {
    /// 本轮测速任务唯一 ID。
    pub task_id: String,
    /// 事件序号（同一 task 内单调递增）。
    pub event_seq: u64,
    /// 事件类型：node_stage | metric_instant | metric_final | node_completed | node_error | info_update
    pub event_type: String,
    pub total: usize,
    pub completed: usize,
    pub current_node: String,
    /// 当前节点唯一 ID（例如 node-0）。
    pub node_id: Option<String>,
    pub stage: String, // "connecting" | "tcp_ping" | "site_ping" | "downloading" | "uploading" | "completed" | "error"
    pub message: String,
    /// 指标唯一 ID（例如 node-0:tcp_ping_ms）。
    pub metric_id: Option<String>,
    /// 指标值（用于 metric_* 事件）。
    pub metric_value: Option<f64>,
    /// 指标单位（ms / Mbps）。
    pub metric_unit: Option<String>,
    /// 是否该指标最终值（通常与 metric_final 一致）。
    pub metric_final: Option<bool>,
    /// 当前节点 TCP 延迟（ms），tcp_ping 阶段完成后有效
    pub tcp_ping_ms: Option<u32>,
    /// 当前节点 Site 延迟（ms），site_ping 阶段完成后有效
    pub site_ping_ms: Option<u32>,
    /// 当前节点下载速度（Mbps），仅在 downloading 阶段有效
    pub avg_download_mbps: Option<f32>,
    /// 当前节点下载峰值速度（Mbps），仅在 downloading 阶段有效
    pub max_download_mbps: Option<f32>,
    /// 当前节点上传速度（Mbps），仅在 uploading 阶段有效
    pub avg_upload_mbps: Option<f32>,
    /// 当前节点上传峰值速度（Mbps），仅在 uploading 阶段有效
    pub max_upload_mbps: Option<f32>,
    /// 入口 GeoIP 信息，node_completed 事件时有效
    pub ingress_geoip: Option<GeoIpInfo>,
    /// 出口 GeoIP 信息，node_completed 事件时有效
    pub egress_geoip: Option<GeoIpInfo>,
    /// 全量 GeoIP 快照（geoip_snapshot 事件时有效）
    #[serde(default)]
    pub geoip_snapshot: Option<Vec<GeoIpSnapshotItem>>,
}

/// 内核下载进度事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KernelDownloadProgressEvent {
    pub version: String,
    pub stage: String, // "downloading" | "extracting" | "completed" | "error"
    pub progress: f32, // 0.0 - 100.0
    pub message: String,
}

/// GeoIP 数据库下载进度事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoIpDownloadProgressEvent {
    pub stage: String, // "downloading" | "completed" | "error"
    pub progress: f32, // 0.0 - 100.0
    pub message: String,
}

/// 订阅获取进度事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubscriptionFetchProgressEvent {
    pub stage: String, // "fetching" | "parsing" | "completed" | "error"
    pub progress: f32, // 0.0 - 100.0
    pub message: String,
    pub nodes_count: Option<usize>,
}

/// 内核版本列表获取进度事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KernelListProgressEvent {
    pub stage: String, // "fetching" | "completed" | "error"
    pub progress: f32, // 0.0 - 100.0
    pub message: String,
    pub versions_count: Option<usize>,
}

/// 客户端更新检查进度事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateCheckProgressEvent {
    pub stage: String, // "checking" | "completed" | "error"
    pub progress: f32, // 0.0 - 100.0
    pub message: String,
}

/// 客户端更新下载进度事件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateDownloadProgressEvent {
    pub version: String,
    pub stage: String, // "downloading" | "verifying" | "completed" | "error"
    pub progress: f32, // 0.0 - 100.0
    pub message: String,
}

/// SQLite 中的批次摘要（用于列表展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSummary {
    pub batch_id: i64,
    pub created_at: i64,
    pub node_count: usize,
    pub config_json: String,
}

/// 散点图数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterPoint {
    pub batch_id: i64,
    pub finished_at: i64, // Unix timestamp
    pub hour: f64,        // 0.0 ~ 23.99，一天中的小时
    pub country_code: String,
    pub avg_download_mbps: f64,
    pub avg_upload_mbps: Option<f64>,
    pub node_name: String,
}

/// 本地用户数据目录信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataDirectoryInfo {
    pub path: String,
    pub logs_path: String,
    pub total_bytes: u64,
    pub file_count: u64,
}

/// 用户数据导出结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserDataExportResult {
    pub archive_path: String,
}
