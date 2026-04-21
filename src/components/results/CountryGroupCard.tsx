import { useState } from "react";
import {
  Card,
  CardHeader,
  CardContent,
  Chip,
  ProgressBar,
  Skeleton,
  Button,
} from "@heroui/react";
import type { CountrySpeedSummary, SpeedTestResult } from "../../types/speedtest";
import { FlagIcon } from "../ui/FlagChip";
import { SPEED_LABELS, getProgressColor } from "../map/colorScheme";

interface CountryGroupCardProps {
  summary: CountrySpeedSummary;
  isLoading?: boolean;
  completedCount?: number;
  onNodeClick?: (result: SpeedTestResult) => void;
}

export default function CountryGroupCard({
  summary,
  isLoading = false,
  completedCount = 0,
  onNodeClick,
}: CountryGroupCardProps) {
  const [isExpanded, setIsExpanded] = useState(false);
  const { country_code, country_name, node_count, max_download_mbps, avg_tcp_ping_ms, status, results } = summary;

  // 计算加载中的节点数据（用于 Skeleton 显示）
  const loadingNodes = completedCount < node_count;

  // 默认显示前3个节点
  const visibleResults = isExpanded ? results : results.slice(0, 3);
  const hasMore = results.length > 3;

  return (
    <Card className="w-full">
      <CardHeader className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
        <div className="flex min-w-0 items-start gap-3">
          <FlagIcon countryCode={country_code} />
          <div className="flex min-w-0 flex-col">
            <span className="truncate font-semibold text-medium">
              {country_name} ({country_code})
            </span>
            <span className="text-small text-foreground-500 break-words">
              {node_count} 节点 | 峰值 {max_download_mbps.toFixed(1)} Mbps | 平均延迟 {avg_tcp_ping_ms.toFixed(0)} ms
            </span>
          </div>
        </div>
        <div className="flex w-full flex-wrap items-center gap-2 md:w-auto md:justify-end">
          <Chip
            color={getChipColor(status)}
            variant="soft"
            size="sm"
          >
            {SPEED_LABELS[status]}
          </Chip>
          {isLoading && (
            <Chip variant="soft" size="sm" color="accent">
              {completedCount}/{node_count}
            </Chip>
          )}
          {(hasMore || isExpanded) && (
            <Button
              size="sm"
              variant="ghost"
              onPress={() => setIsExpanded(!isExpanded)}
            >
              {isExpanded ? "收起" : `展开更多 (${results.length - 3})`}
            </Button>
          )}
        </div>
      </CardHeader>

      {isExpanded && (
        <CardContent>
          {isLoading && loadingNodes ? (
            // Skeleton 占位
            <div className="flex flex-col gap-2">
              {visibleResults.slice(0, completedCount).map((result, index) => (
                <NodeRow key={result.node.name} result={result} rank={index + 1} maxSpeed={max_download_mbps} />
              ))}
              {completedCount < node_count && (
                <Skeleton className="h-8 rounded-medium" />
              )}
            </div>
          ) : (
            // 真实数据
            <div className="flex flex-col gap-2">
              {visibleResults.map((result, index) => (
                <NodeRow
                  key={result.node.name}
                  result={result}
                  rank={index + 1}
                  maxSpeed={max_download_mbps}
                  onClick={() => onNodeClick?.(result)}
                />
              ))}
            </div>
          )}
        </CardContent>
      )}
    </Card>
  );
}

interface NodeRowProps {
  result: SpeedTestResult;
  rank: number;
  maxSpeed: number;
  onClick?: () => void;
}

function NodeRow({ result, rank, maxSpeed, onClick }: NodeRowProps) {
  const { node, avg_download_mbps, tcp_ping_ms } = result;
  const progressValue = maxSpeed > 0 ? (avg_download_mbps / maxSpeed) * 100 : 0;
  const progressColor = getProgressColor(avg_download_mbps, maxSpeed);

  return (
    <div
      className="flex items-center justify-between gap-3 rounded-medium p-2 transition-colors hover:bg-default-100 cursor-pointer"
      onClick={onClick}
    >
      <div className="flex min-w-0 flex-1 items-center gap-3">
        <span className="w-8 shrink-0 text-small text-foreground-500">#{rank}</span>
        <span className="min-w-0 flex-1 text-small font-medium break-all">{node.name}</span>
      </div>
      <div className="hidden w-32 shrink-0 md:block">
        <ProgressBar
          value={progressValue}
          color="success"
          size="sm"
        />
      </div>
      <span className="w-24 shrink-0 text-right text-small text-foreground-600">
        {avg_download_mbps.toFixed(1)} Mbps
      </span>
      <span className="w-16 shrink-0 text-right text-small text-foreground-500">
        {tcp_ping_ms > 0 ? `${tcp_ping_ms.toFixed(0)} ms` : "-"}
      </span>
    </div>
  );
}

function getChipColor(status: string): "success" | "accent" | "warning" | "danger" | "default" {
  switch (status) {
    case "fast":
      return "success";
    case "available":
      return "accent";
    case "slow":
      return "warning";
    case "very-slow":
    case "unavailable":
      return "danger";
    default:
      return "default";
  }
}
