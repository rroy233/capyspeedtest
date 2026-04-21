import { beforeEach, describe, expect, it } from "vitest";
import type { SpeedTestResult, SpeedTestTaskConfig } from "../types/speedtest";
import { appendHistoryRecord, clearHistoryRecords, queryHistoryRecords, toResultsCsv } from "./history";

const baseConfig: SpeedTestTaskConfig = {
  concurrency: 4,
  target_sites: ["https://example.com"],
  enable_upload_test: true,
  timeout_ms: 8000,
};

const baseResult: SpeedTestResult = {
  node: {
    name: "香港-HK-01",
    protocol: "vless",
    country: "HK",
    raw: "vless://abc#香港-HK-01",
  },
  tcp_ping_ms: 20,
  site_ping_ms: 30,
  packet_loss_rate: 0.01,
  avg_download_mbps: 88.1,
  max_download_mbps: 102.4,
  avg_upload_mbps: 35.1,
  max_upload_mbps: 48.2,
  ingress_geoip: {
    ip: "21.10.0.1",
    country_code: "HK",
    country_name: "Hong Kong",
    isp: "Ingress",
  },
  egress_geoip: {
    ip: "31.20.0.1",
    country_code: "HK",
    country_name: "Hong Kong",
    isp: "Egress",
  },
  nat_type: "Full Cone",
  finished_at: "1710000000",
};

describe("history 工具", () => {
  beforeEach(() => {
    clearHistoryRecords();
  });

  it("应写入并按条件查询历史记录", () => {
    appendHistoryRecord("vless://a#香港", baseConfig, [baseResult]);
    appendHistoryRecord("trojan://b#日本", baseConfig, [
      {
        ...baseResult,
        node: { ...baseResult.node, name: "日本-JP-01", protocol: "trojan", country: "JP" },
      },
    ]);

    const byProtocol = queryHistoryRecords({ protocol: "trojan" });
    expect(byProtocol).toHaveLength(1);
    expect(byProtocol[0].results[0].node.country).toBe("JP");

    const byCountry = queryHistoryRecords({ country: "HK" });
    expect(byCountry).toHaveLength(1);
    expect(byCountry[0].results[0].node.name).toContain("香港");
  });

  it("应生成包含表头与数据行的 CSV", () => {
    const record = appendHistoryRecord("vless://a#香港", baseConfig, [baseResult]);
    const csv = toResultsCsv(record);
    expect(csv).toContain("节点名称");
    expect(csv).toContain("香港-HK-01");
    expect(csv.split("\n").length).toBeGreaterThanOrEqual(2);
  });
});
