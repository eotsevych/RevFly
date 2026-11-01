import type { BackendSettings } from "@/lib/tauri";

export type Preset = "natural" | "ai" | "verbatim";
type FormatKey =
  | "text_normalization"
  | "remove_filler_words"
  | "convert_numbers"
  | "remove_stutters"
  | "apply_self_corrections"
  | "remove_noise_markers"
  | "collapse_redundancy"
  | "annotate_ambiguity"
  | "normalize_structured_values";

/** Each preset sets these 9 settings; any other combination is Custom. */
export const PRESETS: Record<Preset, Record<FormatKey, boolean>> = {
  natural: {
    text_normalization: true,
    remove_filler_words: true,
    convert_numbers: false,
    remove_stutters: true,
    // It can drop real text ("I'm sorry, ...", "But no, ..."); for AI prompts the gain is worth it.
    apply_self_corrections: false,
    remove_noise_markers: true,
    collapse_redundancy: false,
    annotate_ambiguity: false,
    normalize_structured_values: true,
  },
  ai: {
    text_normalization: true,
    remove_filler_words: true,
    convert_numbers: true,
    remove_stutters: true,
    apply_self_corrections: true,
    remove_noise_markers: true,
    collapse_redundancy: true,
    annotate_ambiguity: true,
    normalize_structured_values: true,
  },
  verbatim: {
    text_normalization: false,
    remove_filler_words: false,
    convert_numbers: false,
    remove_stutters: false,
    apply_self_corrections: false,
    remove_noise_markers: true,
    collapse_redundancy: false,
    annotate_ambiguity: false,
    normalize_structured_values: false,
  },
};

export const STYLES: { value: Preset | "custom"; label: string; hint: string }[] = [
  {
    value: "natural",
    label: "Natural speech",
    hint: "Reads like you talk, for messages and documents: no ums or stutters, numbers stay words",
  },
  {
    value: "ai",
    label: "For AI prompts",
    hint: "Compact text for AI tools: numbers as digits, 'Monday, no, Tuesday' becomes 'Tuesday', repeated phrases merged, unsure words marked [?]",
  },
  {
    value: "verbatim",
    label: "Verbatim",
    hint: "What the model heard, untouched apart from tags like [applause]: for quotes and interviews",
  },
  { value: "custom", label: "Custom", hint: "Your own mix of the steps below" },
];

/** The preset these settings match, or "custom". Unset toggles count as on, as in the backend. */
export function presetOf(settings: BackendSettings): Preset | "custom" {
  const presets = Object.keys(PRESETS) as Preset[];
  const keys = Object.keys(PRESETS.ai) as FormatKey[];
  return (
    presets.find((p) => keys.every((k) => (settings[k] ?? true) === PRESETS[p][k])) ?? "custom"
  );
}
