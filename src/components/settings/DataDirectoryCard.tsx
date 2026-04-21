import type { DataDirectoryInfo } from "../../types/settings";
import { Button, Surface } from "@heroui/react";

interface DataDirectoryCardProps {
  dataDirectoryInfo: DataDirectoryInfo | null;
  isDataActionPending: boolean;
  dataActionType: "open" | "export" | "clear" | null;
  onOpenDataDirectory: () => void;
  onExportUserDataArchive: () => void;
  onRequestClearUserData: () => void;
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(unitIndex === 0 ? 0 : 2)} ${units[unitIndex]}`;
}

export function DataDirectoryCard({
  dataDirectoryInfo,
  isDataActionPending,
  dataActionType,
  onOpenDataDirectory,
  onExportUserDataArchive,
  onRequestClearUserData,
}: DataDirectoryCardProps) {
  return (
    <>
      <Surface variant="secondary" className="px-3 py-2">
        <p className="text-xs text-foreground-500">目录路径</p>
        <p className="mt-1 break-all text-sm font-medium">{dataDirectoryInfo?.path ?? "加载中..."}</p>
      </Surface>

      <div className="grid grid-cols-2 gap-3">
        <Surface variant="secondary" className="px-3 py-2">
          <p className="text-xs text-foreground-500">文件总数</p>
          <p className="mt-1 font-medium">{dataDirectoryInfo?.file_count ?? "-"}</p>
        </Surface>
        <Surface variant="secondary" className="px-3 py-2">
          <p className="text-xs text-foreground-500">总占用</p>
          <p className="mt-1 font-medium">{formatBytes(dataDirectoryInfo?.total_bytes ?? 0)}</p>
        </Surface>
      </div>

      <div className="grid grid-cols-1 gap-2">
        <Button
          variant="outline"
          onPress={onOpenDataDirectory}
          isDisabled={isDataActionPending}
          isPending={dataActionType === "open"}
        >
          打开目录
        </Button>
        <Button
          variant="outline"
          onPress={onExportUserDataArchive}
          isDisabled={isDataActionPending}
          isPending={dataActionType === "export"}
        >
          导出数据包
        </Button>
        <Button
          variant="danger"
          onPress={onRequestClearUserData}
          isDisabled={isDataActionPending}
          isPending={dataActionType === "clear"}
        >
          清理历史数据
        </Button>
      </div>
    </>
  );
}

export default DataDirectoryCard;
