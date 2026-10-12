import { useState, useEffect } from "react";
import type { Tokens } from "@/lib/tokens";
import { errorMessage } from "@/lib/utils";
import Toggle from "@/components/ui/Toggle";
import Segmented from "@/components/ui/Segmented";
import { Section, Row, FieldSelect, FieldInput } from "./SettingsPrimitives";
import { useSettingsContext } from "./useSettingsContext";
import {
  triggerDownloadSpecificModel,
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

const MODEL_LOAD_OPTIONS: { value: "on_start" | "on_stop"; label: string }[] = [
  { value: "on_start", label: "Faster" },
  { value: "on_stop", label: "Save memory" },
];

/** How speech becomes text: the model, its memory use and how recordings are cut up. */
export default function TranscriptionTab({ t }: { t: Tokens }) {
  const { settings, updateSettings, models, downloadProgress, refreshModels } =
    useSettingsContext();

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
          hint="Language you speak into the microphone (Auto-detect or lock to one). With Automatic translation, only this language is translated"
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

      {/* ── Memory ── */}
      <Section title="Memory" t={t}>
        <Row label="Idle timeout" hint={idleTimeoutHint(idleSec)} t={t}>
          <Segmented
            options={idleOptionsFor(idleSec)}
            value={String(idleSec)}
            onChange={(v) => updateSettings({ model_idle_unload_sec: Number(v) })}
            t={t}
          />
        </Row>

        <Row
          label="Load speech model"
          hint={
            (settings.model_load_mode ?? "on_start") === "on_stop"
              ? "After you stop talking: saves memory while you record, adds about a second to each transcription"
              : "As soon as you start talking, so it's ready the moment you stop"
          }
          t={t}
          last
        >
          <Segmented
            options={MODEL_LOAD_OPTIONS}
            value={settings.model_load_mode ?? "on_start"}
            onChange={(v) => updateSettings({ model_load_mode: v })}
            t={t}
          />
        </Row>
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
    </div>
  );
}
