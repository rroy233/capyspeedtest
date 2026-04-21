import type { IpDatabaseStatus, GeoIpDownloadProgressEvent } from "../../types/settings";
import { Button, Surface, ProgressBar, Chip } from "@heroui/react";

interface GeoIpSettingsCardProps {
  snapshot: IpDatabaseStatus;
  geoIpProgress: GeoIpDownloadProgressEvent | null;
  snapshotReady: boolean;
  loadingIpDb: boolean;
  isDownloadingGeoIp: boolean;
  onRefreshIpDatabase: () => void;
}

function formatTimestamp(timestamp: string): string {
  const numeric = Number(timestamp);
  if (!Number.isFinite(numeric) || numeric <= 0) {
    return "从未检查";
  }
  return new Date(numeric * 1000).toLocaleString();
}

export function GeoIpSettingsCard({
  snapshot,
  geoIpProgress,
  snapshotReady,
  loadingIpDb,
  isDownloadingGeoIp,
  onRefreshIpDatabase,
}: GeoIpSettingsCardProps) {
  const currentExists = snapshot.current_exists ?? false;
  const latestVersion = snapshot.latest_version ?? snapshot.current_version;
  const statusLabel = currentExists ? "已下载" : "未下载";
  const statusTone = currentExists ? "success" : "warning";

  return (
    <Surface variant="secondary" className="p-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="font-semibold">GeoIP 数据库</h3>
        <Chip size="sm" variant="secondary" color={statusTone}>{statusLabel}</Chip>
      </div>

      <div className="mb-3 grid grid-cols-1 gap-3 sm:grid-cols-3">
        <Surface variant="default" className="px-3 py-2">
          <p className="text-xs text-foreground-500">本地版本</p>
          <p className="mt-0.5 font-medium">{currentExists ? snapshot.current_version : "未下载"}</p>
        </Surface>
        <Surface variant="default" className="px-3 py-2">
          <p className="text-xs text-foreground-500">可用最新版本</p>
          <p className="mt-0.5 font-medium">{latestVersion}</p>
        </Surface>
        <Surface variant="default" className="px-3 py-2">
          <p className="text-xs text-foreground-500">最近检查</p>
          <p className="mt-0.5 font-medium">{formatTimestamp(snapshot.last_checked_at)}</p>
        </Surface>
      </div>

      {!currentExists && (
        <Surface variant="default" className="mb-3 border border-warning/40 px-3 py-2">
          <p className="text-sm font-medium text-warning">本地未检测到 GeoIP 数据库</p>
          <p className="mt-1 text-xs text-foreground-600">请点击下方按钮下载数据库，否则地理信息会降级。</p>
        </Surface>
      )}

      {isDownloadingGeoIp && geoIpProgress && (
        <div className="mb-3">
          <ProgressBar value={geoIpProgress.progress} size="sm" color="accent" />
        </div>
      )}

      <Button
        variant="outline"
        onPress={onRefreshIpDatabase}
        isDisabled={!snapshotReady || loadingIpDb}
        isPending={loadingIpDb}
      >
        {currentExists ? "更新 GeoIP 到最新版本" : "下载 GeoIP 数据库"}
      </Button>
    </Surface>
  );
}

export default GeoIpSettingsCard;
