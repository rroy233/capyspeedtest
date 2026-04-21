import { useEffect, useRef, useState } from "react";
import type { KernelDownloadProgressEvent, SpeedTestProgressEvent, NodeTestState } from "../types/speedtest";
import { listenKernelDownloadProgress, listenSpeedTestProgress } from "../api/speedtest";
import { APP_EXIT_EVENT, setSpeedtestRunning } from "../utils/runtimeLifecycle";

interface UseSpeedTestProgressOptions {
  onKernelDownloadAlert?: (event: KernelDownloadProgressEvent) => void;
}

interface UseSpeedTestProgressResult {
  progressEvent: SpeedTestProgressEvent | null;
  kernelDownloadProgress: KernelDownloadProgressEvent | null;
  nodeStates: Map<string, NodeTestState>;
  activeTaskIdRef: React.MutableRefObject<string>;
  prevNodeNameRef: React.MutableRefObject<string | null>;
  nodeOrderRef: React.MutableRefObject<string[]>;
  nodeIdToNameRef: React.MutableRefObject<Map<string, string>>;
  setNodeStates: React.Dispatch<React.SetStateAction<Map<string, NodeTestState>>>;
}

export function useSpeedTestProgress(
  options: UseSpeedTestProgressOptions = {}
): UseSpeedTestProgressResult {
  const { onKernelDownloadAlert } = options;
  const [progressEvent, setProgressEvent] = useState<SpeedTestProgressEvent | null>(null);
  const [kernelDownloadProgress, setKernelDownloadProgress] = useState<KernelDownloadProgressEvent | null>(null);
  const [nodeStates, setNodeStates] = useState<Map<string, NodeTestState>>(new Map());
  const prevNodeNameRef = useRef<string | null>(null);
  const nodeOrderRef = useRef<string[]>([]);
  const nodeIdToNameRef = useRef<Map<string, string>>(new Map());
  const activeTaskIdRef = useRef<string>("");

  useEffect(() => {
    let mounted = true;
    let disposer: (() => void) | null = null;
    let kernelDisposer: (() => void) | null = null;
    let cleaned = false;

    const cleanupConnections = () => {
      if (cleaned) return;
      cleaned = true;
      mounted = false;
      prevNodeNameRef.current = null;
      setSpeedtestRunning(false);
      if (disposer) disposer();
      if (kernelDisposer) kernelDisposer();
    };

    const handleAppExit = () => {
      cleanupConnections();
    };

    async function bindListener() {
      disposer = await listenSpeedTestProgress((event) => {
        if (!mounted) return;
        if (activeTaskIdRef.current && event.task_id && activeTaskIdRef.current !== event.task_id) {
          return;
        }
        if (!activeTaskIdRef.current && event.task_id) {
          activeTaskIdRef.current = event.task_id;
        }
        setProgressEvent(event);
        let resolvedCurrentNode: string | null = null;

        setNodeStates((prev) => {
          const next = new Map(prev);
          const prevNode = prevNodeNameRef.current;

          const eventNodeName = (event.current_node || "").trim();
          let targetNodeKey: string | null = null;
          if (event.node_id) {
            const mappedName = nodeIdToNameRef.current.get(event.node_id);
            if (mappedName && next.has(mappedName)) {
              targetNodeKey = mappedName;
            }
          }
          if (eventNodeName && next.has(eventNodeName)) {
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
            const candidate = Array.from(next.entries()).find(([, v]) => v.status === "pending" || v.status === "testing");
            targetNodeKey = candidate?.[0] ?? null;
          }
          if (!targetNodeKey) {
            const byOrder = nodeOrderRef.current[event.completed];
            if (byOrder && next.has(byOrder)) {
              targetNodeKey = byOrder;
            }
          }

          if (targetNodeKey) {
            resolvedCurrentNode = targetNodeKey;
            const existing = next.get(targetNodeKey);
            if (existing) {
              let newStatus: "pending" | "testing" | "completed" | "error" = "testing";
              if (event.stage === "completed") {
                newStatus = "completed";
              } else if (event.stage === "error") {
                newStatus = "error";
              }

              next.set(targetNodeKey, {
                ...existing,
                status: newStatus,
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
            }
          }

          return next;
        });

        if (resolvedCurrentNode) {
          prevNodeNameRef.current = resolvedCurrentNode;
        } else if (event.current_node) {
          prevNodeNameRef.current = event.current_node;
        }
      });

      kernelDisposer = await listenKernelDownloadProgress((event) => {
        if (!mounted) return;
        setKernelDownloadProgress(event);
        onKernelDownloadAlert?.(event);
      });
    }

    window.addEventListener(APP_EXIT_EVENT, handleAppExit);
    void bindListener();
    return () => {
      window.removeEventListener(APP_EXIT_EVENT, handleAppExit);
      cleanupConnections();
    };
  }, [onKernelDownloadAlert]);

  return {
    progressEvent,
    kernelDownloadProgress,
    nodeStates,
    activeTaskIdRef,
    prevNodeNameRef,
    nodeOrderRef,
    nodeIdToNameRef,
    setNodeStates,
  };
}
