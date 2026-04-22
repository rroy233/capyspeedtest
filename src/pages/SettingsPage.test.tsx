import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import SettingsPage from "./SettingsPage";
import { AlertProvider } from "../contexts/AlertContext";

vi.mock("../contexts/ThemeContext", () => ({
  useTheme: () => ({
    theme: "light",
    setTheme: vi.fn(),
    resolvedTheme: "light" as "light" | "dark",
  }),
}));

vi.mock("../api/settings", () => ({
  listenGeoIpDownloadProgress: vi.fn(async () => () => {}),
  listenKernelDownloadProgress: vi.fn(async () => () => {}),
  listenKernelListProgress: vi.fn(async () => () => {}),
  listenUpdateCheckProgress: vi.fn(async () => () => {}),
  listenUpdateDownloadProgress: vi.fn(async () => () => {}),
  getSettingsSnapshot: vi.fn(async () => ({
    kernel: {
      platform: "windows",
      current_version: "v1.19.1",
      installed_versions: ["v1.19.1", "v1.19.0"],
      last_checked_at: "1710000000",
    },
    ip_database: {
      current_version: "2026.04.15",
      last_checked_at: "1710000000",
    },
    client_update: {
      current_version: "0.1.0",
      latest_version: "0.2.0",
      has_update: true,
      download_url: "https://example.test/release",
      release_notes: "mock release note",
    },
    update_preferences: {
      receive_prerelease: false,
    },
  })),
  getDataDirectoryInfo: vi.fn(async () => ({
    path: "C:/Users/test/AppData/Local/capyspeedtest",
    logs_path: "C:/Users/test/AppData/Local/capyspeedtest/logs",
    total_bytes: 1024,
    file_count: 12,
  })),
  openDataDirectory: vi.fn(async () => {}),
  exportUserDataArchive: vi.fn(async () => ({
    archive_path: "C:/Users/test/AppData/Local/capyspeedtest/exports/data.zip",
  })),
  clearUserData: vi.fn(async () => {}),
  listKernelVersions: vi.fn(async () => ["v1.19.1", "v1.19.0"]),
  selectKernelVersion: vi.fn(async (version: string) => ({
    platform: "windows",
    current_version: version,
    installed_versions: ["v1.19.1", "v1.19.0"],
    last_checked_at: "1710000100",
  })),
  refreshIpDatabase: vi.fn(async () => ({
    current_version: "2026.04.15",
    last_checked_at: "1710000200",
  })),
  checkClientUpdate: vi.fn(async () => ({
    current_version: "0.1.0",
    latest_version: "0.2.0",
    has_update: true,
    download_url: "https://example.test/release",
    release_notes: "mock release note",
  })),
  setUpdatePreferences: vi.fn(async (receivePrerelease: boolean) => ({
    receive_prerelease: receivePrerelease,
  })),
  downloadClientUpdate: vi.fn(async () => ({
    version: "0.2.0",
    package_path: "mock/updates/client-0.2.0.pkg",
    rolled_back: false,
  })),
  checkKernelGeoipUpdates: vi.fn(async () => ({
    kernel: {
      platform: "windows",
      current_version: "v1.19.1",
      installed_versions: ["v1.19.1", "v1.19.0"],
      last_checked_at: "1710000300",
    },
    ip_database: {
      current_version: "2026.04.15",
      last_checked_at: "1710000300",
    },
  })),
}));

vi.mock("../api/subscription", () => ({
  parseSubscriptionNodes: vi.fn(async () => [
    {
      name: "香港-HK-01",
      protocol: "vless",
      country: "HK",
      raw: "vless://token@example.com:443?security=tls#香港-HK-01",
    },
  ]),
  fetchSubscriptionNodesFromUrl: vi.fn(async () => []),
}));

describe("SettingsPage", () => {
  it("应展示设置快照并支持更新安装", async () => {
    render(
      <AlertProvider>
        <SettingsPage />
      </AlertProvider>
    );

    await waitFor(() => {
      expect(screen.getAllByText("v1.19.1").length).toBeGreaterThan(0);
    });

    fireEvent.click(await screen.findByRole("button", { name: /一键安装并重启/ }));

    await waitFor(() => {
      expect(screen.getByText(/应用即将自动重启并应用新版本/)).toBeInTheDocument();
    });
  });
});
