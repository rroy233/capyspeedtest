import { FormEvent, useEffect, useMemo, useRef, useState, useCallback } from "react";
import { runSpeedTestBatch, resumeSpeedtestFromCheckpoint } from "../../api/speedtest";
import { fetchSubscriptionNodesFromUrl, parseSubscriptionNodes } from "../../api/subscription";
import type { CountrySpeedSummary, KernelDownloadProgressEvent, NodeInfo, NodeTestState, SpeedTestProgressEvent, SpeedTestResult, SpeedTestTaskConfig } from "../../types/speedtest";
import { getFormState, saveFormState, type SpeedTestFormState } from "../../utils/formState";
import { useAlert } from "../../contexts/AlertContext";
import { useSpeedtestContext } from "../../contexts/SpeedtestContext";
import { getCountryName } from "../../utils/countryMapping";
import { getSpeedStatus } from "../../utils/groupResults";
import { filterNodesByNameRegex } from "../../utils/nodeRegexFilter";
import SpeedTestMap from "../map/SpeedTestMap";
import { SpeedTestConfigForm } from "./SpeedTestConfigForm";
import { LiveSpeedCard } from "./LiveSpeedCard";
import { CountryRankingGrid } from "./CountryRankingGrid";
import { ISPFlowSankey } from "./ISPFlowSankey";
import type { SelectedFlow } from "./ISPFlowSankey";
import {
  Alert,
  CloseButton,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Chip,
  ProgressBar,
  Spinner,
  Button,
  Table,
} from "@heroui/react";

const defaultSubscription = [
  "vless://token@example.com:443?security=tls#香港-HK-01",
  "trojan://password@example.com:443?security=tls#日本-JP-01",
  "ss://YWJzLTEyOC1nY206cGFzc0BleGFtcGxlLmNvbTo0NDM=#新加坡-SG-01",
].join("\n");

const ISP_TABLE_LOG_PREFIX = "[ISPFlowTable]";

