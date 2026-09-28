import { useEffect, useRef } from "react";
import type { Tokens } from "../../lib/tokens";

const WAVE_BARS = 7;
const WAVE_HEIGHT = 38;
const WAVE_MIN_SCALE = 0.22;
// Centre bars react most, edges least, so the wave reads as one shape rather than random bars.
const WAVE_PROFILE = [0.55, 0.78, 0.94, 1, 0.94, 0.78, 0.55];
// Exponential smoothing time constants (ms): rise quickly with the voice, fall back gently.
const WAVE_ATTACK_MS = 45;
const WAVE_RELEASE_MS = 160;

/** Level for bar `i`, interpolated across however many level bands the backend sends. */
function levelForBar(levels: number[], i: number): number {
  if (levels.length === 0) return 0;
  const pos = (i / (WAVE_BARS - 1)) * (levels.length - 1);
  const lo = Math.floor(pos);
  const hi = Math.min(levels.length - 1, lo + 1);
  const a = levels[lo] ?? 0;
  const b = levels[hi] ?? a;
  return a + (b - a) * (pos - lo);
}

export function SoundWave({ accent, levels }: { accent: string; levels?: number[] | undefined }) {
  const barsRef = useRef<(HTMLDivElement | null)[]>([]);
  const levelsRef = useRef<number[]>(levels ?? []);
  levelsRef.current = levels ?? [];

  // Animate on every frame and ease towards the latest levels, so 25 Hz level updates
  // render as continuous motion instead of steps. A gentle idle breath keeps the wave alive in silence.
  useEffect(() => {
    const current = new Array<number>(WAVE_BARS).fill(WAVE_MIN_SCALE);
    let last = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      const dt = Math.min(64, now - last);
      last = now;
      for (let i = 0; i < WAVE_BARS; i++) {
        const idle = WAVE_MIN_SCALE + 0.1 * (0.5 + 0.5 * Math.sin(now / 320 + i * 0.85));
        const voice = levelForBar(levelsRef.current, i) * (WAVE_PROFILE[i] ?? 1);
        const target = Math.min(1, Math.max(idle, voice));
        const cur = current[i] ?? WAVE_MIN_SCALE;
        const tau = target > cur ? WAVE_ATTACK_MS : WAVE_RELEASE_MS;
        const next = cur + (target - cur) * (1 - Math.exp(-dt / tau));
        current[i] = next;
        const bar = barsRef.current[i];
        if (bar) bar.style.transform = `scaleY(${next.toFixed(3)})`;
      }
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <div className="flex items-center gap-[3.5px]" style={{ height: WAVE_HEIGHT }}>
      {Array.from({ length: WAVE_BARS }, (_, i) => (
        <div
          key={i}
          ref={(el) => {
            barsRef.current[i] = el;
          }}
          style={{
            width: 3.5,
            height: WAVE_HEIGHT,
            borderRadius: 2,
            transform: `scaleY(${WAVE_MIN_SCALE})`,
            transformOrigin: "center",
            willChange: "transform",
            background: `linear-gradient(180deg, ${accent}, #00d4ff)`,
          }}
        />
      ))}
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

export function RecordingErrorIndicator({ accent }: { accent?: string | undefined }) {
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

export function TranscriptionErrorIndicator({ accent }: { accent?: string | undefined }) {
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

export function TranslationErrorIndicator({ accent }: { accent?: string | undefined }) {
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
  title?: string | undefined;
  accent?: string | undefined;
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
  levels?: number[] | undefined;
  title?: string | undefined;
}) {
  if (state === "listening") return <SoundWave accent={accent} levels={levels} />;
  if (state === "transcribing") return <TranscribingIndicator t={t} />;
  if (state === "translating") return <TranslatingIndicator accent={accent} />;
  if (state === "error") return <ErrorIndicator t={t} title={title} accent={accent} />;
  return <CopiedIndicator />;
}
