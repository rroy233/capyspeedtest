import type { SettingsSnapshot } from "../../types/settings";
import { Card, CardContent, Chip, Surface } from "@heroui/react";

interface SettingsHeaderProps {
  snapshot: SettingsSnapshot;
  snapshotReady: boolean;
  updateStatusLabel: string;
  updateStatusTone: "warning" | "success" | "danger";
}

export function SettingsHeader({ snapshot, snapshotReady, updateStatusLabel, updateStatusTone }: SettingsHeaderProps) {
  const kernelText = snapshot.kernel.current_exists ? snapshot.kernel.current_version : "未安装";
  const geoIpText = snapshot.ip_database.current_exists ? snapshot.ip_database.current_version : "未下载";

  return (
    <Card className="overflow-hidden border-none shadow-xl">
      <CardContent className="p-0">
        <div className="relative overflow-hidden bg-gradient-to-r from-default-100 via-background to-default-50 px-5 py-5 lg:px-6">
          <div className="absolute -left-20 -top-20 h-60 w-60 rounded-full bg-primary/10 blur-3xl" />
          <div className="absolute -bottom-16 -right-14 h-44 w-44 rounded-full bg-warning/10 blur-3xl" />

          <div className="relative flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
            <div>
              <h1 className="text-2xl font-bold tracking-tight">设置中心</h1>
            </div>

            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
              <Surface variant="secondary" className="px-3 py-2 backdrop-blur">
                <p className="text-[11px] text-foreground-500">内核版本</p>
                <p className="mt-1 text-sm font-semibold">{kernelText}</p>
              </Surface>
              <Surface variant="secondary" className="px-3 py-2 backdrop-blur">
                <p className="text-[11px] text-foreground-500">GeoIP</p>
                <p className="mt-1 text-sm font-semibold">{geoIpText}</p>
              </Surface>
              <Surface variant="secondary" className="px-3 py-2 backdrop-blur">
                <p className="text-[11px] text-foreground-500">客户端</p>
                <p className="mt-1 text-sm font-semibold">{snapshot.client_update.current_version}</p>
              </Surface>
              <Surface variant="secondary" className="px-3 py-2 backdrop-blur">
                <p className="text-[11px] text-foreground-500">更新状态</p>
                <p className="mt-1 text-sm font-semibold">{updateStatusLabel}</p>
              </Surface>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

export default SettingsHeader;
