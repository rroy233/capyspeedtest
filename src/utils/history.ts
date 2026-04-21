import type { SpeedTestResult, SpeedTestTaskConfig } from "../types/speedtest";
import type { SpeedTestHistoryQuery, SpeedTestHistoryRecord } from "../types/history";

const HISTORY_STORAGE_KEY = "capyspeedtest:history:v1";
const MAX_HISTORY_SIZE = 100;
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

function safeParseRecords(raw: string | null): SpeedTestHistoryRecord[] {
  if (!raw) {
    return [];
  }
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed as SpeedTestHistoryRecord[];
  } catch {
    return [];
  }
}

export function getAllHistoryRecords(): SpeedTestHistoryRecord[] {
  const records = safeParseRecords(getStorage().getItem(HISTORY_STORAGE_KEY));
  return records.sort((left, right) => Number(right.created_at) - Number(left.created_at));
}

export function appendHistoryRecord(
  subscriptionText: string,
  config: SpeedTestTaskConfig,
  results: SpeedTestResult[]
): SpeedTestHistoryRecord {
  const now = String(Math.floor(Date.now() / 1000));
  const record: SpeedTestHistoryRecord = {
    id: `${now}-${Math.random().toString(36).slice(2, 8)}`,
    created_at: now,
    subscription_text: subscriptionText,
    config: {
      concurrency: config.concurrency,
      target_sites: [...config.target_sites],
      enable_upload_test: config.enable_upload_test,
      timeout_ms: config.timeout_ms,
    },
    results: [...results],
  };

  const next = [record, ...getAllHistoryRecords()].slice(0, MAX_HISTORY_SIZE);
  getStorage().setItem(HISTORY_STORAGE_KEY, JSON.stringify(next));
  return record;
}

export function queryHistoryRecords(query: SpeedTestHistoryQuery): SpeedTestHistoryRecord[] {
  const keyword = (query.keyword ?? "").trim().toLowerCase();
  const protocol = (query.protocol ?? "").trim().toLowerCase();
  const country = (query.country ?? "").trim().toUpperCase();
  const from = Number(query.from ?? 0);
  const to = Number(query.to ?? Number.MAX_SAFE_INTEGER);

  return getAllHistoryRecords().filter((record) => {
    const createdAt = Number(record.created_at);
    if (createdAt < from || createdAt > to) {
      return false;
    }

    const hasProtocol = protocol
      ? record.results.some((result) => result.node.protocol.toLowerCase() === protocol)
      : true;
    if (!hasProtocol) {
      return false;
    }

    const hasCountry = country
      ? record.results.some((result) => result.node.country.toUpperCase() === country)
      : true;
    if (!hasCountry) {
      return false;
    }

    if (!keyword) {
      return true;
    }

    return record.results.some((result) => {
      const haystack = `${result.node.name} ${result.node.protocol} ${result.nat_type} ${result.egress_geoip.country_code}`.toLowerCase();
      return haystack.includes(keyword);
    });
  });
}

export function getHistoryRecordById(id: string): SpeedTestHistoryRecord | null {
  return getAllHistoryRecords().find((record) => record.id === id) ?? null;
}

// 导出字段覆盖测速核心指标，便于在外部表格工具二次分析。
export function toResultsCsv(record: SpeedTestHistoryRecord): string {
  const headers = [
    "节点名称",
    "协议",
    "地区",
    "TCP延迟(ms)",
    "站点延迟(ms)",
    "丢包率",
    "平均下载(Mbps)",
    "峰值下载(Mbps)",
    "平均上传(Mbps)",
    "峰值上传(Mbps)",
    "NAT类型",
    "出口IP",
    "出口地区",
  ];
  const rows = record.results.map((item) => [
    item.node.name,
    item.node.protocol,
    item.node.country,
    String(item.tcp_ping_ms),
    String(item.site_ping_ms),
    String(item.packet_loss_rate),
    String(item.avg_download_mbps),
    String(item.max_download_mbps),
    item.avg_upload_mbps == null ? "" : String(item.avg_upload_mbps),
    item.max_upload_mbps == null ? "" : String(item.max_upload_mbps),
    item.nat_type,
    item.egress_geoip.ip,
    item.egress_geoip.country_code,
  ]);

  return [headers, ...rows]
    .map((line) =>
      line
        .map((cell) => `"${String(cell).replace(/"/g, '""')}"`)
        .join(",")
    )
    .join("\n");
}

export function downloadTextFile(filename: string, content: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}

export function clearHistoryRecords(): void {
  getStorage().removeItem(HISTORY_STORAGE_KEY);
}
