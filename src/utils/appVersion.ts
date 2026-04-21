const DEFAULT_VERSION = "0.1.0";

let cachedVersionPromise: Promise<string> | null = null;

export function getInjectedAppVersion(): string {
  const env = (import.meta as ImportMeta & { env?: Record<string, unknown> }).env;
  const injected = typeof env?.VITE_APP_VERSION === "string" ? env.VITE_APP_VERSION : "";
  return injected || DEFAULT_VERSION;
}

export async function getAppVersion(): Promise<string> {
  if (cachedVersionPromise) {
    return cachedVersionPromise;
  }

  cachedVersionPromise = (async () => {
    const fallback = getInjectedAppVersion();
    const hasTauriRuntime = Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
    if (!hasTauriRuntime) {
      return fallback;
    }

    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      const runtimeVersion = await getVersion();
      return runtimeVersion || fallback;
    } catch {
      return fallback;
    }
  })();

  return cachedVersionPromise;
}
