import type { Tokens } from "../../lib/tokens";

export function SoundWave({ accent, levels }: { accent: string; levels?: number[] }) {
  const baseHeights = [16, 28, 20, 34, 18, 26, 14];

  return (
    <div className="flex items-center gap-[3.5px]">
      {[0, 1, 2, 3, 4, 5, 6].map((i) => {
        const lvl = levels ? (levels[i % levels.length] ?? 0) : 0;
        const hasLiveInput = levels && levels.some((v) => v > 0.02);
        const base = baseHeights[i];
        const h = hasLiveInput
          ? Math.min(38, Math.max(10, Math.round(base * 0.45 + lvl * 32)))
          : undefined;

        return (
          <div
            key={i}
            className={hasLiveInput ? undefined : "wave-bar"}
            style={{
              width: 3.5,
              borderRadius: 2,
              height: h ?? undefined,
              transition: hasLiveInput ? "height 75ms cubic-bezier(0.2, 0.9, 0.3, 1)" : undefined,
              background: `linear-gradient(180deg, ${accent}, #00d4ff)`,
            }}
          />
        );
      })}
    </div>
  );
}

export function TranscribingIndicator({ t }: { t: Tokens }) {
  return (
    <div
      className="relative flex items-center gap-2 overflow-hidden rounded-full px-3.5 py-1.5"
      style={{ background: t.transcribeBg, border: `1px solid ${t.transcribeBr}` }}
    >
      <div
        className="scan-line absolute inset-0"
        style={{ background: `linear-gradient(90deg, transparent, ${t.scanShimmer}, transparent)` }}
      />
      <div className="flex gap-[8px]">
        {[0, 1, 2].map((i) => (
          <div key={i} className="transcribe-dot" style={{ background: "#00d4ff" }} />
        ))}
      </div>
    </div>
  );
}

export function TranslatingIndicator({ accent }: { accent: string }) {
  return (
    <div className="relative flex items-center justify-center" style={{ width: 38, height: 38 }}>
      <div className="translate-ring" style={{ borderColor: accent }} />
      <div className="translate-ring" style={{ borderColor: accent }} />
      <div className="translate-ring" style={{ borderColor: accent }} />
      <div className="absolute inset-0 flex items-center justify-center">
        <div style={{ width: 12, height: 12, borderRadius: "50%", background: accent }} />
      </div>
    </div>
  );
}

export function CopiedIndicator() {
  return (
    <svg width="26" height="26" viewBox="0 0 22 22" fill="none">
      <rect
        x="7"
        y="2"
        width="11"
        height="13"
        rx="2.5"
        stroke="rgba(0,229,160,0.4)"
        strokeWidth="1.3"
      />
      <rect
        x="4"
        y="7"
        width="11"
        height="13"
        rx="2.5"
        stroke="#00e5a0"
        strokeWidth="1.3"
        fill="rgba(0,229,160,0.08)"
      />
      <path
        className="check-path"
        d="M7 14l3 3 6-6.5"
        stroke="#00e5a0"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function RecordingErrorIndicator({ accent }: { accent?: string }) {
  const color = accent || "#ff4d6d";
  return (
    <div
      className="flex items-center justify-center rounded-full"
      style={{
        width: 32,
        height: 32,
        background: `${color}18`,
        border: `1px solid ${color}44`,
      }}
    >
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke={color}
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <line x1="2" y1="2" x2="22" y2="22" />
        <path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2" />
        <path d="M5 10v2a7 7 0 0 0 12 5" />
        <line x1="12" y1="19" x2="12" y2="22" />
      </svg>
    </div>
  );
}

export function TranscriptionErrorIndicator({ accent }: { accent?: string }) {
  const color = accent || "#ff6b6b";
  return (
    <div
      className="flex items-center justify-center rounded-full"
      style={{
        width: 32,
        height: 32,
        background: `${color}18`,
        border: `1px solid ${color}44`,
      }}
    >
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke={color}
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="8" x2="12" y2="12" />
        <line x1="12" y1="16" x2="12.01" y2="16" />
      </svg>
    </div>
  );
}

export function TranslationErrorIndicator({ accent }: { accent?: string }) {
  const color = accent || "#ff9f43";
  return (
    <div
      className="flex items-center justify-center rounded-full relative"
      style={{
        width: 32,
        height: 32,
        background: `${color}18`,
        border: `1px solid ${color}44`,
      }}
    >
      <svg
        width="18"
        height="18"
        viewBox="0 0 24 24"
        fill="none"
        stroke={color}
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <circle cx="12" cy="12" r="10" />
        <line x1="2" y1="12" x2="22" y2="12" />
        <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
      </svg>
      <div
        style={{
          position: "absolute",
          top: -2,
          right: -2,
          width: 8,
          height: 8,
          borderRadius: "50%",
          background: color,
        }}
      />
    </div>
  );
}

export function ErrorIndicator({
  t,
  title,
  accent,
}: {
  t: Tokens;
  title?: string;
  accent?: string;
}) {
  const lower = (title || "").toLowerCase();
  if (lower.includes("translat")) return <TranslationErrorIndicator accent={accent} />;
  if (lower.includes("record")) return <RecordingErrorIndicator accent={accent} />;
  if (lower.includes("transcrib") || lower.includes("speech"))
    return <TranscriptionErrorIndicator accent={accent} />;
  return (
    <div
      className="flex items-center justify-center rounded-full"
      style={{
        width: 32,
        height: 32,
        background: t.dangerBg,
        border: `1px solid ${t.dangerBorder}`,
      }}
    >
      <span style={{ color: t.dangerText, fontSize: 14, fontWeight: 700 }}>!</span>
    </div>
  );
}

export function StateIndicator({
  state,
  t,
  accent,
  levels,
  title,
}: {
  state: "idle" | "listening" | "transcribing" | "translating" | "done" | "error";
  t: Tokens;
  accent: string;
  levels?: number[];
  title?: string;
}) {
  if (state === "listening") return <SoundWave accent={accent} levels={levels} />;
  if (state === "transcribing") return <TranscribingIndicator t={t} />;
  if (state === "translating") return <TranslatingIndicator accent={accent} />;
  if (state === "error") return <ErrorIndicator t={t} title={title} accent={accent} />;
  return <CopiedIndicator />;
}
