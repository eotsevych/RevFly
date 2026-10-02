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
