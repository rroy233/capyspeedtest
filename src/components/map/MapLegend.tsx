import { Card, CardHeader, CardContent } from "@heroui/react";
import type { SpeedStatus } from "../../types/speedtest";
import { SPEED_COLORS, SPEED_LABELS } from "./colorScheme";

const legendItems: { status: SpeedStatus; label: string }[] = [
  { status: "fast", label: "快速 (>100 Mbps)" },
  { status: "available", label: "可用 (30-100 Mbps)" },
  { status: "slow", label: "较慢 (5-30 Mbps)" },
  { status: "very-slow", label: "极慢 (<5 Mbps)" },
  { status: "unavailable", label: "不可用" },
];

export default function MapLegend() {
  return (
    <Card className="w-fit">
      <CardHeader className="pb-0">
        <span className="text-sm font-semibold">速度图例</span>
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap gap-3">
          {legendItems.map((item) => (
            <div key={item.status} className="flex items-center gap-1.5">
              <div
                className="w-3 h-3 rounded-full"
                style={{ backgroundColor: SPEED_COLORS[item.status] }}
              />
              <span className="text-xs text-foreground-600">{item.label}</span>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
