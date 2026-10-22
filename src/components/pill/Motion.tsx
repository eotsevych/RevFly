import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { AudioLevels } from "@/lib/tauri";
import { getBarVoiceScale, VOICE_CLOSE } from "./voiceLevel";

// Small looping animations shared by the pill styles.

/** Second dot of the translating orbit. */
const ORBIT_CYAN = "#22d3ee";

const center: CSSProperties = { display: "flex", alignItems: "center", justifyContent: "center" };

/** A counter that ticks every `ms`, for the small looping animations. */
function useTick(ms = 80): number {
  const [n, setN] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setN((x) => x + 1), ms);
    return () => clearInterval(id);
  }, [ms]);
  return n;
}

/** Processing: a crest that sweeps across flat bars. */
export function ScanWave({
  color,
  n,
  w,
  g,
  amp,
}: {
  color: string;
  n: number;
  w: number;
  g: number;
  amp: number;
}) {
  const pos = ((useTick() * 0.45) % (n + 4)) - 2;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: g, height: Math.max(20, w + amp) }}>
      {Array.from({ length: n }, (_, i) => {
        const k = Math.max(0, 1 - Math.abs(i - pos) / 1.6);
        return (
          <div
            key={i}
            style={{
              width: w,
              height: w + amp * k,
              borderRadius: w,
              background: color,
              opacity: 0.35 + 0.65 * k,
              transition: "height 80ms linear",
            }}
          />
        );
      })}
    </div>
  );
}

/** Translating: two dots circling each other. */
export function OrbitDots({ color, r, d }: { color: string; r: number; d: number }) {
  const a = useTick() * 0.22;
  const dot = (ang: number, background: string) => (
    <div
      style={{
        position: "absolute",
        left: "50%",
        top: "50%",
        width: d,
        height: d,
        marginLeft: -d / 2,
        marginTop: -d / 2,
        borderRadius: "50%",
        background,
        transform: `translate(${Math.cos(ang) * r}px, ${Math.sin(ang) * r * 0.45}px)`,
        opacity: 0.55 + 0.45 * Math.sin(ang),
      }}
    />
  );
  return (
    <div style={{ position: "relative", width: r * 2 + d, height: Math.max(20, d * 2) }}>
      {dot(a, ORBIT_CYAN)}
      {dot(a + Math.PI, color)}
    </div>
  );
}

/** Listening (Dot style): a record dot with a halo that swells with the voice. */
export function RecDot({
  levels,
  color,
  size,
}: {
  levels?: AudioLevels | undefined;
  color: string;
  size: number;
}) {
  const haloRef = useRef<HTMLDivElement>(null);
  const levelsRef = useRef(levels);
  levelsRef.current = levels;
  useEffect(() => {
    let v = 0;
    let frame = requestAnimationFrame(function tick() {
      const scale = getBarVoiceScale();
      const rawVolume = levelsRef.current?.volume ?? 0;
      const cleanVolume = Math.max(0, (rawVolume - VOICE_CLOSE) / (1 - VOICE_CLOSE));
      const peakFactor = Math.min(1, cleanVolume * 2.5);
      const effectiveScale = 1 + (scale - 1) * peakFactor;
      const peakVolume = Math.min(1, cleanVolume * effectiveScale);
      const target = Math.min(1, Math.max(0, (peakVolume - 0.03) / 0.5));
      v = target > v ? target : v * 0.88 + target * 0.12;
      const s = size * (0.45 + 0.55 * v);
      if (haloRef.current) {
        haloRef.current.style.width = `${s}px`;
        haloRef.current.style.height = `${s}px`;
      }
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [size]);
  return (
    <div style={{ ...center, position: "relative", width: size, height: size }}>
      <div
        ref={haloRef}
        style={{ position: "absolute", borderRadius: "50%", background: color, opacity: 0.22 }}
      />
      <div style={{ width: 9, height: 9, borderRadius: "50%", background: color }} />
    </div>
  );
}

export function Spinner({ color }: { color: string }) {
  return (
    <div
      className="animate-spin"
      style={{
        width: 12,
        height: 12,
        borderRadius: "50%",
        border: `2px solid ${color}`,
        borderTopColor: "transparent",
        boxSizing: "border-box",
      }}
    />
  );
}

export function ResultIcon({
  kind,
  color,
  size,
}: {
  kind: "check" | "x";
  color: string;
  size: number;
}) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" fill="none">
      <circle cx="10" cy="10" r="8.5" stroke={color} strokeWidth="1.6" />
      <path
        d={kind === "check" ? "M5 10.5l3.2 3L15 6.5" : "M6.5 6.5l7 7M13.5 6.5l-7 7"}
        stroke={color}
        strokeWidth="1.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function CopyIcon({ color, size }: { color: string; size: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" fill="none">
      <rect
        x="7"
        y="2.5"
        width="10.5"
        height="10.5"
        rx="2.5"
        stroke={color}
        strokeWidth="1.6"
        opacity="0.55"
      />
      <rect x="2.5" y="7" width="10.5" height="10.5" rx="2.5" stroke={color} strokeWidth="1.6" />
      <path
        d="M5.3 12.4l1.7 1.6 3.2-3.4"
        stroke={color}
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
