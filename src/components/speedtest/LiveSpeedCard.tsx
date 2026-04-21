import { useMemo } from "react";
import type { SpeedTestProgressEvent, NodeTestState } from "../../types/speedtest";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Chip,
  ProgressBar,
} from "@heroui/react";
import { NodeListItem } from "./NodeListItem";

interface LiveSpeedCardProps {
  progressEvent: SpeedTestProgressEvent | null;
  running: boolean;
  nodeStates: Map<string, NodeTestState>;
}

export function LiveSpeedCard({ progressEvent, running, nodeStates }: LiveSpeedCardProps) {
  const progressPercent = useMemo(() => {
    if (!progressEvent || progressEvent.total <= 0) return 0;
    return Math.round((progressEvent.completed / progressEvent.total) * 100);
  }, [progressEvent]);

  const sortedNodeEntries = useMemo(() => {
    const entries = Array.from(nodeStates.entries());
    return entries.sort(([nameA, stateA], [nameB, stateB]) => {
      const aScore = stateA.status === "testing" ? 0 : stateA.status === "pending" ? 1 : 2;
      const bScore = stateB.status === "testing" ? 0 : stateB.status === "pending" ? 1 : 2;
      if (aScore !== bScore) return aScore - bScore;
      const aIdx = parseInt(nameA.replace("node-", ""), 10);
      const bIdx = parseInt(nameB.replace("node-", ""), 10);
      return aIdx - bIdx;
    });
  }, [nodeStates]);

  return (
    <Card className="h-[480px]">
      <CardHeader>
        <div className="flex items-center gap-2">
          <CardTitle>实时测速</CardTitle>
          {progressEvent && (
            <Chip size="sm" variant="soft" color="warning">
              进行中
            </Chip>
          )}
          {progressEvent && (
            <Chip size="sm" variant="soft" color="accent">
              {progressEvent.completed}/{progressEvent.total}
            </Chip>
          )}
          {nodeStates.size > 0 && (
            <Chip variant="soft" color="success" size="sm">
              {nodeStates.size} 个节点
            </Chip>
          )}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 h-[calc(100%-60px)]">
        {progressEvent && (
          <ProgressBar
            value={progressPercent}
            color={progressEvent.stage === "error" ? "danger" : progressEvent.stage === "completed" ? "success" : "default"}
            aria-label="测速进度"
            size="md"
          />
        )}

        {progressEvent && (
          <div className="flex flex-col gap-3">
            {/* 实时速度显示（仅在下载/上传阶段显示） */}
            {progressEvent.stage === "downloading" && progressEvent.avg_download_mbps !== undefined && (
              <div className="flex items-center justify-center gap-6 py-2 px-4 rounded-lg bg-success-50 dark:bg-success-900/20 border border-success-200 dark:border-success-800">
                <div className="text-center">
                  <p className="text-xs text-success-600 dark:text-success-400">当前下载</p>
                  <p className="text-2xl font-bold text-success">{progressEvent.avg_download_mbps.toFixed(1)}</p>
                  <p className="text-xs text-success-600 dark:text-success-400">Mbps</p>
                </div>
                {progressEvent.max_download_mbps !== undefined && progressEvent.max_download_mbps > 0 && (
                  <div className="text-center">
                    <p className="text-xs text-success-600 dark:text-success-400">峰值</p>
                    <p className="text-lg font-semibold text-success">{progressEvent.max_download_mbps.toFixed(1)}</p>
                    <p className="text-xs text-success-600 dark:text-success-400">Mbps</p>
                  </div>
                )}
              </div>
            )}
            {progressEvent.stage === "uploading" && progressEvent.avg_upload_mbps !== undefined && (
              <div className="flex items-center justify-center gap-6 py-2 px-4 rounded-lg bg-primary-50 dark:bg-primary-900/20 border border-primary-200 dark:border-primary-800">
                <div className="text-center">
                  <p className="text-xs text-primary-600 dark:text-primary-400">当前上传</p>
                  <p className="text-2xl font-bold text-primary">{progressEvent.avg_upload_mbps.toFixed(1)}</p>
                  <p className="text-xs text-primary-600 dark:text-primary-400">Mbps</p>
                </div>
                {progressEvent.max_upload_mbps !== undefined && progressEvent.max_upload_mbps > 0 && (
                  <div className="text-center">
                    <p className="text-xs text-primary-600 dark:text-primary-400">峰值</p>
                    <p className="text-lg font-semibold text-primary">{progressEvent.max_upload_mbps.toFixed(1)}</p>
                    <p className="text-xs text-primary-600 dark:text-primary-400">Mbps</p>
                  </div>
                )}
              </div>
            )}

            <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
              <div className="p-3 rounded-lg bg-default-100">
                <span className="text-xs text-foreground-500 block">当前状态</span>
                <div className="mt-1">
                  {progressEvent.stage === "connecting" && <span className="text-sm font-medium text-warning">启动代理...</span>}
                  {progressEvent.stage === "tcp_ping" && <span className="text-sm font-medium text-primary">TCP 延迟测试...</span>}
                  {progressEvent.stage === "site_ping" && <span className="text-sm font-medium text-primary">网页延迟测试...</span>}
                  {progressEvent.stage === "downloading" && <span className="text-sm font-medium text-success">下载测速中...</span>}
                  {progressEvent.stage === "uploading" && <span className="text-sm font-medium text-secondary">上传测速中...</span>}
                  {progressEvent.stage === "completed" && <span className="text-sm font-medium text-success">测速完成</span>}
                  {progressEvent.stage === "error" && <span className="text-sm font-medium text-danger">测速失败</span>}
                </div>
              </div>
              <div className="p-3 rounded-lg bg-default-100">
                <span className="text-xs text-foreground-500 block">进度</span>
                <p className="font-semibold mt-1">
                  {progressEvent.completed} / {progressEvent.total}
                </p>
              </div>
              <div className="p-3 rounded-lg bg-default-100 col-span-2">
                <span className="text-xs text-foreground-500 block">当前节点</span>
                <p className="font-medium text-sm truncate mt-1">{progressEvent.current_node || "-"}</p>
              </div>
            </div>
          </div>
        )}

        <div className="flex-1 overflow-y-auto pr-1 flex flex-col gap-3">
          {sortedNodeEntries.map(([name, state]) => (
            <NodeListItem key={name} name={name} state={state} isTesting={state.status === "testing"} />
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

export default LiveSpeedCard;
