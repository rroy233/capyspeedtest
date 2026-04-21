/**
 * ISPFlowSankey — 入口/出口 ISP 桑基图
 *
 * ── 事件绑定架构说明 ──────────────────────────────────────────────────────────
 *
 * @ant-design/charts 内部（useChart hook）的关键行为：
 *
 *   1. 图表实例只在 mount 时创建一次（mount effect 依赖数组为 []）。
 *   2. config props 变化时走 update effect：lodash.isEqual(旧config, 新config)
 *      为 false 时调用 chart.update(config) + chart.render()。
 *   3. 函数引用不同即被 isEqual 判定为不等。
 *   4. onEvent 留在 config 里（BaseChart 只剔除 onReady），引用变化就会触发
 *      chart.update()。
 *   5. chart._emitter（chart.on/off 的底层）在 render/update 后【不会】被清空，
 *      只有 destroy 才清空。任何在 onReady 或 useEffect 里调用
 *      chart.on('click', fn) 的做法，都会随着每次 chart.update() 叠加新
 *      handler，最终 N 个 handler 同时触发，形成正反馈死循环。
 *   6. onEvent 在 mount effect 里通过 chartInstance.on('*', handler) 绑定，
 *      且只绑一次，永远不会重新绑定或叠加。
 *
 * 本文件实现：
 *
 *   • 完全不使用 chart.on() / onReady 绑定点击事件，从根本上消除叠加风险。
 *   • 使用官方 onEvent prop，它在 mount 时绑定一次，终身不变。
 *   • onEvent 的引用用 useRef().current 固定为永久不变的 stableOnEvent，
 *     保证 isEqual 始终认为 config 的这一字段没变，不触发 chart.update()。
 *   • stableOnEvent 内部通过 handleClickRef 转发到最新的 handleClick 闭包，
 *     保证处理逻辑始终是最新的（可读到最新的 onFlowSelect）。
 *   • config 用 useMemo 包裹，只在 sankeyData/flowMeta/nodeMeta 变化时重建。
 *   • 所有其他函数字段（tooltip items、style.labelText、linkColorField）关闭
 *     在 flowMeta/nodeMeta（stable Maps）或模块级纯函数上，引用稳定。
 *
 * ─────────────────────────────────────────────────────────────────────────────
 */

import { memo, useCallback, useEffect, useMemo, useRef } from "react";
import { Sankey } from "@ant-design/charts";
import type { GeoIpInfo } from "../../types/speedtest";

// ─── Types ────────────────────────────────────────────────────────────────────

export interface SelectedFlow {
  sourceLabel: string;
  targetLabel: string;
}

interface ISPFlowSankeyProps {
  ispFlowByNode: Map<
    string,
    {
      node_name: string;
      ingress_geoip: GeoIpInfo;
      egress_geoip: GeoIpInfo;
    }
  >;
  onFlowSelect?: (flow: SelectedFlow | null) => void;
}

interface SankeyLink {
  source: string;
  target: string;
  source_label: string;
  target_label: string;
  value: number;
  node_count: number;
  node_names: string[];
  node_names_summary: string;
}

// ─── Module-level constants ───────────────────────────────────────────────────

const OFFICIAL_LIKE_COLORS = [
  "#5B8FF9", "#61DDAA", "#65789B", "#F6BD16", "#7262FD",
  "#78D3F8", "#9661BC", "#F6903D", "#008685", "#F08BB4",
];

const INGRESS_PREFIX = "__ingress__:";
const EGRESS_PREFIX  = "__egress__:";

const ISP_SANKEY_LOG_PREFIX = "[ISPFlowSankey]";

// Fields that never change across any render — defined at module level so
// isEqual always sees the same object reference for these sub-trees.
const STATIC_CONFIG = {
  sourceField: "source",
  targetField: "target",
  weightField: "value",
  autoFit: true,
  height: 360,
  padding: [24, 180, 24, 180] as [number, number, number, number],
  scale: { color: { range: OFFICIAL_LIKE_COLORS } },
  layout: { nodeAlign: "justify" as const, nodeWidth: 0.01, nodePadding: 0.03 },
  animation: false as const,
} as const;