export default function SpeedTestPage() {
  // 状态
  const [inputMode, setInputMode] = useState<"manual" | "url">("manual");
  const [subscriptionText, setSubscriptionText] = useState(defaultSubscription);
  const [subscriptionUrl, setSubscriptionUrl] = useState("");
  const [concurrency, setConcurrency] = useState("4");
  const [targetSites, setTargetSites] = useState("https://www.google.com,https://www.youtube.com");
  const [enableUploadTest, setEnableUploadTest] = useState(true);
  const [timeoutMs, setTimeoutMs] = useState("8000");
  const [kernelDownloadProgress, setKernelDownloadProgress] = useState<KernelDownloadProgressEvent | null>(null);
  const [fetchingUrl, setFetchingUrl] = useState(false);
  const [error, setError] = useState("");
  const [configCollapsed, setConfigCollapsed] = useState(false);
  const [expandedCountries, setExpandedCountries] = useState<Set<string>>(new Set());

  // 从 SpeedtestContext 获取测速状态
  const speedtestCtx = useSpeedtestContext();
  const running = speedtestCtx.running;
  const progressEvent = speedtestCtx.progressEvent;
  const nodeStates = speedtestCtx.nodeStates;
  const results = speedtestCtx.results;
  // FIX: Prefer the reactive ispFlowByNode state value exposed by the context
  // so that useMemo dependencies (selectedFlowRows, ISPFlowSankey sankeyData)
  // correctly recompute whenever new GeoIP entries are added during a test run.
  // Reading directly from _ispFlowByNodeRef.current returns the same Map
  // reference even after entries are added, which silently breaks memoisation.
  // If the context has not yet been updated to expose a reactive value, fall
  // back to the ref so existing behaviour is preserved without a crash.
  const ispFlowByNode: Map<
    string,
    { node_name: string; ingress_geoip: { ip: string; country_code: string; country_name: string; isp: string }; egress_geoip: { ip: string; country_code: string; country_name: string; isp: string } }
  > = (speedtestCtx as unknown as { ispFlowByNode?: typeof speedtestCtx._ispFlowByNodeRef.current }).ispFlowByNode
    ?? speedtestCtx._ispFlowByNodeRef.current;
  const { showAlert } = useAlert();
  const alertIdRef = useRef<string | null>(null);
  const resumeInFlightRef = useRef(false);

  // 使用 context 中的 refs
  const prevNodeNameRef = speedtestCtx._prevNodeNameRef;
  const nodeOrderRef = speedtestCtx._nodeOrderRef;
  const nodeIdToNameRef = speedtestCtx._nodeIdToNameRef;
  const nodeNameToIdRef = speedtestCtx._nodeNameToIdRef;
  const activeTaskIdRef = speedtestCtx._activeTaskIdRef;

  // === 订阅链接3步流程状态 ===
  const [urlPhase, setUrlPhase] = useState<"input" | "fetching" | "filtering">("input");
  const [fetchedNodes, setFetchedNodes] = useState<NodeInfo[]>([]);
  const [filterMode, setFilterMode] = useState<"all" | "regex" | "region">("all");
  const [regexFilter, setRegexFilter] = useState("");
  const [regexFilterError, setRegexFilterError] = useState("");
  const [selectedRegions, setSelectedRegions] = useState<Map<string, number>>(new Map());
  const [isLoadingModalOpen, setIsLoadingModalOpen] = useState(false);
  const [isErrorDialogOpen, setIsErrorDialogOpen] = useState(false);
  const [errorDialogMessage, setErrorDialogMessage] = useState("");
  const [selectedIspFlow, setSelectedIspFlow] = useState<SelectedFlow | null>(null);
  const lastFlowClickRef = useRef<{ key: string; at: number } | null>(null);

  const toIspKey = useCallback(
    (geoip: { country_name: string; isp: string }): string =>
      `${geoip.country_name || "Unknown"} ${geoip.isp || "Unknown ISP"}`.trim(),
    []
  );

  const parseNodesWithFallback = useCallback(async (rawInput: string): Promise<NodeInfo[]> => {
    try {
      return await parseSubscriptionNodes(rawInput);
    } catch {
      return rawInput
        .split("\n")
        .filter((line) => line.trim())
        .map((line, idx) => ({
          name: `节点 ${idx + 1}`,
          protocol: "unknown",
          country: "未知",
          raw: line.trim(),
        }));
    }
  }, []);

  const filteredUrlNodes = useMemo(() => {
    if (filterMode === "all") {
      return { nodes: fetchedNodes, regexError: null as string | null };
    }

    if (filterMode === "regex") {
      if (!regexFilter.trim()) {
        return { nodes: fetchedNodes, regexError: null as string | null };
      }
      const filtered = filterNodesByNameRegex(fetchedNodes, regexFilter);
      return {
        nodes: filtered.nodes,
        regexError: filtered.error,
      };
    }

    if (filterMode === "region") {
      const selected = new Map<string, number>();
      selectedRegions.forEach((count, country) => {
        if (count > 0) {
          selected.set(country, count);
        }
      });

      const result: NodeInfo[] = [];
      selected.forEach((count, country) => {
        let used = 0;
        for (const node of fetchedNodes) {
          if (node.country !== country) {
            continue;
          }
          result.push(node);
          used += 1;
          if (used >= count) {
            break;
          }
        }
      });
      return { nodes: result, regexError: null as string | null };
    }

    return { nodes: fetchedNodes, regexError: null as string | null };
  }, [fetchedNodes, filterMode, regexFilter, selectedRegions]);

  // 加载保存的表单状态
  useEffect(() => {
    const saved = getFormState();
    if (saved) {
      setInputMode(saved.inputMode);
      setSubscriptionText(saved.subscriptionText);
      setSubscriptionUrl(saved.subscriptionUrl);
      setConcurrency(saved.concurrency);
      setTargetSites(saved.targetSites);
      setEnableUploadTest(saved.enableUploadTest);
      setTimeoutMs(saved.timeoutMs);
    }
  }, []);

  // 防抖保存表单状态
  useEffect(() => {
    const timeoutId = setTimeout(() => {
      const state: SpeedTestFormState = {
        inputMode,
        subscriptionText,
        subscriptionUrl,
        concurrency,
        targetSites,
        enableUploadTest,
        timeoutMs,
      };
      saveFormState(state);
    }, 500);
    return () => clearTimeout(timeoutId);
  }, [inputMode, subscriptionText, subscriptionUrl, concurrency, targetSites, enableUploadTest, timeoutMs]);

  // 切换到手动输入模式时，重置订阅链接流程状态
  useEffect(() => {
    if (inputMode === "manual") {
      setUrlPhase("input");
      setFetchedNodes([]);
      setFilterMode("all");
      setRegexFilter("");
      setRegexFilterError("");
      setSelectedRegions(new Map());
    }
  }, [inputMode]);

  // URL模式下修改订阅链接时，重置为input阶段
  useEffect(() => {
    if (inputMode === "url" && urlPhase === "filtering") {
      setUrlPhase("input");
      setFetchedNodes([]);
      setFilterMode("all");
      setRegexFilter("");
      setRegexFilterError("");
      setSelectedRegions(new Map());
    }
  }, [subscriptionUrl]);

  useEffect(() => {
    if (filterMode !== "regex") {
      setRegexFilterError("");
      return;
    }
    setRegexFilterError(filteredUrlNodes.regexError ?? "");
  }, [filterMode, filteredUrlNodes.regexError]);

  // 内核下载进度监听和 app exit 处理
  useEffect(() => {
    let kernelDisposer: (() => void) | null = null;

    const handleAppExit = () => {
      speedtestCtx.stopSpeedtest();
    };

    const bindListener = async () => {
      const tauriInternals = (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      if (!tauriInternals) {
        return;
      }
      const { listenKernelDownloadProgress } = await import("../../api/speedtest");

      kernelDisposer = await listenKernelDownloadProgress((event) => {
        setKernelDownloadProgress(event);

        if (!alertIdRef.current) {
          alertIdRef.current = "kernel-download-alert";
        }

        let title = "正在下载内核";
        let status: "default" | "accent" | "success" | "warning" | "danger" = "default";
        let timeout = 0;

        if (event.stage === "extracting") {
          title = "正在解压内核";
          status = "accent";
        } else if (event.stage === "completed") {
          title = "内核下载完成";
          status = "success";
          timeout = 3000;
        } else if (event.stage === "error") {
          title = "内核下载失败";
          status = "danger";
          timeout = 5000;
        }

        const description = (
          <div className="flex flex-col gap-2 mt-1">
            <div className="text-sm whitespace-pre-wrap break-all">{event.message}</div>
            {event.stage !== "completed" && event.stage !== "error" && (
              <ProgressBar value={event.progress} color="default" size="sm" />
            )}
          </div>
        );

        showAlert({
          id: alertIdRef.current,
          title: `${title} (${event.version})`,
          description,
          status,
          timeout,
        });

        if (event.stage === "completed" || event.stage === "error") {
          alertIdRef.current = null;
        }
      });
    };

    const APP_EXIT_EVENT = "capyspeedtest://app-exit";
    window.addEventListener(APP_EXIT_EVENT, handleAppExit);
    void bindListener();
    return () => {
      window.removeEventListener(APP_EXIT_EVENT, handleAppExit);
      if (kernelDisposer) kernelDisposer();
    };
  }, [showAlert, speedtestCtx]);

  // 从 checkpoint 自动恢复测速
  useEffect(() => {
    if (!speedtestCtx.resumeRequested) {
      return;
    }
    if (resumeInFlightRef.current) {
      return;
    }
    const checkpoint = speedtestCtx.checkpoint;
    if (!checkpoint || checkpoint.completed >= checkpoint.total) {
      speedtestCtx.clearResumeRequest();
      return;
    }

    resumeInFlightRef.current = true;
    let cancelled = false;
    const unknownGeo = {
      ip: "0.0.0.0",
      country_code: "UN",
      country_name: "Unknown",
      isp: "Unknown",
    };

    const resume = async () => {
      setError("");
      setConfigCollapsed(true);
      setExpandedCountries(new Set());

      if (checkpoint.config) {
        setConcurrency(String(checkpoint.config.concurrency));
        setTargetSites(checkpoint.config.target_sites.join(","));
        setEnableUploadTest(checkpoint.config.enable_upload_test);
        setTimeoutMs(String(checkpoint.config.timeout_ms));
      }

      const parsedNodes = await parseNodesWithFallback(checkpoint.raw_input);
      if (cancelled) return;
      if (parsedNodes.length === 0) {
        setError("断点恢复失败：未解析到可测速节点");
        speedtestCtx.clearResumeRequest();
        resumeInFlightRef.current = false;
        return;
      }

      const completedCount = Math.min(checkpoint.completed, parsedNodes.length);
      const initialStates = new Map<string, NodeTestState>();
      const seededResults: SpeedTestResult[] = [];

      parsedNodes.forEach((node, idx) => {
        const nodeKey = `node-${idx}`;
        if (idx < completedCount) {
          const snapshot = checkpoint.node_results[idx];
          const isError = snapshot?.status === "error";
          const result: SpeedTestResult = {
            node,
            tcp_ping_ms: snapshot?.tcp_ping_ms ?? 9999,
            site_ping_ms: snapshot?.site_ping_ms ?? 9999,
            packet_loss_rate: isError ? 1 : 0,
            avg_download_mbps: snapshot?.avg_download_mbps ?? 0,
            max_download_mbps: snapshot?.max_download_mbps ?? 0,
            avg_upload_mbps: snapshot?.avg_upload_mbps,
            max_upload_mbps: snapshot?.max_upload_mbps,
            ingress_geoip: snapshot?.ingress_geoip ?? unknownGeo,
            egress_geoip: snapshot?.egress_geoip ?? unknownGeo,
            nat_type: "Unknown",
            finished_at: new Date().toISOString(),
          };
          initialStates.set(nodeKey, {
            node,
            status: isError ? "error" : "completed",
            result,
            currentStage: isError ? "error" : "completed",
            errorMessage: isError ? "上次测速记录为失败" : undefined,
          });
          seededResults.push(result);
          return;
        }

        initialStates.set(nodeKey, {
          node,
          status: "pending",
        });
      });

      nodeOrderRef.current = parsedNodes.map((_, idx) => `node-${idx}`);
      nodeIdToNameRef.current = new Map(parsedNodes.map((_, idx) => [`node-${idx}`, `node-${idx}`]));
      nodeNameToIdRef.current = new Map(parsedNodes.map((node, idx) => [node.name.trim(), `node-${idx}`]));

      speedtestCtx.hydrateFromCheckpoint({
        taskId: checkpoint.task_id,
        totalNodes: parsedNodes.length,
        completedNodes: completedCount,
        nodeStates: initialStates,
        results: seededResults,
      });

      try {
        const speedtestResults = await resumeSpeedtestFromCheckpoint();
        if (cancelled) return;
        speedtestCtx.replaceResults(speedtestResults);
      } catch (err) {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : "恢复测速失败");
      } finally {
        resumeInFlightRef.current = false;
        if (cancelled) return;
        speedtestCtx.stopSpeedtest();
        speedtestCtx.clearResumeRequest();
        void speedtestCtx.refreshCheckpoint();
      }
    };

    void resume();
    return () => {
      cancelled = true;
    };
  }, [parseNodesWithFallback, speedtestCtx]);

  // 开始测速
  async function onStartSpeedTest(event: FormEvent) {
    event.preventDefault();

    if (inputMode === "url" && urlPhase === "input" && subscriptionUrl.trim()) {
      await fetchSubscriptionNodes();
      return;
    }

    await executeSpeedTest();
  }

  // 获取订阅节点（步骤1）
  async function fetchSubscriptionNodes() {
    setUrlPhase("fetching");
    setIsLoadingModalOpen(true);
    setError("");
    try {
      const nodes = await fetchSubscriptionNodesFromUrl(subscriptionUrl.trim());
      setFetchedNodes(nodes);
      setUrlPhase("filtering");
    } catch (err) {
      setErrorDialogMessage(err instanceof Error ? err.message : "获取订阅失败");
      setIsErrorDialogOpen(true);
      setUrlPhase("input");
    } finally {
      setIsLoadingModalOpen(false);
    }
  }

  // 执行测速
  async function executeSpeedTest() {
    let rawInput = subscriptionText;
    if (inputMode === "url" && urlPhase === "filtering") {
      if (filteredUrlNodes.regexError) {
        setRegexFilterError(filteredUrlNodes.regexError);
        setErrorDialogMessage(filteredUrlNodes.regexError);
        setIsErrorDialogOpen(true);
        return;
      }
      setRegexFilterError("");
      rawInput = filteredUrlNodes.nodes.map((node) => node.raw).join("\n");
    }

    setError("");
    setConfigCollapsed(true);
    setExpandedCountries(new Set());
    speedtestCtx.startSpeedtest();

    const parsedNodes = await parseNodesWithFallback(rawInput);

    const initialStates = new Map<string, NodeTestState>();
    parsedNodes.forEach((node, idx) => {
      const nodeKey = `node-${idx}`;
      initialStates.set(nodeKey, {
        node,
        status: "pending",
      });
    });
    nodeOrderRef.current = parsedNodes.map((_, idx) => `node-${idx}`);
    nodeIdToNameRef.current = new Map(parsedNodes.map((_, idx) => [`node-${idx}`, `node-${idx}`]));
    nodeNameToIdRef.current = new Map(parsedNodes.map((node, idx) => [node.name.trim(), `node-${idx}`]));
    // Initialize node states in context
    parsedNodes.forEach((_, idx) => {
      const nodeKey = `node-${idx}`;
      const existing = initialStates.get(nodeKey);
      if (existing) {
        speedtestCtx.updateNodeState(nodeKey, existing);
      }
    });

    try {
      const numericConcurrency = Number(concurrency);
      const numericTimeout = Number(timeoutMs);
      const config: SpeedTestTaskConfig = {
        concurrency: Number.isFinite(numericConcurrency) && numericConcurrency > 0 ? numericConcurrency : 4,
        target_sites: targetSites
          .split(",")
          .map((item) => item.trim())
          .filter((item) => item.length > 0),
        enable_upload_test: enableUploadTest,
        timeout_ms: Number.isFinite(numericTimeout) && numericTimeout > 0 ? numericTimeout : 8000,
      };
      const speedtestResults = await runSpeedTestBatch(rawInput, undefined, config);
      speedtestCtx.replaceResults(speedtestResults);

    } catch (err) {
      setError(err instanceof Error ? err.message : "测速任务执行失败");
    } finally {
      speedtestCtx.stopSpeedtest();
    }
  }

  const toggleCountryExpanded = useCallback((countryCode: string) => {
    setExpandedCountries((prev) => {
      const next = new Set(prev);
      if (next.has(countryCode)) {
        next.delete(countryCode);
      } else {
        next.add(countryCode);
      }
      return next;
    });
  }, []);

  // 实时地区汇总
  const liveCountrySummaries = useMemo((): CountrySpeedSummary[] => {
    const countryMap = new Map<string, { results: SpeedTestResult[]; testingNodes: NodeTestState[]; completed: number; total: number }>();

    nodeStates.forEach((state) => {
      const code = state.node.country.toUpperCase();
      if (!countryMap.has(code)) {
        countryMap.set(code, { results: [], testingNodes: [], completed: 0, total: 0 });
      }
      const entry = countryMap.get(code)!;
      entry.total++;
      if (state.status === "completed" && state.result) {
        entry.results.push(state.result);
        entry.completed++;
      } else {
        entry.testingNodes.push(state);
      }
    });

    return Array.from(countryMap.entries()).map(([code, data]) => {
      const speeds = data.results.map((r) => r.avg_download_mbps);
      const pings = data.results.map((r) => r.tcp_ping_ms).filter((p) => p > 0);
      const maxSpeed = speeds.length > 0 ? Math.max(...speeds) : 0;
      const avgSpeed = speeds.length > 0 ? speeds.reduce((a, b) => a + b, 0) / speeds.length : 0;
      const avgPing = pings.length > 0 ? pings.reduce((a, b) => a + b, 0) / pings.length : 0;
      const minPing = pings.length > 0 ? Math.min(...pings) : 0;

      return {
        country_code: code,
        country_name: getCountryName(code),
        node_count: data.total,
        avg_download_mbps: avgSpeed,
        max_download_mbps: maxSpeed,
        avg_tcp_ping_ms: avgPing,
        min_tcp_ping_ms: minPing,
        status: getSpeedStatus(avgSpeed),
        results: data.results.sort((a, b) => b.avg_download_mbps - a.avg_download_mbps),
        testingNodes: data.testingNodes.sort((a, b) => {
          const statusOrder: Record<string, number> = { testing: 0, pending: 1, completed: 2, error: 3 };
          return (statusOrder[a.status] ?? 4) - (statusOrder[b.status] ?? 4);
        }),
      };
    }).filter((s) => s.node_count > 0).sort((a, b) => b.avg_download_mbps - a.avg_download_mbps);
  }, [nodeStates]);

  // 当前正在测速的地区代码
  const activeCountryCode = useMemo(() => {
    for (const state of nodeStates.values()) {
      if (state.status === "testing") {
        return state.node.country.toUpperCase();
      }
    }
    return undefined;
  }, [nodeStates]);

  const selectedFlowRows = useMemo(() => {
    if (!selectedIspFlow) return [];
    const rows: Array<{
      node_name: string;
      ingress_geoip: {
        ip: string;
        country_code: string;
        country_name: string;
        isp: string;
      };
      egress_geoip: {
        ip: string;
        country_code: string;
        country_name: string;
        isp: string;
      };
    }> = [];
    for (const flow of ispFlowByNode.values()) {
      const sourceLabel = toIspKey(flow.ingress_geoip);
      const targetLabel = toIspKey(flow.egress_geoip);
      if (
        sourceLabel === selectedIspFlow.sourceLabel &&
        targetLabel === selectedIspFlow.targetLabel
      ) {
        rows.push(flow);
      }
    }
    return rows.sort((a, b) => a.node_name.localeCompare(b.node_name, "zh-CN"));
  }, [ispFlowByNode, selectedIspFlow, toIspKey]);

  // FIX: The previous useEffect that called setSelectedIspFlow(null) when
  // selectedFlowRows.length === 0 has been removed. It caused a redundant
  // setState → re-render cycle every time a flow was deselected (the rows
  // become empty at the same moment selectedIspFlow is set to null, so the
  // effect fired immediately and triggered another render for no reason).
  // Clearing the selection on toggle is already handled inside
  // handleIspFlowSelect below, so no separate effect is needed.

  const handleIspFlowSelect = useCallback((flow: SelectedFlow | null) => {
    if (!flow) {
      console.info(`${ISP_TABLE_LOG_PREFIX} onFlowSelect(null)`);
      setSelectedIspFlow(null);
      return;
    }
    const key = `${flow.sourceLabel}-->${flow.targetLabel}`;
    const now = Date.now();
    if (
      lastFlowClickRef.current &&
      lastFlowClickRef.current.key === key &&
      now - lastFlowClickRef.current.at < 250
    ) {
      return;
    }
    lastFlowClickRef.current = { key, at: now };
    setSelectedIspFlow((prev) => {
      if (
        prev &&
        prev.sourceLabel === flow.sourceLabel &&
        prev.targetLabel === flow.targetLabel
      ) {
        return null;
      }
      return flow;
    });
  }, []);

  return (
    <div className="flex flex-col gap-6 max-w-5xl">
      {/* 页面标题 */}
      <div className="flex items-center gap-3">
        {/* <div className="p-2.5 rounded-xl bg-primary/10">
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" className="text-primary">
            <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" fill="currentColor" />
          </svg>
        </div> */}
        <div>
          <h1 className="text-2xl font-bold">测速中心</h1>
          <p className="text-foreground-500 text-sm">配置测速任务并执行，支持批量输入节点以及自动从订阅链接获取。</p>
        </div>
      </div>

      {/* 测速错误提示 */}
      {progressEvent && progressEvent.stage === "error" && (
        <Alert status="danger" className="w-full">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>测速错误</Alert.Title>
            <Alert.Description>{progressEvent.message}</Alert.Description>
          </Alert.Content>
        </Alert>
      )}

      {/* 通用错误提示 */}
      {error && (
        <Alert status="danger" className="w-full">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>操作失败</Alert.Title>
            <Alert.Description>{error}</Alert.Description>
          </Alert.Content>
          <CloseButton onPress={() => setError("")} />
        </Alert>
      )}

      {/* 测速配置卡片 */}
      <Card>
        <CardHeader>
          <div className="flex w-full items-center justify-between gap-3">
            <div className="flex flex-col gap-1">
              <CardTitle>测速任务配置</CardTitle>
              <CardTitle className="text-foreground-500 text-sm font-normal">选择输入模式并配置测速参数</CardTitle>
            </div>
            <Button
              variant="ghost"
              size="sm"
              onPress={() => setConfigCollapsed((v) => !v)}
            >
              {configCollapsed ? "展开" : "折叠"}
            </Button>
          </div>
        </CardHeader>
        {!configCollapsed && (
          <CardContent>
            <SpeedTestConfigForm
              inputMode={inputMode}
              setInputMode={setInputMode}
              subscriptionText={subscriptionText}
              setSubscriptionText={setSubscriptionText}
              subscriptionUrl={subscriptionUrl}
              setSubscriptionUrl={setSubscriptionUrl}
              fetchingUrl={fetchingUrl}
              concurrency={concurrency}
              setConcurrency={setConcurrency}
              timeoutMs={timeoutMs}
              setTimeoutMs={setTimeoutMs}
              targetSites={targetSites}
              setTargetSites={setTargetSites}
              enableUploadTest={enableUploadTest}
              setEnableUploadTest={setEnableUploadTest}
              urlPhase={urlPhase}
              fetchedNodes={fetchedNodes}
              filterMode={filterMode}
              setFilterMode={setFilterMode}
              regexFilter={regexFilter}
              setRegexFilter={setRegexFilter}
              regexFilterError={regexFilterError}
              filteredNodes={filteredUrlNodes.nodes}
              selectedRegions={selectedRegions}
              setSelectedRegions={setSelectedRegions}
              running={running}
              onStartSpeedTest={onStartSpeedTest}
            />
          </CardContent>
        )}
      </Card>

      {/* 实时全球测速地图 */}
      {nodeStates.size > 0 && (
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <CardTitle>实时全球测速</CardTitle>
              {running && (
                <Chip variant="soft" color="warning" size="sm">
                  测速中
                </Chip>
              )}
            </div>
          </CardHeader>
          <CardContent>
            <div className="h-[300px] w-full">
              <SpeedTestMap
                countrySummaries={liveCountrySummaries}
                activeCountryCode={activeCountryCode}
              />
            </div>
          </CardContent>
        </Card>
      )}

      {/* 实时测速卡片 */}
      {running && (
        <LiveSpeedCard
          progressEvent={progressEvent}
          running={running}
          nodeStates={nodeStates}
        />
      )}

      {/* 地区排名卡片 */}
      {liveCountrySummaries.length > 0 && (
        <CountryRankingGrid
          liveCountrySummaries={liveCountrySummaries}
          expandedCountries={expandedCountries}
          toggleCountryExpanded={toggleCountryExpanded}
        />
      )}

      {/* ISP 流量 Sankey 图 */}
      {nodeStates.size > 0 && (
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <CardTitle>入口/出口ISP分析</CardTitle>
              <Chip variant="soft" color="accent" size="sm">
                {(() => {
                  let completed = 0;
                  for (const state of nodeStates.values()) {
                    if (state.status === "completed") completed++;
                  }
                  return `${completed} 个节点已测速`;
                })()}
              </Chip>
            </div>
          </CardHeader>
          <CardContent>
            <div className="h-[400px] w-full">
              <ISPFlowSankey
                ispFlowByNode={ispFlowByNode}
                onFlowSelect={handleIspFlowSelect}
              />
            </div>
            <div className="mt-1 p-3">
              {!selectedIspFlow && (
                <div className="text-sm text-foreground-500">
                  请点击上方桑基图中的某一条流向，查看该流向下的节点入口/出口 ISP 明细。
                </div>
              )}
              {selectedIspFlow && (
                <div className="flex flex-col gap-3">
                  <div className="text-sm font-medium">
                    已选流向：{selectedIspFlow.sourceLabel} → {selectedIspFlow.targetLabel}
                    <span className="ml-2 text-foreground-500">({selectedFlowRows.length} 个节点)</span>
                  </div>
                  <div>
                    <Table variant="secondary">
                      <Table.ScrollContainer className="max-h-[320px] rounded-large">
                        <Table.Content aria-label="流向节点入口出口明细表">
                          <Table.Header>
                            <Table.Column isRowHeader className="min-w-[220px]">节点名</Table.Column>
                            <Table.Column className="min-w-[220px]">入口 ISP</Table.Column>
                            <Table.Column className="min-w-[150px]">入口 IP</Table.Column>
                            <Table.Column className="min-w-[220px]">出口 ISP</Table.Column>
                            <Table.Column className="min-w-[150px]">出口 IP</Table.Column>
                          </Table.Header>
                          <Table.Body>
                            {selectedFlowRows.map((row) => (
                              <Table.Row key={`${row.node_name}-${row.ingress_geoip.ip}-${row.egress_geoip.ip}`}>
                                <Table.Cell>{row.node_name}</Table.Cell>
                                <Table.Cell>
                                  {row.ingress_geoip.country_name} {row.ingress_geoip.isp}
                                </Table.Cell>
                                <Table.Cell>
                                  <span className="font-mono text-xs">{row.ingress_geoip.ip}</span>
                                </Table.Cell>
                                <Table.Cell>
                                  {row.egress_geoip.country_name} {row.egress_geoip.isp}
                                </Table.Cell>
                                <Table.Cell>
                                  <span className="font-mono text-xs">{row.egress_geoip.ip}</span>
                                </Table.Cell>
                              </Table.Row>
                            ))}
                          </Table.Body>
                        </Table.Content>
                      </Table.ScrollContainer>
                    </Table>
                  </div>
                </div>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {/* Loading Overlay */}
      {isLoadingModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
          <Card className="min-w-[300px] max-w-md mx-4">
            <CardContent className="flex flex-col items-center gap-4 py-6">
              <Spinner size="lg" color="accent" />
              <div className="text-center">
                <p className="font-medium">正在获取节点</p>
                <p className="text-sm text-foreground-500 mt-1">正在从订阅链接获取并解析节点，请稍候...</p>
              </div>
            </CardContent>
          </Card>
        </div>
      )}

      {/* Error Dialog */}
      {isErrorDialogOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
          <Card className="min-w-[300px] max-w-md mx-4">
            <CardHeader className="pb-0">
              <div className="flex items-center gap-2 text-danger">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/>
                </svg>
                <span className="font-semibold">获取订阅失败</span>
              </div>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-foreground-600 mt-2">{errorDialogMessage}</p>
            </CardContent>
            <CardContent className="justify-end flex">
              <Button variant="primary" size="sm" onPress={() => setIsErrorDialogOpen(false)}>确定</Button>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}
