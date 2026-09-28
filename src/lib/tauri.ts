import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const isTauri = (): boolean => {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
};

export interface BackendSettings {
  api_key: string;
  source_lang: string;
  target_lang: string;
  skip_languages: string;
  hotkey: string;
  sound_effect: boolean;
  auto_paste: boolean;
  storage_mode: string;
  storage_cap_mb: number;
  retention_days: number;
  model_name: string;
  input_device?: string | null;
  output_device?: string | null;
  theme?: string;
  model_idle_unload_sec?: number;
  window_x?: number | null;
  window_y?: number | null;
  text_normalization?: boolean;
  remove_filler_words?: boolean;
  convert_numbers?: boolean;
  remove_stutters?: boolean;
  apply_self_corrections?: boolean;
  remove_noise_markers?: boolean;
  collapse_redundancy?: boolean;
  annotate_ambiguity?: boolean;
  normalize_structured_values?: boolean;
  translation_provider?: string;
  device_backend?: string;
  chunk_pause_ms?: number;
  chunk_safety_sec?: number;
  chunk_overlap_ms?: number;
  gemini_model?: string;
  mask_confidential?: boolean;
  mask_words?: string;
  mask_threshold?: number;
  mask_format?: string;
  mask_emails?: boolean;
  mask_phones?: boolean;
  mask_cards?: boolean;
  excluded_languages?: string;
  llm_endpoint?: string;
  llm_model?: string;
  llm_api_key?: string;
  local_llm_url?: string;
  local_llm_model?: string;
  local_llm_api_key?: string;
  custom_api_url?: string;
  custom_api_key?: string;
  custom_api_model?: string;
  prompt_template?: string;
}

export interface HardwareProfile {
  os: string;
  arch: string;
  logical_cores: number;
  recommended_threads: number;
  has_avx2: boolean;
  has_avx512: boolean;
  has_vnni: boolean;
  has_neon: boolean;
  has_metal: boolean;
  summary: string;
}

export async function fetchHardwareProfile(): Promise<HardwareProfile | null> {
  if (!isTauri()) {
    return {
      os: "macos",
      arch: "aarch64",
      logical_cores: 8,
      recommended_threads: 6,
      has_avx2: false,
      has_avx512: false,
      has_vnni: false,
      has_neon: true,
      has_metal: true,
      summary:
        "MACOS aarch64 (8 cores) | Accelerators: [ARM NEON, Apple Metal/ANE] | Recommended Threads: 6",
    };
  }
  try {
    return await invoke<HardwareProfile>("get_hardware_profile");
  } catch (err) {
    console.error("Failed to get hardware profile:", err);
    return null;
  }
}

export interface PostProcessedTranscript {
  raw_text: string;
  clean_text: string;
  uncertain_spans: string[];
  requires_clarification: boolean;
}

export interface HistoryItem {
  id: number;
  timestamp: string;
  raw_text: string;
  translated_text: string;
  source_lang: string;
  target_lang: string;
  duration: number;
  audio_path?: string | null;
}

export interface AssistantStateEvent {
  state: "idle" | "listening" | "transcribing" | "translating" | "done" | "error";
  title: string;
  subtitle: string | null;
  text?: string;
}

export async function fetchSettings(): Promise<BackendSettings | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<BackendSettings>("get_settings");
  } catch (err) {
    console.error("Failed to get settings from Tauri:", err);
    return null;
  }
}

export async function persistSettings(settings: BackendSettings): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    await invoke("save_settings", { newSettings: settings });
    return true;
  } catch (err) {
    console.error("Failed to save settings to Tauri:", err);
    return false;
  }
}

export async function triggerToggleRecording(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("toggle_recording");
  } catch (err) {
    console.error("Failed to toggle recording:", err);
  }
}

export async function triggerCancelRecording(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("cancel_recording");
  } catch (err) {
    console.error("Failed to cancel recording:", err);
  }
}

export async function fetchHistory(limit = 50): Promise<HistoryItem[]> {
  if (!isTauri()) return [];
  try {
    return await invoke<HistoryItem[]>("get_history", { limit });
  } catch (err) {
    console.error("Failed to fetch history:", err);
    return [];
  }
}

export async function triggerClearHistory(): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    await invoke("clear_history");
    return true;
  } catch (err) {
    console.error("Failed to clear history:", err);
    return false;
  }
}

