//! Voice leveling: brings quiet speech up to a steady level before recognition and playback.
//!
//! Chain (same idea as Cap's Studio Sound): gated speech-level measurement → static gain
//! → 2:1 soft-knee compressor → make-up gain → look-ahead peak limiter.

/// Level the speech is brought to (gated RMS, dBFS).
const TARGET_DB: f32 = -18.0;
const MAX_GAIN_DB: f32 = 24.0;
const MIN_GAIN_DB: f32 = -12.0;
const MAX_MAKEUP_DB: f32 = 6.0;
/// Below this speech level the recording holds no voice (muted or blocked mic) and is left untouched.
const SILENCE_DB: f32 = -60.0;

/// Measurement blocks: 100 ms, absolute gate at -70 dB, relative gate 10 dB below the gated mean.
const BLOCK_MS: f32 = 100.0;
const GATE_ABS_DB: f32 = -70.0;
const GATE_REL_DB: f32 = -10.0;

const COMP_THRESHOLD_DB: f32 = TARGET_DB;
const COMP_RATIO: f32 = 2.0;
const COMP_KNEE_DB: f32 = 6.0;
const COMP_ATTACK_MS: f32 = 15.0;
const COMP_RELEASE_MS: f32 = 180.0;

/// Peak ceiling of -1.5 dBFS.
const LIMIT_CEILING: f32 = 0.841;
const LIMIT_LOOKAHEAD_MS: f32 = 5.0;
const LIMIT_RELEASE_MS: f32 = 80.0;

/// Gains measured on one copy of a recording, so every copy (16 kHz and native rate) is leveled alike.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelPlan {
    pub gain_db: f32,
    pub makeup_db: f32,
}

impl LevelPlan {
    pub fn is_identity(&self) -> bool {
        self.gain_db == 0.0 && self.makeup_db == 0.0
    }
}

/// Measures the recording, levels it and returns the plan used, for leveling other copies.
pub fn level(samples: &[f32], sample_rate: u32) -> (Vec<f32>, LevelPlan) {
    let Some(speech_db) = speech_level_db(samples, sample_rate) else {
        return (samples.to_vec(), LevelPlan::default());
    };
    if speech_db < SILENCE_DB {
        return (samples.to_vec(), LevelPlan::default());
    }

    let gain_db = (TARGET_DB - speech_db).clamp(MIN_GAIN_DB, MAX_GAIN_DB);
    let mut out = samples.to_vec();
    apply_gain(&mut out, gain_db);
    compress(&mut out, sample_rate);

    let makeup_db = speech_level_db(&out, sample_rate)
        .map(|db| (TARGET_DB - db).clamp(0.0, MAX_MAKEUP_DB))
        .unwrap_or(0.0);
    apply_gain(&mut out, makeup_db);
    limit(&mut out, sample_rate);

    (out, LevelPlan { gain_db, makeup_db })
}

/// Levels another copy of the same recording with a plan from [`level`].
pub fn apply(samples: &[f32], sample_rate: u32, plan: LevelPlan) -> Vec<f32> {
    let mut out = samples.to_vec();
    if plan.is_identity() {
        return out;
    }
    apply_gain(&mut out, plan.gain_db);
    compress(&mut out, sample_rate);
    apply_gain(&mut out, plan.makeup_db);
    limit(&mut out, sample_rate);
    out
}

/// Gated RMS level of the speech in dBFS, ignoring pauses. None when there is no audio.
pub fn speech_level_db(samples: &[f32], sample_rate: u32) -> Option<f32> {
    let block = ((sample_rate as f32 * BLOCK_MS / 1000.0) as usize).max(1);
    let powers: Vec<f64> = samples
        .chunks(block)
        .filter(|c| c.len() * 2 >= block)
        .map(|c| c.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>() / c.len() as f64)
        .collect();

    let gated_mean = |min_db: f64| -> Option<f64> {
        let min_power = 10f64.powf(min_db / 10.0);
        let kept: Vec<f64> = powers.iter().copied().filter(|&p| p > min_power).collect();
        (!kept.is_empty()).then(|| kept.iter().sum::<f64>() / kept.len() as f64)
    };

    let absolute = gated_mean(GATE_ABS_DB as f64)?;
    let relative_db = 10.0 * absolute.log10() + GATE_REL_DB as f64;
    let speech = gated_mean(relative_db).unwrap_or(absolute);
    Some((10.0 * speech.log10()) as f32)
}

fn apply_gain(samples: &mut [f32], gain_db: f32) {
    if gain_db == 0.0 {
        return;
    }
    let g = 10f32.powf(gain_db / 20.0);
    samples.iter_mut().for_each(|x| *x *= g);
}

fn time_coef(ms: f32, sample_rate: u32) -> f32 {
    (-1.0 / (ms / 1000.0 * sample_rate as f32)).exp()
}

