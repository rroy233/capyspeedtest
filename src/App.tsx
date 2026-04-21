import { useCallback, useEffect, useRef, useState } from "react";
import { Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { ThemeProvider, useTheme } from "./contexts/ThemeContext";
import { AlertProvider, useAlert } from "./contexts/AlertContext";
import { SpeedtestProvider, useSpeedtestContext } from "./contexts/SpeedtestContext";
import { getSettingsSnapshot, prepareAppExit, runScheduledUpdateChecks } from "./api/settings";
import { getAppVersion } from "./utils/appVersion";
import { cleanupLocalRuntimeCache, isSpeedtestRunning, notifyAppExit } from "./utils/runtimeLifecycle";
import HomePage from "./pages/HomePage";
import AboutPage from "./pages/AboutPage";
import NotFoundPage from "./pages/NotFoundPage";
import SettingsPage from "./pages/SettingsPage";
import ResultsPage from "./pages/ResultsPage";
import {
  Tabs,
  Button,
  ModalBackdrop,
  ModalContainer,
  ModalDialog,
  ModalHeader,
  ModalBody,
  ModalFooter,
} from "@heroui/react";
import appIcon from "../src-tauri/icons/icon.png";

export function AppRoutes() {
  const location = useLocation();
  const isHome = location.pathname === "/";

  return (
    <>
      <section className={isHome ? "" : "hidden"} aria-hidden={!isHome}>
        <HomePage />
      </section>
      <Routes>
        <Route path="/" element={<></>} />
        <Route path="/results" element={<ResultsPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/about" element={<AboutPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Routes>
    </>
  );
}

const navItems = [
  { key: "home", to: "/", label: "首页" },
  { key: "results", to: "/results", label: "结果" },
  { key: "settings", to: "/settings", label: "设置" },
  { key: "about", to: "/about", label: "关于" },
];

function ThemeIcon() {
  const { resolvedTheme } = useTheme();
  if (resolvedTheme === "dark") {
    return (
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
      </svg>
    );
  }
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <circle cx="12" cy="12" r="5" />
      <line x1="12" y1="1" x2="12" y2="3" />
      <line x1="12" y1="21" x2="12" y2="23" />
      <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
      <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
      <line x1="1" y1="12" x2="3" y2="12" />
      <line x1="21" y1="12" x2="23" y2="12" />
      <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
      <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
    </svg>
  );
}

function AppContent() {
  const location = useLocation();
  const navigate = useNavigate();
  const { resolvedTheme, setTheme } = useTheme();
  const { showAlert } = useAlert();
  const allowWindowCloseRef = useRef(false);
  const closeWindowRef = useRef<null | (() => Promise<void>)>(null);
  const scheduledCheckTriggeredRef = useRef(false);
  const [showExitConfirm, setShowExitConfirm] = useState(false);
  const [exiting, setExiting] = useState(false);
  const [showResumeConfirm, setShowResumeConfirm] = useState(false);
  const [showKernelMissingConfirm, setShowKernelMissingConfirm] = useState(false);

  const speedtestCtx = useSpeedtestContext();
  const { setHomeActive, refreshCheckpoint } = speedtestCtx;

  useEffect(() => {
    setHomeActive(location.pathname === "/");
  }, [location.pathname, setHomeActive]);

  useEffect(() => {
    let cancelled = false;
    const checkCheckpointAfterBoot = async () => {
      await refreshCheckpoint();
      if (cancelled) return;

      // 某些环境下 Tauri internals 注入略晚，做一次延迟补偿检测
      const tauriInternals = (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      if (!tauriInternals) {
        await new Promise((resolve) => setTimeout(resolve, 600));
        if (!cancelled) {
          await refreshCheckpoint();
        }
      }
    };

    void checkCheckpointAfterBoot();
    return () => {
      cancelled = true;
    };
  }, [refreshCheckpoint]);

  const performShutdownAndClose = useCallback(async () => {
    if (allowWindowCloseRef.current || exiting) {
      return;
    }

    setExiting(true);
    notifyAppExit();

    try {
      await prepareAppExit();
    } catch (error) {
      console.warn("[退出] prepare_app_exit 执行失败:", error);
    } finally {
      cleanupLocalRuntimeCache();
    }

    allowWindowCloseRef.current = true;
    try {
      if (closeWindowRef.current) {
        await closeWindowRef.current();
      } else {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().close();
      }
    } finally {
      setExiting(false);
    }
  }, [exiting]);

  useEffect(() => {
    if (scheduledCheckTriggeredRef.current) {
      return;
    }
    scheduledCheckTriggeredRef.current = true;

    async function runBackgroundChecks() {
      try {
        const appVersion = await getAppVersion();
        const result = await runScheduledUpdateChecks(appVersion);
        if (!result.client_update?.has_update) {
          return;
        }
        showAlert({
          id: "client-update-available",
          title: `发现新版本 ${result.client_update.latest_version}`,
          description: (
            <div className="mt-2 flex justify-end">
              <Button
                size="sm"
                variant="outline"
                onPress={() => navigate("/settings")}
              >
                前往设置页
              </Button>
            </div>
          ),
          status: "warning",
          timeout: 0,
        });
      } catch (error) {
        console.warn("[后台检查] 执行失败:", error);
      }
    }

    void runBackgroundChecks();
  }, [navigate, showAlert]);

  useEffect(() => {
    let cancelled = false;

    async function checkLocalKernelStatus() {
      try {
        const appVersion = await getAppVersion();
        const snapshot = await getSettingsSnapshot(appVersion);
        if (cancelled) {
          return;
        }
        const localInstalled = snapshot.kernel.local_installed_versions ?? [];
        const currentExists = snapshot.kernel.current_exists ?? false;
        if (!currentExists || localInstalled.length === 0) {
          setShowKernelMissingConfirm(true);
        }
      } catch (error) {
        console.warn("[启动检查] 本地内核状态检查失败:", error);
      }
    }

    void checkLocalKernelStatus();
    return () => {
      cancelled = true;
    };
  }, []);

  // 检测未完成的测速 checkpoint，提供恢复选项
  useEffect(() => {
    const checkpoint = speedtestCtx.checkpoint;
    if (checkpoint && checkpoint.completed < checkpoint.total) {
      setShowResumeConfirm(true);
      return;
    }
    setShowResumeConfirm(false);
  }, [speedtestCtx.checkpoint]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    async function bindCloseGuard() {
      const tauriInternals = (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      if (!tauriInternals) {
        return;
      }

      const { getCurrentWindow } = await import("@tauri-apps/api/window");

      const currentWindow = getCurrentWindow();
      closeWindowRef.current = () => currentWindow.close();
      unlisten = await currentWindow.onCloseRequested(async (event) => {
        if (allowWindowCloseRef.current) {
          return;
        }

        event.preventDefault();

        const running = isSpeedtestRunning();
        if (running) {
          setShowExitConfirm(true);
          return;
        }

        await performShutdownAndClose();
      });
    }

    void bindCloseGuard();

    return () => {
      closeWindowRef.current = null;
      if (unlisten) {
        unlisten();
      }
    };
  }, [performShutdownAndClose]);

  const getSelectedTab = () => {
    const path = location.pathname;
    if (path === "/") return "home";
    if (path.startsWith("/results")) return "results";
    if (path === "/settings") return "settings";
    if (path === "/about") return "about";
    return "home";
  };

  return (
    <div className="min-h-screen bg-background text-foreground flex flex-col">
      {/* 顶部导航栏 */}
      <header className="border-b border-divider sticky top-0 z-50 bg-background/80 backdrop-blur-md">
        <div className="max-w-7xl mx-auto px-6 py-3">
          <div className="flex items-center justify-between">
            {/* Logo 区 */}
            <div className="flex items-center gap-3">
              <img
                src={appIcon}
                alt="CapySpeedtest 图标"
                className="h-10 w-10 rounded-xl border border-default-200 shadow-sm"
              />
              <div className="flex flex-col leading-none">
                <span className="bg-gradient-to-r from-[#7B3F20] via-[#A35F2F] to-[#2F6FA3] bg-clip-text text-xl font-black tracking-tight text-transparent">
                  CapySpeedtest
                </span>
                <span className="text-[11px] font-medium tracking-wide text-[#5D89B0]">
                  科学上网测速工具
                </span>
              </div>
              <span className="text-xs px-2 py-0.5 rounded-full bg-warning/10 text-warning font-medium">
                Beta
              </span>
            </div>

            {/* 导航 Tabs */}
            <Tabs
              aria-label="主导航"
              variant="primary"
              selectedKey={getSelectedTab()}
              onSelectionChange={(key) => {
                const item = navItems.find((n) => n.key === key);
                if (item) navigate(item.to);
              }}
            >
              <Tabs.ListContainer>
                <Tabs.List aria-label="主导航">
                  {navItems.map((item) => (
                    <Tabs.Tab key={item.key} id={item.key}>
                      {item.label}
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  ))}
                </Tabs.List>
              </Tabs.ListContainer>
            </Tabs>

            {/* 主题切换按钮 */}
            <button
              onClick={() => {
                const next = resolvedTheme === "dark" ? "light" : "dark";
                setTheme(next);
              }}
              className="p-2 rounded-medium hover:bg-default-100 transition-colors"
              aria-label="切换主题"
            >
              <ThemeIcon />
            </button>
          </div>
        </div>
      </header>

      {/* 主内容区 */}
      <main className="max-w-7xl mx-auto p-6 w-full flex-1">
        <AppRoutes />
      </main>

      {/* 页脚
      <footer className="mt-auto border-t border-divider bg-gradient-to-r from-[#F9F4EE] via-[#EEF6FB] to-[#F7FBFF] py-5 dark:bg-none">
        <div className="mx-auto flex max-w-7xl items-center justify-between gap-4 px-6">
          <div className="flex items-center gap-3">
            <img
              src={appIcon}
              alt="CapySpeedtest 图标"
              className="h-8 w-8 rounded-lg border border-default-200/80 shadow-sm"
            />
            <div className="leading-tight">
              <p className="bg-gradient-to-r from-[#7B3F20] via-[#A35F2F] to-[#2F6FA3] bg-clip-text text-sm font-bold text-transparent">
                CapySpeedtest
              </p>
              <p className="text-xs text-[#6A8EAD]">Proxy Batch Benchmark</p>
            </div>
          </div>
          <p className="text-right text-xs font-medium text-foreground-500">
            科学上网节点批量测速工具
          </p>
        </div>
      </footer> */}

      <ModalBackdrop
        isOpen={showExitConfirm}
        onOpenChange={(open) => {
          if (exiting) return;
          setShowExitConfirm(open);
        }}
      >
        <ModalContainer>
          <ModalDialog aria-label="确认退出应用" className="sm:max-w-[380px]">
            <ModalHeader>确认退出</ModalHeader>
            <ModalBody>
              <p className="text-foreground-500">
                当前正在执行测速任务，退出将暂时中断任务。是否确认退出？
              </p>
            </ModalBody>
            <ModalFooter>
              <Button
                variant="secondary"
                isDisabled={exiting}
                onPress={() => setShowExitConfirm(false)}
              >
                取消
              </Button>
              <Button
                variant="danger"
                isPending={exiting}
                onPress={() => {
                  setShowExitConfirm(false);
                  void performShutdownAndClose();
                }}
              >
                确认退出
              </Button>
            </ModalFooter>
          </ModalDialog>
        </ModalContainer>
      </ModalBackdrop>

      {/* 恢复测速确认对话框 */}
      <ModalBackdrop
        isOpen={showResumeConfirm}
        onOpenChange={(open) => {
          setShowResumeConfirm(open);
        }}
      >
        <ModalContainer>
          <ModalDialog aria-label="恢复测速" className="sm:max-w-[380px]">
            <ModalHeader>发现未完成的测速</ModalHeader>
            <ModalBody>
              <p className="text-foreground-500">
                上次测速进行到 {speedtestCtx.checkpoint?.completed ?? 0}/{speedtestCtx.checkpoint?.total ?? 0} 时被中断。
                是否要继续上次测速？
              </p>
            </ModalBody>
            <ModalFooter>
              <Button
                variant="secondary"
                onPress={() => {
                  setShowResumeConfirm(false);
                  speedtestCtx.clearResumeRequest();
                  void speedtestCtx.clearCheckpoint();
                }}
              >
                放弃进度
              </Button>
              <Button
                variant="primary"
                onPress={() => {
                  setShowResumeConfirm(false);
                  speedtestCtx.requestResume();
                  navigate("/");
                }}
              >
                继续测速
              </Button>
            </ModalFooter>
          </ModalDialog>
        </ModalContainer>
      </ModalBackdrop>

      <ModalBackdrop
        isOpen={showKernelMissingConfirm}
        onOpenChange={(open) => {
          setShowKernelMissingConfirm(open);
        }}
      >
        <ModalContainer>
          <ModalDialog aria-label="本地内核缺失" className="sm:max-w-[420px]">
            <ModalHeader>未检测到本地内核</ModalHeader>
            <ModalBody>
              <p className="text-foreground-500">
                当前设备未检测到可用 Mihomo 内核。请前往设置页选择内核版本并下载后再开始测速。
              </p>
            </ModalBody>
            <ModalFooter>
              <Button
                variant="secondary"
                onPress={() => setShowKernelMissingConfirm(false)}
              >
                稍后处理
              </Button>
              <Button
                variant="primary"
                onPress={() => {
                  setShowKernelMissingConfirm(false);
                  navigate("/settings");
                }}
              >
                前往设置页
              </Button>
            </ModalFooter>
          </ModalDialog>
        </ModalContainer>
      </ModalBackdrop>
    </div>
  );
}

export default function App() {
  return (
    <ThemeProvider>
      <AlertProvider>
        <SpeedtestProvider>
          <AppContent />
        </SpeedtestProvider>
      </AlertProvider>
    </ThemeProvider>
  );
}
