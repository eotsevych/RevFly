import { useEffect, useRef } from "react";
import type { AudioLevels } from "../../lib/tauri";
import type { Tokens } from "../../lib/tokens";

// Seven capsule towers. The centre tower follows low pitch (voice), the edges follow high pitch
// ("s", "t", breath); the backend sends band energy already mirrored in that order.
const BAR_COUNT = 7;
const BAR_WIDTH = 4;
const BAR_GAP = 3;
const BAR_MIN_H = 8; // never collapse, so the capsule shape stays visible
const BAR_MAX_H = 29; // stays clear of the pill edge
const BAR_GROWTH = 20; // extra height at full energy and volume
const BAR_OVERSHOOT_MAX = 2; // spring overshoot when a loud word starts
const WAVE_BOX_H = BAR_MAX_H + BAR_OVERSHOOT_MAX + 1;
// Silence: a slow rolling wave around the resting height shows the app is still listening.
const IDLE_REST_H = 11;
const IDLE_AMPLITUDE = 3;
const SILENCE_VOLUME = 0.02;
// Falling bars keep 80% of their height per 60 fps frame; rises are instant.
const FALL_KEEP_PER_FRAME = 0.8;
const FRAME_MS = 1000 / 60;
// Loudness where the glow and colour shift start, and where they reach full strength.
const LOUD_START = 0.35;
const LOUD_FULL = 0.8;

const GRADIENT_QUIET = ["#6468da", "#389aff", "#04caff"] as const;
const GRADIENT_LOUD = ["#8f6bff", "#4aa3ff", "#1ae4ff"] as const;

function mixHex(a: string, b: string, t: number): string {
  const pa = parseInt(a.slice(1), 16);
  const pb = parseInt(b.slice(1), 16);
  const ch = (shift: number) =>
    Math.round(((pa >> shift) & 255) + (((pb >> shift) & 255) - ((pa >> shift) & 255)) * t);
  return `rgb(${ch(16)}, ${ch(8)}, ${ch(0)})`;
}

function barGradient(loud: number): string {
  const [top, mid, bottom] = GRADIENT_QUIET.map((c, i) => mixHex(c, GRADIENT_LOUD[i] ?? c, loud));
  return `linear-gradient(180deg, ${top} 0%, ${mid} 50%, ${bottom} 100%)`;
}

export function SoundWave({ levels }: { levels?: AudioLevels | undefined }) {
  const barsRef = useRef<(HTMLDivElement | null)[]>([]);
  const levelsRef = useRef<AudioLevels | undefined>(levels);
  levelsRef.current = levels;

  useEffect(() => {
    const heights = new Array<number>(BAR_COUNT).fill(IDLE_REST_H);
    let loud = 0;
    let paintedLoud = -1;
    let last = performance.now();

    let frame = requestAnimationFrame(function tick(now) {
      const dt = Math.min(64, now - last);
      last = now;
      const fallKeep = Math.pow(FALL_KEEP_PER_FRAME, dt / FRAME_MS);
      const volume = levelsRef.current?.volume ?? 0;
      const bands = levelsRef.current?.bands ?? [];
      const silent = volume < SILENCE_VOLUME;

      // Glow and colour shift follow loudness with the same fast-rise, slow-fall feel.
      const loudTarget = Math.min(1, Math.max(0, (volume - LOUD_START) / (LOUD_FULL - LOUD_START)));
      loud = loudTarget > loud ? loudTarget : loud * fallKeep + loudTarget * (1 - fallKeep);

      for (let i = 0; i < BAR_COUNT; i++) {
        const target = silent
          ? IDLE_REST_H + Math.sin((now / 1000) * 3 + i * 0.8) * IDLE_AMPLITUDE
          : BAR_MIN_H + (bands[i] ?? 0) * volume * BAR_GROWTH;
        const cur = heights[i] ?? IDLE_REST_H;
        let next: number;
        if (target > cur) {
          // Instant rise, with a small spring overshoot on the onset of a loud word.
          const jump = target - cur;
          next =
            target + (!silent && loudTarget > 0 ? Math.min(BAR_OVERSHOOT_MAX, jump * 0.15) : 0);
        } else {
          next = cur * fallKeep + target * (1 - fallKeep);
        }
        next = Math.min(BAR_MAX_H + BAR_OVERSHOOT_MAX, Math.max(BAR_MIN_H, next));
        heights[i] = next;
        const bar = barsRef.current[i];
        if (bar) bar.style.height = `${next.toFixed(2)}px`;
      }

      if (Math.abs(loud - paintedLoud) > 0.02) {
        paintedLoud = loud;
        const background = barGradient(loud);
        const glow =
          loud > 0.01 ? `0 0 10px rgba(4, 202, 255, ${(0.7 * loud).toFixed(3)})` : "none";
        for (const bar of barsRef.current) {
          if (!bar) continue;
          bar.style.background = background;
          bar.style.boxShadow = glow;
        }
      }

      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <div style={{ display: "flex", alignItems: "center", height: WAVE_BOX_H, gap: BAR_GAP }}>
      {Array.from({ length: BAR_COUNT }, (_, i) => (
        <div
          key={i}
          ref={(el) => {
            barsRef.current[i] = el;
          }}
          style={{
            width: BAR_WIDTH,
            height: IDLE_REST_H,
            borderRadius: BAR_WIDTH,
            background: barGradient(0),
            willChange: "height",
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
  levels?: AudioLevels | undefined;
  title?: string | undefined;
}) {
  if (state === "listening") return <SoundWave levels={levels} />;
  if (state === "transcribing") return <TranscribingIndicator t={t} />;
  if (state === "translating") return <TranslatingIndicator accent={accent} />;
  if (state === "error") return <ErrorIndicator t={t} title={title} accent={accent} />;
  return <CopiedIndicator />;
}
