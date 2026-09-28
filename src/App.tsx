import { useEffect, useState, Component, type ReactNode } from "react";
import { VoicePill } from "./components/VoicePill";
import { AssistantSettings } from "./components/AssistantSettings";
import { fetchSettings, isTauri } from "./lib/tauri";
import { applyTheme, initThemeListener, type ThemeMode } from "./lib/theme";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("App ErrorBoundary caught:", error, info);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div
          style={{
            padding: 24,
            fontFamily: "system-ui, sans-serif",
            color: "#e11d48",
            background: "#fff",
            height: "100vh",
            boxSizing: "border-box",
          }}
        >
          <h2 style={{ fontSize: 18, fontWeight: 600 }}>Settings Error</h2>
          <pre
            style={{
              fontSize: 12,
              background: "#f1f5f9",
              padding: 12,
              borderRadius: 8,
              overflow: "auto",
              marginTop: 8,
            }}
          >
            {this.state.error?.message || String(this.state.error)}
          </pre>
          <button
            onClick={() => window.location.reload()}
            style={{
              marginTop: 16,
              padding: "8px 16px",
              borderRadius: 6,
              background: "#0f172a",
              color: "#fff",
              border: "none",
              cursor: "pointer",
              fontWeight: 500,
            }}
          >
            Reload Window
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

function checkIsPreferences(): boolean {
  if (typeof window === "undefined") return false;
  if (window.location.hash === "#preferences" || window.location.href.includes("preferences")) {
    return true;
  }
  const anyWin = window as any;
  const label =
    anyWin?.__TAURI_INTERNALS__?.metadata?.currentWebview?.label ||
    anyWin?.__TAURI_INTERNALS__?.metadata?.currentWindow?.label;
  if (label === "preferences") {
    return true;
  }
  return false;
}

export default function App() {
  const [isPreferences, setIsPreferences] = useState(checkIsPreferences);

  useEffect(() => {
    // 1. Initial theme application
    const savedTheme = (localStorage.getItem("aura_theme") as ThemeMode) || "system";
    applyTheme(savedTheme);

    fetchSettings().then((settings) => {
      if (settings?.theme) {
        const t = settings.theme as ThemeMode;
        localStorage.setItem("aura_theme", t);
        applyTheme(t);
      }
    });

    // 2. Listen to system preference and Tauri app changes
    const cleanupTheme = initThemeListener();

    // 3. Detect view window
    const detectView = async () => {
      if (checkIsPreferences()) {
        setIsPreferences(true);
        return;
      }
      if (isTauri()) {
        try {
          const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
          const win = getCurrentWebviewWindow();
          if (win.label === "preferences") {
            setIsPreferences(true);
          }
        } catch {
          // ignore
        }
      }
    };
    detectView();

    const onHashChange = () => {
      setIsPreferences(checkIsPreferences());
    };
    window.addEventListener("hashchange", onHashChange);

    return () => {
      cleanupTheme();
      window.removeEventListener("hashchange", onHashChange);
    };
  }, []);

  if (isPreferences) {
    return (
      <ErrorBoundary>
        <AssistantSettings />
      </ErrorBoundary>
    );
  }

  return (
    <ErrorBoundary>
      <VoicePill />
    </ErrorBoundary>
  );
}
