import React, { useEffect, useState } from "react";
import {
  fetchSettings,
  isTauri,
  saveWindowPosition,
  subscribeToAudioLevels,
  subscribeToStateChanges,
  triggerCancelRecording,
  triggerStartDragging,
  type AssistantStateEvent,
  type BackendSettings,
} from "@/lib/tauri";
import { tok, type AccentColor, type Theme } from "@/lib/tokens";
import { useResolvedTheme } from "@/lib/theme";
import { StateIndicator } from "./pill/Indicators";

export const STATE_META: Record<
  "idle" | "listening" | "transcribing" | "translating" | "done" | "error",
  { label: string; sublabel: string }
> = {
  idle: { label: "Ready", sublabel: "Press hotkey to start" },
  listening: { label: "Listening", sublabel: "Capturing audio input" },
  transcribing: { label: "Transcribing", sublabel: "Converting speech to text" },
  translating: { label: "Translating", sublabel: "Processing translation" },
  done: { label: "Copied!", sublabel: "Translation in clipboard" },
  error: { label: "Error", sublabel: "Action could not complete" },
};

export function getStateAccent(
  state: "idle" | "listening" | "transcribing" | "translating" | "done" | "error",
  accentColor: string,
  title?: string,
): string {
  if (state === "transcribing") return "#00d4ff";
  if (state === "done") return "#00e5a0";
  if (state === "error") {
    const lower = (title || "").toLowerCase();
    if (lower.includes("translat")) return "#ff9f43"; // Amber for translation
    if (lower.includes("record")) return "#ff4d6d"; // Crimson for recording
    return "#ff6b6b"; // Coral for transcription
  }
  return accentColor;
}

