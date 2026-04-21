import type { SpeedTestTaskConfig } from "../types/speedtest";

const FORM_STATE_KEY = "capyspeedtest:formstate:v1";
const memoryStorage = new Map<string, string>();

interface StorageLike {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
  removeItem: (key: string) => void;
}

function getStorage(): StorageLike {
  if (typeof window === "undefined") {
    return {
      getItem: (key) => memoryStorage.get(key) ?? null,
      setItem: (key, value) => {
        memoryStorage.set(key, value);
      },
      removeItem: (key) => {
        memoryStorage.delete(key);
      },
    };
  }
  const storage = (window as { localStorage?: unknown }).localStorage as Partial<StorageLike> | undefined;
  if (storage && typeof storage.getItem === "function" && typeof storage.setItem === "function" && typeof storage.removeItem === "function") {
    return storage as StorageLike;
  }
  return {
    getItem: (key) => memoryStorage.get(key) ?? null,
    setItem: (key, value) => {
      memoryStorage.set(key, value);
    },
    removeItem: (key) => {
      memoryStorage.delete(key);
    },
  };
}

export interface SpeedTestFormState {
  inputMode: "manual" | "url";
  subscriptionText: string;
  subscriptionUrl: string;
  concurrency: string;
  targetSites: string;
  enableUploadTest: boolean;
  timeoutMs: string;
}

export function getFormState(): SpeedTestFormState | null {
  const raw = getStorage().getItem(FORM_STATE_KEY);
  if (!raw) {
    return null;
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (typeof parsed !== "object" || parsed === null) {
      return null;
    }
    return parsed as SpeedTestFormState;
  } catch {
    return null;
  }
}

export function saveFormState(state: SpeedTestFormState): void {
  try {
    getStorage().setItem(FORM_STATE_KEY, JSON.stringify(state));
  } catch {
    // Ignore quota errors
  }
}

export function clearFormState(): void {
  getStorage().removeItem(FORM_STATE_KEY);
}
