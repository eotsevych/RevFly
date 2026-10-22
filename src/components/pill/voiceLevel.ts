// How the pill's live indicators read the mic level.

// A voice opens the gate above VOICE_OPEN and it closes below VOICE_CLOSE. The gate ignores the bar
// scale, so silence looks the same at every setting.
export const VOICE_OPEN = 0.035;
export const VOICE_CLOSE = 0.025;

// How much higher the bars reach on speech peaks (the `bar_scale` setting): 1 is the original
// response, 3.2 suits a desk mic 25–35 cm away. The pill and Settings set it from their settings.
export const DEFAULT_BAR_SCALE = 3.2;
let barScale = DEFAULT_BAR_SCALE;

export function setBarVoiceScale(scale: number | undefined) {
  barScale = scale && scale > 0 ? scale : DEFAULT_BAR_SCALE;
}

export function getBarVoiceScale(): number {
  return barScale;
}