export function VoicePill() {
  const [state, setState] = useState<AssistantStateEvent>({
    state: "idle",
    title: "Ready",
    subtitle: null,
  });
  const [audioLevels, setAudioLevels] = useState<number[]>([0.0, 0.0, 0.0, 0.0, 0.0]);
  const [settings, setSettings] = useState<BackendSettings | null>(null);
  const [elapsedSec, setElapsedSec] = useState(0);

  useEffect(() => {
    fetchSettings().then((s) => {
      if (s) setSettings(s);
    });

    const unlistenState = subscribeToStateChanges((payload) => {
      setState(payload);
    });

    const unlistenAudio = subscribeToAudioLevels((levels) => {
      if (levels && levels.length > 0) {
        setAudioLevels(levels);
      }
    });

    let unlistenMoved: (() => void) | null = null;
    let saveTimeout: ReturnType<typeof setTimeout> | null = null;

    if (isTauri()) {
      import("@tauri-apps/api/webviewWindow").then(({ getCurrentWebviewWindow }) => {
        const win = getCurrentWebviewWindow();
        win
          .onMoved(({ payload: position }) => {
            if (saveTimeout) clearTimeout(saveTimeout);
            saveTimeout = setTimeout(() => {
              saveWindowPosition(position.x, position.y);
            }, 300);
          })
          .then((unlisten) => {
            unlistenMoved = unlisten;
          });
      });
    }

    const handlePointerUp = async () => {
      if (isTauri()) {
        try {
          const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
          const pos = await getCurrentWebviewWindow().outerPosition();
          saveWindowPosition(pos.x, pos.y);
        } catch {
          // ignore
        }
      }
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        triggerCancelRecording();
      }
    };

    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("mouseup", handlePointerUp);
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      unlistenState.then((fn) => fn?.());
      unlistenAudio.then((fn) => fn?.());
      if (saveTimeout) clearTimeout(saveTimeout);
      unlistenMoved?.();
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("mouseup", handlePointerUp);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | null = null;
    if (state.state === "listening") {
      setElapsedSec(0);
      fetchSettings().then((s) => s && setSettings(s));
      const start = Date.now();
      timer = setInterval(() => {
        setElapsedSec(Math.floor((Date.now() - start) / 1000));
      }, 500);
    } else {
      setElapsedSec(0);
    }
    return () => {
      if (timer) clearInterval(timer);
    };
  }, [state.state]);

  const handleCancel = async (e: React.MouseEvent) => {
    e.stopPropagation();
    await triggerCancelRecording();
  };

  const handleMouseDown = async (e: React.MouseEvent) => {
    if (e.button === 0 && !(e.target as HTMLElement).closest("button")) {
      await triggerStartDragging();
    }
  };

  const formatTimer = (secs: number) => {
    const mins = Math.floor(secs / 60);
    const s = secs % 60;
    return `${mins}:${s < 10 ? "0" : ""}${s}`;
  };

  const isIdle = state.state === "idle";
  const themeMode: Theme = useResolvedTheme(settings?.theme);
  const accentKey: AccentColor = (localStorage.getItem("aura_accent") as AccentColor) || "violet";
  const t = tok(themeMode, accentKey);
  const accent = getStateAccent(state.state, t.accent, state.title);

  const targetLang = settings?.target_lang || "English";
  const isNoTranslation =
    settings?.translation_provider === "No Translation" ||
    settings?.translation_provider === "none" ||
    targetLang.toLowerCase().includes("no translation") ||
    targetLang.toLowerCase() === "none";

  const meta = STATE_META[state.state];
  const dynamicSublabel =
    state.subtitle ||
    (state.state === "listening"
      ? isNoTranslation
        ? "Voice to Text"
        : meta.sublabel
      : state.state === "transcribing"
        ? isNoTranslation
          ? "Voice to Text"
          : meta.sublabel
        : meta.sublabel);

  const displayedSubtitle =
    state.state === "error" && state.text ? `Copied: "${state.text}"` : dynamicSublabel;

  if (isIdle) {
    return null;
  }

  return (
    <main
      data-tauri-drag-region
      onMouseDown={handleMouseDown}
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
      className="w-full h-full min-h-screen flex items-center justify-center p-2 bg-transparent select-none overflow-hidden cursor-grab active:cursor-grabbing"
    >
      <div
        data-tauri-drag-region
        onMouseDown={handleMouseDown}
        style={
          {
            WebkitAppRegion: "drag",
            height: 68,
            width: "fit-content",
            minWidth: 220,
            maxWidth: 320,
            borderRadius: 40,
            background: t.pillBg,
            border: `1px solid ${t.pillBorder}`,
            boxShadow: [
              `0 0 0 1px ${t.pillInset} inset`,
              t.pillShadow,
              `0 0 20px ${accent}26`,
            ].join(", "),
            transition: "box-shadow 0.3s ease, background 0.3s ease, opacity 0.2s ease",
          } as React.CSSProperties
        }
        className="flex items-center gap-3 pl-4 pr-3 select-none cursor-grab active:cursor-grabbing opacity-100 scale-100"
      >
        {/* State visual indicator icon */}
        <div
          className="flex items-center justify-center shrink-0"
          style={{ width: 44, minWidth: 44 }}
        >
          <StateIndicator
            state={state.state}
            t={t}
            accent={accent}
            levels={audioLevels}
            title={state.title}
          />
        </div>

        {/* Text column: Monospace label + Inter sublabel */}
        <div className="flex flex-col justify-center min-w-0">
          <div className="flex items-center gap-2">
            <span
              style={{
                fontFamily: "JetBrains Mono, monospace",
                fontSize: 13,
                fontWeight: 600,
                letterSpacing: "0.08em",
                color: accent,
                textTransform: "uppercase",
                transition: "color 0.3s ease",
              }}
            >
              {state.title && state.state !== "listening" ? state.title : meta.label}
            </span>

            {state.state === "listening" && (
              <span
                style={{
                  fontFamily: "JetBrains Mono, monospace",
                  fontSize: 12,
                  fontWeight: 500,
                  color: accent,
                  background: `${accent}18`,
                  padding: "1px 6px",
                  borderRadius: 6,
                  border: `1px solid ${accent}33`,
                }}
              >
                {formatTimer(elapsedSec)}
              </span>
            )}
          </div>

          <span
            className="truncate"
            style={{
              fontSize: 13,
              color: state.state === "error" ? t.text : t.textMuted,
              marginTop: 2,
              fontFamily: "Inter, sans-serif",
              maxWidth: 200,
            }}
            title={displayedSubtitle}
          >
            {displayedSubtitle}
          </span>
        </div>

        {/* Dismiss Button */}
        <button
          type="button"
          onClick={handleCancel}
          style={
            {
              WebkitAppRegion: "no-drag",
              color: t.textMuted,
            } as React.CSSProperties
          }
          className="flex h-6 w-6 items-center justify-center shrink-0 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
          aria-label="Dismiss"
          title="Dismiss (Esc)"
        >
          <span style={{ fontSize: 18, lineHeight: 1 }}>×</span>
        </button>
      </div>
    </main>
  );
}

export default VoicePill;