// ─── Module-level pure helpers ────────────────────────────────────────────────

const toIspKey = (geoip: GeoIpInfo): string =>
  `${geoip.country_name || "Unknown"} ${geoip.isp || "Unknown ISP"}`.trim();

const makeIngressKey = (label: string) => `${INGRESS_PREFIX}${label}`;
const makeEgressKey  = (label: string) => `${EGRESS_PREFIX}${label}`;

const stripSidePrefix = (key: string): string =>
  key.startsWith(INGRESS_PREFIX)
    ? key.slice(INGRESS_PREFIX.length)
    : key.startsWith(EGRESS_PREFIX)
      ? key.slice(EGRESS_PREFIX.length)
      : key;

const readNodeKey = (value: unknown): string => {
  if (typeof value === "string") return value;
  if (value && typeof value === "object") {
    const maybe = value as { key?: string; id?: string; name?: string };
    return maybe.key ?? maybe.id ?? maybe.name ?? "";
  }
  return "";
};

function mergeNodeNamesBySharedPrefix(names: string[]): string {
  const clean = Array.from(new Set(names.map((n) => n.trim()).filter(Boolean)));
  if (clean.length === 0) return "-";
  if (clean.length === 1) return clean[0];

  const tokenized = clean.map((name) => name.split(/\s+/).filter(Boolean));
  const minLen = Math.min(...tokenized.map((parts) => parts.length));

  let prefixLen = 0;
  while (
    prefixLen < minLen &&
    tokenized.every((parts) => parts[prefixLen] === tokenized[0][prefixLen])
  ) prefixLen++;

  let suffixLen = 0;
  while (suffixLen < minLen - prefixLen) {
    const idx   = tokenized[0].length - 1 - suffixLen;
    const token = tokenized[0][idx];
    const same  = tokenized.every((parts) => {
      const partIdx = parts.length - 1 - suffixLen;
      return partIdx >= prefixLen && parts[partIdx] === token;
    });
    if (!same) break;
    suffixLen++;
  }

  const prefix = tokenized[0].slice(0, prefixLen).join(" ");
  const suffix = suffixLen > 0
    ? tokenized[0].slice(tokenized[0].length - suffixLen).join(" ")
    : "";

  const middles = tokenized.map((parts) => {
    const end = suffixLen > 0 ? parts.length - suffixLen : parts.length;
    return parts.slice(prefixLen, end).join(" ").trim();
  });

  const uniqueMiddles = Array.from(new Set(middles.filter(Boolean)));
  const output = [prefix, uniqueMiddles.join(" "), suffix]
    .filter(Boolean)
    .join(" ")
    .trim();
  return output || clean.join("、");
}

// ─── Component ────────────────────────────────────────────────────────────────

