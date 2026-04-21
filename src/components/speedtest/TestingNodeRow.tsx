import type { NodeTestState } from "../../types/speedtest";
import { Card, Chip, Skeleton, Spinner } from "@heroui/react";

interface TestingNodeRowProps {
  nodeState: NodeTestState;
}

export function TestingNodeRow({ nodeState }: TestingNodeRowProps) {
  const { node, status, result, currentStage, currentSpeed, errorMessage } = nodeState;

  // Pending 状态 - 原有样式 + Skeleton
  if (status === "pending") {
    return (
      <div className="h-[56px] w-full flex items-center justify-between rounded-[8px] bg-default-50 px-2.5 py-0 hover:bg-default-100/80 hover:shadow-lg hover:shadow-default-900/10 transition-all duration-200">
        <div className="min-w-0 flex flex-1 flex-col justify-center">
          <div className="truncate text-sm font-medium leading-5">{node.name}</div>
          <div className="text-xs text-foreground-400">等待测速</div>
        </div>
        <div className="shrink-0 flex items-center gap-4">
          <Skeleton className="h-4 w-12 rounded" />
          <Skeleton className="h-4 w-16 rounded" />
        </div>
      </div>
    );
  }

  // Completed 状态（result 尚未从 runSpeedTestBatch 到达）
  if (status === "completed") {
    const tcpPing = result?.tcp_ping_ms ?? currentSpeed?.tcp_ping_ms;
    const downloadSpeed = result?.avg_download_mbps ?? currentSpeed?.avg_download_mbps;

    return (
      <div className="h-[56px] w-full flex items-center justify-between rounded-[8px] bg-success-50/60 px-2.5 py-0 hover:bg-success-100/80 hover:shadow-lg hover:shadow-success-900/10 transition-all duration-200">
        <div className="min-w-0 flex flex-1 flex-col justify-center">
          <div className="truncate text-sm font-medium leading-5">{node.name}</div>
          <div className="text-xs text-foreground-400">
            TCP {tcpPing != null ? `${tcpPing}ms` : "-"}
          </div>
        </div>
        <Chip size="sm" variant="soft" color="success" className="mx-3 h-6 shrink-0 self-center leading-none">
          已完成
        </Chip>
        <div className="shrink-0 flex items-center gap-4">
          {tcpPing !== undefined ? (
            <span className={`text-xs font-semibold ${tcpPing > 500 ? "text-danger" : tcpPing > 200 ? "text-warning" : "text-success"}`}>
              {tcpPing}ms
            </span>
          ) : (
            <Skeleton className="h-4 w-12 rounded" />
          )}
          {downloadSpeed !== undefined ? (
            <span className="text-sm font-semibold text-success whitespace-nowrap">
              {downloadSpeed.toFixed(1)} Mbps
            </span>
          ) : (
            <Skeleton className="h-4 w-16 rounded" />
          )}
        </div>
      </div>
    );
  }

  // Testing 状态 - 原有样式 + 渐进呈现
  if (status === "testing") {
    const tcpPing = currentSpeed?.tcp_ping_ms ?? result?.tcp_ping_ms;
    const downloadSpeed = currentSpeed?.avg_download_mbps;

    const isLoadingTcp = currentStage === "connecting" || currentStage === "tcp_ping";
    const isLoadingDownload = currentStage === "downloading";

    return (
      <div className="h-[56px] w-full flex items-center justify-between rounded-[8px] bg-default-50 px-2.5 py-0 hover:bg-default-100/80 hover:shadow-lg hover:shadow-default-900/10 transition-all duration-200">
        <div className="min-w-0 flex flex-1 flex-col justify-center">
          <div className="min-w-0 flex items-center gap-1.5">
            <Spinner size="sm" color="accent" className="shrink-0" />
            <span className="truncate text-sm font-medium leading-5">{node.name}</span>
          </div>
          <div className="text-xs text-foreground-400">
            TCP {tcpPing != null ? `${tcpPing}ms` : "测速中"}
          </div>
        </div>
        <div className="shrink-0 flex items-center gap-4">
          {isLoadingTcp ? (
            <Spinner size="sm" color="accent" />
          ) : tcpPing !== undefined ? (
            <span className={`text-xs font-semibold ${tcpPing > 500 ? "text-danger" : tcpPing > 200 ? "text-warning" : "text-success"}`}>
              {tcpPing}ms
            </span>
          ) : (
            <Skeleton className="h-4 w-12 rounded" />
          )}
          {isLoadingDownload ? (
            <Spinner size="sm" color="success" />
          ) : downloadSpeed !== undefined ? (
            <span className="text-sm font-semibold text-success whitespace-nowrap">
              {downloadSpeed.toFixed(1)} Mbps
            </span>
          ) : (
            <Skeleton className="h-4 w-16 rounded" />
          )}
        </div>
      </div>
    );
  }

  // Error 状态
  if (status === "error") {
    return (
      <div className="h-[56px] w-full flex items-center justify-between rounded-[8px] bg-danger-50 px-2.5 py-0 hover:bg-danger-100/80 hover:shadow-lg hover:shadow-danger-900/10 transition-all duration-200">
        <div className="min-w-0 flex flex-1 flex-col justify-center">
          <div className="truncate text-sm font-medium leading-5 text-danger">{node.name}</div>
          <div className="text-xs text-danger-400">{errorMessage || "测速失败"}</div>
        </div>
        <Chip size="sm" variant="soft" color="danger" className="h-6 shrink-0">失败</Chip>
      </div>
    );
  }

  return null;
}

export default TestingNodeRow;
