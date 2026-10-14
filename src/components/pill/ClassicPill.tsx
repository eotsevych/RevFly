import type React from "react";
import {
  chooseMicrophone,
  triggerCancelRecording,
  triggerRetryTranslation,
  type AudioLevels,
} from "@/lib/tauri";
import type { Tokens } from "@/lib/tokens";
import { StateIndicator } from "./Indicators";
import { TRANSLATE_ACCENT, formatTimer, languageCode, type PillView } from "./view";

/** Width of the pill in every stage except long messages (the window is 340 wide). */
const PILL_WIDTH = 320;

/** The original pill: indicator, status with what happens to the speech, and the timer. Every
 * style falls back to it for messages that need reading or a click. */
export function ClassicPill({
  view,
  t,
  levels,
}: {
  view: PillView;
  t: Tokens;
  levels?: AudioLevels | undefined;
}) {
  const { accent, long: isLong } = view;
  return (
    <div
      data-tauri-drag-region
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
          boxShadow: [`0 0 0 1px ${t.pillInset} inset`, t.pillShadow, `0 0 20px ${accent}26`].join(
            ", ",
          ),
          transition: "box-shadow 0.3s ease, background 0.3s ease, opacity 0.2s ease",
        } as React.CSSProperties
      }
      className="flex items-center gap-3 pl-4 pr-4 select-none cursor-grab active:cursor-grabbing opacity-100 scale-100"
    >
      {/* State visual indicator icon */}
      <div className="flex items-center justify-center shrink-0" style={{ minWidth: 48 }}>
        <StateIndicator
          state={view.state}
          t={t}
          accent={accent}
          levels={levels}
          title={view.rawTitle}
          micDown={view.micWarning ? view.micDown : undefined}
        />
      </div>

      {/* Text column: Monospace label + Inter sublabel, centred in the space between the icon
          and the timer; long messages stay left-aligned so they read as paragraphs. */}
      <div
        className={`flex flex-1 flex-col justify-center min-w-0 ${isLong ? "" : "items-center text-center"}`}
      >
        <div className={`flex items-center gap-2 ${isLong ? "" : "justify-center"}`}>
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
            {view.title}
          </span>
          {view.run === "auto" && !view.micWarning && (
            <span
              title={`Auto-translate to ${view.targetLang}`}
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
              AUTO→{languageCode(view.targetLang)}
            </span>
          )}
          {view.run === "translate" && (
            <span
              title={`Translating to ${view.targetLang}`}
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
              {languageCode(view.targetLang)}
            </span>
          )}
        </div>

        <span
          className={isLong ? undefined : "truncate"}
          style={{
            fontSize: isLong ? 12.5 : 13,
            lineHeight: isLong ? 1.35 : undefined,
            color: view.state === "error" ? t.text : t.textMuted,
            marginTop: 2,
            fontFamily: "Inter, sans-serif",
            maxWidth: isLong ? "none" : "100%",
            whiteSpace: isLong ? "normal" : undefined,
          }}
          title={view.subtitle}
        >
          {view.subtitle}
        </span>

        {view.state === "error" && view.retry && (
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

        {/* The chosen mic isn't connected: pick another. A native <select>, so its options open
            in a system menu outside the pill and a long device list never grows the pill. */}
        {view.devices && (
          <div className="flex items-center gap-2" style={{ marginTop: 8 }}>
            <select
              aria-label="Microphone"
              value=""
              onChange={(e) => {
                if (e.target.value) void chooseMicrophone(e.target.value);
              }}
              style={
                {
                  WebkitAppRegion: "no-drag",
                  appearance: "none",
                  flex: 1,
                  minWidth: 0,
                  padding: "5px 28px 5px 12px",
                  borderRadius: 10,
                  border: `1px solid ${accent}55`,
                  backgroundColor: `${accent}12`,
                  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%23888' stroke-width='1.5'/%3E%3C/svg%3E")`,
                  backgroundRepeat: "no-repeat",
                  backgroundPosition: "right 10px center",
                  color: t.text,
                  fontFamily: "Inter, sans-serif",
                  fontSize: 12.5,
                  fontWeight: 500,
                  textOverflow: "ellipsis",
                  cursor: "pointer",
                } as React.CSSProperties
              }
            >
              <option value="" disabled>
                Choose a microphone…
              </option>
              {view.devices.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={() => void triggerCancelRecording()}
              style={
                {
                  WebkitAppRegion: "no-drag",
                  flexShrink: 0,
                  padding: "2px 4px",
                  border: "none",
                  background: "none",
                  color: t.textMuted,
                  fontFamily: "Inter, sans-serif",
                  fontSize: 12,
                  cursor: "pointer",
                } as React.CSSProperties
              }
            >
              Cancel
            </button>
          </div>
        )}
      </div>

      {/* Recording timer, vertically centred on the right edge of the pill */}
      {view.state === "listening" && (
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
          {formatTimer(view.elapsedSec)}
        </span>
      )}
    </div>
  );
}
