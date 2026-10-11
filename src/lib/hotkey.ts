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

/**
 * The hotkey a key press stands for, in the backend's format ("RightOption",
 * "CommandOrControl+Shift+Space"), or null while only modifiers of a combo are held.
 * Single-modifier hotkeys rely on the macOS event tap; other platforms only support combos.
 */
export function hotkeyFromKeyEvent(e: KeyLike): string | null {
  if (IS_MAC && SINGLE_MODIFIERS[e.code]) return SINGLE_MODIFIERS[e.code] ?? null;

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

  if (["Meta", "Control", "Alt", "Shift"].includes(e.key)) return null;
  parts.push(e.key === " " ? "Space" : e.key.length === 1 ? e.key.toUpperCase() : e.key);
  return parts.join("+");
}
