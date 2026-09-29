import { useState, useEffect } from "react";
import type { Tokens } from "@/lib/tokens";
import { errorMessage } from "@/lib/utils";
import HotkeyRecorder from "@/components/ui/HotkeyRecorder";
import Toggle from "@/components/ui/Toggle";
import Segmented from "@/components/ui/Segmented";
import { Section, Row, FieldSelect, FieldTextarea, FieldInput } from "./SettingsPrimitives";
import { useSettingsContext } from "./useSettingsContext";
import UpdatesSection from "./UpdatesSection";
import {
  startMicTest,
  stopMicTest,
  playTestSound,
  subscribeToMicTestLevels,
  triggerDownloadSpecificModel,
  checkAccessibility,
  requestAccessibility,
  openAccessibilitySettings,
  openModelsFolder,
  pickAndImportModel,
  downloadCustomModel,
  getModelsDirPath,
  fetchHardwareProfile,
  type HardwareProfile,
} from "@/lib/tauri";

const IDLE_OPTIONS = [
  { value: "5", label: "5 s" },
  { value: "15", label: "15 s" },
  { value: "30", label: "30 s" },
  { value: "60", label: "1 min" },
  { value: "0", label: "Off" },
];

const DEFAULT_IDLE_SEC = 30;

function formatIdle(sec: number): string {
  if (sec < 60) return `${sec} s`;
  const min = sec / 60;
  return Number.isInteger(min) ? `${min} min` : `${min.toFixed(1)} min`;
}

// Keeps a saved value that isn't a preset (e.g. 120 s from older versions) visible and selected.
function idleOptionsFor(sec: number) {
  if (IDLE_OPTIONS.some((o) => o.value === String(sec))) return IDLE_OPTIONS;
  const custom = { value: String(sec), label: formatIdle(sec) };
  const presets = IDLE_OPTIONS.filter((o) => o.value !== "0");
  const off = IDLE_OPTIONS.filter((o) => o.value === "0");
  return [...presets, custom].sort((a, b) => Number(a.value) - Number(b.value)).concat(off);
}

function idleTimeoutHint(sec: number): string {
  return sec === 0
    ? "Off: the speech model stays in RAM"
    : `Unloads the speech model from RAM after ${formatIdle(sec)} without use`;
}

const SOURCE_LANGUAGES = [
  { value: "Auto", label: "Auto-detect" },
  { value: "Ukrainian", label: "Ukrainian (українська)" },
  { value: "English", label: "English" },
  { value: "Spanish", label: "Spanish (español)" },
  { value: "French", label: "French (français)" },
  { value: "German", label: "German (Deutsch)" },
  { value: "Polish", label: "Polish (polski)" },
  { value: "Italian", label: "Italian (italiano)" },
  { value: "Japanese", label: "Japanese (日本語)" },
  { value: "Chinese", label: "Chinese (中文)" },
  { value: "Russian", label: "Russian (русский)" },
];

