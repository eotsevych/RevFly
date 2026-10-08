import React, { useEffect, useState } from "react";
import {
  fetchSettings,
  isTauri,
  saveWindowPosition,
  subscribeToAudioLevels,
  subscribeToStateChanges,
  triggerCancelRecording,
  triggerRetryTranslation,
  triggerStartDragging,
  type AssistantStateEvent,
  type AudioLevels,
  type BackendSettings,
} from "@/lib/tauri";
import { tok, type AccentColor, type Theme } from "@/lib/tokens";
import { useResolvedTheme } from "@/lib/theme";
import { formatDisplay } from "@/lib/hotkey";
import { StateIndicator } from "./pill/Indicators";

const STATE_META: Record<
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

/** Color of a recording started with the translate hotkey, from listening through translating. */
const TRANSLATE_ACCENT = "#14b8a6";

/** Width of the pill in every stage except long messages (the window is 340 wide). */
const PILL_WIDTH = 300;

/** How many recordings show "Press … to finish" after install or after a hotkey changes. */
const FINISH_HINT_RECORDINGS = 3;
const FINISH_HINT_KEY = "revfly_finish_hint";

const LANGUAGE_CODES: Record<string, string> = {
  english: "EN",
  ukrainian: "UK",
  spanish: "ES",
  french: "FR",
  german: "DE",
  italian: "IT",
  polish: "PL",
  japanese: "JA",
  chinese: "ZH",
  russian: "RU",
};

function languageCode(name: string): string {
  return LANGUAGE_CODES[name.trim().toLowerCase()] ?? name.trim().slice(0, 2).toUpperCase();
}

/** Whether this recording shows the finish hint; counts it if so. The count restarts whenever
 * either hotkey changes, so a new key is explained a few times too. */
function takeFinishHint(s: BackendSettings): boolean {
  if (s.show_hints === false) return false;
  const keys = `${s.hotkey}|${s.translate_hotkey ?? ""}`;
  try {
    const saved = JSON.parse(localStorage.getItem(FINISH_HINT_KEY) || "null") as {
      keys: string;
      shown: number;
    } | null;
    const shown = saved?.keys === keys ? saved.shown : 0;
    if (shown >= FINISH_HINT_RECORDINGS) return false;
    localStorage.setItem(FINISH_HINT_KEY, JSON.stringify({ keys, shown: shown + 1 }));
    return true;
  } catch {
    return true;
  }
}

