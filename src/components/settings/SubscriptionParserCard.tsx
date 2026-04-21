import { FormEvent } from "react";
import type { NodeInfo } from "../../types/speedtest";
import { parseSubscriptionNodes } from "../../api/subscription";
import { TextField, Label, Input, TextArea, Button, Chip } from "@heroui/react";

interface SubscriptionParserCardProps {
  subscriptionText: string;
  setSubscriptionText: (text: string) => void;
  nameRegex: string;
  setNameRegex: (v: string) => void;
  countryFilter: string;
  setCountryFilter: (v: string) => void;
  limit: string;
  setLimit: (v: string) => void;
  parsedNodes: NodeInfo[];
  loadingParse: boolean;
  onParseNodes: (event: FormEvent) => void;
}

export function SubscriptionParserCard({
  subscriptionText,
  setSubscriptionText,
  nameRegex,
  setNameRegex,
  countryFilter,
  setCountryFilter,
  limit,
  setLimit,
  parsedNodes,
  loadingParse,
  onParseNodes,
}: SubscriptionParserCardProps) {
  const nodeCountLabel = `共解析 ${parsedNodes.length} 个节点`;

  return (
    <>
      <form onSubmit={onParseNodes} className="flex flex-col gap-4">
        <TextField>
          <Label>节点列表</Label>
          <TextArea
            placeholder="每行一个节点链接"
            rows={5}
            value={subscriptionText}
            onChange={(e) => setSubscriptionText(e.target.value)}
          />
        </TextField>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          <TextField>
            <Label>名称正则</Label>
            <Input
              placeholder="如 HK|JP"
              value={nameRegex}
              onChange={(e) => setNameRegex(e.target.value)}
            />
          </TextField>
          <TextField>
            <Label>地区过滤</Label>
            <Input
              placeholder="如 HK,JP,SG"
              value={countryFilter}
              onChange={(e) => setCountryFilter(e.target.value)}
            />
          </TextField>
          <TextField>
            <Label>数量上限</Label>
            <Input
              inputMode="numeric"
              placeholder="20"
              value={limit}
              onChange={(e) => setLimit(e.target.value)}
            />
          </TextField>
        </div>

        <Button variant="primary" type="submit" isPending={loadingParse}>
          解析并过滤节点
        </Button>
      </form>

      <div className="mt-4 border-t border-divider pt-4">
        <p className="mb-3 text-sm text-foreground-500">{nodeCountLabel}</p>
        {parsedNodes.length > 0 && (
          <div className="flex flex-wrap gap-2">
            {parsedNodes.slice(0, 20).map((node, index) => (
              <Chip
                key={`${node.protocol}-${node.name}-${index}`}
                size="sm"
                variant="secondary"
                className="capitalize"
              >
                {node.name}
              </Chip>
            ))}
            {parsedNodes.length > 20 && (
              <Chip size="sm" variant="secondary">
                +{parsedNodes.length - 20} 更多
              </Chip>
            )}
          </div>
        )}
      </div>
    </>
  );
}

export default SubscriptionParserCard;
