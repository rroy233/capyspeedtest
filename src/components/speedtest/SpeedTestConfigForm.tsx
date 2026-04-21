import type { FormEvent } from "react";
import {
  Button,
  TextField,
  Label,
  Input,
  TextArea,
  Checkbox,
} from "@heroui/react";
import type { NodeInfo } from "../../types/speedtest";
import { NodeFilterPanel } from "./NodeFilterPanel";

interface SpeedTestConfigFormProps {
  inputMode: "manual" | "url";
  setInputMode: (mode: "manual" | "url") => void;
  subscriptionText: string;
  setSubscriptionText: (text: string) => void;
  subscriptionUrl: string;
  setSubscriptionUrl: (url: string) => void;
  fetchingUrl: boolean;
  concurrency: string;
  setConcurrency: (value: string) => void;
  timeoutMs: string;
  setTimeoutMs: (value: string) => void;
  targetSites: string;
  setTargetSites: (value: string) => void;
  enableUploadTest: boolean;
  setEnableUploadTest: (value: boolean) => void;
  urlPhase: "input" | "fetching" | "filtering";
  fetchedNodes: NodeInfo[];
  filterMode: "all" | "regex" | "region";
  setFilterMode: (mode: "all" | "regex" | "region") => void;
  regexFilter: string;
  setRegexFilter: (regex: string) => void;
  regexFilterError?: string;
  filteredNodes: NodeInfo[];
  selectedRegions: Map<string, number>;
  setSelectedRegions: (regions: Map<string, number>) => void;
  running: boolean;
  onStartSpeedTest: (event: FormEvent) => void;
}

export function SpeedTestConfigForm({
  inputMode,
  setInputMode,
  subscriptionText,
  setSubscriptionText,
  subscriptionUrl,
  setSubscriptionUrl,
  fetchingUrl,
  concurrency,
  setConcurrency,
  timeoutMs,
  setTimeoutMs,
  targetSites,
  setTargetSites,
  enableUploadTest,
  setEnableUploadTest,
  urlPhase,
  fetchedNodes,
  filterMode,
  setFilterMode,
  regexFilter,
  setRegexFilter,
  regexFilterError,
  filteredNodes,
  selectedRegions,
  setSelectedRegions,
  running,
  onStartSpeedTest,
}: SpeedTestConfigFormProps) {
  return (
    <form className="flex flex-col gap-5" onSubmit={onStartSpeedTest}>
      {/* 输入模式切换 */}
      <div className="flex gap-2">
        <Button
          variant={inputMode === "manual" ? "primary" : "ghost"}
          size="sm"
          onPress={() => setInputMode("manual")}
        >
          手动输入
        </Button>
        <Button
          variant={inputMode === "url" ? "primary" : "ghost"}
          size="sm"
          onPress={() => setInputMode("url")}
        >
          订阅链接
        </Button>
      </div>

      {/* 手动输入模式 */}
      {inputMode === "manual" ? (
        <TextField>
          <Label>节点列表</Label>
          <TextArea
            placeholder="每行一个节点链接，支持 vmess://、vless://、trojan://、ss:// 等协议"
            rows={5}
            value={subscriptionText}
            onChange={(e) => setSubscriptionText(e.target.value)}
          />
        </TextField>
      ) : (
        <TextField>
          <Label>订阅链接 URL</Label>
          <Input
            placeholder="https://example.com/subscription"
            type="url"
            value={subscriptionUrl}
            onChange={(e) => setSubscriptionUrl(e.target.value)}
            disabled={fetchingUrl}
          />
        </TextField>
      )}

      {/* 高级配置 */}
      <details className="group">
        <summary className="cursor-pointer text-sm font-medium text-foreground-600 hover:text-foreground-800 list-none flex items-center gap-1">
          <span className="transition-transform group-open:rotate-90">▶</span>
          高级配置
        </summary>
        <div className="mt-4 grid grid-cols-1 md:grid-cols-2 gap-4">
          <TextField>
            <Label>并发线程数</Label>
            <Input
              type="number"
              min={1}
              max={64}
              value={concurrency}
              onChange={(e) => setConcurrency(e.target.value)}
            />
          </TextField>

          <TextField>
            <Label>超时时间（毫秒）</Label>
            <Input
              type="number"
              min={1000}
              max={60000}
              value={timeoutMs}
              onChange={(e) => setTimeoutMs(e.target.value)}
            />
          </TextField>

          <TextField className="md:col-span-2">
            <Label>目标站点（逗号分隔）</Label>
            <Input
              placeholder="https://www.google.com,https://www.youtube.com"
              value={targetSites}
              onChange={(e) => setTargetSites(e.target.value)}
            />
          </TextField>

          <div className="md:col-span-2">
            <Checkbox
              isSelected={enableUploadTest}
              onChange={setEnableUploadTest}
              id="enable-upload-test"
            >
              <Checkbox.Control>
                <Checkbox.Indicator />
              </Checkbox.Control>
              <Checkbox.Content>
                <Label>开启上传测速</Label>
              </Checkbox.Content>
            </Checkbox>
          </div>
        </div>
      </details>

      {/* 节点筛选面板 - 订阅URL模式且已完成获取时显示 */}
      {inputMode === "url" && urlPhase === "filtering" && fetchedNodes.length > 0 && (
        <NodeFilterPanel
          nodes={fetchedNodes}
          filterMode={filterMode}
          setFilterMode={setFilterMode}
          regexFilter={regexFilter}
          setRegexFilter={setRegexFilter}
          regexFilterError={regexFilterError}
          filteredNodes={filteredNodes}
          selectedRegions={selectedRegions}
          setSelectedRegions={setSelectedRegions}
          concurrency={concurrency}
        />
      )}

      <Button
        variant="primary"
        type="submit"
        isDisabled={running || (inputMode === "manual" && !subscriptionText.trim()) || (inputMode === "url" && !subscriptionUrl.trim())}
        isPending={running}
        className="w-full md:w-auto self-start"
      >
        {running ? "测速中..." : inputMode === "url" && urlPhase === "input" ? "下一步" : "开始批量测速"}
      </Button>
    </form>
  );
}

export default SpeedTestConfigForm;
