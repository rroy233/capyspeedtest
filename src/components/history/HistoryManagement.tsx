import { useEffect, useMemo, useState } from "react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Button,
  Chip,
  ProgressBar,
  Table,
  Checkbox,
  Label,
  Select,
  ListBox,
  Modal,
  ModalBackdrop,
  ModalContainer,
  ModalDialog,
  ModalHeader,
  ModalBody,
  ModalFooter,
} from "@heroui/react";
import {
  dbGetBatches,
  dbGetBatchResults,
  dbDeleteBatches,
  dbDeleteBatchesOlderThan,
  dbClearAllBatches,
} from "../../api/settings";
import type { BatchSummary, SpeedTestResult } from "../../types/history";
import { FlagIcon } from "../ui/FlagChip";
import type { Selection } from "react-aria-components/Table";

function formatTimestamp(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString();
}

type SortField =
  | "nodeName"
  | "country"
  | "download"
  | "upload"
  | "tcp"
  | "site"
  | "loss"
  | "nat"
  | "finishedAt";
type SortDirection = "asc" | "desc";

export default function HistoryManagement() {
  const [batches, setBatches] = useState<BatchSummary[]>([]);
  const [selectedKeys, setSelectedKeys] = useState<Selection>(new Set());
  const [viewBatchId, setViewBatchId] = useState<number | null>(null);
  const [viewBatchResults, setViewBatchResults] = useState<SpeedTestResult[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [deleteMonths, setDeleteMonths] = useState<string>("");
  const [sortField, setSortField] = useState<SortField>("download");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");

  useEffect(() => {
    loadBatches();
  }, []);

  function loadBatches() {
    setIsLoading(true);
    dbGetBatches(undefined, undefined, 1000, 0)
      .then((data) => {
        setBatches(data);
      })
      .catch((error) => {
        console.error("加载批次失败:", error);
      })
      .finally(() => {
        setIsLoading(false);
      });
  }

  function handleViewBatch(batchId: number) {
    setViewBatchId(batchId);
    setSortField("download");
    setSortDirection("desc");
    dbGetBatchResults(batchId)
      .then((results) => {
        setViewBatchResults(results);
      })
      .catch((error) => {
        console.error("加载批次详情失败:", error);
        setViewBatchResults([]);
      });
  }

  function toggleSort(field: SortField) {
    if (field === sortField) {
      setSortDirection((prev) => (prev === "asc" ? "desc" : "asc"));
      return;
    }
    setSortField(field);
    setSortDirection(field === "download" ? "desc" : "asc");
  }

  function renderSortHeader(field: SortField, label: string) {
    const isActive = sortField === field;
    const arrow = isActive ? (sortDirection === "asc" ? "↑" : "↓") : "↕";
    return (
      <button
        type="button"
        className={`inline-flex items-center gap-1 text-xs font-medium ${
          isActive ? "text-primary" : "text-foreground-600"
        }`}
        onClick={() => toggleSort(field)}
      >
        <span>{label}</span>
        <span className="text-[10px] leading-none">{arrow}</span>
      </button>
    );
  }

  function handleDeleteSelected() {
    if (selectedKeys === "all" || selectedKeys.size === 0) return;
    const idsToDelete = [...selectedKeys].map((k) => Number(k));
    dbDeleteBatches(idsToDelete)
      .then(() => {
        setSelectedKeys(new Set());
        loadBatches();
      })
      .catch((error) => {
        console.error("删除失败:", error);
      });
  }

  function handleClearAll() {
    dbClearAllBatches()
      .then(() => {
        setSelectedKeys(new Set());
        setShowClearConfirm(false);
        loadBatches();
      })
      .catch((error) => {
        console.error("清空失败:", error);
      });
  }

  function handleDeleteOldData() {
    if (!deleteMonths) return;
    const months = parseInt(deleteMonths, 10);
    if (isNaN(months) || months <= 0) return;
    dbDeleteBatchesOlderThan(months)
      .then(() => {
        setDeleteMonths("");
        loadBatches();
      })
      .catch((error) => {
        console.error("删除旧数据失败:", error);
      });
  }

  const sortedBatchResults = useMemo(() => {
    const list = [...viewBatchResults];
    list.sort((a, b) => {
      const numCmp = (left: number, right: number) => left - right;
      let result = 0;
      switch (sortField) {
        case "nodeName":
          result = a.node.name.localeCompare(b.node.name, "zh-CN");
          break;
        case "country":
          result = a.node.country.localeCompare(b.node.country, "zh-CN");
          break;
        case "download":
          result = numCmp(a.avg_download_mbps, b.avg_download_mbps);
          break;
        case "upload":
          result = numCmp(a.avg_upload_mbps ?? -1, b.avg_upload_mbps ?? -1);
          break;
        case "tcp":
          result = numCmp(a.tcp_ping_ms, b.tcp_ping_ms);
          break;
        case "site":
          result = numCmp(a.site_ping_ms, b.site_ping_ms);
          break;
        case "loss":
          result = numCmp(a.packet_loss_rate, b.packet_loss_rate);
          break;
        case "nat":
          result = a.nat_type.localeCompare(b.nat_type, "zh-CN");
          break;
        case "finishedAt":
          result = numCmp(Number(a.finished_at) || 0, Number(b.finished_at) || 0);
          break;
      }
      return sortDirection === "asc" ? result : -result;
    });
    return list;
  }, [viewBatchResults, sortField, sortDirection]);

  const maxDownloadMbps = useMemo(() => {
    if (viewBatchResults.length === 0) return 1;
    return Math.max(1, ...viewBatchResults.map((item) => item.avg_download_mbps));
  }, [viewBatchResults]);

  const selectedCount =
    selectedKeys === "all" ? batches.length : selectedKeys.size;

  return (
    <div className="flex flex-col gap-4">
      {/* 操作区域 */}
      <Card>
        <CardHeader>
          <CardTitle>操作</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-wrap gap-4 items-center">
            {/* 清空所有 */}
            <Button
              variant="danger"
              onPress={() => setShowClearConfirm(true)}
            >
              清空所有历史记录
            </Button>

            {/* 删除选中 */}
            <Button
              variant="danger"
              isDisabled={selectedCount === 0}
              onPress={handleDeleteSelected}
            >
              删除选中 ({selectedCount})
            </Button>

            {/* 按月删除 */}
            <div className="flex items-center gap-2">
              <Select
                className="w-40"
                placeholder="选择时间范围"
                value={deleteMonths}
                onChange={(value) => setDeleteMonths(typeof value === "string" ? value : "")}
              >
                <Label>选择时间范围</Label>
                <Select.Trigger>
                  <Select.Value />
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    <ListBox.Item key="1" id="1" textValue="1 个月前">
                      1 个月前
                    </ListBox.Item>
                    <ListBox.Item key="3" id="3" textValue="3 个月前">
                      3 个月前
                    </ListBox.Item>
                    <ListBox.Item key="6" id="6" textValue="6 个月前">
                      6 个月前
                    </ListBox.Item>
                    <ListBox.Item key="12" id="12" textValue="1 年前">
                      1 年前
                    </ListBox.Item>
                  </ListBox>
                </Select.Popover>
              </Select>
              <Button
                variant="danger"
                isDisabled={!deleteMonths}
                onPress={handleDeleteOldData}
              >
                删除旧数据
              </Button>
            </div>

            {/* 刷新 */}
            <Button variant="ghost" onPress={loadBatches} isPending={isLoading}>
              刷新
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* 批次列表 */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between w-full">
            <CardTitle>历史批次 ({batches.length})</CardTitle>
            {selectedCount > 0 && (
              <Chip variant="primary">
                已选择 {selectedCount} 项
              </Chip>
            )}
          </div>
        </CardHeader>
        <CardContent>
          {batches.length === 0 ? (
            <div className="text-center py-8 text-foreground-500">
              暂无历史记录
            </div>
          ) : (
            <Table>
              <Table.ScrollContainer>
                <Table.Content
                  aria-label="历史批次列表"
                  selectionMode="multiple"
                  selectedKeys={selectedKeys}
                  onSelectionChange={setSelectedKeys}
                >
                  <Table.Header>
                    <Table.Column>
                      <Checkbox aria-label="全选" slot="selection">
                        <Checkbox.Control>
                          <Checkbox.Indicator />
                        </Checkbox.Control>
                      </Checkbox>
                    </Table.Column>
                    <Table.Column>批次ID</Table.Column>
                    <Table.Column>测速时间</Table.Column>
                    <Table.Column>节点数</Table.Column>
                    <Table.Column>操作</Table.Column>
                  </Table.Header>
                  <Table.Body>
                    {batches.map((batch) => (
                      <Table.Row key={String(batch.batch_id)}>
                        <Table.Cell>
                          <Checkbox slot="selection">
                            <Checkbox.Control>
                              <Checkbox.Indicator />
                            </Checkbox.Control>
                          </Checkbox>
                        </Table.Cell>
                        <Table.Cell>
                          <Chip size="sm" variant="soft">
                            #{batch.batch_id}
                          </Chip>
                        </Table.Cell>
                        <Table.Cell>
                          {formatTimestamp(batch.created_at)}
                        </Table.Cell>
                        <Table.Cell>
                          <Chip size="sm" variant="soft" color="success">
                            {batch.node_count} 节点
                          </Chip>
                        </Table.Cell>
                        <Table.Cell>
                          <Button
                            size="sm"
                            variant="secondary"
                            onPress={() => handleViewBatch(batch.batch_id)}
                          >
                            查看
                          </Button>
                        </Table.Cell>
                      </Table.Row>
                    ))}
                  </Table.Body>
                </Table.Content>
              </Table.ScrollContainer>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* 清空确认弹窗 */}
      <ModalBackdrop
        isOpen={showClearConfirm}
        onOpenChange={(open) => setShowClearConfirm(open)}
      >
        <ModalContainer>
          <ModalDialog className="sm:max-w-[360px]">
            <ModalHeader>确认清空所有历史记录？</ModalHeader>
            <ModalBody>
              <p className="text-foreground-500">
                此操作不可恢复，确定要清空所有历史记录吗？
              </p>
            </ModalBody>
            <ModalFooter>
              <Button
                variant="secondary"
                onPress={() => setShowClearConfirm(false)}
              >
                取消
              </Button>
              <Button variant="danger" onPress={handleClearAll}>
                确认清空
              </Button>
            </ModalFooter>
          </ModalDialog>
        </ModalContainer>
      </ModalBackdrop>

      {/* 批次详情弹窗 */}
      <ModalBackdrop
        isOpen={viewBatchId !== null}
        onOpenChange={(open) => {
          if (!open) {
            setViewBatchId(null);
            setViewBatchResults([]);
          }
        }}
      >
        <ModalContainer
          placement="center"
          className="w-screen h-screen max-w-none p-3 md:p-6"
          scroll="inside"
        >
          <ModalDialog
            aria-label={`批次 #${viewBatchId ?? ""} 测速结果`}
            className="h-[92vh] w-[94vw] max-w-none md:h-[86vh] md:w-[88vw] lg:h-[80vh] lg:w-[75vw]"
          >
            <ModalHeader>批次 #{viewBatchId} 测速结果</ModalHeader>
            <ModalBody className="flex h-full flex-col gap-3 px-6 pb-4">
              {viewBatchResults.length === 0 ? (
                <div className="text-center py-8 text-foreground-500">
                  暂无数据
                </div>
              ) : (
                <div className="flex min-h-0 flex-1 flex-col gap-3">
                  <div className="flex items-center justify-between">
                    <Chip color="success" variant="soft">
                      {viewBatchResults.length} 个节点
                    </Chip>
                    <span className="text-xs text-foreground-500">
                      默认按下载速率降序，可点击表头排序
                    </span>
                  </div>
                  <div className="min-h-0 flex-1 overflow-auto rounded-large border border-divider">
                    <Table>
                      <Table.ScrollContainer className="min-w-[1360px]">
                        <Table.Content aria-label="批次节点测速结果表">
                          <Table.Header>
                            <Table.Column isRowHeader className="min-w-[360px] w-[360px]">
                              {renderSortHeader("nodeName", "节点")}
                            </Table.Column>
                            <Table.Column className="min-w-[220px] w-[220px]">
                              {renderSortHeader("download", "下载")}
                            </Table.Column>
                            <Table.Column className="min-w-[140px] w-[140px]">
                              {renderSortHeader("upload", "上传")}
                            </Table.Column>
                            <Table.Column className="min-w-[110px] w-[110px]">
                              {renderSortHeader("tcp", "TCP")}
                            </Table.Column>
                            <Table.Column className="min-w-[110px] w-[110px]">
                              {renderSortHeader("site", "Site")}
                            </Table.Column>
                            <Table.Column className="min-w-[110px] w-[110px]">
                              {renderSortHeader("loss", "丢包")}
                            </Table.Column>
                            <Table.Column className="min-w-[140px] w-[140px]">
                              {renderSortHeader("nat", "NAT")}
                            </Table.Column>
                            <Table.Column className="min-w-[230px] w-[230px]">
                              {renderSortHeader("finishedAt", "测速时间")}
                            </Table.Column>
                          </Table.Header>
                          <Table.Body>
                            {sortedBatchResults.map((result, index) => (
                              <Table.Row key={`${result.node.name}-${result.finished_at}-${index}`}>
                                <Table.Cell>
                                  <div className="flex min-w-[220px] items-center gap-2">
                                    <FlagIcon countryCode={result.node.country.toUpperCase()} />
                                    <div className="flex min-w-0 flex-col">
                                      <span className="truncate text-sm font-medium">{result.node.name}</span>
                                      <span className="text-xs text-foreground-500">
                                        {result.node.country.toUpperCase()} · {result.node.protocol.toUpperCase()}
                                      </span>
                                    </div>
                                  </div>
                                </Table.Cell>
                                <Table.Cell>
                                  <div className="flex min-w-[180px] items-center gap-2">
                                    <span className="w-20 text-sm font-semibold text-success-600">
                                      {result.avg_download_mbps.toFixed(1)} Mbps
                                    </span>
                                    <div className="w-20">
                                      <ProgressBar
                                        size="sm"
                                        color="success"
                                        value={(result.avg_download_mbps / maxDownloadMbps) * 100}
                                      />
                                    </div>
                                  </div>
                                </Table.Cell>
                                <Table.Cell>
                                  {result.avg_upload_mbps != null
                                    ? `${result.avg_upload_mbps.toFixed(1)} Mbps`
                                    : "—"}
                                </Table.Cell>
                                <Table.Cell>{result.tcp_ping_ms} ms</Table.Cell>
                                <Table.Cell>{result.site_ping_ms} ms</Table.Cell>
                                <Table.Cell>{(result.packet_loss_rate * 100).toFixed(1)}%</Table.Cell>
                                <Table.Cell>
                                  <Chip size="sm" variant="soft">{result.nat_type}</Chip>
                                </Table.Cell>
                                <Table.Cell>
                                  {formatTimestamp(Number(result.finished_at) || 0)}
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
            </ModalBody>
            <ModalFooter>
              <Button variant="secondary" onPress={() => setViewBatchId(null)}>
                关闭
              </Button>
            </ModalFooter>
          </ModalDialog>
        </ModalContainer>
      </ModalBackdrop>
    </div>
  );
}
