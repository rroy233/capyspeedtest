import { useState } from "react";
import { Tabs } from "@heroui/react";
import HistoryStatistics from "../components/history/HistoryStatistics";
import HistoryManagement from "../components/history/HistoryManagement";

export default function ResultsPage() {
  const [activeTab, setActiveTab] = useState<string>("statistics");

  return (
    <div className="flex flex-col gap-6">
      {/* 页面标题 */}
      <div>
        <h1 className="text-2xl font-bold">历史记录</h1>
        <p className="text-foreground-500 mt-1">
          查看历史测速统计和管理测速记录
        </p>
      </div>

      {/* 主内容 - Tab 切换 */}
      <Tabs
        aria-label="历史记录"
        variant="primary"
        selectedKey={activeTab}
        onSelectionChange={(key) => setActiveTab(String(key))}
      >
        <Tabs.ListContainer>
          <Tabs.List aria-label="历史记录">
            <Tabs.Tab key="statistics" id="statistics">
              历史统计
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab key="management" id="management">
              历史记录管理
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>

      {/* Tab 内容 */}
      {activeTab === "statistics" && <HistoryStatistics />}
      {activeTab === "management" && <HistoryManagement />}
    </div>
  );
}
