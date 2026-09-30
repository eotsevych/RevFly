//! Noise reduction with RNNoise (the nnnoiseless port), the first stage of Cap's Studio Sound.
//!
//! RNNoise works on 48 kHz audio in 10 ms frames. Other rates are converted to 48 kHz and back.

use audioadapter_buffers::direct::InterleavedSlice;
use nnnoiseless::DenoiseState;
use rubato::{Fft, FixedSync, Resampler};

const RNNOISE_RATE: u32 = 48000;
const FRAME: usize = DenoiseState::FRAME_SIZE;
/// RNNoise works on 16-bit sample values held in f32.
const I16_SCALE: f32 = 32768.0;

/// Share of denoised signal in the mix; the rest is the original, which keeps the voice natural.
/// 0.9 is Cap's default ("Balanced").
pub const DEFAULT_WET: f32 = 0.9;

/// Mix for a noise reduction setting: "light", "balanced" or "strong" (Cap's tiers); None when off.
pub fn strength_wet(strength: &str) -> Option<f32> {
    match strength {
        "light" => Some(0.7),
        "balanced" => Some(DEFAULT_WET),
        "strong" => Some(0.98),
        _ => None,
    }
}

/// Removes steady background noise (fans, hum, room tone) from mono audio at any sample rate.
pub fn denoise(samples: &[f32], sample_rate: u32, wet: f32) -> Vec<f32> {
    if samples.is_empty() || sample_rate == 0 {
        return samples.to_vec();
    }
    let wet = if wet.is_finite() { wet.clamp(0.0, 1.0) } else { DEFAULT_WET };
    if sample_rate == RNNOISE_RATE {
        return denoise_48k(samples, wet);
    }
    let (Some(up), true) = (resample(samples, sample_rate, RNNOISE_RATE), wet > 0.0) else {
        return samples.to_vec();
    };
    let clean = denoise_48k(&up, wet);
    match resample(&clean, RNNOISE_RATE, sample_rate) {
        Some(mut out) => {
            out.resize(samples.len(), 0.0);
            out
        }
        None => samples.to_vec(),
    }
}

fn denoise_48k(samples: &[f32], wet: f32) -> Vec<f32> {
    let n = samples.len();
    let mut state = DenoiseState::new();
    let mut input = [0.0f32; FRAME];
    let mut output = [0.0f32; FRAME];
    // One extra frame flushes the one-frame delay of RNNoise's overlap-add.
    let frames = n.div_ceil(FRAME) + 1;
    let mut clean = Vec::with_capacity(frames * FRAME);
    for f in 0..frames {
        for (i, slot) in input.iter_mut().enumerate() {
            let x = samples.get(f * FRAME + i).copied().unwrap_or(0.0);
            *slot = if x.is_finite() { x.clamp(-1.0, 1.0) * I16_SCALE } else { 0.0 };
        }
        state.process_frame(&mut output, &input);
        clean.extend(output.iter().map(|&y| y / I16_SCALE));
    }
    // Output lags input by one frame; the first frame also holds RNNoise's fade-in.
    clean[FRAME..FRAME + n]
        .iter()
        .zip(samples)
        .map(|(&c, &dry)| c * wet + dry * (1.0 - wet))
        .collect()
}

/// High-quality fixed-ratio (FFT) resampling of a whole mono clip.
pub(crate) fn resample(samples: &[f32], from: u32, to: u32) -> Option<Vec<f32>> {
    let mut resampler = Fft::<f32>::new(from as usize, to as usize, 1024, 1, 1, FixedSync::Both).ok()?;
    let input = InterleavedSlice::new(samples, 1, samples.len()).ok()?;
    let mut out = vec![0.0f32; resampler.process_all_needed_output_len(samples.len())];
    let out_len = out.len();
    let mut output = InterleavedSlice::new_mut(&mut out, 1, out_len).ok()?;
    let (_, written) = resampler
        .process_all_into_buffer(&input, &mut output, samples.len(), None)
        .ok()?;
    out.truncate(written);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic white noise in [-amplitude, amplitude].
    fn noise(amplitude: f32, len: usize) -> Vec<f32> {
        let mut seed = 0x2545F491u32;
        (0..len)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                amplitude * (seed as f32 / u32::MAX as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn steady_noise_is_reduced() {
        for rate in [48000, 44100, 16000] {
            let input = noise(0.05, rate as usize * 2);
            let out = denoise(&input, rate, 1.0);
            assert_eq!(out.len(), input.len());
            let tail = rate as usize / 2..;
            let drop_db = 20.0 * (rms(&out[tail.clone()]) / rms(&input[tail])).log10();
            // 16 kHz noise has no energy above 8 kHz, where RNNoise removes the most.
            assert!(drop_db < -6.0, "{rate} Hz: noise only dropped {drop_db:.1} dB");
        }
    }

    /// Speech-like voiced sound: rising pitch with harmonics and a 4 Hz syllable rhythm, never periodic.
    fn voice(rate: u32, seconds: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let phase = 2.0 * std::f32::consts::PI * (150.0 * t + 60.0 * t * t);
                let envelope = 0.6 + 0.4 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
                0.3 * envelope * (phase.sin() + 0.5 * (3.0 * phase).sin() + 0.3 * (7.0 * phase).sin())
            })
            .collect()
    }

    #[test]
    fn output_is_time_aligned_with_input() {
        let input = voice(48000, 2.0);
        let out = denoise(&input, 48000, 1.0);
        let score = |lag: usize| -> f32 { (4800..90000).map(|i| out[i] * input[i - lag]).sum() };
        let best = (0..1000).max_by(|&a, &b| score(a).total_cmp(&score(b))).unwrap();
        assert!(best <= 1, "denoised audio lags the input by {best} samples");
    }

    #[test]
    fn voice_is_kept() {
        let input = voice(48000, 2.0);
        let out = denoise(&input, 48000, 1.0);
        let kept_db = 20.0 * (rms(&out[4800..]) / rms(&input[4800..])).log10();
        assert!(kept_db > -6.0, "voice lost {kept_db:.1} dB");
    }

    #[test]
    fn dry_mix_and_empty_input_pass_through() {
        let input = noise(0.05, 4800);
        assert_eq!(denoise(&input, 48000, 0.0), input);
        assert!(denoise(&[], 48000, 0.9).is_empty());
    }
}
