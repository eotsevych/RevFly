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

/** Modifier names in the backend's order, keyed by what the modifier is called in a combo. */
const MODIFIER_ORDER = IS_MAC
  ? ["CommandOrControl", "Control", "Alt", "Shift"]
  : ["CommandOrControl", "Super", "Alt", "Shift"];

/** The combo name of a modifier key (`AltLeft` → `Alt`), or null for any other key. */
function modifierName(code: string): string | null {
  if (code.startsWith("Meta")) return IS_MAC ? "CommandOrControl" : "Super";
  if (code.startsWith("Control")) return IS_MAC ? "Control" : "CommandOrControl";
  if (code.startsWith("Alt")) return "Alt";
  if (code.startsWith("Shift")) return "Shift";
  return null;
}

/** The modifiers an event says are held, which covers ones pressed before recording began. */
function heldModifiers(e: KeyLike): string[] {
  const names: string[] = [];
  if (e.metaKey) names.push(IS_MAC ? "CommandOrControl" : "Super");
  if (e.ctrlKey) names.push(IS_MAC ? "Control" : "CommandOrControl");
  if (e.altKey) names.push("Alt");
  if (e.shiftKey) names.push("Shift");
  return names;
}

/** The backend name of a non-modifier key. */
function keyName(e: KeyLike): string {
  // Letters and digits come from the physical key: with ⌥ held, macOS types "˚" for K.
  const physical = /^(?:Key([A-Z])|Digit(\d))$/.exec(e.code);
  if (physical) return physical[1] ?? physical[2] ?? e.key;
  if (e.key === " ") return "Space";
  return e.key.length === 1 ? e.key.toUpperCase() : e.key;
}

/**
 * Records a hotkey from key presses, in the backend's format. Hold keys down one after another,
 * and the hotkey is everything held when the first key is released:
 *
 * - a modifier alone is a single-key hotkey ("RightOption"),
 * - several modifiers are a chord ("Alt+Shift"),
 * - modifiers plus another key are a combo ("CommandOrControl+Shift+Space").
 *
 * Modifier-only hotkeys need the macOS event tap, so elsewhere they are ignored. Feed it every
 * keydown and keyup while recording.
 */
export function createHotkeyCapture(onHotkey: (hotkey: string) => void) {
  const modifiers = new Set<string>();
  // The one modifier key pressed, by side ("AltRight"), while it's the only one.
  let soloModifier: string | null = null;
  let key: string | null = null;

  const reset = () => {
    modifiers.clear();
    soloModifier = null;
    key = null;
  };

  const finish = () => {
    const mods = MODIFIER_ORDER.filter((name) => modifiers.has(name));
    let hotkey: string | null = null;
    if (key) hotkey = [...mods, key].join("+");
    else if (IS_MAC && mods.length === 1 && soloModifier)
      hotkey = SINGLE_MODIFIERS[soloModifier] ?? null;
    else if (IS_MAC && mods.length > 1) hotkey = mods.join("+");
    reset();
    if (hotkey) onHotkey(hotkey);
  };

  return {
    keydown(e: KeyLike) {
      const held = heldModifiers(e);
      const name = modifierName(e.code);
      if (name) {
        const firstPress = modifiers.size === 0 && !key;
        soloModifier = firstPress && held.length <= 1 ? e.code : null;
        modifiers.add(name);
      } else {
        soloModifier = null;
        key = keyName(e);
      }
      held.forEach((m) => modifiers.add(m));
    },
    keyup() {
      if (modifiers.size > 0 || key) finish();
    },
  };
}
