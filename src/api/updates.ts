import type { ClientUpdateDownloadResult, ClientUpdateStatus, KernelGeoIpCheckResult, ScheduledUpdateCheckResult, UpdateCheckProgressEvent, UpdateDownloadProgressEvent } from "../types/settings";
import { invokeTauri } from "./helpers";

export async function checkClientUpdate(currentVersion: string): Promise<ClientUpdateStatus> {
  return invokeTauri<ClientUpdateStatus>("check_client_update", { currentVersion });
}

export async function checkKernelGeoipUpdates(): Promise<KernelGeoIpCheckResult> {
  return invokeTauri<KernelGeoIpCheckResult>("check_kernel_geoip_updates");
}

export async function runScheduledUpdateChecks(
  currentClientVersion?: string
): Promise<ScheduledUpdateCheckResult> {
  return invokeTauri<ScheduledUpdateCheckResult>(
    "run_scheduled_update_checks",
    currentClientVersion ? { currentClientVersion } : undefined
  );
}

export async function downloadClientUpdate(
  version: string,
  downloadUrl: string,
  expectedSha256?: string
): Promise<ClientUpdateDownloadResult> {
  return invokeTauri<ClientUpdateDownloadResult>("download_client_update", {
    version,
    downloadUrl,
    expectedSha256,
  });
}

export async function listenUpdateCheckProgress(
  callback: (event: UpdateCheckProgressEvent) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<UpdateCheckProgressEvent>("updater://check/progress", (event) => {
    callback(event.payload);
  });
  return () => {
    void unlisten();
  };
}

export async function listenUpdateDownloadProgress(
  callback: (event: UpdateDownloadProgressEvent) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<UpdateDownloadProgressEvent>("updater://download/progress", (event) => {
    callback(event.payload);
  });
  return () => {
    void unlisten();
  };
}
