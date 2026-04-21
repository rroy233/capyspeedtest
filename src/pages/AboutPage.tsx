import {
  Card,
  Chip,
  Link,
} from "@heroui/react";
import { useEffect, useState } from "react";
import { getAppVersion, getInjectedAppVersion } from "../utils/appVersion";

const features = [
  { title: "批量节点测速", description: "多线程并发测试代理节点性能" },
  { title: "内核管理", description: "自动下载切换 Mihomo 内核版本" },
  { title: "GeoIP 库管理", description: "自动更新 IP 地理信息数据库" },
  { title: "订阅解析", description: "支持主流机场订阅格式与节点链接" },
  { title: "多协议支持", description: "VLESS、Trojan、Shadowsocks 等" },
  { title: "结果导出", description: "CSV 表格与 PNG 图片导出" },
  { title: "历史记录", description: "本地持久化测速历史" },
];

const techStack = [
  { label: "前端框架", value: "React 19 + TypeScript" },
  { label: "桌面容器", value: "Tauri 2" },
  { label: "UI 组件库", value: "HeroUI + TailwindCSS v4" },
  { label: "后端逻辑", value: "Rust" },
];

const acknowledgements = [
  { name: "Mihomo 内核", url: "https://github.com/MetaCubeX/mihomo", description: "MetaCubeX/mihomo" },
  { name: "GeoIP 数据", url: "https://github.com/Loyalsoldier/geoip", description: "Loyalsoldier/geoip" },
  { name: "UI 组件库", url: "https://github.com/heroui-group/heroui", description: "HeroUI" },
  { name: "桌面框架", url: "https://github.com/tauri-apps/tauri", description: "Tauri" },
];

export default function AboutPage() {
  const [appVersion, setAppVersion] = useState(getInjectedAppVersion());

  useEffect(() => {
    void getAppVersion().then(setAppVersion);
  }, []);

  return (
    <div className="flex flex-col gap-6">
      {/* 页面标题 */}
      <div>
        <h1 className="text-2xl font-bold">关于</h1>
        {/* <p className="text-foreground-500 mt-1">了解 CapySpeedtest 的功能与技术栈</p> */}
      </div>

      {/* 应用信息卡片 */}
      <Card>
        <Card.Header>
          <Card.Title>应用信息</Card.Title>
        </Card.Header>
        <Card.Content className="flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <span className="font-medium">名称：</span>
            <Chip size="sm" variant="secondary">CapySpeedtest</Chip>
          </div>
          <div className="flex items-center gap-2">
            <span className="font-medium">版本：</span>
            <Chip size="sm" variant="secondary">v{appVersion}</Chip>
          </div>
          <div className="pt-2 border-t border-divider">
            <span className="font-medium">简介：</span>
            <p className="text-foreground-500 mt-1">
              一款专注于批量代理节点测速的桌面工具，支持多线程并发测试、GeoIP
              信息解析与测速结果可视化导出。
            </p>
          </div>
        </Card.Content>
      </Card>

      {/* 核心功能卡片
      <Card>
        <Card.Header>
          <div className="flex flex-col gap-1">
            <Card.Title>核心功能</Card.Title>
            <Card.Description>CapySpeedtest 提供的主要功能特性</Card.Description>
          </div>
        </Card.Header>
        <Card.Content>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {features.map((feature, index) => (
              <Card key={index} className="bg-default-50">
                <Card.Content className="flex flex-row items-center gap-3 p-3">
                  <Chip size="sm" variant="secondary">{index + 1}</Chip>
                  <div className="flex flex-col">
                    <span className="font-medium text-sm">{feature.title}</span>
                    <span className="text-xs text-foreground-500">{feature.description}</span>
                  </div>
                </Card.Content>
              </Card>
            ))}
          </div>
        </Card.Content>
      </Card> */}

      {/* 技术栈卡片
      <Card>
        <Card.Header>
          <div className="flex flex-col gap-1">
            <Card.Title>技术栈</Card.Title>
            <Card.Description>构建 CapySpeedtest 使用的技术</Card.Description>
          </div>
        </Card.Header>
        <Card.Content>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            {techStack.map((item) => (
              <Card key={item.label} className="bg-default-50">
                <Card.Content className="text-center p-4">
                  <p className="text-xs text-foreground-500 mb-1">{item.label}</p>
                  <p className="font-semibold text-sm">{item.value}</p>
                </Card.Content>
              </Card>
            ))}
          </div>
        </Card.Content>
      </Card> */}

      {/* 相关项目卡片 */}
      <Card>
        <Card.Header>
          <div className="flex flex-col gap-1">
            <Card.Title>相关项目</Card.Title>
            <Card.Description>感谢以下开源项目的贡献</Card.Description>
          </div>
        </Card.Header>
        <Card.Content className="flex flex-col gap-3">
          {acknowledgements.map((item) => (
            <div key={item.name} className="flex items-center gap-3">
              <span className="font-medium min-w-[100px] text-sm">{item.name}：</span>
              <Link
                href={item.url}
                target="_blank"
                rel="noopener noreferrer"
              >
                {item.description}
              </Link>
            </div>
          ))}
        </Card.Content>
      </Card>

      <div className="pt-4 border-t border-divider text-center">
        <p className="text-sm text-foreground-400">
          © {new Date().getFullYear()} CapySpeedtest. 开源项目，仅供学习与研究使用。
        </p>
      </div>
    </div>
  );
}