export default function GeneralTab({ t }: { t: Tokens }) {
  const {
    settings,
    updateSettings,
    inputDevices,
    outputDevices,
    refreshAudioDevices,
    models,
    downloadProgress,
    refreshModels,
  } = useSettingsContext();

  const [isTestingMic, setIsTestingMic] = useState(false);
  const [micLevel, setMicLevel] = useState(0);
  const [micPeak, setMicPeak] = useState(0);
  const [noiseFloor, setNoiseFloor] = useState(0);
  const [testDuration, setTestDuration] = useState(0);
  const [isPlayingTestSound, setIsPlayingTestSound] = useState(false);
  const [isRefreshingAudio, setIsRefreshingAudio] = useState(false);
  const [isAccessibilityGranted, setIsAccessibilityGranted] = useState<boolean | null>(null);
  const [isCheckingAccessibility, setIsCheckingAccessibility] = useState(false);
  const [modelsFolderPath, setModelsFolderPath] = useState<string | null>(null);
  const [customUrl, setCustomUrl] = useState("");
  const [customUrlError, setCustomUrlError] = useState<string | null>(null);
  const [isImporting, setIsImporting] = useState(false);
  const [isDownloadingCustom, setIsDownloadingCustom] = useState(false);
  const [importFeedback, setImportFeedback] = useState<string | null>(null);
  const [hwProfile, setHwProfile] = useState<HardwareProfile | null>(null);

  useEffect(() => {
    getModelsDirPath()
      .then(setModelsFolderPath)
      .catch(() => {});
    fetchHardwareProfile()
      .then(setHwProfile)
      .catch(() => {});
  }, []);

  // Sync accessibility permission
  const syncAccessibility = async () => {
    setIsCheckingAccessibility(true);
    try {
      const granted = await checkAccessibility();
      setIsAccessibilityGranted(granted);
    } catch {
      setIsAccessibilityGranted(false);
    } finally {
      setTimeout(() => setIsCheckingAccessibility(false), 300);
    }
  };

  useEffect(() => {
    syncAccessibility();
    const handleFocus = () => syncAccessibility();
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, []);

  // Live microphone VU level listener & quality analysis
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let isMounted = true;
    let timer: ReturnType<typeof setInterval> | null = null;

    if (isTestingMic) {
      setMicLevel(0);
      setMicPeak(0);
      setNoiseFloor(0);
      setTestDuration(0);
      const startTime = Date.now();

      timer = setInterval(() => {
        if (!isMounted) return;
        setTestDuration(Math.floor((Date.now() - startTime) / 1000));
      }, 500);

      startMicTest(settings.input_device ?? null);
      const samples: number[] = [];

      subscribeToMicTestLevels((data) => {
        if (!isMounted) return;
        const rawLvl = typeof data === "number" ? data : (data?.level ?? 0);
        const rawPeak = typeof data === "number" ? data : (data?.peak ?? rawLvl);
        const lvlPct = Math.min(100, Math.max(0, Math.round(rawLvl * 100)));
        const peakPct = Math.min(100, Math.max(0, Math.round(rawPeak * 100)));

        setMicLevel(lvlPct);
        setMicPeak((prev) => Math.max(prev, peakPct));

        if (samples.length < 35) {
          samples.push(lvlPct);
          if (samples.length >= 8) {
            const sorted = [...samples].sort((a, b) => a - b);
            const baseline = sorted[Math.floor(sorted.length * 0.25)] ?? 0;
            setNoiseFloor(baseline);
          }
        }
      }).then((un) => {
        unlisten = un;
      });
    } else {
      stopMicTest();
      setMicLevel(0);
      setMicPeak(0);
      setNoiseFloor(0);
      setTestDuration(0);
    }

    return () => {
      isMounted = false;
      if (timer) clearInterval(timer);
      if (unlisten) unlisten();
      stopMicTest();
    };
  }, [isTestingMic, settings.input_device]);

  const handleTestSound = async () => {
    try {
      setIsPlayingTestSound(true);
      await playTestSound();
      setTimeout(() => setIsPlayingTestSound(false), 800);
    } catch {
      setIsPlayingTestSound(false);
    }
  };

  const handleRefreshAudio = async () => {
    setIsRefreshingAudio(true);
    await refreshAudioDevices();
    setTimeout(() => setIsRefreshingAudio(false), 500);
  };

  const getQualityAssessment = () => {
    if (testDuration < 1) {
      return {
        text: "Listening to microphone input… Speak normally.",
        color: t.accent,
        icon: "🎤",
      };
    }
    if (micPeak > 92) {
      return {
        text: "Too loud / Clipping! Move away from the mic or reduce input volume.",
        color: t.errorColor,
        icon: "⚠️",
      };
    }
    if (noiseFloor > 18) {
      return {
        text: "High background noise detected! May degrade speech recognition.",
        color: t.warnColor,
        icon: "⚠️",
      };
    }
    if (micPeak < 10 && testDuration >= 3) {
      return {
        text: "Too quiet! Cannot hear clearly. Speak closer or increase mic volume.",
        color: t.warnColor,
        icon: "⚠️",
      };
    }
    if (micPeak >= 20 && noiseFloor <= 15) {
      return {
        text: "Optimal audio clarity! Clean signal for speech recognition.",
        color: t.successColor,
        icon: "✓",
      };
    }
    return {
      text: "Signal detected. Speak normally to measure audio clarity.",
      color: t.accent,
      icon: "🎤",
    };
  };

  const micOptions = (() => {
    const base = inputDevices.map((d) => ({ value: d, label: d }));
    const sel = (settings.input_device || "").trim();
    if (sel && sel !== "Default" && !inputDevices.includes(sel)) {
      base.unshift({ value: sel, label: sel + " (unavailable — will fallback to Default)" });
    }
    return [{ value: "", label: "System Default" }, ...base];
  })();

  const outputOptions = (() => {
    const base = outputDevices.map((d) => ({ value: d, label: d }));
    const sel = (settings.output_device || "").trim();
    if (sel && sel !== "Default" && !outputDevices.includes(sel)) {
      base.unshift({ value: sel, label: sel + " (unavailable — will fallback to Default)" });
    }
    return [{ value: "", label: "System Default" }, ...base];
  })();

  const modelOptions =
    models.length > 0
      ? models.map((m) => ({
          value: m.filename,
          label: m.downloaded
            ? `${m.name} — ${m.size_desc}`
            : `${m.name} — ${m.size_desc} (Needs download)`,
        }))
      : [
          { value: "ggml-medium-q5_0.bin", label: "Whisper Medium (q5_0 local)" },
          { value: "ggml-small-q5_0.bin", label: "Whisper Small (q5_0 local)" },
          { value: "ggml-base.bin", label: "Whisper Base (local)" },
          { value: "parakeet-tdt-0.6b-v3", label: "Parakeet TDT 0.6B (Fast, 25 languages)" },
        ];

  const idleSec = settings.model_idle_unload_sec ?? DEFAULT_IDLE_SEC;
  const selectedModel = models.find((m) => m.filename === settings.model_name);
  const isSelectedModelDownloaded = selectedModel ? selectedModel.downloaded : true;

  return (
    <div>
      {/* ── System Permissions Section ── */}
      <Section title="System Permissions" t={t}>
        <Row
          label="Accessibility sync"
          hint="Check or grant macOS accessibility permission for hotkeys and auto-paste"
          t={t}
          last
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span
              style={{
                fontSize: 11,
                fontFamily: "JetBrains Mono, monospace",
                padding: "4px 8px",
                borderRadius: 6,
                fontWeight: 600,
                background:
                  isAccessibilityGranted === true
                    ? `${t.successColor}18`
                    : isAccessibilityGranted === false
                      ? `${t.warnColor}18`
                      : t.surface,
                color:
                  isAccessibilityGranted === true
                    ? t.successColor
                    : isAccessibilityGranted === false
                      ? t.warnColor
                      : t.textDim,
                border: `1px solid ${
                  isAccessibilityGranted === true
                    ? `${t.successColor}33`
                    : isAccessibilityGranted === false
                      ? `${t.warnColor}33`
                      : t.border
                }`,
              }}
            >
              {isAccessibilityGranted === null
                ? "Checking…"
                : isAccessibilityGranted
                  ? "✓ Granted"
                  : "⚠ Not Granted"}
            </span>

            {isAccessibilityGranted === false && (
              <button
                type="button"
                onClick={() => requestAccessibility()}
                style={{
                  padding: "5px 10px",
                  borderRadius: 6,
                  border: `1px solid ${t.warnColor}`,
                  background: `${t.warnColor}20`,
                  color: t.warnColor,
                  fontSize: 11,
                  cursor: "pointer",
                  fontWeight: 500,
                  whiteSpace: "nowrap",
                }}
              >
                Grant Access
              </button>
            )}

            <button
              type="button"
              onClick={syncAccessibility}
              title="Sync accessibility status"
              disabled={isCheckingAccessibility}
              style={{
                padding: "5px 10px",
                borderRadius: 6,
                border: `1px solid ${t.border}`,
                background: t.surface,
                color: t.textMuted,
                fontSize: 11,
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                gap: 4,
              }}
            >
              <span
                style={{
                  display: "inline-block",
                  transform: isCheckingAccessibility ? "rotate(180deg)" : "none",
                  transition: "transform 0.4s",
                }}
              >
                ↻
              </span>
              <span>Sync</span>
            </button>
          </div>
        </Row>
      </Section>

      <UpdatesSection t={t} />

      {/* ── Input Section ── */}
      <Section title="Input" t={t}>
        <Row label="Launch hotkey" hint="Global shortcut to start recording" t={t}>
          <HotkeyRecorder
            value={settings.hotkey}
            onChange={(v) => updateSettings({ hotkey: v })}
            t={t}
          />
        </Row>

        <Row label="Microphone input" hint="Audio capture device" t={t}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <FieldSelect
              value={settings.input_device || ""}
              onChange={(v) => updateSettings({ input_device: v || null })}
              options={micOptions}
              t={t}
            />
            <button
              type="button"
              onClick={() => setIsTestingMic((prev) => !prev)}
              style={{
                padding: "6px 12px",
                borderRadius: 7,
                border: `1px solid ${isTestingMic ? t.accent : t.border}`,
                background: isTestingMic ? `${t.accent}20` : t.surface,
                color: isTestingMic ? t.accent : t.textMuted,
                fontSize: 11,
                fontFamily: "Inter, sans-serif",
                cursor: "pointer",
                whiteSpace: "nowrap",
                transition: "all 0.15s",
              }}
            >
              {isTestingMic ? "Stop Test" : "Test Mic"}
            </button>
            <button
              type="button"
              onClick={handleRefreshAudio}
              title="Refresh audio devices"
              style={{
                padding: "6px 8px",
                borderRadius: 7,
                border: `1px solid ${t.border}`,
                background: t.surface,
                color: t.textMuted,
                fontSize: 12,
                cursor: "pointer",
              }}
            >
              <span
                style={{
                  display: "inline-block",
                  transform: isRefreshingAudio ? "rotate(180deg)" : "none",
                  transition: "transform 0.4s",
                }}
              >
                ↻
              </span>
            </button>
          </div>
        </Row>

        {isTestingMic && (
          <div
            style={{
              padding: "12px 16px",
              background: t.inputBg,
              borderBottom: `1px solid ${t.border}`,
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                marginBottom: 6,
                fontSize: 10,
                fontFamily: "JetBrains Mono, monospace",
                color: t.textDim,
              }}
            >
              <span>MIC INPUT LEVEL</span>
              <div style={{ display: "flex", gap: 10 }}>
                <span>
                  PEAK:{" "}
                  <strong style={{ color: micPeak > 90 ? t.errorColor : t.text }}>
                    {micPeak}%
                  </strong>
                </span>
                <span>
                  NOISE:{" "}
                  <strong style={{ color: noiseFloor > 18 ? t.warnColor : t.textDim }}>
                    {noiseFloor}%
                  </strong>
                </span>
                <span style={{ color: t.accent }}>LIVE: {micLevel}%</span>
              </div>
            </div>
            <div style={{ display: "flex", gap: 3, height: 10 }}>
              {Array.from({ length: 24 }).map((_, i) => {
                const stepVal = (i + 1) * (100 / 24);
                const isActive = micLevel >= stepVal;
                const isWarn = i >= 18;
                const isCrit = i >= 21;
                const color = isCrit ? t.errorColor : isWarn ? t.warnColor : t.accent;
                return (
                  <div
                    key={i}
                    style={{
                      flex: 1,
                      borderRadius: 2,
                      background: isActive ? color : t.stepTrack,
                      transition: "background 0.08s ease",
                    }}
                  />
                );
              })}
            </div>

            {/* Live Audio Quality Guidance */}
            {(() => {
              const quality = getQualityAssessment();
              return (
                <div
                  style={{
                    marginTop: 8,
                    padding: "6px 10px",
                    borderRadius: 6,
                    fontSize: 11,
                    fontFamily: "Inter, sans-serif",
                    background: `${quality.color}14`,
                    border: `1px solid ${quality.color}28`,
                    color: quality.color,
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                  }}
                >
                  <span>{quality.icon}</span>
                  <span>{quality.text}</span>
                </div>
              );
            })()}
          </div>
        )}

        <Row label="Audio output" hint="Chime and playback speaker" t={t}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <FieldSelect
              value={settings.output_device || ""}
              onChange={(v) => updateSettings({ output_device: v || null })}
              options={outputOptions}
              t={t}
            />
            <button
              type="button"
              onClick={handleTestSound}
              disabled={isPlayingTestSound}
              style={{
                padding: "6px 12px",
                borderRadius: 7,
                border: `1px solid ${t.border}`,
                background: isPlayingTestSound ? `${t.accent}20` : t.surface,
                color: isPlayingTestSound ? t.accent : t.textMuted,
                fontSize: 11,
                fontFamily: "Inter, sans-serif",
                cursor: isPlayingTestSound ? "default" : "pointer",
                whiteSpace: "nowrap",
                transition: "all 0.15s",
              }}
            >
              {isPlayingTestSound ? "Playing…" : "Test Sound"}
            </button>
          </div>
        </Row>

        <Row label="Idle timeout" hint={idleTimeoutHint(idleSec)} t={t} last>
          <Segmented
            options={idleOptionsFor(idleSec)}
            value={String(idleSec)}
            onChange={(v) => updateSettings({ model_idle_unload_sec: Number(v) })}
            t={t}
          />
        </Row>
      </Section>

      {/* ── Behavior Section ── */}
      <Section title="Behavior" t={t}>
        <Row label="Sound feedback" hint="Play audio cues on recording state changes" t={t}>
          <Toggle
            value={settings.sound_effect}
            onChange={(v) => updateSettings({ sound_effect: v })}
            accent={t.accent}
          />
        </Row>
        <Row
          label="Auto-paste"
          hint="Paste translation automatically into frontmost window"
          t={t}
          last
        >
          <Toggle
            value={settings.auto_paste}
            onChange={(v) => updateSettings({ auto_paste: v })}
            accent={t.accent}
          />
        </Row>
      </Section>

      {/* ── Model Section ── */}
      <Section title="Model" t={t}>
        <Row
          label="Default transcription model"
          hint="Local speech recognition engine"
          t={t}
          last={false}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <FieldSelect
              value={settings.model_name}
              onChange={(v) => updateSettings({ model_name: v })}
              options={modelOptions}
              t={t}
            />
            {!isSelectedModelDownloaded && !downloadProgress && (
              <button
                type="button"
                onClick={() => triggerDownloadSpecificModel(settings.model_name)}
                style={{
                  padding: "6px 12px",
                  borderRadius: 7,
                  border: "none",
                  background: t.accent,
                  color: "#fff",
                  fontSize: 11,
                  fontFamily: "Inter, sans-serif",
                  fontWeight: 500,
                  cursor: "pointer",
                  whiteSpace: "nowrap",
                }}
              >
                Download
              </button>
            )}
          </div>
        </Row>

        <Row
          label="Hardware acceleration"
          hint={
            hwProfile
              ? `Detected: ${hwProfile.os.toUpperCase()} ${hwProfile.arch} (${hwProfile.logical_cores} cores) · ${hwProfile.has_metal ? "Apple Metal / ANE supported" : "Standard CPU SIMD"}`
              : "Detecting computer hardware…"
          }
          t={t}
          last={false}
        >
          <FieldSelect
            value={settings.device_backend || "auto"}
            onChange={(v) => updateSettings({ device_backend: v })}
            options={[
              {
                value: "auto",
                label: hwProfile?.has_metal
                  ? "Auto (Recommended — ANE on Apple Silicon)"
                  : "Auto (Recommended — Optimized CPU)",
              },
              {
                value: "ane",
                label: "Apple Neural Engine (ANE / Metal)",
              },
              {
                value: "cpu",
                label: "Standard CPU / RAM",
              },
            ]}
            t={t}
          />
        </Row>

        <Row
          label="Spoken language"
          hint="Language you speak into the microphone (Auto-detect or lock to Ukrainian/English)"
          t={t}
          last={false}
        >
          <FieldSelect
            value={settings.source_lang || "Auto"}
            onChange={(v) => updateSettings({ source_lang: v })}
            options={SOURCE_LANGUAGES}
            t={t}
          />
        </Row>

        <Row
          label="Excluded languages"
          hint="Languages you do not speak. If mistakenly detected, speech is automatically re-recognized into your spoken language"
          t={t}
          last={false}
        >
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 6,
              minWidth: 260,
              maxWidth: 360,
            }}
          >
            <FieldInput
              value={settings.excluded_languages ?? "Russian"}
              onChange={(v) => updateSettings({ excluded_languages: v })}
              placeholder="e.g. Russian, Polish"
              t={t}
            />
            <div style={{ display: "flex", flexWrap: "wrap", gap: 5 }}>
              {["Russian", "Polish", "Belarusian", "Czech", "Slovak"].map((lang) => {
                const currentList = (settings.excluded_languages ?? "Russian")
                  .split(",")
                  .map((s) => s.trim().toLowerCase());
                const isSelected = currentList.includes(lang.toLowerCase());
                return (
                  <button
                    key={lang}
                    type="button"
                    onClick={() => {
                      const cur = (settings.excluded_languages ?? "Russian")
                        .split(",")
                        .map((s) => s.trim())
                        .filter(Boolean);
                      let next: string[];
                      if (isSelected) {
                        next = cur.filter((s) => s.toLowerCase() !== lang.toLowerCase());
                      } else {
                        next = [...cur, lang];
                      }
                      updateSettings({ excluded_languages: next.join(", ") });
                    }}
                    style={{
                      padding: "2px 8px",
                      borderRadius: 12,
                      fontSize: 10,
                      fontFamily: "Inter, sans-serif",
                      cursor: "pointer",
                      border: `1px solid ${isSelected ? `${t.accent}66` : t.border}`,
                      background: isSelected ? `${t.accent}20` : t.inputBg,
                      color: isSelected ? t.accent : t.textMuted,
                      fontWeight: isSelected ? 600 : 400,
                      transition: "all 0.15s ease",
                    }}
                  >
                    {isSelected ? `✓ ${lang}` : `+ ${lang}`}
                  </button>
                );
              })}
            </div>
          </div>
        </Row>

        {/* — Custom model helpers — */}
        <Row
          label="Models folder"
          hint={modelsFolderPath ?? "Where .bin files are stored — any .bin appears as [Custom]"}
          t={t}
        >
          <button
            type="button"
            onClick={() => openModelsFolder()}
            style={{
              padding: "6px 12px",
              borderRadius: 7,
              border: `1px solid ${t.border}`,
              background: t.surface,
              color: t.textMuted,
              fontSize: 11,
              fontFamily: "Inter, sans-serif",
              cursor: "pointer",
              whiteSpace: "nowrap",
            }}
          >
            Open Folder
          </button>
        </Row>

        <Row
          label="Import model file"
          hint="Pick a .bin (GGML) file anywhere on disk — copied into the models folder"
          t={t}
        >
          <button
            type="button"
            disabled={isImporting}
            onClick={async () => {
              setIsImporting(true);
              setImportFeedback(null);
              try {
                const item = await pickAndImportModel();
                if (item) {
                  await refreshModels();
                  setImportFeedback(`Imported ${item.filename}`);
                  setTimeout(() => setImportFeedback(null), 3000);
                }
              } catch (e) {
                setImportFeedback(errorMessage(e));
                setTimeout(() => setImportFeedback(null), 4000);
              } finally {
                setIsImporting(false);
              }
            }}
            style={{
              padding: "6px 12px",
              borderRadius: 7,
              border: `1px solid ${isImporting ? t.border : t.accent}55`,
              background: isImporting ? t.surface : `${t.accent}18`,
              color: isImporting ? t.textMuted : t.accent,
              fontSize: 11,
              fontFamily: "Inter, sans-serif",
              fontWeight: 500,
              cursor: isImporting ? "default" : "pointer",
              whiteSpace: "nowrap",
            }}
          >
            {isImporting ? "Importing…" : "Import .bin…"}
          </button>
        </Row>

        <div
          style={{
            padding: "10px 16px",
            borderBottom: `1px solid ${t.border}`,
            background: t.inputBg,
          }}
        >
          <div
            style={{
              fontSize: 10,
              fontWeight: 600,
              letterSpacing: "0.06em",
              textTransform: "uppercase",
              color: t.textDim,
              fontFamily: "JetBrains Mono, monospace",
              marginBottom: 6,
            }}
          >
            Download from URL
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "flex-start" }}>
            <input
              value={customUrl}
              onChange={(e) => {
                setCustomUrl(e.target.value);
                if (customUrlError) setCustomUrlError(null);
              }}
              placeholder="https://huggingface.co/.../ggml-model.bin"
              spellCheck={false}
              style={{
                flex: 1,
                background: t.surface,
                border: `1px solid ${customUrlError ? t.errorColor : t.inputBorder}`,
                borderRadius: 7,
                padding: "7px 10px",
                color: t.inputText,
                fontSize: 11,
                fontFamily: "JetBrains Mono, monospace",
                outline: "none",
                minWidth: 0,
              }}
            />
            <button
              type="button"
              disabled={isDownloadingCustom || !customUrl.trim()}
              onClick={async () => {
                const url = customUrl.trim();
                if (!url) return;
                if (!(url.startsWith("http://") || url.startsWith("https://"))) {
                  setCustomUrlError("URL must start with http:// or https://");
                  return;
                }
                setCustomUrlError(null);
                setIsDownloadingCustom(true);
                try {
                  await downloadCustomModel(url, null);
                  await refreshModels();
                  setCustomUrl("");
                  setImportFeedback("Download complete");
                  setTimeout(() => setImportFeedback(null), 3000);
                } catch (e) {
                  setCustomUrlError(e == null ? "Download failed" : errorMessage(e));
                } finally {
                  setIsDownloadingCustom(false);
                }
              }}
              style={{
                padding: "7px 14px",
                borderRadius: 7,
                border: "none",
                background: isDownloadingCustom || !customUrl.trim() ? t.surface : t.accent,
                color: isDownloadingCustom || !customUrl.trim() ? t.textDim : "#fff",
                fontSize: 11,
                fontFamily: "Inter, sans-serif",
                fontWeight: 600,
                cursor: isDownloadingCustom || !customUrl.trim() ? "default" : "pointer",
                whiteSpace: "nowrap",
                opacity: isDownloadingCustom ? 0.7 : 1,
              }}
            >
              {isDownloadingCustom ? "Downloading…" : "Download"}
            </button>
          </div>
          <div
            style={{
              fontSize: 10,
              color: t.textDim,
              fontFamily: "Inter, sans-serif",
              marginTop: 6,
              lineHeight: 1.4,
            }}
          >
            GGML .bin from Hugging Face (e.g. ggerganov/whisper.cpp). Shows a live progress bar
            below.
          </div>
          {customUrlError && (
            <div
              style={{
                marginTop: 6,
                padding: "6px 8px",
                borderRadius: 6,
                background: `${t.errorColor}14`,
                border: `1px solid ${t.errorColor}33`,
                color: t.errorColor,
                fontSize: 11,
                fontFamily: "Inter, sans-serif",
              }}
            >
              {customUrlError}
            </div>
          )}
        </div>

        {importFeedback && (
          <div
            style={{
              padding: "8px 16px",
              background: `${t.successColor}10`,
              borderBottom: `1px solid ${t.border}`,
              fontSize: 11,
              color: t.successColor,
              fontFamily: "Inter, sans-serif",
            }}
          >
            {importFeedback}
          </div>
        )}

        {downloadProgress &&
          (downloadProgress.model === settings.model_name || isDownloadingCustom) && (
            <div style={{ padding: "10px 16px", background: t.inputBg }}>
              <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 5 }}>
                <span
                  style={{
                    fontSize: 11,
                    color: t.textMuted,
                    fontFamily: "JetBrains Mono, monospace",
                  }}
                >
                  Downloading {downloadProgress.model}…
                </span>
                <span
                  style={{ fontSize: 11, color: t.accent, fontFamily: "JetBrains Mono, monospace" }}
                >
                  {downloadProgress.percent}%
                </span>
              </div>
              <div
                style={{ height: 4, borderRadius: 2, background: t.stepTrack, overflow: "hidden" }}
              >
                <div
                  style={{
                    height: "100%",
                    borderRadius: 2,
                    width: `${downloadProgress.percent}%`,
                    background: `linear-gradient(90deg, ${t.accent}, #00d4ff)`,
                    transition: "width 0.3s ease",
                  }}
                />
              </div>
            </div>
          )}
      </Section>

      {/* ── Audio Chunking & VAD ── */}
      <Section title="Audio Chunking & VAD" t={t}>
        <Row
          label="Audio chunking"
          hint="Off: each recording is kept and transcribed as one whole track"
          t={t}
          last={!settings.audio_chunking}
        >
          <Toggle
            value={settings.audio_chunking ?? false}
            onChange={(v) => updateSettings({ audio_chunking: v })}
            accent={t.accent}
          />
        </Row>

        {settings.audio_chunking && (
          <>
            <Row
              label="Pause detection trigger"
              hint="Silence duration that cuts a speech chunk cleanly (default 500 ms)"
              t={t}
            >
              <Segmented
                options={[
                  { value: "300", label: "300 ms" },
                  { value: "500", label: "500 ms" },
                  { value: "700", label: "700 ms" },
                  { value: "1000", label: "1 s" },
                ]}
                value={String(settings.chunk_pause_ms ?? 500)}
                onChange={(v) => updateSettings({ chunk_pause_ms: Number(v) })}
                t={t}
              />
            </Row>

            <Row
              label="Safety cut limit"
              hint="Maximum continuous speech length before forcing a chunk cut (default 10 s)"
              t={t}
            >
              <Segmented
                options={[
                  { value: "5", label: "5 s" },
                  { value: "8", label: "8 s" },
                  { value: "10", label: "10 s" },
                  { value: "15", label: "15 s" },
                ]}
                value={String(settings.chunk_safety_sec ?? 10)}
                onChange={(v) => updateSettings({ chunk_safety_sec: Number(v) })}
                t={t}
              />
            </Row>

            <Row
              label="Safety cut overlap"
              hint="Audio slice preserved across forced cuts to prevent severed words (default 400 ms)"
              t={t}
              last
            >
              <Segmented
                options={[
                  { value: "200", label: "200 ms" },
                  { value: "300", label: "300 ms" },
                  { value: "400", label: "400 ms" },
                  { value: "500", label: "500 ms" },
                ]}
                value={String(settings.chunk_overlap_ms ?? 400)}
                onChange={(v) => updateSettings({ chunk_overlap_ms: Number(v) })}
                t={t}
              />
            </Row>
          </>
        )}
      </Section>

      {/* ── Post-Processing & Formatting (LLM-Friendly) ── */}
      <Section title="Post-Processing & Formatting (LLM-Friendly)" t={t}>
        <Row
          label="LLM-Friendly Normalization"
          hint="Master toggle to prepare spoken audio cleanly for downstream LLMs"
          t={t}
          last={!settings.text_normalization}
        >
          <Toggle
            value={settings.text_normalization ?? true}
            onChange={(v) => updateSettings({ text_normalization: v })}
            accent={t.accent}
          />
        </Row>

        {settings.text_normalization && (
          <>
            <Row
              label="Remove filler words"
              hint="Strip spoken 'um', 'uh', 'you know', 'like'"
              t={t}
            >
              <Toggle
                value={settings.remove_filler_words ?? true}
                onChange={(v) => updateSettings({ remove_filler_words: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Convert numbers to digits"
              hint="Convert 'twenty twenty four' to 2024"
              t={t}
            >
              <Toggle
                value={settings.convert_numbers ?? true}
                onChange={(v) => updateSettings({ convert_numbers: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Remove stutters"
              hint="Collapse immediate repeated words (e.g. 'I I want' → 'I want')"
              t={t}
            >
              <Toggle
                value={settings.remove_stutters ?? true}
                onChange={(v) => updateSettings({ remove_stutters: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Apply self-corrections"
              hint="Process self-correction edits (e.g. 'on Monday, no Tuesday')"
              t={t}
            >
              <Toggle
                value={settings.apply_self_corrections ?? true}
                onChange={(v) => updateSettings({ apply_self_corrections: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Strip noise markers"
              hint="Strip audio tags like [applause], [laughter], (cough)"
              t={t}
            >
              <Toggle
                value={settings.remove_noise_markers ?? true}
                onChange={(v) => updateSettings({ remove_noise_markers: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Collapse redundancy"
              hint="Merge immediate duplicate phrases into one"
              t={t}
            >
              <Toggle
                value={settings.collapse_redundancy ?? true}
                onChange={(v) => updateSettings({ collapse_redundancy: v })}
                accent={t.accent}
              />
            </Row>
            <Row label="Annotate ambiguities" hint="Mark uncertain phonetic spans with [?]" t={t}>
              <Toggle
                value={settings.annotate_ambiguity ?? true}
                onChange={(v) => updateSettings({ annotate_ambiguity: v })}
                accent={t.accent}
              />
            </Row>
            <Row
              label="Structured values"
              hint="Format currency, dates, percentages into consistent tokens"
              t={t}
              last
            >
              <Toggle
                value={settings.normalize_structured_values ?? true}
                onChange={(v) => updateSettings({ normalize_structured_values: v })}
                accent={t.accent}
              />
            </Row>
          </>
        )}
      </Section>

      {/* ── Confidential Text & Masking ── */}
      <Section title="Confidential Text & Masking" t={t}>
        <Row
          label="Mask Confidential Text"
          hint="Redact or replace secret words, names, and patterns before translation or pasting"
          t={t}
          last={!settings.mask_confidential}
        >
          <Toggle
            value={settings.mask_confidential ?? false}
            onChange={(v) => updateSettings({ mask_confidential: v })}
            accent={t.accent}
          />
        </Row>

        {settings.mask_confidential && (
          <>
            <div style={{ padding: "12px 16px", borderBottom: `1px solid ${t.border}` }}>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "baseline",
                  marginBottom: 6,
                }}
              >
                <p
                  style={{
                    fontSize: 13,
                    color: t.text,
                    fontFamily: "Inter, sans-serif",
                    margin: 0,
                    fontWeight: 450,
                  }}
                >
                  Words & Replacement Rules
                </p>
                <span
                  style={{
                    fontSize: 10,
                    color: t.textDim,
                    fontFamily: "JetBrains Mono, monospace",
                  }}
                >
                  e.g. SecretKey -&gt; [KEY] or plain word
                </span>
              </div>
              <p
                style={{
                  fontSize: 11,
                  color: t.textMuted,
                  fontFamily: "Inter, sans-serif",
                  margin: "0 0 8px",
                }}
              >
                Add one word or phrase per line (or comma-separated). Use <code>-&gt;</code> or{" "}
                <code>=</code> for custom replacements.
              </p>
              <FieldTextarea
                value={settings.mask_words ?? ""}
                onChange={(v) => updateSettings({ mask_words: v })}
                placeholder="John -&gt; [NAME]&#10;SecretProject -&gt; [PROJECT]&#10;password&#10;api_key"
                rows={4}
                mono
                t={t}
              />
            </div>

            <Row
              label="Match Sensitivity"
              hint="Fuzzy threshold to catch speech recognition typos (e.g. 90% catches speech slips)"
              t={t}
            >
              <Segmented
                options={[
                  { value: "100", label: "100% (Exact)" },
                  { value: "90", label: "90% (Rec.)" },
                  { value: "85", label: "85%" },
                  { value: "80", label: "80%" },
                ]}
                value={String(settings.mask_threshold ?? 90)}
                onChange={(v) => updateSettings({ mask_threshold: Number(v) })}
                t={t}
              />
            </Row>

            <Row
              label="Default Mask Token"
              hint="Used when no specific -&gt; replacement token is configured"
              t={t}
            >
              <Segmented
                options={[
                  { value: "***", label: "***" },
                  { value: "[REDACTED]", label: "[REDACTED]" },
                  { value: "[CONFIDENTIAL]", label: "[CONFIDENTIAL]" },
                  { value: "••••", label: "••••" },
                ]}
                value={settings.mask_format ?? "***"}
                onChange={(v) => updateSettings({ mask_format: v })}
                t={t}
              />
            </Row>

            <Row
              label="Auto-mask email addresses"
              hint="Replace emails like user@example.com with [EMAIL]"
              t={t}
            >
              <Toggle
                value={settings.mask_emails ?? true}
                onChange={(v) => updateSettings({ mask_emails: v })}
                accent={t.accent}
              />
            </Row>

            <Row label="Auto-mask phone numbers" hint="Replace phone numbers with [PHONE]" t={t}>
              <Toggle
                value={settings.mask_phones ?? true}
                onChange={(v) => updateSettings({ mask_phones: v })}
                accent={t.accent}
              />
            </Row>

            <Row
              label="Auto-mask card numbers"
              hint="Replace 16-digit credit card numbers with ****-****-****-****"
              t={t}
              last
            >
              <Toggle
                value={settings.mask_cards ?? true}
                onChange={(v) => updateSettings({ mask_cards: v })}
                accent={t.accent}
              />
            </Row>
          </>
        )}
      </Section>
    </div>
  );
}
