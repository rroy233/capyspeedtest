import type {
  DataDirectoryInfo,
  GeoIpDownloadProgressEvent,
  IpDatabaseStatus,
  KernelListProgressEvent,
  KernelStatus,
  SettingsSnapshot,
  UserDataExportResult,
} from "../types/settings";
import { invokeTauri } from "./helpers";

// Re-export all API functions for backward compatibility
export * from "./speedtest";
export * from "./subscription";
export * from "./updates";
export * from "./database";

// Settings-related API functions
export async function getSettingsSnapshot(currentClientVersion?: string): Promise<SettingsSnapshot> {
  return invokeTauri<SettingsSnapshot>(
    "get_settings_snapshot",
    currentClientVersion ? { currentClientVersion } : undefined
  );
}

export async function listKernelVersions(platform?: string): Promise<string[]> {
  return invokeTauri<string[]>("list_kernel_versions_cmd", { platform });
}

export async function refreshKernelVersions(platform?: string): Promise<string[]> {
  return invokeTauri<string[]>("list_kernel_versions_cmd", {
    platform,
    forceRefresh: true,
  });
}

export async function selectKernelVersion(version: string): Promise<KernelStatus> {
  return invokeTauri<KernelStatus>("select_kernel_version", { version });
}

export async function refreshIpDatabase(): Promise<IpDatabaseStatus> {
  return invokeTauri<IpDatabaseStatus>("refresh_ip_database");
}

export async function getDataDirectoryInfo(): Promise<DataDirectoryInfo> {
  return invokeTauri<DataDirectoryInfo>("get_data_directory_info");
}

export async function openDataDirectory(): Promise<void> {
  return invokeTauri<void>("open_data_directory");
}

export async function exportUserDataArchive(): Promise<UserDataExportResult> {
  return invokeTauri<UserDataExportResult>("export_user_data_archive");
}

export async function clearUserData(): Promise<void> {
  return invokeTauri<void>("clear_user_data");
}

export async function prepareAppExit(): Promise<void> {
  return invokeTauri<void>("prepare_app_exit");
}

export async function listenGeoIpDownloadProgress(
  callback: (event: GeoIpDownloadProgressEvent) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<GeoIpDownloadProgressEvent>("geoip://download/progress", (event) => {
    callback(event.payload);
  });
  return () => {
    void unlisten();
  };
}

export async function listenKernelListProgress(
  callback: (event: KernelListProgressEvent) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<KernelListProgressEvent>("kernel://list/progress", (event) => {
    callback(event.payload);
  });
  return () => {
    void unlisten();
  };
}
