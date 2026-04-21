import { FormEvent, useEffect, useRef, useState } from "react";
import {
  clearUserData,
  checkKernelGeoipUpdates,
  checkClientUpdate,
  downloadClientUpdate,
  exportUserDataArchive,
  getDataDirectoryInfo,
  getSettingsSnapshot,
  listKernelVersions,
  listenGeoIpDownloadProgress,
  listenKernelDownloadProgress,
  listenKernelListProgress,
  listenUpdateCheckProgress,
  listenUpdateDownloadProgress,
  openDataDirectory,
  selectKernelVersion,
} from "../../api/settings";
import { parseSubscriptionNodes } from "../../api/subscription";
import { useTheme } from "../../contexts/ThemeContext";
import { useAlert } from "../../contexts/AlertContext";
import { getAppVersion, getInjectedAppVersion } from "../../utils/appVersion";
import type {
  DataDirectoryInfo,
  GeoIpDownloadProgressEvent,
  KernelListProgressEvent,
  SettingsSnapshot,
  UpdateCheckProgressEvent,
  UpdateDownloadProgressEvent,
} from "../../types/settings";
import type { KernelDownloadProgressEvent, NodeInfo } from "../../types/speedtest";
import { SettingsHeader } from "./SettingsHeader";
import { KernelSettingsCard } from "./KernelSettingsCard";
import { GeoIpSettingsCard } from "./GeoIpSettingsCard";
import { ClientUpdateCard } from "./ClientUpdateCard";
import { AppearanceSettingsCard } from "./AppearanceSettingsCard";
import { DataDirectoryCard } from "./DataDirectoryCard";
import { SubscriptionParserCard } from "./SubscriptionParserCard";
import { ClearDataConfirmDialog } from "./ClearDataConfirmDialog";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, ProgressBar } from "@heroui/react";

const defaultSubscription = [
  "vless://token@example.com:443?security=tls#香港-HK-01",
  "trojan://password@example.com:443#日本-JP-01",
].join("\n");

const initialSnapshot: SettingsSnapshot = {
  kernel: {
    platform: "-",
    current_version: "-",
    installed_versions: [],
    current_exists: false,
    local_installed_versions: [],
    last_checked_at: "0",
  },
  ip_database: {
    current_version: "-",
    current_exists: false,
    latest_version: "-",
    last_checked_at: "0",
  },
  client_update: {
    current_version: getInjectedAppVersion(),
    latest_version: "-",
    has_update: false,
    download_url: "",
    release_notes: "",
  },
};

function toErrorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}

type DataActionType = "open" | "export" | "clear" | null;

