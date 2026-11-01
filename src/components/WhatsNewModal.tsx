import { useState, useEffect, type ReactNode } from "react";
import { useResolvedTheme } from "@/lib/theme";
import { dismissWhatsNew, openSettingsFromWhatsNew, fetchUpdateStatus } from "@/lib/tauri";

const MONO = "'JetBrains Mono', ui-monospace, monospace";
const SANS = "Inter, system-ui, sans-serif";

const DARK = {
  card: "#14141f",
  cardBorder: "rgba(255,255,255,.08)",
  row: "#181828",
  rowBorder: "rgba(255,255,255,.07)",
  text: "#e8e8f0",
  body: "#9a9ab0",
  dim: "#8a8aa2",
  accent: "#a594ff",
  accentSoft: "rgba(165,148,255,.14)",
  accentSoftBorder: "rgba(165,148,255,.3)",
  accentText: "#c4b8ff",
  onAccent: "#0b0b12",
  green: "#34d399",
  greenSoft: "rgba(52,211,153,.12)",
  greenBorder: "rgba(52,211,153,.35)",
  cyan: "#22d3ee",
  cyanSoft: "rgba(34,211,238,.12)",
  cyanBorder: "rgba(34,211,238,.35)",
  closeBtnHover: "rgba(255,255,255,.08)",
};

const LIGHT: typeof DARK = {
  card: "#ffffff",
  cardBorder: "rgba(0,0,0,.08)",
  row: "#f8f8fc",
  rowBorder: "rgba(0,0,0,.06)",
  text: "#1a1a24",
  body: "#5c5c70",
  dim: "#6e6e84",
  accent: "#7c6af0",
  accentSoft: "rgba(124,106,240,.12)",
  accentSoftBorder: "rgba(124,106,240,.3)",
  accentText: "#5b4bd6",
  onAccent: "#ffffff",
  green: "#059669",
  greenSoft: "rgba(5,150,105,.1)",
  greenBorder: "rgba(5,150,105,.3)",
  cyan: "#0891b2",
  cyanSoft: "rgba(8,145,178,.1)",
  cyanBorder: "rgba(8,145,178,.3)",
  closeBtnHover: "rgba(0,0,0,.05)",
};

type Palette = typeof DARK;

const AppMark = () => (
  <div
    style={{
      width: 22,
      height: 22,
      borderRadius: 7,
      background: "linear-gradient(135deg,#6d7cff,#22c5ee)",
      flexShrink: 0,
    }}
  />
);

function Icon({ children }: { children: ReactNode }) {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </svg>
  );
}

function Feature({
  p,
  color,
  soft,
  border,
  icon,
  title,
  children,
}: {
  p: Palette;
  color: string;
  soft: string;
  border: string;
  icon: ReactNode;
  title: string;
  children: ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 12,
        padding: "8px 14px",
        background: p.row,
        border: `1px solid ${p.rowBorder}`,
        borderRadius: 12,
      }}
    >
      <div
        style={{
          width: 32,
          height: 32,
          borderRadius: 8,
          background: soft,
          border: `1px solid ${border}`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
          color,
        }}
      >
        {icon}
      </div>
      <div style={{ flex: 1 }}>
        <div style={{ fontSize: 13, fontWeight: 600, color: p.text, marginBottom: 2 }}>{title}</div>
        <div style={{ fontSize: 12, lineHeight: 1.4, color: p.body }}>{children}</div>
      </div>
    </div>
  );
}

/** Shown once after each update, and from Settings › Updates. Update the features for each release
 * that changes what users see. */
