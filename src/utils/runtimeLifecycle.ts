const RUNTIME_RUNNING_KEY = "capyspeedtest:runtime:running";
const LEGACY_HISTORY_KEY = "capyspeedtest:history:v1";
export const APP_EXIT_EVENT = "capyspeedtest://app-exit";

let speedtestRunning = false;

function hasWindow(): boolean {
  return typeof window !== "undefined";
}

export function setSpeedtestRunning(running: boolean): void {
  speedtestRunning = running;
  if (!hasWindow()) return;
  try {
    if (running) {
      window.sessionStorage.setItem(RUNTIME_RUNNING_KEY, "1");
    } else {
      window.sessionStorage.removeItem(RUNTIME_RUNNING_KEY);
    }
  } catch {
    // ignore storage errors
  }
}

export function isSpeedtestRunning(): boolean {
  if (speedtestRunning) return true;
  if (!hasWindow()) return false;
  try {
    return window.sessionStorage.getItem(RUNTIME_RUNNING_KEY) === "1";
  } catch {
    return false;
  }
}

export function cleanupLocalRuntimeCache(): void {
  if (!hasWindow()) return;
  try {
    // 已迁移到 SQLite，退出时清理遗留 localStorage 历史缓存
    window.localStorage.removeItem(LEGACY_HISTORY_KEY);
  } catch {
    // ignore storage errors
  }
  try {
    window.sessionStorage.removeItem(RUNTIME_RUNNING_KEY);
  } catch {
    // ignore storage errors
  }
}

export function notifyAppExit(): void {
  if (!hasWindow()) return;
  window.dispatchEvent(new CustomEvent(APP_EXIT_EVENT));
}
