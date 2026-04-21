import type { SpeedTestResult, CountrySpeedSummary, SpeedStatus } from "../types/speedtest";
import { getCountryName } from "./countryMapping";

// 速度阈值定义
export const SPEED_THRESHOLDS = {
  VERY_SLOW: 5,    // < 5 Mbps
  SLOW: 30,         // 5-30 Mbps
  AVAILABLE: 100,   // 30-100 Mbps
  FAST: Infinity,   // > 100 Mbps
};

/**
 * 根据平均下载速度判断状态
 */
export function getSpeedStatus(avgDownloadMbps: number | null | undefined): SpeedStatus {
  if (avgDownloadMbps === null || avgDownloadMbps === undefined) {
    return "unavailable";
  }
  if (avgDownloadMbps < SPEED_THRESHOLDS.VERY_SLOW) {
    return "very-slow";
  }
  if (avgDownloadMbps < SPEED_THRESHOLDS.SLOW) {
    return "slow";
  }
  if (avgDownloadMbps < SPEED_THRESHOLDS.AVAILABLE) {
    return "available";
  }
  return "fast";
}

/**
 * 按地区分组测速结果，组内按速度降序排列
 */
export function groupResultsByCountry(results: SpeedTestResult[]): CountrySpeedSummary[] {
  // 按地区代码分组
  const grouped = results.reduce((acc, result) => {
    const code = result.node.country.toUpperCase();
    if (!acc[code]) {
      acc[code] = [];
    }
    acc[code].push(result);
    return acc;
  }, {} as Record<string, SpeedTestResult[]>);

  // 计算每个地区的汇总数据
  const summaries: CountrySpeedSummary[] = Object.entries(grouped).map(([code, countryResults]) => {
    // 按下载速度降序排序
    const sorted = [...countryResults].sort(
      (a, b) => b.avg_download_mbps - a.avg_download_mbps
    );

    const speeds = sorted.map((r) => r.avg_download_mbps);
    const pings = sorted.map((r) => r.tcp_ping_ms).filter((p) => p > 0);
    const maxSpeed = Math.max(...speeds);
    const avgSpeed = speeds.reduce((a, b) => a + b, 0) / speeds.length;
    const avgPing = pings.length > 0 ? pings.reduce((a, b) => a + b, 0) / pings.length : 0;
    const minPing = pings.length > 0 ? Math.min(...pings) : 0;

    return {
      country_code: code,
      country_name: getCountryName(code),
      node_count: sorted.length,
      avg_download_mbps: avgSpeed,
      max_download_mbps: maxSpeed,
      avg_tcp_ping_ms: avgPing,
      min_tcp_ping_ms: minPing,
      status: getSpeedStatus(avgSpeed),
      results: sorted,
      testingNodes: [],
    };
  });

  // 按峰值速度降序排列地区
  return summaries.sort((a, b) => b.max_download_mbps - a.max_download_mbps);
}
