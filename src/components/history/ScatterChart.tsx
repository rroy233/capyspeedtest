import { Scatter } from "@ant-design/charts";
import type { ScatterPoint } from "../../types/history";
import { getCountryColor } from "../../utils/countryColors";

interface ScatterChartProps {
  data: ScatterPoint[];
  metricType: "download" | "upload";
  selectedCountries: string[];
}

export default function ScatterChart({ data, metricType, selectedCountries }: ScatterChartProps) {
  // 根据选择的地区过滤数据
  const filteredData =
    selectedCountries.length === 0
      ? []
      : data.filter((d) => selectedCountries.includes(d.country_code));

  // 转换数据格式以适配 ant-design/charts
  const chartData = filteredData.map((point) => ({
    x: point.hour,
    y: metricType === "download" ? point.avg_download_mbps : (point.avg_upload_mbps ?? 0),
    country: point.country_code,
    nodeName: point.node_name,
    batchId: point.batch_id,
    batchIdText: `批次 #${point.batch_id}`,
    finishedAt: new Date(point.finished_at * 1000).toLocaleString(),
  }));

  // 按地区分组颜色
  const countries = [...new Set(filteredData.map((d) => d.country_code))];
  const colorMap: Record<string, string> = {};
  countries.forEach((c) => {
    colorMap[c] = getCountryColor(c);
  });

  const config = {
    data: chartData,
    xField: "x",
    yField: "y",
    colorField: "country",
    color: (country: string) => colorMap[country] ?? "#888888",
    size: 5,
    shape: "circle",
    scale: {
      x: {
        domain: [0, 24],
        min: 0,
        max: 24,
        tickCount: 25,
        nice: false,
      },
    },
    axis: {
      x: {
        title: "一天中的小时 (0-24)",
        labelFormatter: (v: string | number) => `${Math.floor(Number(v))}时`,
      },
      y: {
        title: metricType === "download" ? "下载速率 (Mbps)" : "上传速率 (Mbps)",
      },
    },
    tooltip: {
      title: "",
      items: [
        {
          field: "nodeName",
          name: "节点名称",
        },
        {
          field: "finishedAt",
          name: "测速时间",
        },
        {
          field: "batchIdText",
          name: "批次ID",
        },
      ],
    },
    animation: true,
    theme: {
      styleSheet: {
        backgroundColor: "transparent",
      },
    },
  };

  if (chartData.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-foreground-500">
        暂无数据
      </div>
    );
  }

  return <Scatter {...config} />;
}
