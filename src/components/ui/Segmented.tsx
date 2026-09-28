import type { Tokens } from "../../lib/tokens";

interface SegmentedProps<T extends string> {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
  t: Tokens;
}

export default function Segmented<T extends string>({
  options,
  value,
  onChange,
  t,
}: SegmentedProps<T>) {
  return (
    <div
      style={{
        display: "flex",
        gap: 2,
        padding: 3,
        background: t.inputBg,
        border: `1px solid ${t.border}`,
        borderRadius: 8,
      }}
    >
      {options.map((o) => (
        <button
          key={o.value}
          onClick={() => onChange(o.value)}
          type="button"
          style={{
            padding: "5px 12px",
            borderRadius: 6,
            border: "none",
            background: value === o.value ? t.accent : "transparent",
            color: value === o.value ? "#fff" : t.textMuted,
            fontSize: 12,
            fontFamily: "Inter, sans-serif",
            cursor: "pointer",
            transition: "all 0.15s",
            fontWeight: value === o.value ? 500 : 400,
            whiteSpace: "nowrap",
          }}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
