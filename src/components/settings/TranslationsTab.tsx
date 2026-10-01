import React from "react";
import type { Tokens } from "@/lib/tokens";
import {
  Section,
  FormLabel,
  FormGroup,
  FieldInput,
  FieldTextarea,
  ApiKeyInput,
  FieldSelect,
} from "./SettingsPrimitives";
import { useSettingsContext } from "./useSettingsContext";

const LLM_MODEL_SUGGESTIONS = [
  "gemini-3.6-flash",
  "gemini-2.0-flash",
  "llama3.2",
  "mistral",
  "gpt-4o-mini",
  "qwen2.5",
];

const SOURCE_LANG_OPTIONS = [
  { value: "Auto", label: "Auto-detect" },
  { value: "English", label: "English" },
  { value: "Ukrainian", label: "Ukrainian" },
  { value: "Spanish", label: "Spanish" },
  { value: "French", label: "French" },
  { value: "German", label: "German" },
  { value: "Italian", label: "Italian" },
  { value: "Polish", label: "Polish" },
  { value: "Japanese", label: "Japanese" },
  { value: "Chinese", label: "Chinese" },
  { value: "Russian", label: "Russian" },
];

const TARGET_LANGUAGES = [
  { value: "English", label: "English" },
  { value: "Ukrainian", label: "Ukrainian" },
  { value: "Spanish", label: "Spanish" },
  { value: "French", label: "French" },
  { value: "German", label: "German" },
  { value: "Italian", label: "Italian" },
  { value: "Polish", label: "Polish" },
  { value: "Japanese", label: "Japanese" },
  { value: "Chinese", label: "Chinese" },
  { value: "Russian", label: "Russian" },
];

// 3 Providers: No Translation, LLM (both local & cloud), Custom API
const PROVIDERS = [
  {
    value: "No Translation",
    label: "No Translation",
    desc: "Keep raw transcript, voice-to-text only",
  },
  {
    value: "LLM",
    label: "LLM (both local & cloud)",
    desc: "Gemini, Ollama, LM Studio, OpenAI, etc.",
  },
  {
    value: "Custom API",
    label: "Custom API",
    desc: "Custom REST translation endpoint",
  },
] as const;

const LLM_PRESETS = [
  {
    name: "Gemini (Cloud)",
    endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
    model: "gemini-3.6-flash",
    hint: "Free daily requests with a Gemini API key (aistudio.google.com/apikey)",
  },
  {
    name: "Ollama (Local)",
    endpoint: "http://localhost:11434/v1/chat/completions",
    model: "llama3.2",
    hint: "100% offline & private on your Mac. No API key needed.",
  },
  {
    name: "LM Studio (Local)",
    endpoint: "http://localhost:1234/v1/chat/completions",
    model: "local-model",
    hint: "Local inference via LM Studio. No API key needed.",
  },
  {
    name: "OpenAI (Cloud)",
    endpoint: "https://api.openai.com/v1/chat/completions",
    model: "gpt-4o-mini",
    hint: "Standard OpenAI endpoint with your OpenAI API key.",
  },
];

const PROMPT_PRESETS = [
  {
    name: "Standard Translation",
    template:
      "You are a strict translation engine. Translate the following text from {source_lang} to {target_lang}. Do not refuse. Do not explain. Do not add conversational text or notes. Output ONLY the exact translation using the native alphabet:\n\n{text}",
  },
  {
    name: "Fix Grammar & Translate",
    template:
      "You are an expert editor and translator. First, fix all speech stutters, repetitions, and grammatical mistakes. Then translate the text from {source_lang} to {target_lang}. Output ONLY the final translated text without any commentary:\n\n{text}",
  },
  {
    name: "Polite & Professional",
    template:
      "Translate the following text from {source_lang} to {target_lang}. Make the tone polite, professional, and clear for business communication. Output ONLY the translation:\n\n{text}",
  },
  {
    name: "Casual & Friendly",
    template:
      "Translate the following text from {source_lang} to {target_lang}. Use a natural, friendly, and casual conversational tone. Output ONLY the translation:\n\n{text}",
  },
];

const DEFAULT_PROMPT = PROMPT_PRESETS[0]!.template;