function getStateAccent(
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
  const [audioLevels, setAudioLevels] = useState<AudioLevels | undefined>(undefined);
  const [settings, setSettings] = useState<BackendSettings | null>(null);
  const [elapsedSec, setElapsedSec] = useState(0);
  const [showFinishHint, setShowFinishHint] = useState(false);
  // The backend sends the mode only while listening; keep it so processing stays in its color.
  const [runMode, setRunMode] = useState<AssistantStateEvent["mode"]>(undefined);

  useEffect(() => {
    fetchSettings().then((s) => {
      if (s) setSettings(s);
    });

    const unlistenState = subscribeToStateChanges((payload) => {
      setState(payload);
      if (payload.state === "listening") setRunMode(payload.mode);
      else if (payload.state === "idle") setRunMode(undefined);
    });

    const unlistenAudio = subscribeToAudioLevels((levels) => {
      if (levels?.bands) setAudioLevels(levels);
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
      setShowFinishHint(false);
      fetchSettings().then((s) => {
        if (!s) return;
        setSettings(s);
        setShowFinishHint(takeFinishHint(s));
      });
      const start = Date.now();
      timer = setInterval(() => {
        setElapsedSec(Math.floor((Date.now() - start) / 1000));
      }, 200);
    } else {
      setElapsedSec(0);
    }
    return () => {
      if (timer) clearInterval(timer);
    };
  }, [state.state]);

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
  const accentKey: AccentColor = (localStorage.getItem("revfly_accent") as AccentColor) || "violet";
  const t = tok(themeMode, accentKey);
  const micWarning = state.state === "listening" && state.warning === true;
  // A recording started with the translate hotkey stays teal until it's done; automatic
  // translation only gets an outlined badge, since it may not translate at all.
  const isTranslateMode = runMode === "translate";
  const isAutoMode = runMode === "auto";
  const inRun = ["listening", "transcribing", "translating"].includes(state.state);
  const inTranslateRun = isTranslateMode && inRun;
  const accent = micWarning
    ? "#ff9f43"
    : inTranslateRun
      ? TRANSLATE_ACCENT
      : getStateAccent(state.state, t.accent, state.title);

  const targetLang = settings?.target_lang || "English";
  const isNoTranslation =
    settings?.translation_mode === "off" ||
    targetLang.toLowerCase().includes("no translation") ||
    targetLang.toLowerCase() === "none";
  const spokenOnly =
    settings?.source_lang && settings.source_lang !== "Auto" ? settings.source_lang : null;

  const meta = STATE_META[state.state];
  // The backend says what this recording does with the speech (see AssistantStateEvent.mode).
  const isTranscribeOnly = runMode === "transcribe";
  const startedWith = isTranslateMode ? settings?.translate_hotkey : settings?.hotkey;
  const finishHint =
    state.state === "listening" && showFinishHint && startedWith
      ? `Press ${formatDisplay(startedWith).replace(/ \(.*\)/, "")} to finish`
      : null;
  // While listening, say what will happen to the speech.
  const listeningIntent = isTranslateMode
    ? `Translating to ${targetLang}`
    : isNoTranslation || isTranscribeOnly
      ? `In ${spokenOnly ?? "your language"}`
      : `Auto-translate on · ${spokenOnly ?? `not ${targetLang}`} → ${targetLang}`;
  const dynamicSublabel =
    state.subtitle || finishHint || (state.state === "listening" ? listeningIntent : meta.sublabel);

  const displayedSubtitle =
    state.state === "error" && state.text ? `Copied: "${state.text}"` : dynamicSublabel;

  // Errors and mic warnings that explain what to fix are shown in full, in a pill that grows to fit them.
  const isLong = (state.state === "error" || state.state === "listening") && state.long === true;

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
            height: isLong ? undefined : 68,
            minHeight: isLong ? 68 : undefined,
            paddingTop: isLong ? 12 : undefined,
            paddingBottom: isLong ? 12 : undefined,
            // One width for every stage so the pill doesn't jump as its content changes; only
            // long messages (errors, mic warnings) grow to fit.
            width: isLong ? "100%" : PILL_WIDTH,
            maxWidth: isLong ? 384 : PILL_WIDTH,
            borderRadius: isLong ? 28 : 40,
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
        className="flex items-center gap-3 pl-4 pr-4 select-none cursor-grab active:cursor-grabbing opacity-100 scale-100"
      >
        {/* State visual indicator icon */}
        <div
          className="flex items-center justify-center shrink-0"
          style={{ width: 48, minWidth: 48 }}
        >
          <StateIndicator
            state={state.state}
            t={t}
            accent={accent}
            levels={audioLevels}
            title={state.title}
            micDown={micWarning ? state.mic : undefined}
          />
        </div>

        {/* Text column: Monospace label + Inter sublabel */}
        <div className="flex flex-1 flex-col justify-center min-w-0">
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
              {state.title && (state.state !== "listening" || micWarning)
                ? state.title
                : meta.label}
            </span>
            {isAutoMode && inRun && !micWarning && (
              <span
                title={`Auto-translate to ${targetLang}`}
                style={{
                  fontFamily: "JetBrains Mono, monospace",
                  fontSize: 10.5,
                  fontWeight: 700,
                  letterSpacing: "0.06em",
                  lineHeight: "16px",
                  color: TRANSLATE_ACCENT,
                  border: `1px solid ${TRANSLATE_ACCENT}88`,
                  borderRadius: 5,
                  padding: "0 5px",
                  whiteSpace: "nowrap",
                }}
              >
                AUTO→{languageCode(targetLang)}
              </span>
            )}
            {inTranslateRun && (
              <span
                title={`Translating to ${targetLang}`}
                style={{
                  fontFamily: "JetBrains Mono, monospace",
                  fontSize: 10.5,
                  fontWeight: 700,
                  letterSpacing: "0.06em",
                  lineHeight: "16px",
                  color: TRANSLATE_ACCENT,
                  background: `${TRANSLATE_ACCENT}1f`,
                  border: `1px solid ${TRANSLATE_ACCENT}55`,
                  borderRadius: 5,
                  padding: "0 5px",
                }}
              >
                {languageCode(targetLang)}
              </span>
            )}
          </div>

          <span
            className={isLong ? undefined : "truncate"}
            style={{
              fontSize: isLong ? 12.5 : 13,
              lineHeight: isLong ? 1.35 : undefined,
              color: state.state === "error" ? t.text : t.textMuted,
              marginTop: 2,
              fontFamily: "Inter, sans-serif",
              maxWidth: isLong ? "none" : "100%",
              whiteSpace: isLong ? "normal" : undefined,
            }}
            title={displayedSubtitle}
          >
            {displayedSubtitle}
          </span>

          {state.state === "error" && state.retry && (
            <button
              type="button"
              onClick={() => void triggerRetryTranslation()}
              style={{
                alignSelf: "flex-start",
                marginTop: 8,
                padding: "4px 12px",
                borderRadius: 999,
                border: `1px solid ${accent}66`,
                background: `${accent}1f`,
                color: accent,
                fontFamily: "Inter, sans-serif",
                fontSize: 12,
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              ↻ Retry translation
            </button>
          )}
        </div>

        {/* Recording timer, vertically centred on the right edge of the pill */}
        {state.state === "listening" && (
          <span
            className="shrink-0 self-center"
            style={{
              fontFamily: "JetBrains Mono, monospace",
              fontSize: 12,
              fontWeight: 500,
              fontVariantNumeric: "tabular-nums",
              lineHeight: "18px",
              textAlign: "center",
              minWidth: 44,
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
    </main>
  );
}

export default VoicePill;
