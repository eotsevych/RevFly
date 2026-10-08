import { useState } from "react";
import type { Tokens } from "../../lib/tokens";
import { formatDisplay } from "@/lib/hotkey";

interface HotkeyRecorderProps {
  value: string;
  onChange: (tauriHotkey: string) => void;
  t: Tokens;
  /** Shows an "Off" button that clears the hotkey (for optional hotkeys). */
  clearable?: boolean;
}

// Single-modifier hotkeys (e.g. Right Option) rely on the macOS event tap; other platforms only support key combos.
const IS_MAC = typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);
const COMBO_PRESET = "CommandOrControl+Shift+Space";

export default function HotkeyRecorder({ value, onChange, t, clearable }: HotkeyRecorderProps) {
  const [recording, setRecording] = useState(false);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();

    // 1. Detect single modifier keys (Right Option, Right Control, etc.)
    const code = e.code;
    const singleModifiers: Record<string, string> = {
      AltRight: "RightOption",
      ControlRight: "RightControl",
      AltLeft: "LeftOption",
      ControlLeft: "LeftControl",
      MetaRight: "RightCommand",
      MetaLeft: "LeftCommand",
      ShiftRight: "RightShift",
      ShiftLeft: "LeftShift",
    };

    if (IS_MAC && singleModifiers[code]) {
      onChange(singleModifiers[code]);
      setRecording(false);
      return;
    }

    // 2. Detect key combinations (e.g. CommandOrControl + Shift + Space)
    const parts: string[] = [];
    if (IS_MAC) {
      if (e.metaKey) parts.push("CommandOrControl");
      if (e.ctrlKey) parts.push("Control");
    } else {
      if (e.ctrlKey) parts.push("CommandOrControl");
      if (e.metaKey) parts.push("Super");
    }
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");

    const key = e.key;
    if (!["Meta", "Control", "Alt", "Shift"].includes(key)) {
      const finalKey = key === " " ? "Space" : key.length === 1 ? key.toUpperCase() : key;
      parts.push(finalKey);
      onChange(parts.join("+"));
      setRecording(false);
    }
  }

  const display = formatDisplay(value);

  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: 8, alignItems: "center" }}>
      <button
        type="button"
        onKeyDown={handleKeyDown}
        onClick={() => {
          setRecording(true);
          // An optional hotkey keeps its value until a new key is pressed, so clicking away
          // doesn't silently turn it off.
          if (!clearable) onChange("");
        }}
        tabIndex={0}
        className={recording ? "recording-hotkey" : ""}
        style={{
          padding: "7px 14px",
          borderRadius: 7,
          background: recording ? "rgba(255,80,80,0.08)" : t.inputBg,
          border: `1px solid ${recording ? "rgba(255,80,80,0.4)" : t.inputBorder}`,
          color: recording ? "rgba(255,120,120,0.9)" : t.inputText,
          fontSize: 12,
          fontFamily: "JetBrains Mono, monospace",
          cursor: "pointer",
          outline: "none",
          minWidth: 140,
          transition: "all 0.15s",
        }}
      >
        {recording ? (
          "⌨  Press key…"
        ) : display ? (
          <span style={{ letterSpacing: "0.04em" }}>{display}</span>
        ) : (
          <span style={{ color: t.textDim }}>Click to record</span>
        )}
      </button>

      {/* Quick Presets for Popular Modifiers */}
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        {IS_MAC && (
          <>
            <button
              type="button"
              onClick={() => onChange("RightOption")}
              style={{
                padding: "5px 9px",
                borderRadius: 6,
                background: value === "RightOption" ? `${t.accent}22` : t.surface,
                border: `1px solid ${value === "RightOption" ? t.accent : t.border}`,
                color: value === "RightOption" ? t.accent : t.textMuted,
                fontSize: 11,
                cursor: "pointer",
                fontWeight: value === "RightOption" ? 600 : 400,
              }}
            >
              Right ⌥
            </button>
            <button
              type="button"
              onClick={() => onChange("RightControl")}
              style={{
                padding: "5px 9px",
                borderRadius: 6,
                background: value === "RightControl" ? `${t.accent}22` : t.surface,
                border: `1px solid ${value === "RightControl" ? t.accent : t.border}`,
                color: value === "RightControl" ? t.accent : t.textMuted,
                fontSize: 11,
                cursor: "pointer",
                fontWeight: value === "RightControl" ? 600 : 400,
              }}
            >
              Right ⌃
            </button>
          </>
        )}
        <button
          type="button"
          onClick={() => onChange(COMBO_PRESET)}
          style={{
            padding: "5px 9px",
            borderRadius: 6,
            background: value === COMBO_PRESET ? `${t.accent}22` : t.surface,
            border: `1px solid ${value === COMBO_PRESET ? t.accent : t.border}`,
            color: value === COMBO_PRESET ? t.accent : t.textMuted,
            fontSize: 11,
            cursor: "pointer",
            fontWeight: value === COMBO_PRESET ? 600 : 400,
          }}
        >
          {IS_MAC ? "⌘+⇧+Space" : "Ctrl+⇧+Space"}
        </button>
        {clearable && (
          <button
            type="button"
            onClick={() => {
              setRecording(false);
              onChange("");
            }}
            style={{
              padding: "5px 9px",
              borderRadius: 6,
              background: !value ? `${t.accent}22` : t.surface,
              border: `1px solid ${!value ? t.accent : t.border}`,
              color: !value ? t.accent : t.textMuted,
              fontSize: 11,
              cursor: "pointer",
              fontWeight: !value ? 600 : 400,
            }}
          >
            Off
          </button>
        )}
      </div>
    </div>
  );
}
