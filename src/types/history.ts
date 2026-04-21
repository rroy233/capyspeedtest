import type { SpeedTestResult, SpeedTestTaskConfig } from "./speedtest";
export type { SpeedTestResult };

export interface SpeedTestHistoryRecord {
  id: string;
  created_at: string;
  subscription_text: string;
  config: SpeedTestTaskConfig;
  results: SpeedTestResult[];
}

export interface SpeedTestHistoryQuery {
  keyword?: string;
  protocol?: string;
  country?: string;
  from?: string;
  to?: string;
}

// SQLite 批次摘要
export interface BatchSummary {
  batch_id: number;
  created_at: number;
  node_count: number;
  config_json: string;
}

// 散点图数据点
export interface ScatterPoint {
  batch_id: number;
  finished_at: number;
  hour: number;
  country_code: string;
  avg_download_mbps: number;
  avg_upload_mbps?: number;
  node_name: string;
}
