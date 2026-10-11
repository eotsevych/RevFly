import type { Tokens, AccentColor } from "@/lib/tokens";
import { ACCENT_PALETTE } from "@/lib/tokens";
import Segmented from "@/components/ui/Segmented";
import { Section, Row } from "./SettingsPrimitives";
import { useSettingsContext } from "./useSettingsContext";
import { useResolvedTheme, type ThemeMode } from "@/lib/theme";
import { PillPreview } from "@/components/pill/PillPreview";
import { PILL_STYLES, pillStyleOf } from "@/components/pill/view";

interface Props {
  t: Tokens;
  accent: AccentColor;
  onAccentChange: (accent: AccentColor) => void;
}

export default function ThemeTab({ t, accent, onAccentChange }: Props) {
  const { settings, updateSettings } = useSettingsContext();

  const currentMode = (settings.theme || "system") as ThemeMode;
  const effectiveTheme = useResolvedTheme(currentMode);
  const pillStyle = pillStyleOf(settings.pill_style);

  return (
    <div>
      <Section title="Appearance" t={t}>
        <Row
          label="Color scheme"
          hint={
            currentMode === "system"
              ? `Follows system theme (${effectiveTheme === "dark" ? "Dark" : "Light"} active)`
              : "Overall application theme"
          }
          t={t}
        >
          <Segmented
            options={[
              { value: "system", label: "💻 Follow System" },
              { value: "dark", label: "⬤ Dark" },
              { value: "light", label: "○ Light" },
            ]}
            value={currentMode}
            onChange={(v) => updateSettings({ theme: v })}
            t={t}
          />
        </Row>
      </Section>

      <Section title="Accent Color" t={t}>
        <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 14 }}>
          <div style={{ display: "flex", gap: 12, flexWrap: "wrap", alignItems: "center" }}>
            {(
              Object.entries(ACCENT_PALETTE) as [
                AccentColor,
                (typeof ACCENT_PALETTE)[AccentColor],
              ][]
            ).map(([key, val]) => (
              <button
                key={key}
                type="button"
                onClick={() => onAccentChange(key)}
                title={val.label}
                style={{
                  width: 34,
                  height: 34,
                  borderRadius: "50%",
                  border: "none",
                  background: val.primary,
                  cursor: "pointer",
                  boxShadow:
                    accent === key
                      ? `0 0 0 3px ${t.modalBg}, 0 0 0 5px ${val.primary}`
                      : "0 2px 8px rgba(0,0,0,0.2)",
                  transform: accent === key ? "scale(1.15)" : "scale(1)",
                  transition: "all 0.2s",
                  position: "relative",
                  outline: "none",
                }}
              >
                {accent === key && (
                  <svg
                    style={{ position: "absolute", inset: 0, margin: "auto" }}
                    width="14"
                    height="14"
                    viewBox="0 0 14 14"
                    fill="none"
                  >
                    <path
                      d="M3 7l3 3 5-6"
                      stroke="#fff"
                      strokeWidth="2"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                )}
              </button>
            ))}
            <span
              style={{
                fontSize: 12,
                color: t.textMuted,
                fontFamily: "Inter, sans-serif",
                marginLeft: 6,
              }}
            >
              {ACCENT_PALETTE[accent]?.label ?? "Violet"}
            </span>
          </div>
        </div>
      </Section>

      <Section title="Pill Style" t={t}>
        <div
          style={{
            padding: 10,
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(270px, 1fr))",
            gap: 8,
          }}
        >
          {PILL_STYLES.map((style) => {
            const selected = pillStyle === style.id;
            return (
              <button
                key={style.id}
                type="button"
                aria-pressed={selected}
                title={style.description}
                onClick={() => void updateSettings({ pill_style: style.id })}
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                  padding: 6,
                  textAlign: "left",
                  borderRadius: 9,
                  border: `1px solid ${selected ? t.accent : t.border}`,
                  background: selected ? `${t.accent}12` : "transparent",
                  boxShadow: selected ? `0 0 0 1px ${t.accent}` : "none",
                  cursor: "pointer",
                  transition: "border-color 0.2s, background 0.2s",
                }}
              >
                {/* Every style at the same 80% scale, so their sizes compare honestly */}
                <div
                  style={{
                    height: 62,
                    borderRadius: 6,
                    background: effectiveTheme === "dark" ? "#0b0b12" : "#ecebf1",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    overflow: "hidden",
                    pointerEvents: "none",
                  }}
                >
                  <div style={{ transform: "scale(0.8)" }}>
                    <PillPreview style={style.id} t={t} />
                  </div>
                </div>
                <div
                  style={{ display: "flex", alignItems: "baseline", gap: 7, padding: "0 4px 2px" }}
                >
                  <span
                    style={{
                      width: 12,
                      height: 12,
                      borderRadius: "50%",
                      border: `1.5px solid ${selected ? t.accent : t.textDim}`,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      flexShrink: 0,
                      alignSelf: "center",
                    }}
                  >
                    {selected && (
                      <span
                        style={{ width: 6, height: 6, borderRadius: "50%", background: t.accent }}
                      />
                    )}
                  </span>
                  <span
                    style={{
                      fontSize: 12.5,
                      fontWeight: 600,
                      color: t.text,
                      fontFamily: "Inter, sans-serif",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {style.name}
                  </span>
                  <span
                    style={{
                      fontSize: 11.5,
                      color: t.textMuted,
                      fontFamily: "Inter, sans-serif",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                      minWidth: 0,
                    }}
                  >
                    {style.hint}
                  </span>
                </div>
              </button>
            );
          })}
        </div>
        <p
          style={{
            margin: 0,
            padding: "0 14px 10px",
            fontSize: 11.5,
            color: t.textDim,
            fontFamily: "Inter, sans-serif",
          }}
        >
          Long errors, microphone warnings and Retry always use the Classic pill. A new style shows
          from your next recording.
        </p>
      </Section>
    </div>
  );
}
