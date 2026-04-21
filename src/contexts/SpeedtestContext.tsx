import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  useCallback,
  useMemo,
  type ReactNode,
} from "react";
import type {
  NodeTestState,
  SpeedTestProgressEvent,
  SpeedTestResult,
  SpeedTestTaskConfig,
} from "../types/speedtest";
import {
  listenSpeedTestProgress,
  getSpeedtestCheckpoint,
  clearSpeedtestCheckpoint,
} from "../api/speedtest";
import { useAlert } from "./AlertContext";
import { setSpeedtestRunning } from "../utils/runtimeLifecycle";

export interface SpeedtestCheckpoint {
  task_id: string;
  total: number;
  completed: number;
  node_names: string[];
  node_results: Array<{
    tcp_ping_ms?: number;
    site_ping_ms?: number;
    avg_download_mbps?: number;
    max_download_mbps?: number;
    avg_upload_mbps?: number;
    max_upload_mbps?: number;
    status: string;
    ingress_geoip?: { ip: string; country_code: string; country_name: string; isp: string };
    egress_geoip?: { ip: string; country_code: string; country_name: string; isp: string };
  } | null>;
  raw_input: string;
  config?: SpeedTestTaskConfig;
  saved_at: number;
}

interface HydrateFromCheckpointPayload {
  taskId: string;
  totalNodes: number;
  completedNodes: number;
  nodeStates: Map<string, NodeTestState>;
  results: SpeedTestResult[];
}

interface SpeedtestContextValue {
  // 运行时状态
  running: boolean;
  progressEvent: SpeedTestProgressEvent | null;
  nodeStates: Map<string, NodeTestState>;
  results: SpeedTestResult[];
  taskId: string;
  totalNodes: number;
  completedNodes: number;
  checkpoint: SpeedtestCheckpoint | null;
  resumeRequested: boolean;

  // Actions
  startSpeedtest: () => void;
  stopSpeedtest: () => void;
  clearSpeedtest: () => void;
  setProgressEvent: (event: SpeedTestProgressEvent) => void;
  setHomeActive: (isHome: boolean) => void;
  updateNodeState: (nodeKey: string, state: NodeTestState) => void;
  replaceResults: (nextResults: SpeedTestResult[]) => void;
  hydrateFromCheckpoint: (payload: HydrateFromCheckpointPayload) => void;
  requestResume: () => void;
  clearResumeRequest: () => void;

  // Checkpoint
  hasCheckpoint: () => boolean;
  getCheckpoint: () => SpeedtestCheckpoint | null;
  clearCheckpoint: () => Promise<void>;
  refreshCheckpoint: () => Promise<boolean>;

  // 内部使用
  _activeTaskIdRef: React.MutableRefObject<string>;
  _nodeOrderRef: React.MutableRefObject<string[]>;
  _nodeIdToNameRef: React.MutableRefObject<Map<string, string>>;
  _nodeNameToIdRef: React.MutableRefObject<Map<string, string>>;
  _prevNodeNameRef: React.MutableRefObject<string | null>;
  _ispFlowByNodeRef: React.MutableRefObject<Map<string, {
    node_name: string;
    ingress_geoip: NonNullable<SpeedTestProgressEvent["ingress_geoip"]>;
    egress_geoip: NonNullable<SpeedTestProgressEvent["egress_geoip"]>;
  }>>;
}

const SpeedtestContext = createContext<SpeedtestContextValue | null>(null);

export function useSpeedtestContext() {
  const ctx = useContext(SpeedtestContext);
  if (!ctx) throw new Error("useSpeedtestContext must be used within SpeedtestProvider");
  return ctx;
}

interface SpeedtestProviderProps {
  children: ReactNode;
}

