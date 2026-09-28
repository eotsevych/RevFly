import { useState } from "react";
import type { Tokens } from "@/lib/tokens";
import { Section, Row, FieldSelect } from "./SettingsPrimitives";
import { useSettingsContext } from "./SettingsContext";
import { triggerClearHistory, triggerOpenAudioFolder } from "@/lib/tauri";

const PURGE_OPTIONS = [
  { value: "7", label: "1 week" },
  { value: "14", label: "2 weeks" },
  { value: "30", label: "1 month" },
  { value: "90", label: "3 months" },
  { value: "-1", label: "Never" },
];

const FORMAT_OPTIONS = [
  {
    value: "text_audio",
    label: "Audio + Transcript",
    hint: "Saves original audio recording and its text",
  },
  {
    value: "text_only",
    label: "Transcript only",
    hint: "Saves only the transcribed and translated text",
  },
  {
    value: "private",
    label: "Don't save",
    hint: "Nothing is persisted to disk (RAM only)",
  },
] as const;

export default function StorageTab({ t }: { t: Tokens }) {
  const { settings, updateSettings, logs, refreshLogs } = useSettingsContext();
  const [cleared, setCleared] = useState(false);

  const rawMode = settings.storage_mode || "text_only";
  const currentMode =
    rawMode === "text_and_audio" ? "text_audio" : rawMode === "private_mode" ? "private" : rawMode;
  const capMb = settings.storage_cap_mb || 500;
  const usedMb =
    logs.length === 0 ? 0 : Math.min(capMb, Math.max(1, Math.round(logs.length * 2.8)));
  const pct = Math.min(100, Math.round((usedMb / capMb) * 100));

  const handleOpenFolder = async () => {
    try {
      await triggerOpenAudioFolder();
    } catch (e) {
      console.error("Failed to open audio folder", e);
    }
  };

  const handleClear = async () => {
    try {
      await triggerClearHistory();
      await refreshLogs();
      setCleared(true);
      setTimeout(() => setCleared(false), 3000);
    } catch (e) {
      console.error("Failed to clear history", e);
    }
  };

  return (
    <div>
      <Section title="Save Format" t={t}>
        {FORMAT_OPTIONS.map((opt, i) => {
          const isSelected = currentMode === opt.value;
          return (
            <Row
              key={opt.value}
              label={opt.label}
              hint={opt.hint}
              t={t}
              last={i === FORMAT_OPTIONS.length - 1}
            >
              <div
                onClick={() => updateSettings({ storage_mode: opt.value })}
                style={{
                  width: 18,
                  height: 18,
                  borderRadius: "50%",
                  border: `2px solid ${isSelected ? t.accent : t.border}`,
                  background: isSelected ? t.accent : "transparent",
                  cursor: "pointer",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  transition: "all 0.15s",
                  flexShrink: 0,
                }}
              >
                {isSelected && (
                  <div
                    style={{
                      width: 6,
                      height: 6,
                      borderRadius: "50%",
                      background: "#fff",
                    }}
                  />
                )}
              </div>
            </Row>
          );
        })}
      </Section>

      <Section title="Retention" t={t}>
        <Row
          label="Storage cap"
          hint={`${usedMb} MB used of ${
            capMb >= 1000 ? (capMb / 1000).toFixed(1) + " GB" : capMb + " MB"
          }`}
          t={t}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <input
              type="range"
              min={100}
              max={10000}
              step={100}
              value={capMb}
              onChange={(e) => updateSettings({ storage_cap_mb: Number(e.target.value) })}
              style={{ width: 100, accentColor: t.accent, cursor: "pointer" }}
            />
            <span
              style={{
                fontSize: 12,
                fontFamily: "JetBrains Mono, monospace",
                color: t.accent,
                minWidth: 54,
                textAlign: "right",
              }}
            >
              {capMb >= 1000 ? `${(capMb / 1000).toFixed(1)} GB` : `${capMb} MB`}
            </span>
          </div>
        </Row>
        <Row label="Auto-purge after" hint="Automatically delete older recordings" t={t} last>
          <FieldSelect
            value={String(settings.retention_days ?? 30)}
            onChange={(v) => updateSettings({ retention_days: Number(v) })}
            options={PURGE_OPTIONS}
            t={t}
          />
        </Row>
      </Section>

      <Section title="Usage" t={t}>
        <div style={{ padding: 16 }}>
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 8 }}>
            <span style={{ fontSize: 12, color: t.textMuted, fontFamily: "Inter, sans-serif" }}>
              {usedMb} MB used
            </span>
            <span
              style={{
                fontSize: 12,
                color: pct > 80 ? t.warnColor : t.textDim,
                fontFamily: "JetBrains Mono, monospace",
              }}
            >
              {pct}%
            </span>
          </div>
          <div
            style={{
              height: 5,
              borderRadius: 3,
              background: t.stepTrack,
              overflow: "hidden",
              marginBottom: 16,
            }}
          >
            <div
              style={{
                height: "100%",
                borderRadius: 3,
                width: `${pct}%`,
                background: pct > 80 ? t.warnColor : `linear-gradient(90deg, ${t.accent}, #00d4ff)`,
                transition: "width 0.4s ease",
              }}
            />
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <button
              type="button"
              onClick={handleOpenFolder}
              style={{
                flex: 1,
                padding: "8px 12px",
                borderRadius: 7,
                cursor: "pointer",
                background: t.surface,
                border: `1px solid ${t.border}`,
                color: t.textMuted,
                fontSize: 12,
                fontFamily: "Inter, sans-serif",
                transition: "all 0.15s",
              }}
            >
              📁 Open Folder
            </button>
            <button
              type="button"
              onClick={handleClear}
              style={{
                flex: 1,
                padding: "8px 12px",
                borderRadius: 7,
                cursor: "pointer",
                background: cleared ? `${t.successColor}18` : t.dangerBg,
                border: `1px solid ${cleared ? t.successColor : t.dangerBorder}`,
                color: cleared ? t.successColor : t.dangerText,
                fontSize: 12,
                fontFamily: "Inter, sans-serif",
                transition: "all 0.2s",
              }}
            >
              {cleared ? "✓ Cleared!" : "🗑 Clear All Recordings"}
            </button>
          </div>
        </div>
      </Section>
    </div>
  );
}
