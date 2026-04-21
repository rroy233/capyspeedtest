import type { NodeTestState } from "../../types/speedtest";
import {
  Card,
  CardContent,
  Chip,
  Skeleton,
  Spinner,
} from "@heroui/react";

interface NodeListItemProps {
  name: string;
  state: NodeTestState;
  isTesting: boolean;
}

// 阶段标签映射
const stageLabels: Record<string, string> = {
  connecting: "连接中",
  tcp_ping: "TCP延迟",
  site_ping: "网页延迟",
  downloading: "下载测速",
  uploading: "上传测速",
  completed: "已完成",
  error: "失败",
};

export function NodeListItem({ name, state, isTesting }: NodeListItemProps) {
  const { node, status, result, currentStage, currentSpeed, errorMessage } = state;

  // Pending 状态 - 显示节点信息，结果区域 Skeleton
  if (status === "pending") {
    return (
      <Card className="opacity-70 h-[72px] shrink-0">
        <CardContent className="h-full px-4 py-0 flex items-center">
          <div className="w-full flex items-center justify-between gap-4">
            {/* 节点基本信息 */}
            <div className="flex min-w-0 flex-1 items-center gap-3">
              <div className="h-8 w-8 shrink-0 rounded bg-default-200 flex items-center justify-center">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" className="text-default-500">
                  <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2" />
                </svg>
              </div>
              <div className="min-w-0 flex items-center gap-2 overflow-hidden">
                <span className="truncate font-medium">{node.name}</span>
                <Chip size="sm" className="shrink-0 capitalize">{node.protocol}</Chip>
                <Chip size="sm" variant="soft" className="shrink-0">{node.country}</Chip>
              </div>
            </div>

            {/* 结果区域 Skeleton */}
            <div className="shrink-0 flex items-center gap-4">
              <div className="text-center">
                <Skeleton className="h-3 w-8 rounded mb-1" />
                <Skeleton className="h-5 w-14 rounded" />
              </div>
              <div className="text-center">
                <Skeleton className="h-3 w-8 rounded mb-1" />
                <Skeleton className="h-5 w-14 rounded" />
              </div>
              <div className="text-center">
                <Skeleton className="h-3 w-10 rounded mb-1" />
                <Skeleton className="h-5 w-16 rounded" />
              </div>
              <div className="text-center">
                <Skeleton className="h-3 w-10 rounded mb-1" />
                <Skeleton className="h-5 w-16 rounded" />
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  // Testing 状态 - 实时显示所有阶段数据
  if (status === "testing") {
    const tcpPing = currentSpeed?.tcp_ping_ms ?? result?.tcp_ping_ms;
    const sitePing = currentSpeed?.site_ping_ms ?? result?.site_ping_ms;
    const downloadSpeed = currentSpeed?.avg_download_mbps;
    const uploadSpeed = currentSpeed?.avg_upload_mbps;

    return (
      <Card className="border-accent/50 bg-accent/5 h-[72px] shrink-0">
        <CardContent className="h-full px-4 py-0 flex items-center">
          <div className="w-full flex items-center justify-between gap-4">
            {/* 节点基本信息 */}
            <div className="flex min-w-0 flex-1 items-center gap-3">
              <Spinner size="sm" color="accent" className="shrink-0" />
              <div className="min-w-0 flex items-center gap-2 overflow-hidden">
                <span className="truncate font-semibold">{node.name}</span>
                <Chip size="sm" className="shrink-0 capitalize">{node.protocol}</Chip>
                <Chip size="sm" variant="soft" color="accent" className="shrink-0">
                  {stageLabels[currentStage || ""] || "测试中"}
                </Chip>
              </div>
            </div>

            {/* 实时结果显示 */}
            <div className="shrink-0 flex items-center gap-4 text-sm">
              {/* TCP 延迟 */}
              <div className="text-center min-w-[60px]">
                <p className="text-xs text-foreground-500">TCP</p>
                {tcpPing !== undefined ? (
                  <p className={`font-semibold ${tcpPing > 500 ? "text-danger" : tcpPing > 200 ? "text-warning" : "text-success"}`}>
                    {tcpPing}ms
                  </p>
                ) : (
                  <Skeleton className="h-5 w-14 rounded mx-auto" />
                )}
              </div>

              {/* Site 延迟 */}
              <div className="text-center min-w-[60px]">
                <p className="text-xs text-foreground-500">Site</p>
                {sitePing !== undefined ? (
                  <p className={`font-semibold ${sitePing > 1000 ? "text-warning" : "text-success"}`}>
                    {sitePing}ms
                  </p>
                ) : (
                  <Skeleton className="h-5 w-14 rounded mx-auto" />
                )}
              </div>

              {/* 下载速度 */}
              <div className="text-center min-w-[70px]">
                <p className="text-xs text-foreground-500">下载</p>
                {downloadSpeed !== undefined ? (
                  <p className="font-bold text-success text-base">
                    {downloadSpeed.toFixed(1)}
                  </p>
                ) : (
                  <Skeleton className="h-6 w-16 rounded mx-auto" />
                )}
              </div>

              {/* 上传速度 */}
              <div className="text-center min-w-[70px]">
                <p className="text-xs text-foreground-500">上传</p>
                {uploadSpeed !== undefined ? (
                  <p className="font-bold text-primary text-base">
                    {uploadSpeed.toFixed(1)}
                  </p>
                ) : (
                  <Skeleton className="h-6 w-16 rounded mx-auto" />
                )}
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  // Completed 状态 - 固定结果
  if (status === "completed" && result) {
    return (
      <Card className="h-[72px] shrink-0 hover:border-primary/30 transition-colors">
        <CardContent className="h-full px-4 py-0 flex items-center">
          <div className="w-full flex items-center justify-between gap-4">
            <div className="flex min-w-0 flex-1 items-center gap-3">
              <div className="h-8 w-8 shrink-0 rounded bg-success/15 flex items-center justify-center">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" className="text-success">
                  <path d="M9.2 16.2L4.8 11.8l1.4-1.4 3 3 8.6-8.6 1.4 1.4-10 10z" fill="currentColor" />
                </svg>
              </div>
              <div className="min-w-0 flex items-center gap-2 overflow-hidden">
                <span className="truncate font-semibold">{node.name}</span>
                <Chip size="sm" className="shrink-0 capitalize">{node.protocol}</Chip>
                <Chip size="sm" variant="soft" color="success" className="shrink-0">已完成</Chip>
              </div>
            </div>

            <div className="shrink-0 flex items-center gap-4 text-sm">
              <div className="text-center min-w-[60px]">
                <p className="text-xs text-foreground-500">TCP</p>
                <p className={`font-semibold ${result.tcp_ping_ms > 500 ? "text-danger" : result.tcp_ping_ms > 200 ? "text-warning" : "text-success"}`}>
                  {result.tcp_ping_ms}ms
                </p>
              </div>
              <div className="text-center min-w-[60px]">
                <p className="text-xs text-foreground-500">Site</p>
                <p className={`font-semibold ${result.site_ping_ms > 1000 ? "text-warning" : "text-success"}`}>
                  {result.site_ping_ms}ms
                </p>
              </div>
              <div className="text-center min-w-[70px]">
                <p className="text-xs text-foreground-500">下载</p>
                <p className="font-semibold text-success">{result.avg_download_mbps.toFixed(1)}</p>
              </div>
              <div className="text-center min-w-[70px]">
                <p className="text-xs text-foreground-500">上传</p>
                <p className="font-semibold text-primary">
                  {result.avg_upload_mbps != null ? result.avg_upload_mbps.toFixed(1) : "-"}
                </p>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  // Error 状态
  if (status === "error") {
    return (
      <Card className="border-danger/50 bg-danger/5 h-[72px] shrink-0">
        <CardContent className="h-full px-4 py-0 flex items-center">
          <div className="w-full flex items-center justify-between gap-4">
            <div className="flex min-w-0 flex-1 items-center gap-3">
              <div className="h-8 w-8 shrink-0 rounded-lg bg-danger/10 flex items-center justify-center">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" className="text-danger">
                  <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z" fill="currentColor" />
                </svg>
              </div>
              <div className="min-w-0 flex items-center gap-2 overflow-hidden">
                <span className="truncate font-semibold text-danger">{node.name}</span>
                <Chip size="sm" className="shrink-0 capitalize">{node.protocol}</Chip>
                <Chip size="sm" variant="soft" color="danger" className="shrink-0">失败</Chip>
              </div>
            </div>
            <p className="max-w-[260px] shrink-0 truncate text-right text-sm text-danger">
              {errorMessage || "测速失败"}
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return null;
}

export default NodeListItem;
