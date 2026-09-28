import { createContext, useContext } from "react";
import type { BackendSettings, ModelCatalogItem, TranscriptionDiagnosticLog } from "@/lib/tauri";

// Kept apart from SettingsProvider so that file exports only components (React fast refresh).
export interface SettingsContextType {
  settings: BackendSettings;
  updateSettings: (patch: Partial<BackendSettings>) => Promise<boolean>;
  inputDevices: string[];
  outputDevices: string[];
  refreshAudioDevices: () => Promise<void>;
  models: ModelCatalogItem[];
  downloadProgress: { model: string; percent: number } | null;
  logs: TranscriptionDiagnosticLog[];
  refreshLogs: () => Promise<void>;
  refreshModels: () => Promise<void>;
  closeWindow: () => void;
}

export const SettingsContext = createContext<SettingsContextType | null>(null);

export function useSettingsContext() {
  const ctx = useContext(SettingsContext);
  if (!ctx) {
    throw new Error("useSettingsContext must be used within a SettingsProvider");
  }
  return ctx;
}
