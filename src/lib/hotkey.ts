const IS_MAC = typeof navigator !== "undefined" && /Mac/i.test(navigator.userAgent);

export function formatDisplay(raw: string): string {
  if (!raw) return "";
  if (raw === "RightOption" || raw === "AltRight") return "Right ⌥ (Option)";
  if (raw === "LeftOption" || raw === "AltLeft") return "Left ⌥ (Option)";
  if (raw === "RightControl" || raw === "ControlRight") return "Right ⌃ (Control)";
  if (raw === "LeftControl" || raw === "ControlLeft") return "Left ⌃ (Control)";
  if (raw === "RightCommand" || raw === "MetaRight") return "Right ⌘ (Command)";
  if (raw === "LeftCommand" || raw === "MetaLeft") return "Left ⌘ (Command)";
  if (raw === "RightShift" || raw === "ShiftRight") return "Right ⇧ (Shift)";
  if (raw === "LeftShift" || raw === "ShiftLeft") return "Left ⇧ (Shift)";
  return raw
    .replace(/CommandOrControl/gi, IS_MAC ? "⌘" : "Ctrl")
    .replace(/Control/gi, "Ctrl")
    .replace(/Command/gi, "⌘")
    .replace(/\+/g, " + ");
}

const SINGLE_MODIFIERS: Record<string, string> = {
  AltRight: "RightOption",
  ControlRight: "RightControl",
  AltLeft: "LeftOption",
  ControlLeft: "LeftControl",
  MetaRight: "RightCommand",
  MetaLeft: "LeftCommand",
  ShiftRight: "RightShift",
  ShiftLeft: "LeftShift",
};

type KeyLike = Pick<KeyboardEvent, "code" | "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

const MODIFIER_KEYS = ["Meta", "Control", "Alt", "Shift"];

/** The key combination of a key press (`CommandOrControl+Shift+Space`), or null for a lone modifier. */
function comboFromKeyEvent(e: KeyLike): string | null {
  if (MODIFIER_KEYS.includes(e.key)) return null;
  const parts: string[] = [];
  if (IS_MAC) {
    if (e.metaKey) parts.push("CommandOrControl");
    if (e.ctrlKey) parts.push("Control");
  } else {
    if (e.ctrlKey) parts.push("CommandOrControl");
    if (e.metaKey) parts.push("Super");
  }
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  // Letters and digits come from the physical key: with ⌥ held, macOS types "˚" for K.
  const physical = /^(?:Key([A-Z])|Digit(\d))$/.exec(e.code);
  const key = physical
    ? (physical[1] ?? physical[2])
    : e.key === " "
      ? "Space"
      : e.key.length === 1
        ? e.key.toUpperCase()
        : e.key;
  parts.push(key ?? e.key);
  return parts.join("+");
}

/**
 * Records a hotkey from key presses, in the backend's format ("RightOption",
 * "CommandOrControl+Shift+Space"). A modifier pressed alone and released alone is a
 * single-modifier hotkey (macOS only: it needs the event tap); a modifier held while another
 * key is pressed is part of a combo. Feed it every keydown and keyup while recording.
 */
export function createHotkeyCapture(onHotkey: (hotkey: string) => void) {
  // The single-modifier hotkey that releasing the key would record.
  let pending: string | null = null;
  return {
    keydown(e: KeyLike) {
      const combo = comboFromKeyEvent(e);
      if (combo) {
        pending = null;
        onHotkey(combo);
        return;
      }
      const held = [e.metaKey, e.ctrlKey, e.altKey, e.shiftKey].filter(Boolean).length;
      // Only the first modifier, pressed on its own, can become a single-modifier hotkey.
      pending = IS_MAC && held === 1 ? (SINGLE_MODIFIERS[e.code] ?? null) : null;
    },
    keyup(e: KeyLike) {
      if (pending && SINGLE_MODIFIERS[e.code] === pending) onHotkey(pending);
      pending = null;
    },
  };
}