/// RMS-detecting soft-knee compressor.
fn compress(samples: &mut [f32], sample_rate: u32) {
    let attack = time_coef(COMP_ATTACK_MS, sample_rate);
    let release = time_coef(COMP_RELEASE_MS, sample_rate);
    let slope = 1.0 / COMP_RATIO - 1.0;
    let mut env = 0.0f32;

    for x in samples.iter_mut() {
        let power = *x * *x;
        let coef = if power > env { attack } else { release };
        env = coef * env + (1.0 - coef) * power;

        let over = 10.0 * (env + 1e-12).log10() - COMP_THRESHOLD_DB;
        let reduction_db = if 2.0 * over < -COMP_KNEE_DB {
            0.0
        } else if 2.0 * over.abs() <= COMP_KNEE_DB {
            slope * (over + COMP_KNEE_DB / 2.0).powi(2) / (2.0 * COMP_KNEE_DB)
        } else {
            slope * over
        };
        if reduction_db < 0.0 {
            *x *= 10f32.powf(reduction_db / 20.0);
        }
    }
}

/// Look-ahead peak limiter. The gain reaches the level a peak needs before the peak arrives,
/// so no sample exceeds the ceiling and nothing is hard-clipped.
fn limit(samples: &mut [f32], sample_rate: u32) {
    let n = samples.len();
    if n == 0 {
        return;
    }
    let window = ((sample_rate as f32 * LIMIT_LOOKAHEAD_MS / 1000.0) as usize).max(1);
    let needed: Vec<f32> = samples
        .iter()
        .map(|x| if x.abs() > LIMIT_CEILING { LIMIT_CEILING / x.abs() } else { 1.0 })
        .collect();
    if needed.iter().all(|&g| g >= 1.0) {
        return;
    }

    // Minimum over the last `window` samples (monotonic deque).
    let mut trailing_min = vec![1.0f32; n];
    let mut deque: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for i in 0..n {
        while deque.back().is_some_and(|&j| needed[j] >= needed[i]) {
            deque.pop_back();
        }
        deque.push_back(i);
        if deque.front().is_some_and(|&j| j + window <= i) {
            deque.pop_front();
        }
        trailing_min[i] = needed[deque[0]];
    }

    // Average over the next `window` samples: a smooth ramp that is never above what any peak needs.
    let mut smoothed = vec![1.0f32; n];
    let mut sum: f64 = trailing_min[..window.min(n)].iter().map(|&g| g as f64).sum();
    for i in 0..n {
        smoothed[i] = (sum / window as f64) as f32;
        let leaving = trailing_min[i] as f64;
        let entering = trailing_min.get(i + window).copied().unwrap_or(trailing_min[n - 1]) as f64;
        sum += entering - leaving;
    }

    let release = time_coef(LIMIT_RELEASE_MS, sample_rate);
    let mut gain = 1.0f32;
    for (x, &target) in samples.iter_mut().zip(&smoothed) {
        gain = target.min(release * gain + (1.0 - release));
        *x = (*x * gain).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16000;

    /// Speech-like signal: bursts of a 220 Hz tone with pauses of near-silence.
    fn bursts(amplitude: f32, seconds: f32) -> Vec<f32> {
        let n = (RATE as f32 * seconds) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let on = (t * 2.0).fract() < 0.6;
                let a = if on { amplitude } else { amplitude * 0.001 };
                a * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn quiet_speech_is_brought_to_target() {
        let input = bursts(0.02, 4.0); // about -37 dBFS during speech
        let (out, plan) = level(&input, RATE);
        assert!(plan.gain_db > 15.0, "{plan:?}");
        let db = speech_level_db(&out, RATE).unwrap();
        assert!((db - TARGET_DB).abs() < 2.0, "level {db}");
    }

    #[test]
    fn loud_input_never_exceeds_ceiling() {
        let (out, _) = level(&bursts(0.99, 3.0), RATE);
        let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= LIMIT_CEILING + 1e-4, "peak {peak}");
    }

    #[test]
    fn silence_is_left_untouched() {
        let input = vec![0.00005f32; RATE as usize * 2];
        let (out, plan) = level(&input, RATE);
        assert_eq!(plan, LevelPlan::default());
        assert_eq!(out, input);
    }

    #[test]
    fn plan_levels_native_rate_copy_alike() {
        let (_, plan) = level(&bursts(0.02, 4.0), RATE);
        let native: Vec<f32> = {
            let n = 48000 * 4;
            (0..n)
                .map(|i| {
                    let t = i as f32 / 48000.0;
                    let a = if (t * 2.0).fract() < 0.6 { 0.02 } else { 0.00002 };
                    a * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                })
                .collect()
        };
        let out = apply(&native, 48000, plan);
        let db = speech_level_db(&out, 48000).unwrap();
        assert!((db - TARGET_DB).abs() < 2.0, "level {db}");
    }
}
