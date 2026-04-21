import {
  Card,
  CardContent,
  NumberField,
  Label,
} from "@heroui/react";
import { FlagIcon } from "../ui/FlagChip";

interface RegionCardProps {
  country: string;
  totalCount: number;
  selectedCount: number;
  onCountChange: (count: number) => void;
}

export function RegionCard({ country, totalCount, selectedCount, onCountChange }: RegionCardProps) {
  const isSelected = selectedCount > 0;

  return (
    <Card
      variant="default"
      className={`h-[64px] cursor-pointer overflow-visible border bg-content1 outline-none transition-[border-color,box-shadow,ring-color] duration-250 ease-out focus:outline-none focus-visible:outline-none data-[focus-visible=true]:outline-none data-[focus-visible=true]:ring-2 data-[focus-visible=true]:ring-primary-400/45 ${
        isSelected
          ? "!border-primary-500 ring-2 ring-primary-400/45 shadow-sm shadow-primary-500/25"
          : "border-divider/60 hover:border-primary/40"
      }`}
      onClick={() => {
        if (isSelected) {
          onCountChange(0);
        } else {
          onCountChange(totalCount);
        }
      }}
    >
      <CardContent className="h-full overflow-visible px-3 py-2">
        <div className="flex h-full items-center gap-2">
          <FlagIcon countryCode={country} />
          <div className="min-w-0 flex-1">
            <span className={`text-sm font-semibold ${isSelected ? "text-primary" : "text-foreground"}`}>
              {country}
            </span>
          </div>

          <div
            className={`overflow-visible transition-all duration-300 ease-out ${
              isSelected ? "max-w-[168px] translate-x-0 opacity-100" : "pointer-events-none max-w-0 translate-x-2 opacity-0"
            }`}
            onClick={(event) => event.stopPropagation()}
          >
            <div className="w-[152px] overflow-visible">
              <NumberField
                value={selectedCount}
                onChange={(val) => onCountChange(val ?? 0)}
                minValue={1}
                maxValue={totalCount}
                className="overflow-visible"
                isDisabled={!isSelected}
              >
                <Label className="sr-only">测试数量</Label>
                <NumberField.Group className="h-8 overflow-visible rounded-lg">
                  <NumberField.DecrementButton className="h-8 w-8 min-w-8" />
                  <NumberField.Input className="h-8 text-sm" />
                  <NumberField.IncrementButton className="h-8 w-8 min-w-8" />
                </NumberField.Group>
              </NumberField>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

export default RegionCard;
