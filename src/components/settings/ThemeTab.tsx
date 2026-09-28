import type { Tokens, AccentColor } from "@/lib/tokens";
import { ACCENT_PALETTE } from "@/lib/tokens";
import Segmented from "@/components/ui/Segmented";
import { Section, Row } from "./SettingsPrimitives";
import { useSettingsContext } from "./useSettingsContext";
import { useResolvedTheme, type ThemeMode } from "@/lib/theme";

interface Props {
  t: Tokens;
  accent: AccentColor;
  onAccentChange: (accent: AccentColor) => void;
}

export default function ThemeTab({ t, accent, onAccentChange }: Props) {
  const { settings, updateSettings } = useSettingsContext();

  const currentMode = (settings.theme || "system") as ThemeMode;
  const effectiveTheme = useResolvedTheme(currentMode);

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

      {/* Live preview */}
      <Section title="Live Preview" t={t}>
        <div style={{ padding: 20 }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 14,
              padding: "14px 22px",
              background:
                effectiveTheme === "dark"
                  ? "linear-gradient(135deg, #111120 0%, #0d0d1e 100%)"
                  : "linear-gradient(135deg, #ffffff 0%, #f7f6ff 100%)",
              border: `1px solid ${t.pillBorder}`,
              borderRadius: 36,
              boxShadow: `0 0 28px ${t.accent}22, ${t.pillShadow}`,
              width: "fit-content",
            }}
          >
            <div style={{ display: "flex", gap: 3, alignItems: "center" }}>
              {[12, 22, 16, 28, 20, 14, 24].map((h, i) => (
                <div
                  key={i}
                  className="wave-bar"
                  style={{
                    background: `linear-gradient(180deg, ${t.accent}, #00d4ff)`,
                    height: h,
                    animationDelay: `${i * 0.12}s`,
                  }}
                />
              ))}
            </div>
            <div>
              <p
                style={{
                  fontSize: 11,
                  fontFamily: "JetBrains Mono, monospace",
                  color: t.accent,
                  fontWeight: 600,
                  margin: 0,
                  letterSpacing: "0.08em",
                  textTransform: "uppercase",
                }}
              >
                Listening
              </p>
              <p
                style={{
                  fontSize: 10,
                  color: t.textMuted,
                  fontFamily: "Inter, sans-serif",
                  margin: "2px 0 0",
                }}
              >
                Capturing audio input
              </p>
            </div>
          </div>
        </div>
      </Section>
    </div>
  );
}