export default function SettingsPage() {
  const { resolvedTheme, setTheme } = useTheme();
  const [snapshot, setSnapshot] = useState<SettingsSnapshot>(initialSnapshot);
  const [snapshotReady, setSnapshotReady] = useState(false);
  const [kernelVersions, setKernelVersions] = useState<string[]>([]);
  const [kernelTarget, setKernelTarget] = useState("");
  const [dataDirectoryInfo, setDataDirectoryInfo] = useState<DataDirectoryInfo | null>(null);
  const [subscriptionText, setSubscriptionText] = useState(defaultSubscription);
  const [nameRegex, setNameRegex] = useState("");
  const [countryFilter, setCountryFilter] = useState("");
  const [limit, setLimit] = useState("20");
  const [parsedNodes, setParsedNodes] = useState<NodeInfo[]>([]);
  const [loadingKernel, setLoadingKernel] = useState(false);
  const [loadingKernelCheck, setLoadingKernelCheck] = useState(false);
  const [loadingIpDb, setLoadingIpDb] = useState(false);
  const [loadingUpdate, setLoadingUpdate] = useState(false);
  const [loadingParse, setLoadingParse] = useState(false);
  const [dataActionType, setDataActionType] = useState<DataActionType>(null);
  const [showClearConfirm, setShowClearConfirm] = useState(false);

  const [geoIpProgress, setGeoIpProgress] = useState<GeoIpDownloadProgressEvent | null>(null);
  const [kernelListProgress, setKernelListProgress] = useState<KernelListProgressEvent | null>(null);
  const [kernelDownloadProgress, setKernelDownloadProgress] = useState<KernelDownloadProgressEvent | null>(null);
  const [updateCheckProgress, setUpdateCheckProgress] = useState<UpdateCheckProgressEvent | null>(null);
  const [updateDownloadProgress, setUpdateDownloadProgress] = useState<UpdateDownloadProgressEvent | null>(null);
  const [updateCheckFailed, setUpdateCheckFailed] = useState(false);

  const { showAlert } = useAlert();
  const ipDbAlertIdRef = useRef<string | null>(null);
  const kernelDownloadAlertIdRef = useRef<string | null>(null);
  const updateCheckAlertIdRef = useRef<string | null>(null);
  const updateDownloadAlertIdRef = useRef<string | null>(null);

  useEffect(() => {
    let mounted = true;
    let geoIpDisposer: (() => void) | null = null;
    let kernelListDisposer: (() => void) | null = null;
    let kernelDownloadDisposer: (() => void) | null = null;
    let updateCheckDisposer: (() => void) | null = null;
    let updateDownloadDisposer: (() => void) | null = null;

    async function bindListeners() {
      geoIpDisposer = await listenGeoIpDownloadProgress((event) => {
        if (!mounted) return;
        setGeoIpProgress(event);

        if (event.stage === "downloading") {
          ipDbAlertIdRef.current = ipDbAlertIdRef.current || "geoip-download-alert";
          showAlert({
            id: ipDbAlertIdRef.current,
            title: "正在下载 GeoIP 数据库",
            description: (
              <div className="mt-1 flex flex-col gap-2">
                <div className="text-sm whitespace-pre-wrap break-all">{event.message}</div>
                <ProgressBar value={event.progress} size="sm" color="accent" />
              </div>
            ),
            status: "accent",
            timeout: 0,
          });
        } else if (event.stage === "completed") {
          showAlert({
            id: ipDbAlertIdRef.current || "geoip-download-alert",
            title: "GeoIP 数据库下载完成",
            description: event.message,
            status: "success",
            timeout: 3200,
          });
          ipDbAlertIdRef.current = null;
        } else if (event.stage === "error") {
          showAlert({
            id: ipDbAlertIdRef.current || "geoip-download-alert",
            title: "GeoIP 数据库下载失败",
            description: event.message,
            status: "danger",
            timeout: 4800,
          });
          ipDbAlertIdRef.current = null;
        }
      });

      kernelListDisposer = await listenKernelListProgress((event) => {
        if (!mounted) return;
        setKernelListProgress(event);
      });

      kernelDownloadDisposer = await listenKernelDownloadProgress((event) => {
        if (!mounted) return;
        setKernelDownloadProgress(event);

        if (event.stage === "downloading" || event.stage === "extracting") {
          kernelDownloadAlertIdRef.current = kernelDownloadAlertIdRef.current || "kernel-download-alert";
          showAlert({
            id: kernelDownloadAlertIdRef.current,
            title: `正在下载内核 ${event.version}`,
            description: (
              <div className="mt-1 flex flex-col gap-2">
                <div className="text-sm whitespace-pre-wrap break-all">{event.message}</div>
                <ProgressBar value={event.progress} size="sm" color="accent" />
              </div>
            ),
            status: "accent",
            timeout: 0,
          });
        } else if (event.stage === "completed") {
          showAlert({
            id: kernelDownloadAlertIdRef.current || "kernel-download-alert",
            title: "内核下载完成",
            description: event.message,
            status: "success",
            timeout: 3200,
          });
          kernelDownloadAlertIdRef.current = null;
        } else if (event.stage === "error") {
          showAlert({
            id: kernelDownloadAlertIdRef.current || "kernel-download-alert",
            title: "内核下载失败",
            description: event.message,
            status: "danger",
            timeout: 4800,
          });
          kernelDownloadAlertIdRef.current = null;
        }
      });

      updateCheckDisposer = await listenUpdateCheckProgress((event) => {
        if (!mounted) return;
        setUpdateCheckProgress(event);

        if (event.stage === "checking") {
          updateCheckAlertIdRef.current = updateCheckAlertIdRef.current || "update-check-alert";
          showAlert({
            id: updateCheckAlertIdRef.current,
            title: "正在检查更新",
            description: (
              <div className="mt-1 flex flex-col gap-2">
                <div className="text-sm whitespace-pre-wrap break-all">{event.message}</div>
                <ProgressBar value={event.progress} size="sm" color="accent" />
              </div>
            ),
            status: "accent",
            timeout: 0,
          });
        } else if (event.stage === "completed") {
          showAlert({
            id: updateCheckAlertIdRef.current || "update-check-alert",
            title: "检查更新完成",
            description: event.message,
            status: "success",
            timeout: 3200,
          });
          updateCheckAlertIdRef.current = null;
        } else if (event.stage === "error") {
          showAlert({
            id: updateCheckAlertIdRef.current || "update-check-alert",
            title: "检查更新失败",
            description: event.message,
            status: "danger",
            timeout: 4800,
          });
          updateCheckAlertIdRef.current = null;
        }
      });

      updateDownloadDisposer = await listenUpdateDownloadProgress((event) => {
        if (!mounted) return;
        setUpdateDownloadProgress(event);

        if (event.stage === "downloading") {
          updateDownloadAlertIdRef.current = updateDownloadAlertIdRef.current || "update-download-alert";
          showAlert({
            id: updateDownloadAlertIdRef.current,
            title: `正在下载更新包 ${event.version}`,
            description: (
              <div className="mt-1 flex flex-col gap-2">
                <div className="text-sm whitespace-pre-wrap break-all">{event.message}</div>
                <ProgressBar value={event.progress} size="sm" color="accent" />
              </div>
            ),
            status: "accent",
            timeout: 0,
          });
        } else if (event.stage === "verifying") {
          showAlert({
            id: updateDownloadAlertIdRef.current || "update-download-alert",
            title: "正在验证更新包",
            description: event.message,
            status: "accent",
            timeout: 0,
          });
        } else if (event.stage === "completed") {
          showAlert({
            id: updateDownloadAlertIdRef.current || "update-download-alert",
            title: "更新包下载完成",
            description: event.message,
            status: "success",
            timeout: 3200,
          });
          updateDownloadAlertIdRef.current = null;
        } else if (event.stage === "error") {
          showAlert({
            id: updateDownloadAlertIdRef.current || "update-download-alert",
            title: "更新包下载失败",
            description: event.message,
            status: "danger",
            timeout: 4800,
          });
          updateDownloadAlertIdRef.current = null;
        }
      });
    }

    void bindListeners();

    return () => {
      mounted = false;
      if (geoIpDisposer) geoIpDisposer();
      if (kernelListDisposer) kernelListDisposer();
      if (kernelDownloadDisposer) kernelDownloadDisposer();
      if (updateCheckDisposer) updateCheckDisposer();
      if (updateDownloadDisposer) updateDownloadDisposer();
    };
  }, [showAlert]);

  useEffect(() => {
    async function bootstrap() {
      try {
        const appVersion = await getAppVersion();
        const [loadedSnapshot, directoryInfo] = await Promise.all([
          getSettingsSnapshot(appVersion),
          getDataDirectoryInfo(),
        ]);

        setSnapshot(loadedSnapshot);
        setDataDirectoryInfo(directoryInfo);

        const cachedVersions =
          loadedSnapshot.kernel.installed_versions.length > 0
            ? loadedSnapshot.kernel.installed_versions
            : await listKernelVersions(loadedSnapshot.kernel.platform);

        setKernelVersions(cachedVersions);
        setKernelTarget(loadedSnapshot.kernel.current_version);
      } catch (error) {
        showAlert({
          title: "加载设置失败",
          description: toErrorMessage(error, "无法获取设置快照"),
          status: "danger",
        });
      } finally {
        setSnapshotReady(true);
      }
    }

    void bootstrap();
  }, [showAlert]);

  async function onSwitchKernel(event: FormEvent) {
    event.preventDefault();
    if (!kernelTarget.trim()) {
      showAlert({ title: "请选择内核版本", status: "warning" });
      return;
    }

    setLoadingKernel(true);
    setKernelDownloadProgress(null);
    kernelDownloadAlertIdRef.current = null;
    try {
      const status = await selectKernelVersion(kernelTarget.trim());
      setSnapshot((prev) => ({ ...prev, kernel: status }));
      showAlert({
        title: "内核切换成功",
        description: `当前已切换到 ${status.current_version}`,
        status: "success",
      });
    } catch (error) {
      showAlert({
        title: "内核切换失败",
        description: toErrorMessage(error, "请稍后重试"),
        status: "danger",
      });
    } finally {
      setLoadingKernel(false);
      setKernelDownloadProgress(null);
    }
  }

  async function onRefreshIpDatabase() {
    setLoadingIpDb(true);
    setGeoIpProgress(null);
    ipDbAlertIdRef.current = null;

    try {
      const { refreshIpDatabase } = await import("../../api/settings");
      const status = await refreshIpDatabase();
      setSnapshot((prev) => ({ ...prev, ip_database: status }));
    } catch (error) {
      showAlert({
        title: "刷新 GeoIP 失败",
        description: toErrorMessage(error, "请检查网络后重试"),
        status: "danger",
      });
    } finally {
      setLoadingIpDb(false);
      setGeoIpProgress(null);
    }
  }

  async function onParseNodes(event: FormEvent) {
    event.preventDefault();
    setLoadingParse(true);

    try {
      const countries = countryFilter
        .split(",")
        .map((item) => item.trim().toUpperCase())
        .filter((item) => item.length > 0);
      const numericLimit = Number(limit);
      const nodes = await parseSubscriptionNodes(subscriptionText, {
        name_regex: nameRegex.trim() || undefined,
        countries: countries.length > 0 ? countries : undefined,
        limit: Number.isFinite(numericLimit) && numericLimit > 0 ? numericLimit : undefined,
      });

      setParsedNodes(nodes);
      showAlert({
        title: "节点解析完成",
        description: `本次共筛选 ${nodes.length} 个节点`,
        status: "success",
      });
    } catch (error) {
      setParsedNodes([]);
      showAlert({
        title: "解析失败",
        description: toErrorMessage(error, "请检查订阅内容或过滤条件"),
        status: "danger",
        timeout: 5200,
      });
    } finally {
      setLoadingParse(false);
    }
  }

  async function onCheckUpdate() {
    setLoadingUpdate(true);
    updateCheckAlertIdRef.current = null;
    setUpdateCheckFailed(false);

    try {
      const status = await checkClientUpdate(snapshot.client_update.current_version);
      setSnapshot((prev) => ({ ...prev, client_update: status }));
      setUpdateCheckFailed(false);

      if (status.has_update) {
        showAlert({
          title: `发现新版本 ${status.latest_version}`,
          description: "可以直接在当前页面一键安装并重启",
          status: "warning",
          timeout: 4200,
        });
      } else {
        showAlert({
          title: "当前已是最新版本",
          description: status.current_version,
          status: "success",
        });
      }
    } catch (error) {
      setUpdateCheckFailed(true);
      setSnapshot((prev) => ({
        ...prev,
        client_update: {
          ...prev.client_update,
          latest_version: "-",
          has_update: false,
          download_url: "",
          release_notes: "",
        },
      }));
      showAlert({
        title: "检查更新失败",
        description: toErrorMessage(error, "请稍后重试"),
        status: "danger",
      });
    } finally {
      setLoadingUpdate(false);
    }
  }

  async function onDownloadUpdate() {
    if (!snapshot.client_update.has_update) {
      showAlert({ title: "当前没有可下载更新", status: "warning" });
      return;
    }

    setLoadingUpdate(true);
    updateDownloadAlertIdRef.current = null;

    try {
      await downloadClientUpdate(snapshot.client_update.latest_version);
      showAlert({
        title: "更新安装完成",
        description: "应用即将自动重启并应用新版本",
        status: "success",
        timeout: 5000,
      });
    } catch (error) {
      showAlert({
        title: "下载更新包失败",
        description: toErrorMessage(error, "请稍后重试"),
        status: "danger",
      });
    } finally {
      setLoadingUpdate(false);
    }
  }

  async function onCheckKernelGeoipUpdates() {
    setLoadingKernelCheck(true);

    try {
      const result = await checkKernelGeoipUpdates();
      setSnapshot((prev) => ({
        ...prev,
        kernel: result.kernel,
        ip_database: result.ip_database,
      }));
      setKernelVersions(result.kernel.installed_versions);
      showAlert({
        title: "检查完成",
        description: `内核 ${result.kernel.current_version} · GeoIP ${result.ip_database.current_version}`,
        status: "success",
      });
    } catch (error) {
      showAlert({
        title: "检查内核/GeoIP失败",
        description: toErrorMessage(error, "请检查网络连接"),
        status: "danger",
      });
    } finally {
      setLoadingKernelCheck(false);
    }
  }

  async function onOpenDataDirectory() {
    setDataActionType("open");

    try {
      await openDataDirectory();
      showAlert({ title: "已打开用户数据目录", status: "success" });
    } catch (error) {
      showAlert({
        title: "打开目录失败",
        description: toErrorMessage(error, "请手动检查系统权限"),
        status: "danger",
      });
    } finally {
      setDataActionType(null);
    }
  }

  async function onExportUserDataArchive() {
    setDataActionType("export");

    try {
      const result = await exportUserDataArchive();
      showAlert({
        title: "数据包导出完成",
        description: result.archive_path,
        status: "success",
        timeout: 5000,
      });
      const info = await getDataDirectoryInfo();
      setDataDirectoryInfo(info);
    } catch (error) {
      showAlert({
        title: "导出失败",
        description: toErrorMessage(error, "请稍后重试"),
        status: "danger",
      });
    } finally {
      setDataActionType(null);
    }
  }

  async function onConfirmClearUserData() {
    setDataActionType("clear");

    try {
      await clearUserData();
      const info = await getDataDirectoryInfo();
      setDataDirectoryInfo(info);
      showAlert({
        title: "用户数据已清理",
        description: "已保留 kernels 与 GeoIP 数据",
        status: "success",
      });
      setShowClearConfirm(false);
    } catch (error) {
      showAlert({
        title: "清理失败",
        description: toErrorMessage(error, "请稍后重试"),
        status: "danger",
      });
    } finally {
      setDataActionType(null);
    }
  }

  const isPendingKernelList = kernelListProgress?.stage === "fetching";
  const isDownloadingGeoIp = loadingIpDb || geoIpProgress?.stage === "downloading";
  const isDownloadingUpdate =
    loadingUpdate ||
    updateDownloadProgress?.stage === "downloading" ||
    updateDownloadProgress?.stage === "verifying";
  const isDataActionPending = dataActionType !== null;

  const updateNeverChecked = snapshot.client_update.latest_version === "-";
  const updateStatusLabel = updateCheckFailed
    ? "检查失败"
    : updateNeverChecked
      ? "未检查"
      : snapshot.client_update.has_update
        ? "有可用更新"
        : "已是最新版本";
  const updateStatusTone = updateCheckFailed
    ? "danger"
    : snapshot.client_update.has_update || updateNeverChecked
      ? "warning"
      : "success";

  return (
    <>
      <div className="flex flex-col gap-5">
        <SettingsHeader
          snapshot={snapshot}
          snapshotReady={snapshotReady}
          updateStatusLabel={updateStatusLabel}
          updateStatusTone={updateStatusTone}
        />

        <div className="grid grid-cols-1 gap-5 xl:grid-cols-12">
          <div className="flex flex-col gap-5 xl:col-span-8">
            <Card>
              <CardHeader>
                <div className="flex w-full items-center justify-between gap-4">
                  <div>
                    <CardTitle>系统维护</CardTitle>
                    <CardDescription>版本管理、GeoIP 数据刷新与客户端更新全部集中处理。</CardDescription>
                  </div>
                  <div className={snapshotReady ? "" : "opacity-50"}>
                    <span className={`text-sm font-medium ${snapshotReady ? "text-success" : "text-warning"}`}>
                      {snapshotReady ? "已就绪" : "加载中"}
                    </span>
                  </div>
                </div>
              </CardHeader>

              <CardContent className="space-y-5">
                <KernelSettingsCard
                  snapshot={snapshot.kernel}
                  kernelVersions={kernelVersions}
                  kernelTarget={kernelTarget}
                  setKernelTarget={setKernelTarget}
                  kernelListProgress={kernelListProgress}
                  kernelDownloadProgress={kernelDownloadProgress}
                  snapshotReady={snapshotReady}
                  loadingKernel={loadingKernel}
                  loadingKernelCheck={loadingKernelCheck}
                  isPendingKernelList={isPendingKernelList}
                  onSwitchKernel={onSwitchKernel}
                  onCheckKernelGeoipUpdates={onCheckKernelGeoipUpdates}
                />

                <GeoIpSettingsCard
                  snapshot={snapshot.ip_database}
                  geoIpProgress={geoIpProgress}
                  snapshotReady={snapshotReady}
                  loadingIpDb={loadingIpDb}
                  isDownloadingGeoIp={isDownloadingGeoIp}
                  onRefreshIpDatabase={onRefreshIpDatabase}
                />

                <ClientUpdateCard
                  snapshot={snapshot.client_update}
                  updateCheckProgress={updateCheckProgress}
                  updateDownloadProgress={updateDownloadProgress}
                  snapshotReady={snapshotReady}
                  loadingUpdate={loadingUpdate}
                  isDownloadingUpdate={isDownloadingUpdate}
                  updateStatusLabel={updateStatusLabel}
                  updateStatusTone={updateStatusTone}
                  onCheckUpdate={onCheckUpdate}
                  onDownloadUpdate={onDownloadUpdate}
                />
              </CardContent>
            </Card>
          </div>

          <div className="flex flex-col gap-5 xl:col-span-4">
            {/* <Card>
              <CardHeader>
                <CardTitle>外观设置</CardTitle>
              </CardHeader>
              <CardContent>
                <AppearanceSettingsCard
                  resolvedTheme={resolvedTheme}
                  setTheme={setTheme}
                />
              </CardContent>
            </Card> */}

            <Card>
              <CardHeader>
                <CardTitle>用户数据目录</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <DataDirectoryCard
                  dataDirectoryInfo={dataDirectoryInfo}
                  isDataActionPending={isDataActionPending}
                  dataActionType={dataActionType}
                  onOpenDataDirectory={onOpenDataDirectory}
                  onExportUserDataArchive={onExportUserDataArchive}
                  onRequestClearUserData={() => setShowClearConfirm(true)}
                />
              </CardContent>
            </Card>
          </div>
        </div>

        {/* <Card>
          <CardHeader>
            <div className="flex flex-col gap-1">
              <CardTitle>订阅解析与节点过滤</CardTitle>
              <CardDescription>快速解析节点并结合正则、国家与数量限制筛选结果。</CardDescription>
            </div>
          </CardHeader>

          <CardContent>
            <SubscriptionParserCard
              subscriptionText={subscriptionText}
              setSubscriptionText={setSubscriptionText}
              nameRegex={nameRegex}
              setNameRegex={setNameRegex}
              countryFilter={countryFilter}
              setCountryFilter={setCountryFilter}
              limit={limit}
              setLimit={setLimit}
              parsedNodes={parsedNodes}
              loadingParse={loadingParse}
              onParseNodes={onParseNodes}
            />
          </CardContent>
        </Card> */}
      </div>

      <ClearDataConfirmDialog
        showClearConfirm={showClearConfirm}
        dataActionType={dataActionType}
        setShowClearConfirm={setShowClearConfirm}
        onConfirmClearUserData={onConfirmClearUserData}
      />
    </>
  );
}
