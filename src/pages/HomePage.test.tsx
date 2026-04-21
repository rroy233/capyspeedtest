import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import HomePage from "./HomePage";
import { AlertProvider } from "../contexts/AlertContext";
import { SpeedtestProvider } from "../contexts/SpeedtestContext";
import type { NodeInfo } from "../types/speedtest";

vi.mock("../components/map/SpeedTestMap", () => ({
  default: () => <div data-testid="speedtest-map" />,
}));

const {
  listenSpeedTestProgressMock,
  runSpeedTestBatchMock,
  fetchSubscriptionNodesFromUrlMock,
  parseSubscriptionNodesMock,
} = vi.hoisted(() => ({
  listenSpeedTestProgressMock: vi.fn(async (callback: (event: unknown) => void) => {
    callback({
      total: 2,
      completed: 1,
      current_node: "香港-HK-01",
      stage: "throughput",
      message: "正在进行吞吐测速",
    });
    return () => {};
  }),
  fetchSubscriptionNodesFromUrlMock: vi.fn<() => Promise<NodeInfo[]>>(async () => []),
  parseSubscriptionNodesMock: vi.fn<(rawInput: string) => Promise<NodeInfo[]>>(async () => [
    {
      name: "香港-HK-01",
      protocol: "vless",
      country: "HK",
      raw: "vless://token@example.com#香港-HK-01",
    },
  ]),
  runSpeedTestBatchMock: vi.fn(async () => [
    {
      node: {
        name: "香港-HK-01",
        protocol: "vless",
        country: "HK",
        raw: "vless://token@example.com#香港-HK-01",
      },
      tcp_ping_ms: 30,
      site_ping_ms: 50,
      packet_loss_rate: 0.01,
      avg_download_mbps: 88.8,
      max_download_mbps: 108.6,
      avg_upload_mbps: 30.1,
      max_upload_mbps: 40.5,
      ingress_geoip: {
        ip: "21.10.0.1",
        country_code: "HK",
        country_name: "Hong Kong",
        isp: "Ingress Telecom",
      },
      egress_geoip: {
        ip: "31.20.0.1",
        country_code: "HK",
        country_name: "Hong Kong",
        isp: "Egress Network",
      },
      nat_type: "Full Cone",
      finished_at: "1710000000",
    },
  ]),
}));

vi.mock("../api/settings", () => ({
  listenSpeedTestProgress: listenSpeedTestProgressMock,
  listenKernelDownloadProgress: vi.fn(async () => () => {}),
  runSpeedTestBatch: runSpeedTestBatchMock,
  fetchSubscriptionNodesFromUrl: fetchSubscriptionNodesFromUrlMock,
  parseSubscriptionNodes: parseSubscriptionNodesMock,
}));

vi.mock("../api/speedtest", () => ({
  runSpeedTestBatch: runSpeedTestBatchMock,
  listenSpeedTestProgress: listenSpeedTestProgressMock,
  listenKernelDownloadProgress: vi.fn(async () => () => {}),
  getSpeedtestCheckpoint: vi.fn(async () => null),
  clearSpeedtestCheckpoint: vi.fn(async () => {}),
  resumeSpeedtestFromCheckpoint: vi.fn(async () => []),
}));

vi.mock("../api/subscription", () => ({
  fetchSubscriptionNodesFromUrl: fetchSubscriptionNodesFromUrlMock,
  parseSubscriptionNodes: parseSubscriptionNodesMock,
}));

