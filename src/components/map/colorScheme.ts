import type { SpeedStatus } from "../../types/speedtest";

// 速度颜色定义
export const SPEED_COLORS: Record<SpeedStatus, string> = {
  fast: "#22c55e",        // 绿色 - 速度快 > 100 Mbps
  available: "#3b82f6",   // 蓝色 - 可用 30-100 Mbps
  slow: "#f97316",        // 橙色 - 慢 5-30 Mbps
  "very-slow": "#ef4444", // 红色 - 极慢 < 5 Mbps
  unavailable: "#a1a1aa",  // 灰色 - 不可用/无数据
};

// 速度标签（用于 Chip 显示）
export const SPEED_LABELS: Record<SpeedStatus, string> = {
  fast: "快速",
  available: "可用",
  slow: "较慢",
  "very-slow": "极慢",
  unavailable: "不可用",
};

/**
 * 根据速度状态获取颜色
 */
export function getSpeedColor(status: SpeedStatus): string {
  return SPEED_COLORS[status] ?? SPEED_COLORS.unavailable;
}

/**
 * 获取进度条颜色（基于速度百分比）
 */
export function getProgressColor(avgMbps: number, maxMbps: number): string {
  if (maxMbps <= 0) return SPEED_COLORS.unavailable;
  const ratio = avgMbps / maxMbps;
  if (ratio >= 0.7) return SPEED_COLORS.fast;
  if (ratio >= 0.4) return SPEED_COLORS.available;
  if (ratio >= 0.15) return SPEED_COLORS.slow;
  return SPEED_COLORS["very-slow"];
}
