import { useMemo } from "react";
import type { NodeInfo } from "../../types/speedtest";
import { Button, TextField, Label, Input, ListBox, Chip } from "@heroui/react";
import { RegionCard } from "./RegionCard";
import { FlagIcon } from "../ui/FlagChip";

interface NodeFilterPanelProps {
  nodes: NodeInfo[];
  filterMode: "all" | "regex" | "region";
  setFilterMode: (mode: "all" | "regex" | "region") => void;
  regexFilter: string;
  setRegexFilter: (regex: string) => void;
  regexFilterError?: string;
  filteredNodes: NodeInfo[];
  selectedRegions: Map<string, number>;
  setSelectedRegions: (regions: Map<string, number>) => void;
  concurrency: string;
}

export function NodeFilterPanel({
  nodes,
  filterMode,
  setFilterMode,
  regexFilter,
  setRegexFilter,
  regexFilterError,
  filteredNodes,
  selectedRegions,
  setSelectedRegions,
  concurrency,
}: NodeFilterPanelProps) {
  const targetNodes = filterMode === "all" ? nodes : filteredNodes;
  const estimatedMinutes =
    targetNodes.length === 0
      ? 0
      : Math.ceil(targetNodes.length / Math.max(1, Number(concurrency) || 4));

  const uniqueCountries = useMemo(() => {
    const countries = new Set(nodes.map((n) => n.country));
    return Array.from(countries).sort();
  }, [nodes]);

  return (
    <div className="mt-3 rounded-lg bg-default-100 p-3">
      {/* 节点摘要 */}
      <p className="mb-2 text-xs text-foreground-600">
        待测速 <strong>{targetNodes.length}</strong> 个节点，
        预计需要 <strong>{estimatedMinutes}</strong> 分钟
        {filterMode !== "all" && (
          <span className="ml-1 text-foreground-500">（原始节点 {nodes.length}）</span>
        )}
      </p>

      {/* 筛选方式按钮组 */}
      <div className="mb-3 flex flex-wrap gap-1.5">
        <Button
          variant={filterMode === "all" ? "primary" : "ghost"}
          size="sm"
          onPress={() => setFilterMode("all")}
        >
          全部
        </Button>
        <Button
          variant={filterMode === "regex" ? "primary" : "ghost"}
          size="sm"
          onPress={() => setFilterMode("regex")}
        >
          正则表达式筛选
        </Button>
        <Button
          variant={filterMode === "region" ? "primary" : "ghost"}
          size="sm"
          onPress={() => setFilterMode("region")}
        >
          按地区
        </Button>
      </div>

      {/* 正则筛选输入 */}
      {filterMode === "regex" && (
        <TextField className="mb-1">
          <Label className="text-xs">正则表达式</Label>
          <Input
            value={regexFilter}
            onChange={(e) => setRegexFilter(e.target.value)}
            placeholder="例如: HK|JP|SG"
          />
          {regexFilterError && (
            <p className="mt-1 text-xs text-danger">{regexFilterError}</p>
          )}
        </TextField>
      )}

      {/* 地区卡片筛选 */}
      {filterMode === "region" && (
        <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {uniqueCountries.map((country) => (
            <RegionCard
              key={country}
              country={country}
              totalCount={nodes.filter((n) => n.country === country).length}
              selectedCount={selectedRegions.get(country) ?? 0}
              onCountChange={(count) => {
                const newRegions = new Map(selectedRegions);
                if (count > 0) {
                  newRegions.set(country, count);
                } else {
                  newRegions.delete(country);
                }
                setSelectedRegions(newRegions);
              }}
            />
          ))}
        </div>
      )}

      {(filterMode === "regex" || filterMode === "region") && (
        <div className="mt-3 rounded-md border border-default-200 bg-content1 p-1">
          <ListBox aria-label="待测速节点列表" className="max-h-56 overflow-y-auto">
            {targetNodes.length === 0 ? (
              <ListBox.Item id="empty" key="empty" textValue="暂无待测速节点" isDisabled>
                <div className="px-2 py-1 text-xs text-foreground-500">暂无待测速节点</div>
              </ListBox.Item>
            ) : (
              targetNodes.map((node, index) => (
                <ListBox.Item
                  id={`${node.name}-${index}`}
                  key={`${node.name}-${index}`}
                  textValue={`${node.name} ${node.protocol} ${node.country}`}
                >
                  <div className="flex items-center gap-2 py-1">
                    <FlagIcon countryCode={node.country} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-sm">{node.name}</div>
                      <div className="text-xs text-foreground-500">{node.country}</div>
                    </div>
                    <Chip size="sm" variant="soft" className="capitalize">
                      {node.protocol}
                    </Chip>
                  </div>
                </ListBox.Item>
              ))
            )}
          </ListBox>
        </div>
      )}
    </div>
  );
}

export default NodeFilterPanel;