export default function TranslationsTab({ t }: { t: Tokens }) {
  const { settings, updateSettings } = useSettingsContext();

  // Normalize active provider to one of the 3 modes
  const rawProvider = settings.translation_provider || "LLM";
  const activeProvider =
    rawProvider.toLowerCase().includes("no") || rawProvider.toLowerCase().includes("none")
      ? "No Translation"
      : rawProvider.toLowerCase().includes("custom")
        ? "Custom API"
        : "LLM";

  const setProvider = (p: string) => {
    updateSettings({ translation_provider: p });
  };

  const currentPrompt = settings.prompt_template || DEFAULT_PROMPT;

  const currentEndpoint =
    settings.llm_endpoint ||
    settings.local_llm_url ||
    "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";

  const currentModel = settings.llm_model || settings.gemini_model || "gemini-3.6-flash";
  const currentApiKey = settings.llm_api_key || settings.api_key || "";

  const applyLlmPreset = (preset: (typeof LLM_PRESETS)[0]) => {
    updateSettings({
      llm_endpoint: preset.endpoint,
      llm_model: preset.model,
      gemini_model: preset.model,
      local_llm_url: preset.endpoint,
    });
  };

  const insertVariable = (varName: string) => {
    updateSettings({ prompt_template: `${currentPrompt} ${varName}` });
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
      {/* ── 3 Provider Selector Cards ── */}
      <Section title="Translation Mode" t={t}>
        <div
          style={{ padding: 12, display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 10 }}
        >
          {PROVIDERS.map((p) => {
            const active = activeProvider === p.value;
            return (
              <button
                key={p.value}
                type="button"
                onClick={() => setProvider(p.value)}
                style={{
                  padding: "14px 14px",
                  borderRadius: 10,
                  cursor: "pointer",
                  background: active ? `${t.accent}18` : t.inputBg,
                  border: `1px solid ${active ? `${t.accent}66` : t.border}`,
                  textAlign: "left",
                  transition: "all 0.2s",
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  minHeight: 90,
                }}
              >
                <div>
                  <p
                    style={{
                      fontSize: 13,
                      fontWeight: 600,
                      color: active ? t.accent : t.text,
                      fontFamily: "Inter, sans-serif",
                      margin: "0 0 4px",
                    }}
                  >
                    {p.label}
                  </p>
                  <p
                    style={{
                      fontSize: 11,
                      color: t.textDim,
                      fontFamily: "Inter, sans-serif",
                      margin: 0,
                      lineHeight: 1.4,
                    }}
                  >
                    {p.desc}
                  </p>
                </div>
                {active && (
                  <div
                    style={{
                      width: 7,
                      height: 7,
                      borderRadius: "50%",
                      background: t.accent,
                      marginTop: 8,
                    }}
                  />
                )}
              </button>
            );
          })}
        </div>
      </Section>

      {/* ── Provider Configuration Section ── */}
      <Section
        title={
          activeProvider === "No Translation"
            ? "Voice-to-Text Mode"
            : activeProvider === "LLM"
              ? "LLM Provider Configuration (Local & Cloud)"
              : "Custom API Configuration"
        }
        t={t}
      >
        <div style={{ padding: 16 }}>
          {/* No Translation Mode */}
          {activeProvider === "No Translation" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <div
                style={{
                  padding: 16,
                  borderRadius: 8,
                  background: t.inputBg,
                  border: `1px solid ${t.border}`,
                  color: t.textMuted,
                  fontSize: 12,
                  fontFamily: "Inter, sans-serif",
                  lineHeight: 1.6,
                }}
              >
                <strong style={{ color: t.text, display: "block", marginBottom: 4 }}>
                  Direct Voice-to-Text Mode Active
                </strong>
                Spoken audio is transcribed locally by the speech recognition model in RAM and
                pasted directly into your active window. No external web requests or translation
                steps are performed.
              </div>
            </div>
          )}

          {/* Combined LLM Mode (Both Local & Cloud) */}
          {activeProvider === "LLM" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              {/* Quick Presets */}
              <div>
                <FormLabel t={t}>Quick Presets</FormLabel>
                <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginTop: 6 }}>
                  {LLM_PRESETS.map((pre) => {
                    const isSelected =
                      currentEndpoint.trim() === pre.endpoint.trim() &&
                      currentModel.trim() === pre.model.trim();
                    return (
                      <button
                        key={pre.name}
                        type="button"
                        onClick={() => applyLlmPreset(pre)}
                        title={pre.hint}
                        style={{
                          padding: "6px 12px",
                          borderRadius: 8,
                          background: isSelected ? `${t.accent}22` : t.inputBg,
                          border: `1px solid ${isSelected ? `${t.accent}66` : t.border}`,
                          color: isSelected ? t.accent : t.textMuted,
                          fontSize: 11,
                          fontFamily: "Inter, sans-serif",
                          fontWeight: isSelected ? 600 : 400,
                          cursor: "pointer",
                          transition: "all 0.15s",
                        }}
                      >
                        {pre.name}
                      </button>
                    );
                  })}
                </div>
              </div>

              {/* Endpoint URL */}
              <FormGroup>
                <FormLabel t={t}>Endpoint URL (OpenAI-compatible)</FormLabel>
                <FieldInput
                  value={currentEndpoint}
                  onChange={(v) =>
                    updateSettings({
                      llm_endpoint: v,
                      local_llm_url: v,
                    })
                  }
                  placeholder="https://generativelanguage.googleapis.com/v1beta/openai/chat/completions"
                  mono
                  t={t}
                />
                <span
                  style={{
                    fontSize: 11,
                    color: t.textDim,
                    fontFamily: "Inter, sans-serif",
                    marginTop: 4,
                    display: "block",
                  }}
                >
                  Works with Google Gemini, local Ollama (http://localhost:11434), LM Studio
                  (http://localhost:1234), or OpenAI.
                </span>
              </FormGroup>

              {/* API Key & Model Name in 2 columns */}
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                <FormGroup>
                  <FormLabel t={t}>API Key (Optional for Local)</FormLabel>
                  <ApiKeyInput
                    value={currentApiKey}
                    onChange={(v) =>
                      updateSettings({
                        llm_api_key: v,
                        api_key: v,
                      })
                    }
                    placeholder="Leave empty for local Ollama / LM Studio"
                    t={t}
                  />
                  <span
                    style={{
                      fontSize: 10,
                      color: t.textDim,
                      fontFamily: "Inter, sans-serif",
                      marginTop: 4,
                      display: "block",
                    }}
                  >
                    Required for Gemini / OpenAI. Leave blank if using localhost.
                  </span>
                </FormGroup>

                <FormGroup>
                  <FormLabel t={t}>Model Name</FormLabel>
                  <FieldInput
                    value={currentModel}
                    onChange={(v) =>
                      updateSettings({
                        llm_model: v,
                        gemini_model: v,
                        local_llm_model: v,
                      })
                    }
                    placeholder="gemini-3.6-flash, llama3.2, etc."
                    mono
                    t={t}
                  />
                  <div style={{ display: "flex", flexWrap: "wrap", gap: 6, marginTop: 7 }}>
                    {LLM_MODEL_SUGGESTIONS.map((m) => (
                      <button
                        key={m}
                        type="button"
                        onClick={() =>
                          updateSettings({
                            llm_model: m,
                            gemini_model: m,
                            local_llm_model: m,
                          })
                        }
                        style={{
                          padding: "2px 8px",
                          borderRadius: 12,
                          background: currentModel === m ? `${t.accent}22` : t.inputBg,
                          border: `1px solid ${currentModel === m ? `${t.accent}66` : t.border}`,
                          color: currentModel === m ? t.accent : t.textMuted,
                          fontSize: 10,
                          fontFamily: "JetBrains Mono, monospace",
                          cursor: "pointer",
                        }}
                      >
                        {m}
                      </button>
                    ))}
                  </div>
                </FormGroup>
              </div>
            </div>
          )}

          {/* Custom API Mode */}
          {activeProvider === "Custom API" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              <FormGroup>
                <FormLabel t={t}>Endpoint URL (REST)</FormLabel>
                <FieldInput
                  value={settings.custom_api_url || "https://api.openai.com/v1/chat/completions"}
                  onChange={(v) => updateSettings({ custom_api_url: v })}
                  placeholder="https://api.openai.com/v1/chat/completions"
                  mono
                  t={t}
                />
              </FormGroup>

              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
                <FormGroup>
                  <FormLabel t={t}>API Key</FormLabel>
                  <ApiKeyInput
                    value={settings.custom_api_key || ""}
                    onChange={(v) => updateSettings({ custom_api_key: v })}
                    placeholder="Bearer sk-…"
                    t={t}
                  />
                </FormGroup>

                <FormGroup>
                  <FormLabel t={t}>Model Name</FormLabel>
                  <FieldInput
                    value={settings.custom_api_model || "gpt-4o-mini"}
                    onChange={(v) => updateSettings({ custom_api_model: v })}
                    placeholder="gpt-4o-mini"
                    mono
                    t={t}
                  />
                </FormGroup>
              </div>
            </div>
          )}
        </div>
      </Section>

      {/* ── Spoken and Target Languages ── */}
      {activeProvider !== "No Translation" && (
        <Section title="Language Configuration" t={t}>
          <div style={{ padding: 16, display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <FormGroup>
              <FormLabel t={t}>Spoken Language (Input)</FormLabel>
              <FieldSelect
                value={settings.source_lang || "Auto"}
                onChange={(v) => updateSettings({ source_lang: v })}
                options={SOURCE_LANG_OPTIONS}
                t={t}
              />
              <span
                style={{
                  fontSize: 11,
                  color: t.textDim,
                  fontFamily: "Inter, sans-serif",
                  marginTop: 4,
                  display: "block",
                }}
              >
                Translation triggers only when this language is spoken (e.g. Ukrainian). If you
                speak any other language, translation is automatically skipped. Choose Auto-detect
                to translate any language.
              </span>
            </FormGroup>

            <FormGroup>
              <FormLabel t={t}>Target Language (Output)</FormLabel>
              <FieldSelect
                value={settings.target_lang || "English"}
                onChange={(v) => updateSettings({ target_lang: v })}
                options={TARGET_LANGUAGES}
                t={t}
              />
              <span
                style={{
                  fontSize: 11,
                  color: t.textDim,
                  fontFamily: "Inter, sans-serif",
                  marginTop: 4,
                  display: "block",
                }}
              >
                The language your text will be translated into. If spoken speech is already in this
                language, translation is automatically skipped.
              </span>
            </FormGroup>
          </div>
        </Section>
      )}

      {/* ── Prompt Template Configuration ── */}
      {activeProvider !== "No Translation" && (
        <Section title="Prompt Template (LLM Instructions)" t={t}>
          <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 14 }}>
            <div>
              <FormLabel t={t}>Prompt Presets</FormLabel>
              <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginTop: 6 }}>
                {PROMPT_PRESETS.map((p) => {
                  const isCurrent = currentPrompt.trim() === p.template.trim();
                  return (
                    <button
                      key={p.name}
                      type="button"
                      onClick={() => updateSettings({ prompt_template: p.template })}
                      style={{
                        padding: "6px 12px",
                        borderRadius: 8,
                        background: isCurrent ? `${t.accent}22` : t.inputBg,
                        border: `1px solid ${isCurrent ? `${t.accent}66` : t.border}`,
                        color: isCurrent ? t.accent : t.textMuted,
                        fontSize: 11,
                        fontFamily: "Inter, sans-serif",
                        fontWeight: isCurrent ? 600 : 400,
                        cursor: "pointer",
                        transition: "all 0.15s",
                      }}
                    >
                      {p.name}
                    </button>
                  );
                })}
              </div>
            </div>

            <FormGroup>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: 6,
                }}
              >
                <FormLabel t={t}>Custom Prompt Template</FormLabel>
                <button
                  type="button"
                  onClick={() => updateSettings({ prompt_template: DEFAULT_PROMPT })}
                  style={{
                    background: "none",
                    border: "none",
                    color: t.accent,
                    fontSize: 11,
                    cursor: "pointer",
                    padding: 0,
                    textDecoration: "underline",
                  }}
                >
                  Reset to default
                </button>
              </div>

              <FieldTextarea
                value={currentPrompt}
                onChange={(v) => updateSettings({ prompt_template: v })}
                placeholder="Enter prompt instructions for translation..."
                rows={5}
                t={t}
              />

              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  marginTop: 8,
                  flexWrap: "wrap",
                }}
              >
                <span style={{ fontSize: 11, color: t.textDim, fontFamily: "Inter, sans-serif" }}>
                  Insert placeholder:
                </span>
                {["{text}", "{source_lang}", "{target_lang}"].map((tag) => (
                  <button
                    key={tag}
                    type="button"
                    onClick={() => insertVariable(tag)}
                    style={{
                      padding: "2px 8px",
                      borderRadius: 4,
                      background: t.inputBg,
                      border: `1px solid ${t.border}`,
                      color: t.accent,
                      fontFamily: "JetBrains Mono, monospace",
                      fontSize: 10,
                      cursor: "pointer",
                    }}
                  >
                    + {tag}
                  </button>
                ))}
              </div>
            </FormGroup>
          </div>
        </Section>
      )}
    </div>
  );
}