export async function fetchAudioDevices(): Promise<{
  input_devices: string[];
  output_devices: string[];
}> {
  if (!isTauri()) return { input_devices: [], output_devices: [] };
  try {
    return await invoke<{ input_devices: string[]; output_devices: string[] }>("get_audio_devices");
  } catch (err) {
    console.error("Failed to fetch audio devices:", err);
    return { input_devices: [], output_devices: [] };
  }
}

export async function setAudioDevice(kind: "input" | "output", name: string): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    await invoke("set_audio_device", { kind, name });
    return true;
  } catch (err) {
    console.error("Failed to set audio device:", err);
    return false;
  }
}

export async function startMicTest(deviceName?: string | null): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    await invoke("start_mic_test", { deviceName: deviceName || null });
    return true;
  } catch (err) {
    console.error("Failed to start mic test:", err);
    return false;
  }
}

export async function stopMicTest(): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    await invoke("stop_mic_test");
    return true;
  } catch (err) {
    console.error("Failed to stop mic test:", err);
    return false;
  }
}

export async function playTestSound(): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    await invoke("play_test_sound");
    return true;
  } catch (err) {
    console.error("Failed to play test sound:", err);
    return false;
  }
}

export async function openPreferencesWindow(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("open_preferences_window");
  } catch (err) {
    console.error("Failed to open preferences window:", err);
  }
}

export async function closePreferencesWindow(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("close_preferences_window");
  } catch (err) {
    console.error("Failed to close preferences window:", err);
  }
}

export async function saveWindowPosition(x: number, y: number): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    await invoke("save_window_position", { x: Math.round(x), y: Math.round(y) });
    return true;
  } catch (err) {
    console.error("Failed to save window position:", err);
    return false;
  }
}

export async function triggerStartDragging(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("start_dragging_window");
  } catch {
    try {
      const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      await getCurrentWebviewWindow().startDragging();
    } catch (err) {
      console.error("Failed to start dragging window:", err);
    }
  }
}

export async function subscribeToStateChanges(
  callback: (payload: AssistantStateEvent) => void,
): Promise<UnlistenFn | null> {
  if (!isTauri()) return null;
  try {
    return await listen<AssistantStateEvent>("assistant-state-changed", (event) => {
      callback(event.payload);
    });
  } catch (err) {
    console.error("Failed to listen to state events:", err);
    return null;
  }
}

export async function subscribeToAudioLevels(
  callback: (levels: number[]) => void,
): Promise<UnlistenFn | null> {
  if (!isTauri()) return null;
  try {
    return await listen<number[]>("audio-level", (event) => {
      callback(event.payload);
    });
  } catch (err) {
    console.error("Failed to listen to audio levels:", err);
    return null;
  }
}

export async function subscribeToMicTestLevels(
  callback: (data: { level: number; peak: number }) => void,
): Promise<UnlistenFn | null> {
  if (!isTauri()) return null;
  try {
    return await listen<{ level: number; peak: number }>("mic-test-level", (event) => {
      callback(event.payload);
    });
  } catch (err) {
    console.error("Failed to listen to mic test levels:", err);
    return null;
  }
}

export async function subscribeToModelDownload(
  callback: (progress: {
    status: string;
    model: string;
    percent: number;
    downloaded_bytes?: number;
    total_bytes?: number;
  }) => void,
): Promise<UnlistenFn | null> {
  if (!isTauri()) return null;
  try {
    return await listen<{
      status: string;
      model: string;
      percent: number;
      downloaded_bytes?: number;
      total_bytes?: number;
    }>("model-download-progress", (event) => {
      callback(event.payload);
    });
  } catch (err) {
    console.error("Failed to listen to model download progress:", err);
    return null;
  }
}

export interface ModelStatus {
  model_name: string;
  exists: boolean;
  path: string;
}

export async function fetchModelStatus(): Promise<ModelStatus | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<ModelStatus>("get_model_status");
  } catch (err) {
    console.error("Failed to get model status:", err);
    return null;
  }
}

export async function triggerDownloadModel(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<string>("download_model");
  } catch (err) {
    console.error("Failed to download model:", err);
    return null;
  }
}

export interface ChunkDiagnosticEvent {
  chunk_index: number;
  trigger_reason: string;
  duration_sec: number;
  has_overlap: boolean;
  overlap_ms: number;
  silence_detected_ms: number;
  rms_energy: number;
  timestamp: string;
  detail: string;
}