export function SpeedtestProvider({ children }: SpeedtestProviderProps) {
  // 核心状态
  const [running, setRunning] = useState(false);
  const [progressEvent, setProgressEventState] = useState<SpeedTestProgressEvent | null>(null);
  const [nodeStates, setNodeStates] = useState<Map<string, NodeTestState>>(new Map());
  const [results, setResults] = useState<SpeedTestResult[]>([]);

  // 任务元数据
  const [taskId, setTaskId] = useState("");
  const [totalNodes, setTotalNodes] = useState(0);
  const [completedNodes, setCompletedNodes] = useState(0);
  const [isHomeTab, setIsHomeTab] = useState(() => window.location.pathname === "/");
  const [resumeRequested, setResumeRequested] = useState(false);
  const [checkpoint, setCheckpoint] = useState<SpeedtestCheckpoint | null>(null);

  // Refs - 用于在回调中保持最新引用而不触发 re-render
  const _activeTaskIdRef = useRef("");
  const _nodeOrderRef = useRef<string[]>([]);
  const _nodeIdToNameRef = useRef<Map<string, string>>(new Map());
  const _nodeNameToIdRef = useRef<Map<string, string>>(new Map());
  const _prevNodeNameRef = useRef<string | null>(null);
  const _ispFlowByNodeRef = useRef<Map<string, {
    node_name: string;
    ingress_geoip: NonNullable<SpeedTestProgressEvent["ingress_geoip"]>;
    egress_geoip: NonNullable<SpeedTestProgressEvent["egress_geoip"]>;
  }>>(new Map());

  const { showAlert, closeAlert } = useAlert();

  // 进度事件监听
  useEffect(() => {
    let disposer: (() => void) | null = null;

    const bindListener = async () => {
      disposer = await listenSpeedTestProgress((event) => {
        // 忽略不相关任务的事件
        if (_activeTaskIdRef.current && event.task_id && _activeTaskIdRef.current !== event.task_id) {
          return;
        }
        if (!_activeTaskIdRef.current && event.task_id) {
          _activeTaskIdRef.current = event.task_id;
        }

        // 更新进度事件
        setProgressEventState(event);
        setTotalNodes(event.total);
        setCompletedNodes(event.completed);

        // 如果事件有 task_id，设置到状态
        if (event.task_id && !taskId) {
          setTaskId(event.task_id);
        }

        // 处理 geoip_snapshot
        if (event.event_type === "geoip_snapshot" && event.geoip_snapshot) {
          _ispFlowByNodeRef.current = new Map(
            event.geoip_snapshot.map((item) => [
              item.node_id,
              {
                node_name: item.node_name,
                ingress_geoip: item.ingress_geoip,
                egress_geoip: item.egress_geoip,
              },
            ])
          );
          return;
        }

        // 处理节点状态更新
        let resolvedCurrentNode: string | null = null;

        setNodeStates((prev) => {
          const next = new Map(prev);
          const prevNode = _prevNodeNameRef.current;

          const eventNodeName = (event.current_node || "").trim();
          const eventNodeId = (event.node_id || "").trim();
          let targetNodeKey: string | null = null;

          // 各种匹配策略
          if (eventNodeId && next.has(eventNodeId)) {
            targetNodeKey = eventNodeId;
          } else if (eventNodeId) {
            const mappedName = _nodeIdToNameRef.current.get(eventNodeId);
            if (mappedName && next.has(mappedName)) {
              targetNodeKey = mappedName;
            }
          }
          if (!targetNodeKey && eventNodeName && _nodeNameToIdRef.current.has(eventNodeName)) {
            const mappedId = _nodeNameToIdRef.current.get(eventNodeName)!;
            if (next.has(mappedId)) {
              targetNodeKey = mappedId;
            }
          }
          if (!targetNodeKey && eventNodeName && next.has(eventNodeName)) {
            targetNodeKey = eventNodeName;
          } else if (eventNodeName) {
            for (const key of next.keys()) {
              if (key.trim() === eventNodeName) {
                targetNodeKey = key;
                break;
              }
            }
          }
          if (!targetNodeKey && next.size === 1) {
            targetNodeKey = Array.from(next.keys())[0] ?? null;
          }
          if (!targetNodeKey && prevNode && next.has(prevNode)) {
            targetNodeKey = prevNode;
          }
          if (!targetNodeKey) {
            const candidate = Array.from(next.entries()).find(
              ([, v]) => v.status === "pending" || v.status === "testing"
            );
            targetNodeKey = candidate?.[0] ?? null;
          }
          if (!targetNodeKey) {
            const byOrder = _nodeOrderRef.current[event.completed];
            if (byOrder && next.has(byOrder)) {
              targetNodeKey = byOrder;
            }
          }

          if (targetNodeKey) {
            resolvedCurrentNode = targetNodeKey;
            const existing = next.get(targetNodeKey);
            if (existing) {
              const isTerminal = existing.status === "completed" || existing.status === "error";
              const incomingTerminal = event.stage === "completed" || event.stage === "error";
              // 忽略延迟到达的旧阶段事件
              if (isTerminal && !incomingTerminal) {
                return next;
              }

              let newStatus: NodeTestState["status"] = "testing";
              if (event.stage === "completed") {
                newStatus = "completed";
              } else if (event.stage === "error") {
                newStatus = "error";
              }

              const result: SpeedTestResult | undefined =
                event.ingress_geoip && event.egress_geoip
                  ? {
                      node: existing.node,
                      tcp_ping_ms: event.tcp_ping_ms ?? 0,
                      site_ping_ms: event.site_ping_ms ?? 0,
                      packet_loss_rate: 0,
                      avg_download_mbps: event.avg_download_mbps ?? 0,
                      max_download_mbps: event.max_download_mbps ?? 0,
                      avg_upload_mbps: event.avg_upload_mbps,
                      max_upload_mbps: event.max_upload_mbps,
                      ingress_geoip: event.ingress_geoip,
                      egress_geoip: event.egress_geoip,
                      nat_type: "",
                      finished_at: new Date().toISOString(),
                    }
                  : existing.result;

              next.set(targetNodeKey, {
                ...existing,
                status: newStatus,
                result,
                currentStage: event.stage,
                currentSpeed: {
                  tcp_ping_ms: event.tcp_ping_ms ?? existing.currentSpeed?.tcp_ping_ms,
                  site_ping_ms: event.site_ping_ms ?? existing.currentSpeed?.site_ping_ms,
                  avg_download_mbps: event.avg_download_mbps ?? existing.currentSpeed?.avg_download_mbps,
                  max_download_mbps: event.max_download_mbps ?? existing.currentSpeed?.max_download_mbps,
                  avg_upload_mbps: event.avg_upload_mbps ?? existing.currentSpeed?.avg_upload_mbps,
                  max_upload_mbps: event.max_upload_mbps ?? existing.currentSpeed?.max_upload_mbps,
                },
                errorMessage: event.stage === "error" ? event.message : undefined,
              });

              if (newStatus === "testing") {
                for (const [key, state] of next.entries()) {
                  if (key !== targetNodeKey && state.status === "testing") {
                    next.set(key, { ...state, status: "pending" });
                  }
                }
              }
            }
          }

          return next;
        });

        if (resolvedCurrentNode) {
          _prevNodeNameRef.current = resolvedCurrentNode;
        } else if (event.current_node) {
          _prevNodeNameRef.current = event.current_node;
        }

        // 处理 ISP 流量更新
        if (
          (event.event_type === "geoip_update" || event.event_type === "node_completed") &&
          event.ingress_geoip &&
          event.egress_geoip
        ) {
          const ingressGeoip = event.ingress_geoip;
          const egressGeoip = event.egress_geoip;
          const ispNodeKey =
            (event.node_id || "").trim() ||
            _nodeNameToIdRef.current.get((event.current_node || "").trim()) ||
            (resolvedCurrentNode || "").trim() ||
            (event.current_node || "").trim();

          if (ispNodeKey) {
            const current = _ispFlowByNodeRef.current.get(ispNodeKey);
            const nodeName =
              current?.node_name ||
              (event.current_node || "").trim() ||
              ispNodeKey;
            if (
              !current ||
              current.ingress_geoip.ip !== ingressGeoip.ip ||
              current.egress_geoip.ip !== egressGeoip.ip ||
              current.ingress_geoip.isp !== ingressGeoip.isp ||
              current.egress_geoip.isp !== egressGeoip.isp
            ) {
              const nextFlow = new Map(_ispFlowByNodeRef.current);
              nextFlow.set(ispNodeKey, {
                node_name: nodeName,
                ingress_geoip: ingressGeoip,
                egress_geoip: egressGeoip,
              });
              _ispFlowByNodeRef.current = nextFlow;
            }
          }
        }
      });
    };

    void bindListener();
    return () => {
      if (disposer) disposer();
    };
  }, [taskId]);

  // 后台测速 Toast 逻辑
  useEffect(() => {
    if (!running) {
      closeAlert("speedtest-background-toast");
      return;
    }
    if (isHomeTab) {
      closeAlert("speedtest-background-toast");
      return;
    }

    showAlert({
      id: "speedtest-background-toast",
      title: "测速进行中",
      description: (
        <div className="flex flex-col gap-1">
          <div className="text-sm">
            已完成: {completedNodes}/{totalNodes}
          </div>
          <div className="text-sm text-foreground-500">
            当前节点: {progressEvent?.current_node || "-"}
          </div>
        </div>
      ),
      status: "accent",
      timeout: 0,
    });
  }, [running, isHomeTab, completedNodes, totalNodes, progressEvent?.current_node, showAlert, closeAlert]);

  const setHomeActive = useCallback((isHome: boolean) => {
    setIsHomeTab(isHome);
  }, []);

  // Actions
  const startSpeedtest = useCallback(() => {
    _activeTaskIdRef.current = "";
    _prevNodeNameRef.current = null;
    _nodeOrderRef.current = [];
    _nodeIdToNameRef.current = new Map();
    _nodeNameToIdRef.current = new Map();
    _ispFlowByNodeRef.current = new Map();
    setRunning(true);
    setSpeedtestRunning(true);
    setProgressEventState(null);
    setResults([]);
    setTaskId("");
    setTotalNodes(0);
    setCompletedNodes(0);
    setCheckpoint(null);
    setResumeRequested(false);
  }, []);

  const stopSpeedtest = useCallback(() => {
    setRunning(false);
    setSpeedtestRunning(false);
    closeAlert("speedtest-background-toast");
  }, [closeAlert]);

  const clearSpeedtest = useCallback(() => {
    setRunning(false);
    setSpeedtestRunning(false);
    setProgressEventState(null);
    setNodeStates(new Map());
    setResults([]);
    setTaskId("");
    setTotalNodes(0);
    setCompletedNodes(0);
    _activeTaskIdRef.current = "";
    _prevNodeNameRef.current = null;
    _nodeOrderRef.current = [];
    _nodeIdToNameRef.current = new Map();
    _nodeNameToIdRef.current = new Map();
    _ispFlowByNodeRef.current = new Map();
    setCheckpoint(null);
    setResumeRequested(false);
    closeAlert("speedtest-background-toast");
  }, [closeAlert]);

  const setProgressEvent = useCallback((event: SpeedTestProgressEvent) => {
    setProgressEventState(event);
  }, []);

  const updateNodeState = useCallback((nodeKey: string, state: NodeTestState) => {
    setNodeStates((prev) => {
      const next = new Map(prev);
      next.set(nodeKey, state);
      return next;
    });
  }, []);

  const replaceResults = useCallback((nextResults: SpeedTestResult[]) => {
    setResults(nextResults);
  }, []);

  const hydrateFromCheckpoint = useCallback((payload: HydrateFromCheckpointPayload) => {
    _activeTaskIdRef.current = payload.taskId;
    _prevNodeNameRef.current = null;
    setRunning(true);
    setSpeedtestRunning(true);
    setTaskId(payload.taskId);
    setTotalNodes(payload.totalNodes);
    setCompletedNodes(payload.completedNodes);
    setNodeStates(new Map(payload.nodeStates));
    setResults(payload.results);
  }, []);

  const requestResume = useCallback(() => {
    setResumeRequested(true);
  }, []);

  const clearResumeRequest = useCallback(() => {
    setResumeRequested(false);
  }, []);

  // Checkpoint 操作
  const hasCheckpoint = useCallback((): boolean => {
    return checkpoint !== null;
  }, [checkpoint]);

  const getCheckpoint = useCallback((): SpeedtestCheckpoint | null => {
    return checkpoint;
  }, [checkpoint]);

  const clearCheckpoint = useCallback(async () => {
    try {
      await clearSpeedtestCheckpoint();
      setCheckpoint(null);
    } catch (e) {
      console.warn("[SpeedtestContext] clearCheckpoint 失败:", e);
    }
  }, []);

  const refreshCheckpoint = useCallback(async (): Promise<boolean> => {
    const tauriInternals = (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    if (!tauriInternals) {
      setCheckpoint(null);
      return false;
    }
    try {
      const latest = await getSpeedtestCheckpoint();
      if (latest && latest.completed < latest.total) {
        setCheckpoint(latest);
        return true;
      }
      setCheckpoint(null);
      return false;
    } catch (e) {
      console.warn("[SpeedtestContext] refreshCheckpoint 失败:", e);
      return false;
    }
  }, []);

  // 初始化时检测 Rust 侧 checkpoint
  useEffect(() => {
    void refreshCheckpoint();
  }, [refreshCheckpoint]);

  const value = useMemo<SpeedtestContextValue>(() => ({
    running,
    progressEvent,
    nodeStates,
    results,
    taskId,
    totalNodes,
    completedNodes,
    checkpoint,
    resumeRequested,
    startSpeedtest,
    stopSpeedtest,
    clearSpeedtest,
    setProgressEvent,
    setHomeActive,
    updateNodeState,
    replaceResults,
    hydrateFromCheckpoint,
    requestResume,
    clearResumeRequest,
    hasCheckpoint,
    getCheckpoint,
    clearCheckpoint,
    refreshCheckpoint,
    _activeTaskIdRef,
    _nodeOrderRef,
    _nodeIdToNameRef,
    _nodeNameToIdRef,
    _prevNodeNameRef,
    _ispFlowByNodeRef,
  }), [
    running,
    progressEvent,
    nodeStates,
    results,
    taskId,
    totalNodes,
    completedNodes,
    checkpoint,
    resumeRequested,
    startSpeedtest,
    stopSpeedtest,
    clearSpeedtest,
    setProgressEvent,
    setHomeActive,
    updateNodeState,
    replaceResults,
    hydrateFromCheckpoint,
    requestResume,
    clearResumeRequest,
    hasCheckpoint,
    getCheckpoint,
    clearCheckpoint,
    refreshCheckpoint,
  ]);

  return (
    <SpeedtestContext.Provider value={value}>
      {children}
    </SpeedtestContext.Provider>
  );
}
