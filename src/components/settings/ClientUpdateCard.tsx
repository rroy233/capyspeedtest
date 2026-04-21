import type { ClientUpdateStatus, UpdateCheckProgressEvent, UpdateDownloadProgressEvent } from "../../types/settings";
import { Button, Chip, Surface, ProgressBar } from "@heroui/react";

interface ClientUpdateCardProps {
  snapshot: ClientUpdateStatus;
  updateCheckProgress: UpdateCheckProgressEvent | null;
  updateDownloadProgress: UpdateDownloadProgressEvent | null;
  snapshotReady: boolean;
  loadingUpdate: boolean;
  isDownloadingUpdate: boolean;
  updateStatusLabel: string;
  updateStatusTone: "warning" | "success" | "danger";
  onCheckUpdate: () => void;
  onDownloadUpdate: () => void;
}

export function ClientUpdateCard({
  snapshot,
  updateCheckProgress,
  updateDownloadProgress,
  snapshotReady,
  loadingUpdate,
  isDownloadingUpdate,
  updateStatusLabel,
  updateStatusTone,
  onCheckUpdate,
  onDownloadUpdate,
}: ClientUpdateCardProps) {
  return (
    <Surface variant="secondary" className="p-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="font-semibold">客户端更新</h3>
        <Chip size="sm" variant="secondary" color={updateStatusTone}>
          {updateStatusLabel}
        </Chip>
      </div>

      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Surface variant="default" className="px-3 py-2">
          <p className="text-xs text-foreground-500">当前版本</p>
          <p className="mt-0.5 font-medium">{snapshot.current_version}</p>
        </Surface>
        <Surface variant="default" className="px-3 py-2">
          <p className="text-xs text-foreground-500">最新版本</p>
          <p className="mt-0.5 font-medium">{snapshot.latest_version}</p>
        </Surface>
      </div>

      {snapshot.release_notes && (
        <Surface variant="default" className="mb-3 px-3 py-2 text-sm">
          <p className="text-xs text-foreground-500">更新说明</p>
          <p className="mt-1 whitespace-pre-wrap text-foreground-700">
            {snapshot.release_notes}
          </p>
        </Surface>
      )}

      {updateCheckProgress?.stage === "checking" && (
        <div className="mb-3">
          <ProgressBar value={updateCheckProgress.progress} size="sm" color="accent" />
        </div>
      )}

      {isDownloadingUpdate && updateDownloadProgress && (
        <div className="mb-3">
          <ProgressBar value={updateDownloadProgress.progress} size="sm" color="accent" />
        </div>
      )}

      <div className="flex flex-wrap gap-2">
        <Button
          variant="outline"
          onPress={onCheckUpdate}
          isDisabled={!snapshotReady || loadingUpdate}
          isPending={loadingUpdate && !isDownloadingUpdate}
        >
          检查更新
        </Button>
        <Button
          variant="primary"
          onPress={onDownloadUpdate}
          isDisabled={!snapshotReady || !snapshot.has_update || loadingUpdate}
          isPending={isDownloadingUpdate}
        >
          下载更新包
        </Button>
      </div>
    </Surface>
  );
}

export default ClientUpdateCard;
