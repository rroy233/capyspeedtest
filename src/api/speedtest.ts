import type { NodeFilter, NodeTestState, SpeedTestProgressEvent, SpeedTestResult, SpeedTestTaskConfig } from "../types/speedtest";
import type { KernelDownloadProgressEvent } from "../types/speedtest";
import { invokeTauri, asNumberOrUndefined, asStringOrUndefined, asBooleanOrUndefined } from "./helpers";

export async function runSpeedTestBatch(
  rawInput: string,
  filter: NodeFilter | undefined,
  config: SpeedTestTaskConfig
): Promise<SpeedTestResult[]> {
  return invokeTauri<SpeedTestResult[]>("run_speedtest_batch", {
    rawInput,
    filter,
    config,
  });
}

export async function getSpeedtestCheckpoint(): Promise<{
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
} | null> {
  return invokeTauri("get_speedtest_checkpoint", {});
}

export async function clearSpeedtestCheckpoint(): Promise<void> {
  return invokeTauri("clear_speedtest_checkpoint", {});
}

export async function resumeSpeedtestFromCheckpoint(): Promise<SpeedTestResult[]> {
  return invokeTauri<SpeedTestResult[]>("resume_speedtest_from_checkpoint", {});
}

export async function listenSpeedTestProgress(
  callback: (event: SpeedTestProgressEvent) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<Record<string, unknown>>("speedtest://progress", (event) => {
    const payload = event.payload ?? {};
    const normalized: SpeedTestProgressEvent = {
      task_id: String(payload.task_id ?? payload.taskId ?? ""),
      event_seq: Number(payload.event_seq ?? payload.eventSeq ?? 0),
      event_type: String(payload.event_type ?? payload.eventType ?? "info_update"),
      total: Number(payload.total ?? 0),
      completed: Number(payload.completed ?? 0),
      current_node: String(payload.current_node ?? payload.currentNode ?? ""),
      node_id: asStringOrUndefined(payload.node_id ?? payload.nodeId),
      stage: String(payload.stage ?? ""),
      message: String(payload.message ?? ""),
      metric_id: asStringOrUndefined(payload.metric_id ?? payload.metricId),
      metric_value: asNumberOrUndefined(payload.metric_value ?? payload.metricValue),
      metric_unit: asStringOrUndefined(payload.metric_unit ?? payload.metricUnit),
      metric_final: asBooleanOrUndefined(payload.metric_final ?? payload.metricFinal),
      tcp_ping_ms: asNumberOrUndefined(payload.tcp_ping_ms ?? payload.tcpPingMs),
      site_ping_ms: asNumberOrUndefined(payload.site_ping_ms ?? payload.sitePingMs),
      avg_download_mbps: asNumberOrUndefined(payload.avg_download_mbps ?? payload.avgDownloadMbps),
      max_download_mbps: asNumberOrUndefined(payload.max_download_mbps ?? payload.maxDownloadMbps),
      avg_upload_mbps: asNumberOrUndefined(payload.avg_upload_mbps ?? payload.avgUploadMbps),
      max_upload_mbps: asNumberOrUndefined(payload.max_upload_mbps ?? payload.maxUploadMbps),
      ingress_geoip: (payload.ingress_geoip ?? payload.ingressGeoip) as SpeedTestProgressEvent["ingress_geoip"],
      egress_geoip: (payload.egress_geoip ?? payload.egressGeoip) as SpeedTestProgressEvent["egress_geoip"],
      geoip_snapshot: (payload.geoip_snapshot ?? payload.geoipSnapshot) as SpeedTestProgressEvent["geoip_snapshot"],
    };
    callback(normalized);
  });
  return () => {
    void unlisten();
  };
}

export async function listenKernelDownloadProgress(
  callback: (event: KernelDownloadProgressEvent) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<KernelDownloadProgressEvent>("kernel://download/progress", (event) => {
    callback(event.payload);
  });
  return () => {
    void unlisten();
  };
}