export interface TranscriptionDiagnosticLog {
  id: number;
  timestamp: string;
  audio_duration_sec: number;
  audio_samples_count: number;
  vad_trim_ms: number;
  vad_original_sec: number;
  vad_trimmed_sec: number;
  vad_silence_removed_sec: number;
  model_name: string;
  gpu_metal_active: boolean;
  threads_count: number;
  whisper_inference_ms: number;
  whisper_speed_factor: number;
  detected_lang: string;
  raw_text: string;
  translation_skipped: boolean;
  translation_skip_reason: string;
  translation_ms: number;
  final_text: string;
  clipboard_paste_ms: number;
  history_save_ms: number;
  total_pipeline_ms: number;
  audio_filename?: string | null;
  vad_audio_filename?: string | null;
  whisper_raw_output?: string | null;
  segments_count?: number;
  chunk_events?: ChunkDiagnosticEvent[];
  action_logs?: string[];
}

export interface ModelCatalogItem {
  id: string;
  name: string;
  size_desc: string;
  speed_desc: string;
  filename: string;
  downloaded: boolean;
  file_size_bytes: number;
}

export async function fetchTranscriptionLogs(limit = 50): Promise<TranscriptionDiagnosticLog[]> {
  if (!isTauri()) return [];
  try {
    return await invoke<TranscriptionDiagnosticLog[]>("get_transcription_logs", { limit });
  } catch (err) {
    console.error("Failed to fetch transcription logs:", err);
    return [];
  }
}

export async function triggerClearTranscriptionLogs(): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    await invoke("clear_transcription_logs");
    return true;
  } catch (err) {
    console.error("Failed to clear transcription logs:", err);
    return false;
  }
}

export async function fetchAvailableModels(): Promise<ModelCatalogItem[]> {
  if (!isTauri()) return [];
  try {
    return await invoke<ModelCatalogItem[]>("get_available_models");
  } catch (err) {
    console.error("Failed to fetch available models:", err);
    return [];
  }
}

export async function triggerDownloadSpecificModel(modelName: string): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<string>("download_specific_model", { modelName });
  } catch (err) {
    console.error(`Failed to download model ${modelName}:`, err);
    return null;
  }
}

export async function subscribeToDiagnosticLogs(
  callback: (payload: TranscriptionDiagnosticLog) => void,
): Promise<UnlistenFn | null> {
  if (!isTauri()) return null;
  try {
    return await listen<TranscriptionDiagnosticLog>("transcription-diagnostic-log", (event) => {
      callback(event.payload);
    });
  } catch (err) {
    console.error("Failed to listen to diagnostic log events:", err);
    return null;
  }
}

export async function fetchAudioData(filename?: string): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<string>("get_audio_data", { filename: filename || null });
  } catch (err) {
    console.error("Failed to fetch audio data:", err);
    return null;
  }
}

export async function triggerPlayAudio(filename?: string): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    await invoke("play_recorded_audio", { filename: filename || null });
    return true;
  } catch (err) {
    console.error("Failed to play audio:", err);
    return false;
  }
}

export async function triggerOpenAudioFolder(): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    await invoke("open_audio_folder");
    return true;
  } catch (err) {
    console.error("Failed to open audio folder:", err);
    return false;
  }
}

export interface DiagnosticSegment {
  start_ms: number;
  end_ms: number;
  text: string;
  no_speech_prob: number;
}

export interface LabAudioItem {
  id: string;
  name: string;
  filename: string;
  duration_sec: number;
  size_bytes: number;
  is_vad_trimmed: boolean;
}

export interface LabExperimentRequest {
  audio_filename: string;
  model_name: string;
  use_vad: boolean;
  language?: string | null;
  text_normalization?: boolean;
  remove_filler_words?: boolean;
  convert_numbers?: boolean;
  remove_stutters?: boolean;
  apply_self_corrections?: boolean;
  remove_noise_markers?: boolean;
  collapse_redundancy?: boolean;
  annotate_ambiguity?: boolean;
  normalize_structured_values?: boolean;
}

export interface LabExperimentResult {
  clean_text: string;
  raw_text: string;
  raw_output: string;
  detected_lang: string;
  segments: DiagnosticSegment[];
  original_duration_sec: number;
  processed_duration_sec: number;
  vad_cut_sec: number;
  vad_time_ms: number;
  inference_time_ms: number;
  speed_factor: number;
  hardware_engine: string;
  original_audio_filename: string;
  vad_audio_filename?: string | null;
  post_applied: string[];
  uncertain_spans: string[];
  requires_clarification: boolean;
}

