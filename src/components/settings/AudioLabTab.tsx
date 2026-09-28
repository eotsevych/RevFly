import { useEffect, useRef, useState } from "react";
import type { Tokens } from "@/lib/tokens";
import { Section, Row, FieldSelect } from "./SettingsPrimitives";
import { useSettingsContext } from "./SettingsContext";
import Toggle from "@/components/ui/Toggle";
import {
  fetchLabAudioFiles,
  triggerRunLabExperiment,
  triggerSaveLabCustomAudio,
  isTauri,
  type LabAudioItem,
  type LabExperimentResult,
} from "@/lib/tauri";

export default function AudioLabTab({
  t,
  initialAudio,
}: {
  t: Tokens;
  initialAudio?: string | null;
}) {
  const { models, settings } = useSettingsContext();
  const [items, setItems] = useState<LabAudioItem[]>([]);
  const [selected, setSelected] = useState<string>(initialAudio || "");
  const [model, setModel] = useState(settings.model_name || "ggml-medium-q5_0.bin");
  const [useVad, setUseVad] = useState(true);
  const [language, setLanguage] = useState<string>("auto");
  const [textNorm, setTextNorm] = useState(!!settings.text_normalization);
  const [remFillers, setRemFillers] = useState(!!settings.remove_filler_words);
  const [convNumbers, setConvNumbers] = useState(!!settings.convert_numbers);
  const [remStutters, setRemStutters] = useState(!!settings.remove_stutters);
  const [selfCorr, setSelfCorr] = useState(!!settings.apply_self_corrections);
  const [noiseMarkers, setNoiseMarkers] = useState(!!settings.remove_noise_markers);
  const [collapse, setCollapse] = useState(!!settings.collapse_redundancy);
  const [ambiguity, setAmbiguity] = useState(!!settings.annotate_ambiguity);
  const [structured, setStructured] = useState(!!settings.normalize_structured_values);
  const [phase, setPhase] = useState<"idle" | "running" | "done" | "error">("idle");
  const [result, setResult] = useState<LabExperimentResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rawUploadName, setRawUploadName] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const availableModels =
    models.length > 0
      ? models.map((m) => ({ value: m.filename, label: m.name }))
      : [
          { value: "ggml-base.bin", label: "Whisper Base (local)" },
          { value: "ggml-medium-q5_0.bin", label: "Whisper Medium" },
          { value: "parakeet-tdt-0.6b-v3", label: "Parakeet TDT 0.6B v3" },
        ];

  const languageOptions = [
    { value: "auto", label: "Auto" },
    { value: "en", label: "English" },
    { value: "uk", label: "Ukrainian" },
    { value: "es", label: "Spanish" },
    { value: "fr", label: "French" },
    { value: "de", label: "German" },
    { value: "ja", label: "Japanese" },
    { value: "zh", label: "Chinese" },
  ];

  useEffect(() => {
    refreshLab(initialAudio);
  }, [initialAudio]);

  useEffect(() => {
    setTextNorm(!!settings.text_normalization);
    setRemFillers(!!settings.remove_filler_words);
    setConvNumbers(!!settings.convert_numbers);
    setRemStutters(!!settings.remove_stutters);
    setSelfCorr(!!settings.apply_self_corrections);
    setNoiseMarkers(!!settings.remove_noise_markers);
    setCollapse(!!settings.collapse_redundancy);
    setAmbiguity(!!settings.annotate_ambiguity);
    setStructured(!!settings.normalize_structured_values);
    setModel(settings.model_name || model);
  }, [settings]);

  async function refreshLab(targetAudio?: string | null) {
    const list = await fetchLabAudioFiles();
    setItems(list);
    const toSelect = targetAudio || initialAudio || selected;
    if (toSelect && list.some((it) => it.filename === toSelect)) {
      setSelected(toSelect);
    } else if (list.length > 0 && !selected) {
      setSelected(list[0].filename);
    }
  }

  async function handleFile(e: React.ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0];
    if (!f) return;
    if (!isTauri()) {
      setError("Upload works only in the desktop app.");
      return;
    }
    const buf = await f.arrayBuffer();
    const b64 = arrayBufferToBase64(buf);
    try {
      const saved = await triggerSaveLabCustomAudio(b64);
      setRawUploadName(f.name);
      await refreshLab();
      if (saved) setSelected(saved);
      setError(null);
    } catch (err: any) {
      setError(String(err?.message ?? err));
    } finally {
      if (fileRef.current) fileRef.current.value = "";
    }
  }

  function handleDrop(e: React.DragEvent) {
    e.preventDefault();
    const f = e.dataTransfer.files?.[0];
    if (f && fileRef.current) {
      const dt = new DataTransfer();
      dt.items.add(f);
      fileRef.current.files = dt.files;
      fileRef.current.dispatchEvent(new Event("change", { bubbles: true }));
    }
  }

  async function run() {
    if (!selected) {
      setError("Pick a recording first.");
      return;
    }
    setPhase("running");
    setError(null);
    setResult(null);
    try {
      const res = await triggerRunLabExperiment({
        audio_filename: selected,
        model_name: model,
        use_vad: useVad,
        language: language === "auto" ? null : language,
        text_normalization: textNorm,
        remove_filler_words: remFillers,
        convert_numbers: convNumbers,
        remove_stutters: remStutters,
        apply_self_corrections: selfCorr,
        remove_noise_markers: noiseMarkers,
        collapse_redundancy: collapse,
        annotate_ambiguity: ambiguity,
        normalize_structured_values: structured,
      });
      if (res) {
        setResult(res);
        setPhase("done");
      } else {
        setError("Empty result.");
        setPhase("error");
      }
    } catch (err: any) {
      setError(String(err?.message ?? err));
      setPhase("error");
    }
  }

  function reset() {
    setPhase("idle");
    setResult(null);
    setError(null);
  }
  const selItem = items.find((x) => x.filename === selected);

  return (
    <div>
      <Section title="Source Recording (Real Audio)" t={t}>
        <div style={{ padding: 12 }}>
          {items.length === 0 ? (
            <p style={{ fontSize: 12, color: t.textMuted, margin: 0, lineHeight: 1.5 }}>
              No recordings yet — record with your hotkey or drop a .wav/.mp3 below.
            </p>
          ) : (
            <FieldSelect
              value={selected}
              onChange={setSelected}
              options={items.map((it) => ({
                value: it.filename,
                label:
                  it.name +
                  " \u2014 " +
                  it.duration_sec.toFixed(1) +
                  "s" +
                  (it.is_vad_trimmed ? " \u00b7 VAD" : ""),
              }))}
              t={t}
            />
          )}
          {selItem && (
            <p
              style={{
                fontSize: 11,
                color: t.textDim,
                margin: "8px 0 0",
                fontFamily: "JetBrains Mono, monospace",
              }}
            >
              {selItem.filename} \u00b7 {(selItem.size_bytes / 1024).toFixed(0)} KB
            </p>
          )}
          <div
            onDragOver={(e) => e.preventDefault()}
            onDrop={handleDrop}
            onClick={() => fileRef.current?.click()}
            style={{
              marginTop: 12,
              border: "2px dashed " + (rawUploadName ? t.accent : t.border),
              borderRadius: 10,
              padding: "16px 14px",
              textAlign: "center",
              cursor: "pointer",
              background: rawUploadName ? t.accent + "08" : t.inputBg,
            }}
          >
            <input
              ref={fileRef}
              type="file"
              accept="audio/*,video/*,.wav,.mp3,.m4a,.ogg,.webm"
              onChange={handleFile}
              style={{ display: "none" }}
            />
            <div style={{ fontSize: 18, marginBottom: 4 }}>🎙</div>
            <p
              style={{
                fontSize: 12,
                color: rawUploadName ? t.accent : t.textMuted,
                margin: 0,
                fontWeight: rawUploadName ? 600 : 400,
              }}
            >
              {rawUploadName ? "Uploaded: " + rawUploadName : "Drop a .wav/.mp3 or click to upload"}
            </p>
            <p style={{ fontSize: 11, color: t.textDim, margin: "4px 0 0" }}>
              Saved as lab_custom.wav
            </p>
          </div>
        </div>
      </Section>

      <Section title="Experiment (Runs on Real Audio)" t={t}>
        <Row label="Model" hint="Whisper GGML or Parakeet — real inference" t={t}>
          <FieldSelect value={model} onChange={setModel} options={availableModels} t={t} />
        </Row>
        <Row label="Language hint" hint="null = auto" t={t}>
          <FieldSelect value={language} onChange={setLanguage} options={languageOptions} t={t} />
        </Row>
        <Row label="VAD trimming" hint="Silero VAD cut + 400ms padding" t={t}>
          <Toggle value={useVad} onChange={setUseVad} accent={t.accent} />
        </Row>
        <Row label="Text normalization master" t={t} last={!textNorm}>
          <Toggle value={textNorm} onChange={setTextNorm} accent={t.accent} />
        </Row>
        {textNorm && (
          <>
            <Row label="Remove filler words" hint="um, uh" t={t}>
              <Toggle value={remFillers} onChange={setRemFillers} accent={t.accent} />
            </Row>
            <Row label="Numbers to digits" hint="twenty four \u2192 24" t={t}>
              <Toggle value={convNumbers} onChange={setConvNumbers} accent={t.accent} />
            </Row>
            <Row label="Remove stutters" hint="the the \u2192 the" t={t}>
              <Toggle value={remStutters} onChange={setRemStutters} accent={t.accent} />
            </Row>
          </>
        )}
        <Row label="Self-corrections" hint="sorry, I mean X" t={t}>
          <Toggle value={selfCorr} onChange={setSelfCorr} accent={t.accent} />
        </Row>
        <Row label="Strip noise markers" hint="[noise], (cough)" t={t}>
          <Toggle value={noiseMarkers} onChange={setNoiseMarkers} accent={t.accent} />
        </Row>
        <Row label="Collapse redundancy" hint="merge immediate duplicate phrase" t={t}>
          <Toggle value={collapse} onChange={setCollapse} accent={t.accent} />
        </Row>
        <Row label="Annotate ambiguity" hint="[?]" t={t}>
          <Toggle value={ambiguity} onChange={setAmbiguity} accent={t.accent} />
        </Row>
        <Row label="Structured values" hint="dates, currencies, %" t={t} last>
          <Toggle value={structured} onChange={setStructured} accent={t.accent} />
        </Row>
      </Section>

      <div style={{ display: "flex", gap: 8, marginBottom: 16 }}>
        <button
          type="button"
          onClick={run}
          disabled={!selected || phase === "running"}
          style={{
            flex: 1,
            padding: 10,
            borderRadius: 8,
            border: "none",
            background:
              !selected || phase === "running"
                ? t.surface
                : "linear-gradient(135deg, " + t.accent + " 0%, " + t.accent + "cc 100%)",
            color: !selected || phase === "running" ? t.textDim : "#fff",
            fontSize: 13,
            fontWeight: 600,
            cursor: !selected || phase === "running" ? "not-allowed" : "pointer",
          }}
        >
          {phase === "running"
            ? "\u23f3 Running on real audio\u2026"
            : "\u25b6  Run on real recording"}
        </button>
        {(phase !== "idle" || result) && (
          <button
            type="button"
            onClick={reset}
            style={{
              padding: "10px 14px",
              borderRadius: 8,
              background: t.surface,
              border: "1px solid " + t.border,
              color: t.textMuted,
              fontSize: 12,
              cursor: "pointer",
            }}
          >
            Reset
          </button>
        )}
      </div>

      {error && (
        <div
          style={{
            padding: "10px 12px",
            borderRadius: 8,
            background: t.errorColor + "14",
            border: "1px solid " + t.errorColor + "33",
            color: t.errorColor,
            fontSize: 12,
            marginBottom: 12,
          }}
        >
          {error}
        </div>
      )}

      {phase === "running" && (
        <div style={{ marginBottom: 16 }}>
          <p
            style={{
              fontSize: 11,
              color: t.textMuted,
              fontFamily: "JetBrains Mono, monospace",
              margin: "0 0 6px",
            }}
          >
            Running {model} {useVad ? "+ VAD" : ""} on {selected}\u2026
          </p>
          <div style={{ height: 4, borderRadius: 2, background: t.stepTrack, overflow: "hidden" }}>
            <div
              style={{
                height: "100%",
                width: "100%",
                background: "linear-gradient(90deg, " + t.accent + ", #00d4ff)",
              }}
            />
          </div>
        </div>
      )}

      {result && phase === "done" && (
        <div>
          <Section title="Raw Whisper" t={t}>
            <div style={{ padding: 12 }}>
              <p
                style={{
                  fontSize: 11,
                  color: t.textDim,
                  fontFamily: "JetBrains Mono, monospace",
                  margin: "0 0 6px",
                  wordBreak: "break-all",
                }}
              >
                {result.raw_output}
              </p>
              <p
                style={{
                  fontSize: 13,
                  color: t.text,
                  lineHeight: 1.6,
                  margin: 0,
                  whiteSpace: "pre-wrap",
                  borderTop: "1px solid " + t.border,
                  paddingTop: 8,
                }}
              >
                {result.raw_text || "(empty)"}
              </p>
            </div>
          </Section>
          <Section title="Clean (Post-Processed)" t={t}>
            <div style={{ padding: 12 }}>
              <p
                style={{
                  fontSize: 13,
                  color: t.text,
                  lineHeight: 1.7,
                  margin: 0,
                  whiteSpace: "pre-wrap",
                }}
              >
                {result.clean_text || "(post blanked it)"}
              </p>
              {(result.uncertain_spans?.length ?? 0) > 0 && (
                <div
                  style={{
                    marginTop: 10,
                    padding: 8,
                    borderRadius: 6,
                    background: t.warnColor + "14",
                    border: "1px solid " + t.warnColor + "33",
                  }}
                >
                  <p
                    style={{ fontSize: 11, color: t.warnColor, margin: "0 0 4px", fontWeight: 600 }}
                  >
                    \u26a0 Uncertain spans
                  </p>
                  {result.uncertain_spans.map((u: string, i: number) => (
                    <p
                      key={i}
                      style={{
                        fontSize: 11,
                        color: t.warnColor,
                        margin: "2px 0",
                        fontFamily: "JetBrains Mono, monospace",
                      }}
                    >
                      \u00b7 {u}
                    </p>
                  ))}
                </div>
              )}
              <p
                style={{
                  fontSize: 11,
                  color: t.textDim,
                  margin: "8px 0 0",
                  fontFamily: "JetBrains Mono, monospace",
                }}
              >
                Post: {(result.post_applied || []).join(", ") || "none"} \u00b7 lang:{" "}
                {result.detected_lang} \u00b7 {result.hardware_engine}
              </p>
            </div>
          </Section>
          <Section title="Metrics" t={t}>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(3,1fr)" }}>
              {[
                { label: "Inference", value: result.inference_time_ms + " ms" },
                { label: "VAD cut", value: result.vad_cut_sec.toFixed(2) + "s" },
                { label: "Speed", value: result.speed_factor.toFixed(1) + "\u00d7" },
                {
                  label: "Duration",
                  value:
                    result.original_duration_sec.toFixed(1) +
                    "s \u2192 " +
                    result.processed_duration_sec.toFixed(1) +
                    "s",
                },
                { label: "Segments", value: String(result.segments.length) },
                { label: "Needs review", value: result.requires_clarification ? "Yes" : "No" },
              ].map((m, i) => (
                <div
                  key={m.label}
                  style={{
                    padding: "10px 12px",
                    textAlign: "center",
                    borderRight: (i + 1) % 3 !== 0 ? "1px solid " + t.border : "none",
                    borderBottom: i < 3 ? "1px solid " + t.border : "none",
                  }}
                >
                  <p style={{ fontSize: 14, fontWeight: 700, color: t.accent, margin: "0 0 2px" }}>
                    {m.value}
                  </p>
                  <p
                    style={{
                      fontSize: 9,
                      color: t.textDim,
                      fontFamily: "JetBrains Mono, monospace",
                      letterSpacing: "0.06em",
                      textTransform: "uppercase",
                      margin: 0,
                    }}
                  >
                    {m.label}
                  </p>
                </div>
              ))}
            </div>
          </Section>
          {result.segments?.length > 0 && (
            <Section title="Segments" t={t}>
              <div style={{ padding: 8, maxHeight: 160, overflow: "auto" }}>
                {result.segments.slice(0, 20).map((s: any, i: number) => (
                  <p
                    key={i}
                    style={{
                      fontSize: 11,
                      color: t.textMuted,
                      fontFamily: "JetBrains Mono, monospace",
                      margin: "4px 0",
                    }}
                  >
                    [{(s.start_ms / 1000).toFixed(2)}\u2013{(s.end_ms / 1000).toFixed(2)}s \u00b7{" "}
                    {(s.no_speech_prob * 100).toFixed(0)}%] {s.text}
                  </p>
                ))}
              </div>
            </Section>
          )}
        </div>
      )}
    </div>
  );
}

function arrayBufferToBase64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let binary = "";
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}
