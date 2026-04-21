import { useMemo } from "react";
import {
  ComposableMap,
  Geographies,
  Geography,
  Marker,
  ZoomableGroup,
} from "react-simple-maps";
import type { CountrySpeedSummary, SpeedStatus } from "../../types/speedtest";
import { getSpeedColor } from "./colorScheme";

// 地图数据源 - 本地文件
const GEO_URL = "/maps/world-countries.json";

interface SpeedTestMapProps {
  countrySummaries: CountrySpeedSummary[];
  activeCountryCode?: string;
  onCountryClick?: (countryCode: string) => void;
}

// 地区代码映射到 ISO numeric id (用于匹配 TopoJSON 中的 id)
const COUNTRY_CODE_TO_ID: Record<string, string> = {
  HK: "344",
  MO: "446",
  JP: "392",
  US: "840",
  SG: "702",
  GB: "826",
  DE: "276",
  FR: "250",
  KR: "410",
  TW: "158",
  AU: "036",
  CA: "124",
  NL: "528",
  IT: "380",
  ES: "724",
  BR: "076",
  IN: "356",
  RU: "643",
  CN: "156",
  TH: "764",
  VN: "704",
  MY: "458",
  ID: "360",
  PH: "608",
  PL: "616",
  SE: "752",
  NO: "578",
  DK: "208",
  FI: "246",
  CH: "756",
  AT: "040",
  BE: "056",
  PT: "620",
  MX: "484",
  AR: "032",
  CL: "152",
  CO: "170",
  PE: "604",
  VE: "862",
  EG: "818",
  ZA: "710",
  NG: "566",
  KE: "404",
  AE: "784",
  SA: "682",
  TR: "792",
  IL: "376",
  PK: "586",
  BD: "050",
};

interface CountryMarkerData {
  code: string;
  name: string;
  centroid: [number, number];
  status: SpeedStatus;
  isActive: boolean;
  maxSpeed: number;
}

interface SpecialRegionMarkerData {
  code: "HK" | "MO";
  centroid: [number, number];
  status: SpeedStatus;
  isActive: boolean;
}

export default function SpeedTestMap({
  countrySummaries,
  activeCountryCode,
  onCountryClick,
}: SpeedTestMapProps) {
  // 构建地区代码到汇总数据的映射
  const summaryMap = useMemo(() => {
    const map = new Map<string, CountrySpeedSummary>();
    countrySummaries.forEach((s) => {
      map.set(s.country_code, s);
    });
    return map;
  }, [countrySummaries]);

  // 计算每个地区的中心点坐标
  const markersData = useMemo((): CountryMarkerData[] => {
    return countrySummaries.map((summary) => {
      const code = summary.country_code;
      // 使用预定义坐标或默认值
      const coords = getCountryCoords(code);
      return {
        code,
        name: summary.country_name,
        centroid: coords,
        status: summary.status,
        isActive: code === activeCountryCode,
        maxSpeed: summary.max_download_mbps,
      };
    });
  }, [countrySummaries, activeCountryCode]);

  const specialRegionMarkers = useMemo((): SpecialRegionMarkerData[] => {
    return countrySummaries
      .filter((s): s is CountrySpeedSummary & { country_code: "HK" | "MO" } => s.country_code === "HK" || s.country_code === "MO")
      .map((summary) => ({
        code: summary.country_code,
        centroid: getCountryCoords(summary.country_code),
        status: summary.status,
        isActive: summary.country_code === activeCountryCode,
      }));
  }, [countrySummaries, activeCountryCode]);

  // 智能缩放：根据现有地区分布让结果区域大约占视图 70%
  const viewport = useMemo(() => {
    if (markersData.length === 0) {
      return { center: [0, 20] as [number, number], zoom: 1 };
    }
    const lons = markersData.map((m) => m.centroid[0]);
    const lats = markersData.map((m) => m.centroid[1]);
    const minLon = Math.min(...lons);
    const maxLon = Math.max(...lons);
    const minLat = Math.min(...lats);
    const maxLat = Math.max(...lats);
    const center: [number, number] = [(minLon + maxLon) / 2, (minLat + maxLat) / 2];

    const lonSpan = Math.max(20, maxLon - minLon);
    const latSpan = Math.max(12, maxLat - minLat);
    const targetFill = 0.7;
    const zoomByLon = 360 / (lonSpan / targetFill);
    const zoomByLat = 170 / (latSpan / targetFill);
    const zoom = Math.max(1, Math.min(8, Math.min(zoomByLon, zoomByLat)));

    return { center, zoom };
  }, [markersData]);

  return (
    <ComposableMap
      projection="geoMercator"
      projectionConfig={{
        scale: 100,
      }}
      style={{ width: "100%", height: "100%" }}
    >
      <ZoomableGroup center={viewport.center} zoom={viewport.zoom}>
      <Geographies geography={GEO_URL}>
        {({ geographies }) =>
          geographies.map((geo) => {
            const countryId = geo.id as string;
            // 尝试通过 ID 匹配地区
            const code = getCountryCodeFromId(countryId);
            const summary = summaryMap.get(code);
            const fillColor = summary
              ? getSpeedColor(summary.status)
              : "#E0E0E0";
            const isActive = code === activeCountryCode;

            return (
              <Geography
                key={geo.rsmKey}
                geography={geo}
                style={{
                  default: {
                    fill: fillColor,
                    stroke: isActive ? "#3B82F6" : "#FFF",
                    strokeWidth: isActive ? 2 : 0.5,
                    outline: "none",
                    opacity: summary ? 0.9 : 0.3,
                  },
                  hover: {
                    fill: summary ? getSpeedColor(summary.status) : "#C0C0C0",
                    stroke: "#3B82F6",
                    strokeWidth: 1,
                    outline: "none",
                    cursor: summary ? "pointer" : "default",
                  },
                  pressed: {
                    fill: summary ? getSpeedColor(summary.status) : "#C0C0C0",
                    stroke: "#3B82F6",
                    strokeWidth: 1,
                    outline: "none",
                  },
                }}
                onClick={() => {
                  if (summary && onCountryClick) {
                    onCountryClick(code);
                  }
                }}
              />
            );
          })
        }
      </Geographies>

      {/* 港澳在部分 topo 数据集中无独立面：使用小尺寸锚点圆 */}
      {specialRegionMarkers.map((marker) => {
        const color = getSpeedColor(marker.status);

        return (
          <Marker key={`special-${marker.code}`} coordinates={marker.centroid}>
            <g
              style={{ cursor: "pointer" }}
              onClick={() => onCountryClick?.(marker.code)}
            >
              <circle
                cx={0}
                cy={0}
                r={2.4}
                fill={color}
                stroke="#ffffff"
                strokeWidth={marker.isActive ? 1.2 : 0.8}
              />
            </g>
          </Marker>
        );
      })}

      </ZoomableGroup>
    </ComposableMap>
  );
}