function ISPFlowSankeyInner({ ispFlowByNode, onFlowSelect }: ISPFlowSankeyProps) {

  // ── Data ──────────────────────────────────────────────────────────────────

  const sankeyData = useMemo((): SankeyLink[] => {
    const flowMap = new Map<string, SankeyLink>();

    for (const flow of ispFlowByNode.values()) {
      const sourceLabel = toIspKey(flow.ingress_geoip);
      const targetLabel = toIspKey(flow.egress_geoip);
      if (!sourceLabel || !targetLabel) continue;

      const source = makeIngressKey(sourceLabel);
      const target = makeEgressKey(targetLabel);
      const key    = `${source} --> ${target}`;

      const existing = flowMap.get(key);
      if (existing) {
        existing.value      += 1;
        existing.node_count += 1;
        existing.node_names.push(flow.node_name);
      } else {
        flowMap.set(key, {
          source, target,
          source_label: sourceLabel,
          target_label: targetLabel,
          value: 1, node_count: 1,
          node_names: [flow.node_name],
          node_names_summary: "",
        });
      }
    }

    return Array.from(flowMap.values())
      .map((item) => {
        const uniqueNodeNames = Array.from(new Set(item.node_names));
        return {
          ...item,
          node_names: uniqueNodeNames,
          node_names_summary: mergeNodeNamesBySharedPrefix(uniqueNodeNames),
        };
      })
      .sort((a, b) => b.value - a.value)
      .slice(0, 20);
  }, [ispFlowByNode]);

  const { flowMeta, nodeMeta } = useMemo(() => {
    const fm = new Map(
      sankeyData.map((item) => [
        `${item.source} --> ${item.target}`,
        { node_count: item.node_count, node_names_summary: item.node_names_summary },
      ])
    );
    const nm = new Map<string, { node_count: number; node_names: Set<string> }>();
    for (const item of sankeyData) {
      for (const side of [item.source, item.target] as const) {
        const entry = nm.get(side) ?? { node_count: 0, node_names: new Set<string>() };
        entry.node_count += item.node_count;
        item.node_names.forEach((n) => entry.node_names.add(n));
        nm.set(side, entry);
      }
    }
    return { flowMeta: fm, nodeMeta: nm };
  }, [sankeyData]);

  // ── Click handling ────────────────────────────────────────────────────────

  const lastEmitRef = useRef<{ key: string; at: number } | null>(null);

  // Recreated only when onFlowSelect changes. Contains the latest closure.
  const handleClick = useCallback((event: unknown) => {
    if (!onFlowSelect) return;

    // G2 click event shape:
    //   { type: 'click', data: { data: <mark datum> }, nativeEvent: true, ... }
    // Sankey link datum: { source: {key, ...}, target: {key, ...}, value, ... }
    // Sankey node datum: { key: string, ... }
    const eventObj = event as Record<string, unknown>;
    const data  = eventObj?.data as Record<string, unknown> | undefined;
    const datum = data?.data as Record<string, unknown> | undefined;
    if (!datum) return;

    let sourceKey: string;
    let targetKey: string;

    if (datum.source !== undefined && datum.target !== undefined) {
      // Link click — this is what we want
      sourceKey = readNodeKey(datum.source);
      targetKey = readNodeKey(datum.target);
    } else if (datum.key !== undefined) {
      // Node click — not a flow, ignore
      return;
    } else {
      // Fallback: scan all strings for ingress/egress prefixes
      const allStrings: string[] = [];
      const collect = (v: unknown, depth = 0) => {
        if (depth > 5 || v == null) return;
        if (typeof v === "string") { allStrings.push(v); return; }
        if (typeof v === "object") {
          for (const child of Object.values(v as Record<string, unknown>)) {
            collect(child, depth + 1);
          }
        }
      };
      collect(datum);
      sourceKey = allStrings.find((s) => s.startsWith(INGRESS_PREFIX)) ?? "";
      targetKey = allStrings.find((s) => s.startsWith(EGRESS_PREFIX))  ?? "";
      if (!sourceKey || !targetKey) return;
    }

    const sourceLabel = stripSidePrefix(sourceKey);
    const targetLabel = stripSidePrefix(targetKey);
    if (!sourceLabel || !targetLabel) return;

    // Deduplicate: G2 can fire multiple click events per user click due to
    // event bubbling through nested shape elements.
    const key = `${sourceLabel}-->${targetLabel}`;
    const now = Date.now();
    if (
      lastEmitRef.current &&
      lastEmitRef.current.key === key &&
      now - lastEmitRef.current.at < 300
    ) return;

    lastEmitRef.current = { key, at: now };
    console.info(`${ISP_SANKEY_LOG_PREFIX} flow selected: ${sourceLabel} → ${targetLabel}`);
    onFlowSelect({ sourceLabel, targetLabel });
  }, [onFlowSelect]);

  // Always tracks the latest handleClick without changing identity.
  const handleClickRef = useRef(handleClick);
  useEffect(() => { handleClickRef.current = handleClick; }, [handleClick]);

  // stableOnEvent: created exactly once for this component's lifetime.
  //
  // This is the value passed as `onEvent` in config. Because it is created via
  // useRef().current it has a fixed identity — the same function reference for
  // every render. lodash.isEqual will therefore always return true for this
  // field, and chart.update() will never be triggered on its behalf.
  //
  // All live logic is forwarded through handleClickRef so the handler always
  // executes with the current onFlowSelect closure regardless.
  const stableOnEvent = useRef(
    (_chartInstance: unknown, event: unknown) => {
      // onEvent receives ALL G2 events. Filter to click only.
      const e = event as { type?: string } | undefined;
      if (e?.type !== "click") return;
      handleClickRef.current(event);
    }
  ).current;

  // ── Chart config ──────────────────────────────────────────────────────────

  const config = useMemo(() => ({
    ...STATIC_CONFIG,
    data: sankeyData,
    // onEvent reference is permanently stable — never causes isEqual diff.
    onEvent: stableOnEvent,
    linkColorField: (d: { source?: { key?: string } }) => d.source?.key ?? "",
    style: {
      labelText:       (d: { key?: string }) => stripSidePrefix(d?.key ?? ""),
      labelSpacing:    6,
      labelFontSize:   22,
      labelFontWeight: 400,
      labelFill:       "#4b5563",
      nodeLineWidth:   0,
      nodeStroke:      "transparent",
      nodeStrokeWidth: 0,
      linkFillOpacity: 0.4,
      linkStroke:      "transparent",
    },
    tooltip: {
      nodeTitle: "",
      nodeItems: [
        (d: unknown) => ({
          name: "ISP",
          value: stripSidePrefix(readNodeKey(d)) || "-",
        }),
        (d: unknown) => ({
          name: "节点数",
          value: String(nodeMeta.get(readNodeKey(d))?.node_count ?? 0),
        }),
        (d: unknown) => {
          const meta = nodeMeta.get(readNodeKey(d));
          return {
            name: "节点名",
            value: meta ? mergeNodeNamesBySharedPrefix(Array.from(meta.node_names)) : "-",
          };
        },
      ],
      linkTitle: "",
      linkItems: [
        (d: unknown) => ({
          name: "入口 ISP",
          value: stripSidePrefix(readNodeKey((d as { source?: unknown })?.source)) || "-",
        }),
        (d: unknown) => ({
          name: "出口 ISP",
          value: stripSidePrefix(readNodeKey((d as { target?: unknown })?.target)) || "-",
        }),
        (d: unknown) => {
          const src  = readNodeKey((d as { source?: unknown })?.source);
          const tgt  = readNodeKey((d as { target?: unknown })?.target);
          const meta = flowMeta.get(`${src} --> ${tgt}`);
          const fallback =
            (d as { value?: number })?.value ??
            (d as { datum?: { value?: number } })?.datum?.value ?? 0;
          return { name: "节点数", value: String(meta?.node_count ?? fallback) };
        },
        (d: unknown) => {
          const src = readNodeKey((d as { source?: unknown })?.source);
          const tgt = readNodeKey((d as { target?: unknown })?.target);
          return {
            name: "节点名",
            value: flowMeta.get(`${src} --> ${tgt}`)?.node_names_summary ?? "-",
          };
        },
      ],
    },
  }), [sankeyData, flowMeta, nodeMeta, stableOnEvent]);

  // ── Render ────────────────────────────────────────────────────────────────

  if (sankeyData.length === 0) {
    return (
      <div className="h-[400px] w-full flex items-center justify-center text-foreground-500 text-sm">
        {ispFlowByNode.size === 0 ? "等待测速结果..." : "暂无 ISP 流量数据"}
      </div>
    );
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (
    <div className="h-full w-full">
      <Sankey {...(config as any)} />
    </div>
  );
}

export const ISPFlowSankey = memo(ISPFlowSankeyInner);
export default ISPFlowSankey;
