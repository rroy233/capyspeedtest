import { useCallback } from "react";
import type { CountrySpeedSummary, NodeTestState, SpeedTestResult } from "../../types/speedtest";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Button,
  Chip,
  Dropdown,
} from "@heroui/react";
import { FlagIcon } from "../ui/FlagChip";
import { TestingNodeRow } from "./TestingNodeRow";

interface CountryRankingGridProps {
  liveCountrySummaries: CountrySpeedSummary[];
  expandedCountries: Set<string>;
  toggleCountryExpanded: (countryCode: string) => void;
}

export function CountryRankingGrid({
  liveCountrySummaries,
  expandedCountries,
  toggleCountryExpanded,
}: CountryRankingGridProps) {
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between w-full">
          <div className="flex items-center gap-2">
            <CardTitle>地区排名</CardTitle>
            <Chip variant="soft" color="success" size="sm">
              {liveCountrySummaries.length} 个地区
            </Chip>
          </div>
        </div>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {liveCountrySummaries.map((summary) => {
            const expanded = expandedCountries.has(summary.country_code);
            const allDisplayNodes: Array<
              { type: "completed"; data: SpeedTestResult; rank: number } |
              { type: "testing"; data: NodeTestState }
            > = [
              ...summary.results.map((r, i) => ({ type: "completed" as const, data: r, rank: i + 1 })),
              ...summary.testingNodes.map((n) => ({ type: "testing" as const, data: n })),
            ];
            const visibleNodes = expanded ? allDisplayNodes : allDisplayNodes.slice(0, 3);
            const totalCount = summary.results.length + summary.testingNodes.length;
            const hasMore = totalCount > 3;

            return (
              <Card key={summary.country_code} className="h-[320px]">
                <CardContent className="p-4 h-full flex flex-col">
                  <div className="flex items-center justify-between gap-2 mb-3">
                    <div className="flex items-center gap-2 min-w-0">
                      <FlagIcon countryCode={summary.country_code} />
                      <div className="min-w-0">
                        <div className="font-semibold truncate">{summary.country_name}</div>
                        <div className="text-xs text-foreground-500">{summary.country_code}</div>
                      </div>
                    </div>
                    <div className="text-right">
                      <div className={`text-lg font-bold ${
                        summary.status === "fast" ? "text-success" : summary.status === "available" ? "text-accent" : summary.status === "slow" ? "text-warning" : "text-danger"
                      }`}>
                        {summary.avg_download_mbps.toFixed(1)}
                      </div>
                      <div className="text-xs text-foreground-500">{summary.node_count} 节点</div>
                    </div>
                  </div>

                  <div className="flex-1 space-y-2 overflow-auto">
                    {visibleNodes.map((item, idx) => {
                      if (item.type === "completed") {
                        const result = item.data;
                        return (
                          <Dropdown
                            key={`${summary.country_code}-${result.node.name}-${idx}`}
                          >
                            <Dropdown.Trigger className="w-full">
                              <div className="h-[56px] w-full flex items-center justify-between rounded-[8px] bg-default-50 px-3 py-0 hover:bg-default-100/80 hover:shadow-lg hover:shadow-default-900/10 transition-all duration-200 cursor-pointer">
                                <div className="min-w-0 flex flex-1 items-center gap-2">
                                  <span className="truncate text-sm font-medium">{item.rank}. {result.node.name}</span>
                                  <Chip size="sm" variant="soft" color="success" className="h-6 shrink-0">
                                    已完成
                                  </Chip>
                                </div>
                                <div className="shrink-0 flex items-center gap-3">
                                  <span className={`text-sm font-semibold whitespace-nowrap ${result.tcp_ping_ms > 500 ? "text-danger" : result.tcp_ping_ms > 200 ? "text-warning" : "text-success"}`}>
                                    {result.tcp_ping_ms}ms
                                  </span>
                                  <span className="text-sm font-semibold text-success whitespace-nowrap">
                                    {result.avg_download_mbps.toFixed(1)} Mbps
                                  </span>
                                </div>
                              </div>
                            </Dropdown.Trigger>
                            <Dropdown.Popover placement="bottom start" className="min-w-[280px]">
                              <Dropdown.Menu aria-label={`节点 ${result.node.name} 详细测速结果`}>
                                <Dropdown.Item id={`name-${idx}`} textValue={`节点 ${result.node.name}`}>节点：{result.node.name}</Dropdown.Item>
                                <Dropdown.Item id={`protocol-${idx}`} textValue={`协议 ${result.node.protocol}`}>协议：{result.node.protocol}</Dropdown.Item>
                                <Dropdown.Item id={`tcp-${idx}`} textValue={`TCP ${result.tcp_ping_ms}`}>TCP：{result.tcp_ping_ms} ms</Dropdown.Item>
                                <Dropdown.Item id={`site-${idx}`} textValue={`Site ${result.site_ping_ms}`}>Site：{result.site_ping_ms} ms</Dropdown.Item>
                                <Dropdown.Item id={`download-${idx}`} textValue={`下载 ${result.avg_download_mbps.toFixed(1)}`}>下载：{result.avg_download_mbps.toFixed(1)} Mbps</Dropdown.Item>
                                <Dropdown.Item id={`download-max-${idx}`} textValue={`峰值下载 ${result.max_download_mbps.toFixed(1)}`}>峰值下载：{result.max_download_mbps.toFixed(1)} Mbps</Dropdown.Item>
                                <Dropdown.Item id={`upload-${idx}`} textValue={`上传 ${result.avg_upload_mbps ?? 0}`}>
                                  上传：{result.avg_upload_mbps != null ? `${result.avg_upload_mbps.toFixed(1)} Mbps` : "未启用"}
                                </Dropdown.Item>
                                <Dropdown.Item id={`loss-${idx}`} textValue={`丢包率 ${(result.packet_loss_rate * 100).toFixed(1)}%`}>
                                  丢包率：{(result.packet_loss_rate * 100).toFixed(1)}%
                                </Dropdown.Item>
                              </Dropdown.Menu>
                            </Dropdown.Popover>
                          </Dropdown>
                        );
                      } else {
                        const nodeState = item.data;
                        return (
                          <TestingNodeRow
                            key={`testing-${summary.country_code}-${nodeState.node.name}`}
                            nodeState={nodeState}
                          />
                        );
                      }
                    })}
                  </div>

                  {hasMore && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="mt-3"
                      onPress={() => toggleCountryExpanded(summary.country_code)}
                    >
                      {expanded ? "收起" : `更多（+${totalCount - 3}）`}
                    </Button>
                  )}
                </CardContent>
              </Card>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}

export default CountryRankingGrid;
