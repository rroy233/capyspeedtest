import { useEffect, useMemo, useState } from "react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Button,
  Label,
  DateField,
  RangeCalendar,
  DateRangePicker,
  Switch,
  Tag,
  TagGroup,
} from "@heroui/react";
import { FlagIcon } from "../ui/FlagChip";
import ScatterChart from "./ScatterChart";
import { dbGetScatterData, dbGetAllCountries } from "../../api/settings";
import type { ScatterPoint } from "../../types/history";
import { getCountryName } from "../../utils/countryMapping";
import type { DateValue } from "@internationalized/date";
import { getLocalTimeZone } from "@internationalized/date";
import type { Selection } from "react-aria-components";

export default function HistoryStatistics() {
  const [scatterData, setScatterData] = useState<ScatterPoint[]>([]);
  const [allCountries, setAllCountries] = useState<string[]>([]);
  const [selectedCountries, setSelectedCountries] = useState<Set<string>>(new Set(["ALL"]));
  const [isDownload, setIsDownload] = useState(true);
  const [dateRange, setDateRange] = useState<{ start: DateValue; end: DateValue } | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const metricType = isDownload ? "download" : "upload";

  // 加载数据
  useEffect(() => {
    loadData();
  }, []);

  function loadData() {
    setIsLoading(true);
    const fromTs = dateRange?.start
      ? dateRange.start.toDate(getLocalTimeZone()).getTime() / 1000
      : undefined;
    const toTs = dateRange?.end
      ? dateRange.end.toDate(getLocalTimeZone()).getTime() / 1000
      : undefined;

    Promise.all([dbGetAllCountries(), dbGetScatterData(fromTs, toTs)])
      .then(([countries, data]) => {
        setAllCountries(countries);
        setScatterData(data);
      })
      .catch((error) => {
        console.error("加载散点图数据失败:", error);
      })
      .finally(() => {
        setIsLoading(false);
      });
  }

  // 处理地区选择
  function handleCountrySelectionChange(keys: Selection) {
    if (keys === "all") {
      setSelectedCountries(new Set<string>(["ALL", ...allCountries]));
      return;
    }

    const nextSelection = new Set<string>([...keys].map((k) => String(k)));

    setSelectedCountries((prev) => {
      const hadAll = prev.has("ALL");
      const hasAll = nextSelection.has("ALL");

      // 选中 ALL：全选所有地区
      if (hasAll && !hadAll) {
        return new Set<string>(["ALL", ...allCountries]);
      }

      // 取消 ALL：清空全部选择
      if (!hasAll && hadAll) {
        return new Set<string>();
      }

      // 根据地区全选状态自动同步 ALL
      nextSelection.delete("ALL");
      const allCountrySelected =
        allCountries.length > 0 &&
        allCountries.every((countryCode) => nextSelection.has(countryCode));

      if (allCountrySelected) {
        nextSelection.add("ALL");
      }

      return nextSelection;
    });
  }

  // 清除日期筛选
  function clearDateFilter() {
    setDateRange(null);
  }

  const displayCountries = useMemo(() => {
    return ["ALL", ...allCountries];
  }, [allCountries]);

  const activeCountries = selectedCountries.has("ALL")
    ? allCountries
    : [...selectedCountries];

  return (
    <div className="flex flex-col gap-4">
      {/* 筛选控件 */}
      <Card>
        <CardHeader>
          <CardTitle>筛选条件</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col gap-4">
            {/* 日期范围 */}
            <div className="flex items-center gap-4 flex-wrap">
              <DateRangePicker
                className="w-[320px]"
                value={dateRange}
                onChange={(value) => setDateRange(value)}
              >
                <Label>日期范围</Label>
                <DateField.Group fullWidth>
                  <DateField.Input slot="start">
                    {(segment) => <DateField.Segment segment={segment} />}
                  </DateField.Input>
                  <DateRangePicker.RangeSeparator />
                  <DateField.Input slot="end">
                    {(segment) => <DateField.Segment segment={segment} />}
                  </DateField.Input>
                  <DateField.Suffix>
                    <DateRangePicker.Trigger>
                      <DateRangePicker.TriggerIndicator />
                    </DateRangePicker.Trigger>
                  </DateField.Suffix>
                </DateField.Group>
                <DateRangePicker.Popover>
                  <RangeCalendar aria-label="选择日期范围">
                    <RangeCalendar.Header>
                      <RangeCalendar.YearPickerTrigger>
                        <RangeCalendar.YearPickerTriggerHeading />
                        <RangeCalendar.YearPickerTriggerIndicator />
                      </RangeCalendar.YearPickerTrigger>
                      <RangeCalendar.NavButton slot="previous" />
                      <RangeCalendar.NavButton slot="next" />
                    </RangeCalendar.Header>
                    <RangeCalendar.Grid>
                      <RangeCalendar.GridHeader>
                        {(day) => <RangeCalendar.HeaderCell>{day}</RangeCalendar.HeaderCell>}
                      </RangeCalendar.GridHeader>
                      <RangeCalendar.GridBody>
                        {(date) => <RangeCalendar.Cell date={date} />}
                      </RangeCalendar.GridBody>
                    </RangeCalendar.Grid>
                    <RangeCalendar.YearPickerGrid>
                      <RangeCalendar.YearPickerGridBody>
                        {({ year }) => <RangeCalendar.YearPickerCell year={year} />}
                      </RangeCalendar.YearPickerGridBody>
                    </RangeCalendar.YearPickerGrid>
                  </RangeCalendar>
                </DateRangePicker.Popover>
              </DateRangePicker>
              <Button size="sm" variant="ghost" onPress={clearDateFilter}>
                清除日期
              </Button>
              <Button
                size="sm"
                variant="primary"
                onPress={loadData}
                isPending={isLoading}
              >
                应用筛选
              </Button>
            </div>

            {/* 地区筛选 - 使用 TagGroup */}
            <TagGroup
              aria-label="选择地区"
              selectionMode="multiple"
              selectedKeys={selectedCountries}
              onSelectionChange={handleCountrySelectionChange}
            >
              <Label>选择地区</Label>
              <TagGroup.List>
                {displayCountries.map((countryCode) => (
                  <Tag key={countryCode} id={countryCode}>
                    {countryCode === "ALL" ? (
                      "全部地区"
                    ) : (
                      <span className="flex items-center gap-1.5">
                        <FlagIcon countryCode={countryCode} />
                        {getCountryName(countryCode)} ({countryCode})
                      </span>
                    )}
                  </Tag>
                ))}
              </TagGroup.List>
            </TagGroup>

            {/* 下载/上传切换 - 使用 Switch */}
            <div className="flex items-center gap-4">
              <Switch
                isSelected={isDownload}
                onChange={setIsDownload}
              >
                <Switch.Control>
                  <Switch.Thumb />
                </Switch.Control>
                <Switch.Content>
                  <Label>{isDownload ? "显示下载速率" : "显示上传速率"}</Label>
                </Switch.Content>
              </Switch>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* 散点图 */}
      <Card>
        <CardHeader>
          <CardTitle>
            {metricType === "download" ? "下载" : "上传"}速率分布 (
            {scatterData.length} 个数据点)
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="w-full h-[400px]">
            <ScatterChart
              data={scatterData}
              metricType={metricType}
              selectedCountries={activeCountries}
            />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
