import type { NodeInfo, NodeFilter } from "../types/speedtest";
import type { SubscriptionFetchProgressEvent } from "../types/settings";
import { invokeTauri } from "./helpers";

export async function parseSubscriptionNodes(rawInput: string, filter?: NodeFilter): Promise<NodeInfo[]> {
  return invokeTauri<NodeInfo[]>("parse_subscription_nodes", { rawInput, filter });
}

export async function fetchSubscriptionNodesFromUrl(url: string): Promise<NodeInfo[]> {
  return invokeTauri<NodeInfo[]>("fetch_subscription_from_url", { url });
}

export async function listenSubscriptionFetchProgress(
  callback: (event: SubscriptionFetchProgressEvent) => void
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<SubscriptionFetchProgressEvent>("subscription://fetch/progress", (event) => {
    callback(event.payload);
  });
  return () => {
    void unlisten();
  };
}