export async function fetchLabAudioFiles(): Promise<LabAudioItem[]> {
  if (!isTauri()) return [];
  try {
    return await invoke<LabAudioItem[]>("list_lab_audio_files");
  } catch (err) {
    console.error("Failed to list lab audio files:", err);
    return [];
  }
}

export async function triggerRunLabExperiment(
  req: LabExperimentRequest,
): Promise<LabExperimentResult | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<LabExperimentResult>("run_lab_experiment", { req });
  } catch (err) {
    console.error("Failed to run lab experiment:", err);
    throw err;
  }
}

export async function triggerSaveLabCustomAudio(base64Wav: string): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<string>("save_lab_custom_audio", { base64Wav });
  } catch (err) {
    console.error("Failed to save custom lab audio:", err);
    return null;
  }
}

export async function checkAccessibility(): Promise<boolean> {
  if (!isTauri()) return true;
  try {
    return await invoke<boolean>("check_accessibility");
  } catch (err) {
    console.error("Failed to check accessibility:", err);
    return false;
  }
}

export async function requestAccessibility(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("request_accessibility");
  } catch (err) {
    console.error("Failed to request accessibility:", err);
  }
}

export async function openAccessibilitySettings(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("open_accessibility_settings");
  } catch (err) {
    console.error("Failed to open accessibility settings:", err);
  }
}

export async function openModelsFolder(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<string>("open_models_folder");
  } catch (err) {
    console.error("Failed to open models folder:", err);
    return null;
  }
}

export async function getModelsDirPath(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<string>("get_models_dir_path");
  } catch (err) {
    console.error("Failed to get models dir:", err);
    return null;
  }
}

export async function pickAndImportModel(): Promise<ModelCatalogItem | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<ModelCatalogItem>("pick_and_import_model");
  } catch (err) {
    const msg = String(err);
    if (msg.includes("No file selected")) return null;
    console.error("Failed to import model:", err);
    throw err;
  }
}

export async function downloadCustomModel(
  url: string,
  fileName?: string | null,
): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<string>("download_custom_model", { url, fileName: fileName || null });
  } catch (err) {
    console.error("Failed to download custom model:", err);
    throw err;
  }
}

export async function postProcessTranscript(text: string): Promise<PostProcessedTranscript | null> {
  if (!isTauri()) {
    return {
      raw_text: text,
      clean_text: text,
      uncertain_spans: [],
      requires_clarification: false,
    };
  }
  try {
    return await invoke<PostProcessedTranscript>("post_process_transcript", { text });
  } catch (err) {
    console.error("Failed to post-process transcript:", err);
    return null;
  }
}

export async function isModelLoaded(): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    return await invoke<boolean>("is_model_loaded");
  } catch {
    return false;
  }
}
export async function ejectModel(): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    await invoke("eject_model");
    return true;
  } catch (err) {
    console.error(err);
    return false;
  }
}
export async function getTrayState(): Promise<{
  is_loaded: boolean;
  phase: string;
  can_eject: boolean;
} | null> {
  if (!isTauri()) return null;
  try {
    return await invoke("get_tray_state");
  } catch {
    return null;
  }
}

export type UpdateState =
  "idle" | "checking" | "up_to_date" | "available" | "downloading" | "installing" | "error";

export interface UpdateStatus {
  state: UpdateState;
  current_version: string;
  version: string | null;
  percent: number | null;
  message: string | null;
}

export async function fetchUpdateStatus(): Promise<UpdateStatus | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<UpdateStatus>("get_update_status");
  } catch {
    return null;
  }
}

export async function triggerCheckForUpdates(): Promise<UpdateStatus | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<UpdateStatus>("check_for_updates");
  } catch {
    return null;
  }
}

/** Resolves with an error message when the install could not start or failed; the app restarts on success. */
export async function triggerInstallUpdate(): Promise<string | null> {
  if (!isTauri()) return "Updates are only available in the desktop app.";
  try {
    await invoke("install_update");
    return null;
  } catch (err) {
    return typeof err === "string" ? err : String(err);
  }
}

export async function subscribeToUpdateStatus(
  callback: (status: UpdateStatus) => void,
): Promise<UnlistenFn | null> {
  if (!isTauri()) return null;
  try {
    return await listen<UpdateStatus>("update-status", (event) => {
      callback(event.payload);
    });
  } catch (err) {
    console.error("Failed to listen to update status:", err);
    return null;
  }
}
