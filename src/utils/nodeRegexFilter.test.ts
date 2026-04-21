import { describe, expect, it } from "vitest";
import { compileUserRegex, filterNodesByNameRegex } from "./nodeRegexFilter";
import type { NodeInfo } from "../types/speedtest";

function createNode(name: string): NodeInfo {
  return {
    name,
    protocol: "vless",
    country: "HK",
    raw: `vless://token@example.com:443#${name}`,
  };
}

describe("nodeRegexFilter", () => {
  it("支持普通正则文本", () => {
    const result = filterNodesByNameRegex(
      [createNode("香港-HK-01"), createNode("日本-JP-01")],
      "HK|JP"
    );

    expect(result.error).toBeNull();
    expect(result.nodes).toHaveLength(2);
  });

  it("支持 /pattern/flags 语法", () => {
    const result = filterNodesByNameRegex(
      [createNode("hongkong-hk-01"), createNode("japan-jp-01"), createNode("singapore-sg-01")],
      "/hk|jp/i"
    );

    expect(result.error).toBeNull();
    expect(result.nodes.map((n) => n.name)).toEqual(["hongkong-hk-01", "japan-jp-01"]);
  });

  it("全局标记不会导致 test 状态污染", () => {
    const result = filterNodesByNameRegex(
      [createNode("HK-01"), createNode("HK-02"), createNode("JP-01")],
      "/HK/g"
    );

    expect(result.error).toBeNull();
    expect(result.nodes.map((n) => n.name)).toEqual(["HK-01", "HK-02"]);
  });

  it("无效正则会返回错误", () => {
    const result = compileUserRegex("/[abc/");

    expect(result.regex).toBeNull();
    expect(result.error).toContain("无效正则表达式");
  });
});
