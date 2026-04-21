export interface KernelStatus {
  platform: string;
  current_version: string;
  installed_versions: string[];
  current_exists?: boolean;
  local_installed_versions?: string[];
  last_checked_at: string;
}

export interface IpDatabaseStatus {
  current_version: string;
  current_exists?: boolean;
  latest_version?: string;
  last_checked_at: string;
}

export interface ClientUpdateStatus {
  current_version: string;
  latest_version: string;
  has_update: boolean;
  download_url: string;
  release_notes: string;
}

export interface UpdatePreferences {
  receive_prerelease: boolean;
}

export interface ClientUpdateDownloadResult {
  version: string;
  package_path: string;
  backup_path?: string;
  rolled_back: boolean;
}

export interface SettingsSnapshot {
  kernel: KernelStatus;
  ip_database: IpDatabaseStatus;
  client_update: ClientUpdateStatus;
  update_preferences: UpdatePreferences;
}

export interface KernelGeoIpCheckResult {
  kernel: KernelStatus;
  ip_database: IpDatabaseStatus;
}

export interface ScheduledUpdateCheckResult {
  client_update?: ClientUpdateStatus;
}

export interface GeoIpDownloadProgressEvent {
  stage: string;
  progress: number;
  message: string;
}

export interface SubscriptionFetchProgressEvent {
  stage: string;
  progress: number;
  message: string;
  nodes_count?: number;
}

export interface KernelListProgressEvent {
  stage: string;
  progress: number;
  message: string;
  versions_count?: number;
}

export interface UpdateCheckProgressEvent {
  stage: string;
  progress: number;
  message: string;
}

export interface UpdateDownloadProgressEvent {
  version: string;
  stage: string;
  progress: number;
  message: string;
}

export interface DataDirectoryInfo {
  path: string;
  logs_path: string;
  total_bytes: number;
  file_count: number;
}

export interface UserDataExportResult {
  archive_path: string;
}