describe("HomePage", () => {
  beforeEach(() => {
    cleanup();
    listenSpeedTestProgressMock.mockClear();
    runSpeedTestBatchMock.mockClear();
    fetchSubscriptionNodesFromUrlMock.mockClear();
    parseSubscriptionNodesMock.mockClear();
  });

  it("应支持任务配置提交并渲染进度与结果", async () => {
    fetchSubscriptionNodesFromUrlMock.mockResolvedValue([]);
    parseSubscriptionNodesMock.mockResolvedValue([
      {
        name: "香港-HK-01",
        protocol: "vless",
        country: "HK",
        raw: "vless://token@example.com#香港-HK-01",
      },
    ]);

    render(
      <AlertProvider>
        <SpeedtestProvider>
          <HomePage />
        </SpeedtestProvider>
      </AlertProvider>
    );

    fireEvent.click(screen.getByRole("button", { name: "开始批量测速" }));

    await waitFor(() => {
      expect(runSpeedTestBatchMock).toHaveBeenCalledTimes(1);
      expect(screen.getByText("地区排名")).toBeInTheDocument();
    });
  });

  it("订阅链接模式应支持 /pattern/flags 正则筛选", async () => {
    fetchSubscriptionNodesFromUrlMock.mockResolvedValue([
      {
        name: "hongkong-hk-01",
        protocol: "vless",
        country: "HK",
        raw: "vless://token@example.com#hongkong-hk-01",
      },
      {
        name: "japan-jp-01",
        protocol: "vless",
        country: "JP",
        raw: "vless://token2@example.com#japan-jp-01",
      },
      {
        name: "singapore-sg-01",
        protocol: "vless",
        country: "SG",
        raw: "vless://token3@example.com#singapore-sg-01",
      },
    ]);
    parseSubscriptionNodesMock.mockResolvedValue([
      {
        name: "hongkong-hk-01",
        protocol: "vless",
        country: "HK",
        raw: "vless://token@example.com#hongkong-hk-01",
      },
      {
        name: "japan-jp-01",
        protocol: "vless",
        country: "JP",
        raw: "vless://token2@example.com#japan-jp-01",
      },
    ]);
    parseSubscriptionNodesMock.mockClear();
    runSpeedTestBatchMock.mockClear();

    render(
      <AlertProvider>
        <SpeedtestProvider>
          <HomePage />
        </SpeedtestProvider>
      </AlertProvider>
    );

    fireEvent.click(screen.getByRole("button", { name: "订阅链接" }));
    fireEvent.change(screen.getByPlaceholderText("https://example.com/subscription"), {
      target: { value: "https://example.com/subscription" },
    });
    fireEvent.click(screen.getByRole("button", { name: "下一步" }));

    await waitFor(() => {
      expect(fetchSubscriptionNodesFromUrlMock).toHaveBeenCalledTimes(1);
      expect(screen.getByRole("button", { name: "开始批量测速" })).toBeInTheDocument();
    });

    fireEvent.click(await screen.findByRole("button", { name: /正则.*筛选/ }));
    fireEvent.change(screen.getByPlaceholderText("例如: HK|JP|SG"), {
      target: { value: "/hk|jp/i" },
    });

    fireEvent.click(screen.getByRole("button", { name: "开始批量测速" }));

    await waitFor(() => {
      expect(parseSubscriptionNodesMock).toHaveBeenCalledWith(
        [
          "vless://token@example.com#hongkong-hk-01",
          "vless://token2@example.com#japan-jp-01",
        ].join("\n")
      );
      expect(runSpeedTestBatchMock).toHaveBeenCalledTimes(1);
    });
  });

  it("订阅链接模式下无效正则应阻止开始测速并提示错误", async () => {
    fetchSubscriptionNodesFromUrlMock.mockResolvedValue([
      {
        name: "hongkong-hk-01",
        protocol: "vless",
        country: "HK",
        raw: "vless://token@example.com#hongkong-hk-01",
      },
    ]);
    parseSubscriptionNodesMock.mockClear();
    runSpeedTestBatchMock.mockClear();

    render(
      <AlertProvider>
        <SpeedtestProvider>
          <HomePage />
        </SpeedtestProvider>
      </AlertProvider>
    );

    fireEvent.click(screen.getByRole("button", { name: "订阅链接" }));
    fireEvent.change(screen.getByPlaceholderText("https://example.com/subscription"), {
      target: { value: "https://example.com/subscription" },
    });
    fireEvent.click(screen.getByRole("button", { name: "下一步" }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "开始批量测速" })).toBeInTheDocument();
    });

    fireEvent.click(await screen.findByRole("button", { name: /正则.*筛选/ }));
    fireEvent.change(screen.getByPlaceholderText("例如: HK|JP|SG"), {
      target: { value: "/[hk/" },
    });
    fireEvent.click(screen.getByRole("button", { name: "开始批量测速" }));

    await waitFor(() => {
      expect(screen.getAllByText(/无效正则表达式/).length).toBeGreaterThan(0);
      expect(parseSubscriptionNodesMock).not.toHaveBeenCalled();
      expect(runSpeedTestBatchMock).not.toHaveBeenCalled();
    });
  });

  it("按地区筛选时应实时展示待测速列表、节点总数和预计用时", async () => {
    fetchSubscriptionNodesFromUrlMock.mockResolvedValue([
      {
        name: "hongkong-hk-01",
        protocol: "vless",
        country: "HK",
        raw: "vless://token@example.com#hongkong-hk-01",
      },
      {
        name: "japan-jp-01",
        protocol: "trojan",
        country: "JP",
        raw: "trojan://token@example.com#japan-jp-01",
      },
      {
        name: "singapore-sg-01",
        protocol: "ss",
        country: "SG",
        raw: "ss://token@example.com#singapore-sg-01",
      },
    ]);

    render(
      <AlertProvider>
        <SpeedtestProvider>
          <HomePage />
        </SpeedtestProvider>
      </AlertProvider>
    );

    fireEvent.click(screen.getByRole("button", { name: "订阅链接" }));
    fireEvent.change(screen.getByPlaceholderText("https://example.com/subscription"), {
      target: { value: "https://example.com/subscription" },
    });
    fireEvent.click(screen.getByRole("button", { name: "下一步" }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "开始批量测速" })).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: "按地区" }));

    expect(screen.getByText("暂无待测速节点")).toBeInTheDocument();
    expect(screen.getAllByText((_, node) => node?.textContent?.includes("待测速 0 个节点") ?? false).length).toBeGreaterThan(0);
    expect(screen.getAllByText((_, node) => node?.textContent?.includes("预计需要 0 分钟") ?? false).length).toBeGreaterThan(0);

    fireEvent.click(screen.getByText("HK"));
    fireEvent.click(screen.getByText("JP"));

    await waitFor(() => {
      expect(screen.getByText("hongkong-hk-01")).toBeInTheDocument();
      expect(screen.getByText("japan-jp-01")).toBeInTheDocument();
      expect(screen.getAllByText((_, node) => node?.textContent?.includes("待测速 2 个节点") ?? false).length).toBeGreaterThan(0);
      expect(screen.getAllByText((_, node) => node?.textContent?.includes("预计需要 1 分钟") ?? false).length).toBeGreaterThan(0);
    });
  });
});
