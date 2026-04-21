export interface GeoIpInfo {
  ip: string;
  country_code: string;
  country_name: string;
  isp: string;
}

export interface NodeInfo {
  name: string;
  protocol: string;
  country: string;
  raw: string;
  parsed_proxy_payload?: string;
}

export interface NodeFilter {
  name_regex?: string;
  countries?: string[];
  limit?: number;
  limit_per_country?: number;
}

export interface SpeedTestTaskConfig {
  concurrency: number;
  target_sites: string[];
  enable_upload_test: boolean;
  timeout_ms: number;
}

export interface SpeedTestResult {
  node: NodeInfo;
  tcp_ping_ms: number;
  site_ping_ms: number;
  packet_loss_rate: number;
  avg_download_mbps: number;
  max_download_mbps: number;
  avg_upload_mbps?: number;
  max_upload_mbps?: number;
  ingress_geoip: GeoIpInfo;
  egress_geoip: GeoIpInfo;
  nat_type: string;
  finished_at: string;
}

export interface GeoIpSnapshotItem {
  node_id: string;
  node_name: string;
  ingress_geoip: GeoIpInfo;
  egress_geoip: GeoIpInfo;
}

export interface SpeedTestProgressEvent {
  task_id: string;
  event_seq: number;
  event_type: string; // node_stage | metric_instant | metric_final | node_completed | node_error | info_update
  total: number;
  completed: number;
  current_node: string;
  node_id?: string;
  stage: string; // "connecting" | "tcp_ping" | "site_ping" | "downloading" | "uploading" | "completed" | "error"
  message: string;
  metric_id?: string;
  metric_value?: number;
  metric_unit?: string;
  metric_final?: boolean;
  /** 当前节点 TCP 延迟（ms），tcp_ping 阶段完成后有效 */
  tcp_ping_ms?: number;
  /** 当前节点 Site 延迟（ms），site_ping 阶段完成后有效 */
  site_ping_ms?: number;
  /** 当前节点下载速度（Mbps），仅在 downloading 阶段有效 */
  avg_download_mbps?: number;
  /** 当前节点下载峰值速度（Mbps），仅在 downloading 阶段有效 */
  max_download_mbps?: number;
  /** 当前节点上传速度（Mbps），仅在 uploading 阶段有效 */
  avg_upload_mbps?: number;
  /** 当前节点上传峰值速度（Mbps），仅在 uploading 阶段有效 */
  max_upload_mbps?: number;
  /** 入口 GeoIP 信息，node_completed 事件时有效 */
  ingress_geoip?: GeoIpInfo;
  /** 出口 GeoIP 信息，node_completed 事件时有效 */
  egress_geoip?: GeoIpInfo;
  /** 全量 GeoIP 快照，geoip_snapshot 事件时有效 */
  geoip_snapshot?: GeoIpSnapshotItem[];
}

export interface KernelDownloadProgressEvent {
  version: string;
  stage: string;
  progress: number;
  message: string;
}

export type NodeTestStatus = "pending" | "testing" | "completed" | "error";

export interface NodeTestState {
  node: NodeInfo;
  status: NodeTestStatus;
  result?: SpeedTestResult;
  currentStage?: string;
  currentSpeed?: {
    tcp_ping_ms?: number;
    site_ping_ms?: number;
    avg_download_mbps?: number;
    max_download_mbps?: number;
    avg_upload_mbps?: number;
    max_upload_mbps?: number;
  };
  errorMessage?: string;
}

// 速度状态类型
export type SpeedStatus = "fast" | "available" | "slow" | "very-slow" | "unavailable";

// 地区测速汇总
export interface CountrySpeedSummary {
  country_code: string;
  country_name: string;
  node_count: number;
  avg_download_mbps: number;
  max_download_mbps: number;
  avg_tcp_ping_ms: number;
  min_tcp_ping_ms: number;
  status: SpeedStatus;
  results: SpeedTestResult[];
  testingNodes: NodeTestState[]; // 进行中的节点（非 completed 状态）
}