// 根据地区代码获取中心坐标
function getCountryCoords(code: string): [number, number] {
  const coords: Record<string, [number, number]> = {
    HK: [114.1694, 22.3193],
    MO: [113.5439, 22.1987],
    JP: [139.6917, 35.6895],
    US: [-95.7129, 37.0902],
    SG: [103.8198, 1.3521],
    GB: [-3.436, 55.3781],
    DE: [10.4515, 51.1657],
    FR: [2.2137, 46.2276],
    KR: [127.7669, 35.9078],
    TW: [121.5654, 25.033],
    AU: [133.7751, -25.2744],
    CA: [-106.3468, 56.1304],
    NL: [4.9041, 52.3676],
    IT: [12.5674, 41.8719],
    ES: [-3.7492, 40.4637],
    BR: [-51.9253, -14.235],
    IN: [78.9629, 20.5937],
    RU: [105.3188, 61.524],
    CN: [104.1954, 35.8617],
    TH: [100.9925, 15.87],
    VN: [108.2772, 14.0583],
    MY: [101.9758, 4.2105],
    ID: [113.9213, -0.7893],
    PH: [121.774, 12.8797],
    PL: [19.1451, 51.9194],
    SE: [18.6435, 60.1282],
    NO: [8.4689, 60.472],
    DK: [9.5018, 56.2639],
    FI: [25.7488, 61.9241],
    CH: [8.2275, 46.8182],
    AT: [14.5501, 47.5162],
    BE: [4.4699, 50.5039],
    PT: [-8.2245, 39.3999],
    MX: [-102.5528, 23.6345],
    AR: [-63.6167, -38.4161],
    CL: [-71.543, -35.6751],
    CO: [-74.0721, 4.711],
    PE: [-75.0152, -9.19],
    VE: [-66.5897, 6.4238],
    EG: [30.8025, 26.8206],
    ZA: [22.9375, -30.5595],
    NG: [8.6753, 9.082],
    KE: [37.9062, -0.0236],
    AE: [53.8478, 23.4241],
    SA: [45.0792, 23.8859],
    TR: [35.2433, 38.9637],
    IL: [34.8516, 31.0461],
    PK: [69.3451, 30.3753],
    BD: [90.3563, 23.685],
  };
  return coords[code] ?? [0, 0];
}

// 从 TopoJSON 的 id 获取地区代码
function getCountryCodeFromId(id: string): string {
  const normalizedId = String(Number(id));
  const entry = Object.entries(COUNTRY_CODE_TO_ID).find(([, v]) => String(Number(v)) === normalizedId);
  return entry ? entry[0] : "";
}
