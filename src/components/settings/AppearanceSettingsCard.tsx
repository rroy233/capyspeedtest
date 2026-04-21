import { Switch } from "@heroui/react";
import { Surface } from "@heroui/react";

interface AppearanceSettingsCardProps {
  resolvedTheme: "light" | "dark";
  setTheme: (theme: "dark" | "light") => void;
}

export function AppearanceSettingsCard({ resolvedTheme, setTheme }: AppearanceSettingsCardProps) {
  return (
    <Surface variant="secondary" className="px-4 py-3">
      <div className="flex items-center justify-between gap-4">
        <div>
          <p className="font-medium">深色模式</p>
          <p className="text-sm text-foreground-500">
            当前: {resolvedTheme === "dark" ? "深色" : "浅色"}
          </p>
        </div>
        <Switch
          isSelected={resolvedTheme === "dark"}
          onChange={(isSelected) => setTheme(isSelected ? "dark" : "light")}
          size="md"
        />
      </div>
    </Surface>
  );
}

export default AppearanceSettingsCard;
