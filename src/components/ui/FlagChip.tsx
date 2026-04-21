import { useEffect, useMemo, useState } from "react";
import { Chip } from "@heroui/react";

interface FlagChipProps {
  countryCode: string;
  showName?: boolean;
}

const CHIP_SIZES = {
  lg: "md" as const,
};

const FLAG_SIZE_PX = 30;
const FLAG_ASSET_ALIASES: Record<string, string[]> = {
  GB: ["GBR", "GB-UKM"],
  UK: ["GBR", "GB-UKM", "GB"],
};

function FlagImage({ countryCode }: { countryCode: string }) {
  const normalizedCode = (countryCode || "").trim().toUpperCase();
  const candidates = useMemo(() => {
    const seen = new Set<string>();
    const list = [normalizedCode, ...(FLAG_ASSET_ALIASES[normalizedCode] ?? [])]
      .filter(Boolean)
      .filter((code) => {
        if (seen.has(code)) {
          return false;
        }
        seen.add(code);
        return true;
      });
    return list;
  }, [normalizedCode]);
  const [candidateIndex, setCandidateIndex] = useState(0);

  useEffect(() => {
    setCandidateIndex(0);
  }, [normalizedCode]);

  const currentCode = candidates[candidateIndex];
  const src = currentCode ? `/assets/flags/${currentCode}.svg` : "";

  if (!currentCode) {
    return (
      <span
        className="inline-flex items-center justify-center rounded border border-default-300 bg-default-100 text-[10px] text-foreground-600"
        style={{ width: FLAG_SIZE_PX, height: Math.round(FLAG_SIZE_PX * 0.75) }}
      >
        {normalizedCode.slice(0, 2) || "--"}
      </span>
    );
  }

  return (
    <img
      src={src}
      alt={`${normalizedCode} flag`}
      width={FLAG_SIZE_PX}
      height={Math.round(FLAG_SIZE_PX * 0.75)}
      className="rounded-sm border border-default-200 object-cover"
      loading="lazy"
      onError={() => setCandidateIndex((index) => index + 1)}
    />
  );
}

export function FlagChip({ countryCode, showName = false }: FlagChipProps) {
  const normalizedCode = (countryCode || "").toUpperCase();

  return (
    <Chip
      size={CHIP_SIZES.lg}
      variant="soft"
      className="flex items-center gap-1.5"
    >
      <FlagImage countryCode={normalizedCode} />
      {showName && (
        <span className="font-medium">{normalizedCode}</span>
      )}
    </Chip>
  );
}

interface FlagIconProps {
  countryCode: string;
}

export function FlagIcon({ countryCode }: FlagIconProps) {
  const normalizedCode = (countryCode || "").toUpperCase();
  return <FlagImage countryCode={normalizedCode} />;
}
