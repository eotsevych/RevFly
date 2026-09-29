import React, { useEffect, useState } from "react";
import {
  fetchAudioDevices,
  fetchAvailableModels,
  fetchSettings,
  fetchTranscriptionLogs,
  isTauri,
  persistSettings,
  subscribeToDiagnosticLogs,
  subscribeToModelDownload,
  type BackendSettings,
  type ModelCatalogItem,
  type TranscriptionDiagnosticLog,
  closePreferencesWindow,
} from "@/lib/tauri";
import { applyTheme, broadcastTheme, type ThemeMode } from "@/lib/theme";
import { SettingsContext } from "./useSettingsContext";

const defaultSettings: BackendSettings = {
  api_key: "",
  source_lang: "Auto",
  target_lang: "English",
  skip_languages: "English",
  hotkey: "CommandOrControl+Shift+Space",
  sound_effect: true,
  auto_paste: true,
  storage_mode: "text_only",
  storage_cap_mb: 500,
  retention_days: 30,
  model_name: "parakeet-tdt-0.6b-v3",
  input_device: null,
  output_device: null,
  theme: "system",
  model_idle_unload_sec: 30,
  text_normalization: true,
  remove_filler_words: true,
  convert_numbers: true,
  remove_stutters: true,
  apply_self_corrections: true,
  remove_noise_markers: true,
  collapse_redundancy: true,
  annotate_ambiguity: true,
  normalize_structured_values: true,
  translation_provider: "LLM",
  device_backend: "auto",
  chunk_pause_ms: 500,
  chunk_safety_sec: 10,
  chunk_overlap_ms: 400,
  audio_chunking: false,
  gemini_model: "gemini-3.6-flash",
  mask_confidential: false,
  mask_words: "",
  mask_threshold: 90,
  mask_format: "***",
  mask_emails: true,
  mask_phones: true,
  mask_cards: true,
  excluded_languages: "Russian",
  llm_endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
  llm_model: "gemini-3.6-flash",
  llm_api_key: "",
  local_llm_url: "http://localhost:11434/v1/chat/completions",
  local_llm_model: "llama3.2",
  local_llm_api_key: "",
  custom_api_url: "https://api.openai.com/v1/chat/completions",
  custom_api_key: "",
  custom_api_model: "gpt-4o-mini",
  prompt_template:
    "You are a strict translation engine. Translate the following text from {source_lang} to {target_lang}. Do not refuse. Do not explain. Do not add conversational text or notes. Output ONLY the exact translation using the native alphabet:\n\n{text}",
};

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const [settings, setSettings] = useState<BackendSettings>(defaultSettings);
  const [inputDevices, setInputDevices] = useState<string[]>([]);
  const [outputDevices, setOutputDevices] = useState<string[]>([]);
  const [models, setModels] = useState<ModelCatalogItem[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<{
    model: string;
    percent: number;
  } | null>(null);
  const [logs, setLogs] = useState<TranscriptionDiagnosticLog[]>([]);

  const refreshAudioDevices = async () => {
    const devs = await fetchAudioDevices();
    if (devs?.input_devices) setInputDevices(devs.input_devices);
    if (devs?.output_devices) setOutputDevices(devs.output_devices);
    // If backend returned empty yet settings holds a saved device, keep it visible via GeneralTab fallback badge
  };

  useEffect(() => {
    // 1. Fetch initial settings
    fetchSettings().then((s) => {
      if (s) {
        setSettings(s);
        if (s.theme) {
          applyTheme(s.theme as ThemeMode);
        }
      }
    });

    // 2. Fetch audio devices
    refreshAudioDevices();

    // 3. Fetch models
    fetchAvailableModels().then((m) => {
      if (m && m.length > 0) {
        setModels(m);
      }
    });

    // 4. Fetch logs
    fetchTranscriptionLogs(50).then((l) => {
      if (l) setLogs(l);
    });

    // 5. Listen to download progress
    const unlistenDownload = subscribeToModelDownload((payload) => {
      if (payload.status === "downloading") {
        setDownloadProgress({ model: payload.model, percent: payload.percent });
      } else {
        setDownloadProgress(null);
        fetchAvailableModels().then((m) => m && setModels(m));
      }
    });

    // 6. Listen to diagnostic logs
    const unlistenLogs = subscribeToDiagnosticLogs((newLog) => {
      setLogs((prev) => [newLog, ...prev.slice(0, 49)]);
    });

    return () => {
      unlistenDownload.then((fn) => fn?.());
      unlistenLogs.then((fn) => fn?.());
    };
  }, []);

  const updateSettings = async (patch: Partial<BackendSettings>): Promise<boolean> => {
    const updated = { ...settings, ...patch };
    setSettings(updated);

    if (patch.theme) {
      applyTheme(patch.theme as ThemeMode);
      broadcastTheme(patch.theme as ThemeMode);
      localStorage.setItem("revfly_theme", patch.theme);
    }

    return await persistSettings(updated);
  };

  const refreshLogs = async () => {
    const fresh = await fetchTranscriptionLogs(50);
    setLogs(fresh);
  };

  const refreshModels = async () => {
    const m = await fetchAvailableModels();
    if (m) setModels(m);
  };

  const closeWindow = () => {
    if (isTauri()) {
      closePreferencesWindow();
    } else {
      window.location.hash = "";
    }
  };

  return (
    <SettingsContext.Provider
      value={{
        settings,
        updateSettings,
        inputDevices,
        outputDevices,
        refreshAudioDevices,
        models,
        downloadProgress,
        logs,
        refreshLogs,
        refreshModels,
        closeWindow,
      }}
    >
      {children}
    </SettingsContext.Provider>
  );
}