export function WhatsNewModal() {
  const theme = useResolvedTheme();
  const p = theme === "dark" ? DARK : LIGHT;
  const [loading, setLoading] = useState<"settings" | "dismiss" | null>(null);
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    fetchUpdateStatus().then((s) => setVersion(s?.current_version ?? null));
  }, []);

  const run = (which: "settings" | "dismiss", action: () => Promise<void>) => async () => {
    if (loading) return;
    setLoading(which);
    try {
      await action();
    } catch (e) {
      console.error("What's New action failed:", e);
      setLoading(null);
    }
  };

  const handleSettings = run("settings", openSettingsFromWhatsNew);
  const handleDismiss = run("dismiss", dismissWhatsNew);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") handleDismiss();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [handleDismiss]);

  return (
    <div
      style={{
        width: "100vw",
        height: "100vh",
        boxSizing: "border-box",
        background: p.card,
        border: `1px solid ${p.cardBorder}`,
        borderRadius: 20,
        color: p.text,
        fontFamily: SANS,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        userSelect: "none",
        padding: "20px 24px 22px",
      }}
    >
      {/* Header */}
      <div
        data-tauri-drag-region
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: 16,
        }}
      >
        <div data-tauri-drag-region style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <AppMark />
          <span data-tauri-drag-region style={{ fontSize: 13, fontWeight: 600 }}>
            RevFly
          </span>
          {version && (
            <span
              style={{
                padding: "2px 7px",
                borderRadius: 5,
                border: `1px solid ${p.accentSoftBorder}`,
                background: p.accentSoft,
                color: p.accentText,
                font: `600 10px ${MONO}`,
                letterSpacing: 0.5,
                textTransform: "uppercase",
              }}
            >
              v{version}
            </span>
          )}
        </div>

        <button
          type="button"
          onClick={handleDismiss}
          disabled={loading !== null}
          title="Close"
          aria-label="Close"
          style={{
            width: 26,
            height: 26,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            background: "none",
            border: "none",
            borderRadius: 6,
            color: p.dim,
            cursor: "pointer",
            padding: 0,
            transition: "background 0.15s",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = p.closeBtnHover;
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "none";
          }}
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path
              d="M3 3L11 11M11 3L3 11"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            />
          </svg>
        </button>
      </div>

      {/* Title & lead */}
      <div style={{ marginBottom: 14 }}>
        <h1
          style={{
            margin: "0 0 6px",
            fontSize: 19,
            fontWeight: 700,
            letterSpacing: -0.3,
            color: p.text,
          }}
        >
          The biggest RevFly update yet
        </h1>
        <p style={{ margin: 0, fontSize: 13, lineHeight: 1.45, color: p.body }}>
          Translation without the internet on every computer, faster transcription on your
          computer&apos;s own chips, and more privacy.
        </p>
      </div>

      {/* Feature highlights; they scroll if the window is smaller than they need. */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 6,
          flex: 1,
          minHeight: 0,
          overflowY: "auto",
        }}
      >
        <Feature
          p={p}
          color={p.green}
          soft={p.greenSoft}
          border={p.greenBorder}
          title="Offline translation on every computer"
          icon={
            <Icon>
              <circle cx="12" cy="12" r="10" />
              <path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
            </Icon>
          }
        >
          Windows, Linux and Mac. Free, no API key, works without internet, and nothing leaves your
          computer.
        </Feature>
        <Feature
          p={p}
          color={p.accent}
          soft={p.accentSoft}
          border={p.accentSoftBorder}
          title="Faster transcription on your hardware"
          icon={
            <Icon>
              <rect x="5" y="5" width="14" height="14" rx="2" />
              <path d="M9 9h6v6H9zM9 2v3M15 2v3M9 19v3M15 19v3M19 9h3M19 15h3M2 9h3M2 15h3" />
            </Icon>
          }
        >
          Parakeet runs on the Apple Neural Engine. Whisper uses the GPU on Windows and Linux.
        </Feature>
        <Feature
          p={p}
          color={p.cyan}
          soft={p.cyanSoft}
          border={p.cyanBorder}
          title="Transcribe while you talk"
          icon={
            <Icon>
              <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
            </Icon>
          }
        >
          Works with every model, so long dictations are ready right after you stop. Turn it on in
          Settings › Transcription.
        </Feature>
        <Feature
          p={p}
          color={p.accent}
          soft={p.accentSoft}
          border={p.accentSoftBorder}
          title="Apple Intelligence translation"
          icon={
            <Icon>
              <path d="M12 3l1.9 5.8L20 11l-6.1 2.2L12 19l-1.9-5.8L4 11l6.1-2.2z" />
            </Icon>
          }
        >
          On macOS 26, translate on your Mac with your own prompt.
        </Feature>
        <Feature
          p={p}
          color={p.green}
          soft={p.greenSoft}
          border={p.greenBorder}
          title="More privacy"
          icon={
            <Icon>
              <rect x="3" y="11" width="18" height="11" rx="2" />
              <path d="M7 11V7a5 5 0 0 1 10 0v4" />
            </Icon>
          }
        >
          &ldquo;Don&apos;t save&rdquo; now keeps nothing on disk. Your saved dictations are listed
          in Settings › History.
        </Feature>
        <Feature
          p={p}
          color={p.cyan}
          soft={p.cyanSoft}
          border={p.cyanBorder}
          title="Text styles"
          icon={
            <Icon>
              <path d="M4 7V5h16v2M9 19h6M12 5v14" />
            </Icon>
          }
        >
          Natural speech, For AI prompts or Verbatim. Pick one in Settings › Text.
        </Feature>
      </div>

      {/* Actions */}
      <div style={{ marginTop: 16, display: "flex", gap: 10 }}>
        <button
          type="button"
          onClick={handleSettings}
          disabled={loading !== null}
          style={{
            flex: 1.4,
            padding: "10px 16px",
            borderRadius: 10,
            background: p.accent,
            color: p.onAccent,
            border: "none",
            fontSize: 13,
            fontWeight: 600,
            cursor: loading ? "default" : "pointer",
            opacity: loading && loading !== "settings" ? 0.6 : 1,
          }}
        >
          {loading === "settings" ? "Opening…" : "Choose a Text Style"}
        </button>
        <button
          type="button"
          onClick={handleDismiss}
          disabled={loading !== null}
          style={{
            flex: 1,
            padding: "10px 16px",
            borderRadius: 10,
            background: "transparent",
            color: p.text,
            border: `1px solid ${p.cardBorder}`,
            fontSize: 13,
            fontWeight: 500,
            cursor: loading ? "default" : "pointer",
            opacity: loading && loading !== "dismiss" ? 0.6 : 1,
          }}
        >
          Close
        </button>
      </div>
    </div>
  );
}
