import { FormEvent } from "react";
import type { KernelListProgressEvent, KernelStatus } from "../../types/settings";
import type { KernelDownloadProgressEvent } from "../../types/speedtest";
import {
  Button,
  Chip,
  Label,
  ListBox,
  ProgressBar,
  Select,
  Surface,
} from "@heroui/react";

interface KernelSettingsCardProps {
  snapshot: KernelStatus;
  kernelVersions: string[];
  kernelTarget: string;
  setKernelTarget: (v: string) => void;
  kernelListProgress: KernelListProgressEvent | null;
  kernelDownloadProgress: KernelDownloadProgressEvent | null;
  snapshotReady: boolean;
  loadingKernel: boolean;
  loadingKernelCheck: boolean;
  isPendingKernelList: boolean;
  onSwitchKernel: (event: FormEvent) => void;
  onCheckKernelGeoipUpdates: () => void;
}

function formatTimestamp(timestamp: string): string {
  const numeric = Number(timestamp);
  if (!Number.isFinite(numeric) || numeric <= 0) {
    return "从未检查";
  }
  return new Date(numeric * 1000).toLocaleString();
}

export function KernelSettingsCard({
  snapshot,
  kernelVersions,
  kernelTarget,
  setKernelTarget,
  kernelListProgress,
  kernelDownloadProgress,
  snapshotReady,
  loadingKernel,
  loadingKernelCheck,
  isPendingKernelList,
  onSwitchKernel,
  onCheckKernelGeoipUpdates,
}: KernelSettingsCardProps) {
  const localInstalled = snapshot.local_installed_versions ?? [];
  const currentExists = snapshot.current_exists ?? false;
  const statusLabel = currentExists ? "已安装" : "未安装";
  const statusTone = currentExists ? "success" : "danger";

  return (
    <Surface variant="secondary" className="p-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="font-semibold">Mihomo 内核</h3>
        <div className="flex items-center gap-2">
          <Chip size="sm" variant="secondary">平台 {snapshot.platform}</Chip>
          <Chip size="sm" variant="secondary" color={statusTone}>{statusLabel}</Chip>
        </div>
      </div>

      <div className="mb-4 grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Surface variant="default" className="px-3 py-2">
          <p className="text-xs text-foreground-500">当前生效内核</p>
          <p className="mt-0.5 font-medium">{currentExists ? snapshot.current_version : "未安装"}</p>
        </Surface>
        <Surface variant="default" className="px-3 py-2">
          <p className="text-xs text-foreground-500">本地已安装版本数</p>
          <p className="mt-0.5 font-medium">{localInstalled.length}</p>
        </Surface>
        <Surface variant="default" className="px-3 py-2 sm:col-span-2">
          <p className="text-xs text-foreground-500">最近检查</p>
          <p className="mt-0.5 font-medium">{formatTimestamp(snapshot.last_checked_at)}</p>
        </Surface>
      </div>

      <Surface variant="default" className="mb-4 px-3 py-2">
        <p className="text-xs text-foreground-500">本地版本</p>
        <p className="mt-1 text-sm font-medium">
          {localInstalled.length > 0 ? localInstalled.join(" / ") : "无本地内核，请下载后使用测速功能"}
        </p>
      </Surface>

      {!currentExists && (
        <Surface variant="default" className="mb-4 border border-danger/40 px-3 py-2">
          <p className="text-sm font-medium text-danger">当前版本文件不存在</p>
          <p className="mt-1 text-xs text-foreground-600">
            已记录版本 {snapshot.current_version}，但本地未检测到对应内核文件。请重新下载。
          </p>
        </Surface>
      )}

      {isPendingKernelList && kernelListProgress && (
        <div className="mb-4">
          <ProgressBar value={kernelListProgress.progress} size="sm" color="accent" />
        </div>
      )}

      {loadingKernel && kernelDownloadProgress && (
        <div className="mb-4 rounded-md border border-default-200 bg-content1 px-3 py-2">
          <p className="mb-2 text-xs text-foreground-600">
            {kernelDownloadProgress.stage === "extracting" ? "正在解压内核..." : "正在下载内核..."}
          </p>
          <ProgressBar value={kernelDownloadProgress.progress} size="sm" color="accent" />
          <p className="mt-2 text-xs text-foreground-500">{kernelDownloadProgress.message}</p>
        </div>
      )}

      <form onSubmit={onSwitchKernel} className="space-y-3">
        <Select
          variant="secondary"
          placeholder="请选择内核版本"
          value={kernelTarget}
          onChange={(value) => setKernelTarget(typeof value === "string" ? value : "")}
          isDisabled={loadingKernel || isPendingKernelList || kernelVersions.length === 0}
          className="w-full"
        >
          <Label>目标版本</Label>
          <Select.Trigger>
            <Select.Value />
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox aria-label="可选内核版本">
              {kernelVersions.map((version) => (
                <ListBox.Item key={version} id={version} textValue={version}>
                  {version}
                </ListBox.Item>
              ))}
            </ListBox>
          </Select.Popover>
        </Select>

        <div className="flex flex-wrap gap-2">
          <Button
            variant="primary"
            type="submit"
            isDisabled={!snapshotReady || !kernelTarget.trim() || loadingKernel || isPendingKernelList}
            isPending={loadingKernel}
          >
            {currentExists ? "下载并切换内核" : "下载内核并启用"}
          </Button>
          <Button
            variant="outline"
            onPress={onCheckKernelGeoipUpdates}
            isDisabled={!snapshotReady || loadingKernelCheck}
            isPending={loadingKernelCheck}
          >
            手动检查内核/GeoIP更新
          </Button>
        </div>
      </form>
    </Surface>
  );
}

export default KernelSettingsCard;
