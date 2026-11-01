//! Live chunking ("Audio chunking" on): while the user is still talking, the recording is cut in
//! the middle of each long pause and the finished piece is transcribed in the background, so at stop
//! only the audio after the last cut is left.
//!
//! A cut needs a pause of `LIVE_PAUSE` (time to think, not a breath) at least `MIN_PIECE` after the
//! last one. Pauses are judged on the raw microphone audio, before noise reduction (which can flatten
//! quiet speech into "silence"), with a sensitive speech threshold, so a cut doesn't land on soft or
//! off-mic speech. Each piece also starts `OVERLAP` before its cut, so a soft word at the cut is
//! heard whole by one of the two pieces. It matters most for Whisper and Parakeet on the CPU; on the Neural Engine (3
//! minutes in about a second) it pays off only for long dictations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::audio::AudioRecorder;

const RATE: usize = 16000;
const LIVE_PAUSE: usize = 2 * RATE;
/// Shorter pieces would give the model less context.
const MIN_PIECE: usize = 20 * RATE;
/// Below this speech probability a frame is silence. Silero's usual threshold is 0.5; lower, so
/// soft speech counts as speech.
const SILENCE_PROB: f32 = 0.2;
const TICK: Duration = Duration::from_millis(250);
/// How far a piece reaches back before its cut. The words both pieces heard are dropped once by
/// `chunking::join_overlapping`.
pub const OVERLAP: usize = RATE;

/// Transcribes one piece of prepared 16 kHz audio.
pub type Transcribe = Box<dyn FnMut(&[f32]) -> Result<Piece, String> + Send>;

pub struct Piece {
    pub text: String,
    /// The language the next pieces and the rest of the recording are transcribed in (Whisper).
    pub lang: Option<String>,
}

/// What was transcribed while recording: everything before `cut` (in 16 kHz samples).
pub struct Done {
    pub cut: usize,
    pub text: String,
    pub lang: Option<String>,
    pub pieces: usize,
}

/// Finds cut points in a stream of per-frame speech probabilities.
pub struct PauseFinder {
    frame: usize,
    /// Samples scored so far.
    at: usize,
    last_cut: usize,
    silence_start: Option<usize>,
}

impl PauseFinder {
    pub fn new(frame: usize) -> Self {
        Self { frame, at: 0, last_cut: 0, silence_start: None }
    }

    /// Scores the next frames and returns the cuts found in them, as positions in samples.
    pub fn push(&mut self, probs: &[f32]) -> Vec<usize> {
        let mut cuts = Vec::new();
        for &prob in probs {
            let frame_start = self.at;
            self.at += self.frame;
            if prob >= SILENCE_PROB {
                self.silence_start = None;
                continue;
            }
            let start = *self.silence_start.get_or_insert(frame_start);
            if self.at - start >= LIVE_PAUSE && start >= self.last_cut + MIN_PIECE {
                self.last_cut = start + (self.at - start) / 2;
                cuts.push(self.last_cut);
            }
        }
        cuts
    }
}

/// Cuts and transcribes the current recording while it goes on. Dropping it stops that.
pub struct LiveChunks {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Option<Done>>>,
}

impl LiveChunks {
    /// Starts on the recording `recorder` has just started. `wet` is the noise reduction mix, as the
    /// whole recording would get it.
    pub fn start(recorder: Arc<AudioRecorder>, wet: Option<f32>, transcribe: Transcribe) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("live-chunks".into())
            .spawn(move || match run(&recorder, wet, transcribe, &stopped) {
                Ok(done) => done,
                Err(e) => {
                    log::warn!("Live chunking stopped: {e}; the recording is transcribed whole at the end");
                    None
                }
            })
            .map_err(|e| log::warn!("Couldn't start live chunking: {e}"))
            .ok();
        Self { stop, thread }
    }

    /// Stops and waits for the piece being transcribed. `None` when nothing was cut, or when a piece
    /// failed: the whole recording is then transcribed the usual way.
    pub fn finish(mut self) -> Option<Done> {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take()?.join().ok().flatten()
    }
}

impl Drop for LiveChunks {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn run(recorder: &AudioRecorder, wet: Option<f32>, mut transcribe: Transcribe, stop: &AtomicBool) -> Result<Option<Done>, String> {
    let started = Instant::now();
    let mut vad = silero_vad_crs::SileroVad::new().map_err(|e| format!("{e:?}"))?;
    let mut finder = PauseFinder::new(vad.source_window_samples());
    let (mut scored, mut cut) = (0usize, 0usize);
    let mut texts = Vec::new();
    let mut lang = None;
    loop {
        std::thread::sleep(TICK);
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let new = recorder.speech_since(scored);
        scored += new.len();
        for next in finder.push(&vad.push(&new).map_err(|e| format!("{e:?}"))?) {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            let start = cut.saturating_sub(OVERLAP);
            let mut piece = recorder.speech_since(start);
            if piece.len() < next - start {
                break; // the recording stopped meanwhile
            }
            piece.truncate(next - start);
            if cut == 0 {
                if let Some(until) = recorder.chime_until() {
                    // Measured from before the first sample arrived, so it covers the chime.
                    let window = until.saturating_duration_since(started).as_secs_f64();
                    crate::audio::remove_start_chime(&mut piece, RATE as u32, (window * RATE as f64) as usize);
                }
            }
            // As the whole recording would be prepared (see CapturedAudio::new), piece by piece.
            if let Some(wet) = wet {
                piece = crate::denoise::denoise(&piece, RATE as u32, wet);
            }
            let (piece, _) = crate::leveler::level(&piece, RATE as u32);
            let t0 = Instant::now();
            let done = transcribe(&piece)?;
            log::info!(
                "Live chunking: transcribed {:.1}-{:.1} s in {} ms while recording",
                cut as f32 / RATE as f32,
                next as f32 / RATE as f32,
                t0.elapsed().as_millis()
            );
            texts.push(done.text);
            lang = done.lang;
            cut = next;
        }
    }
    Ok((!texts.is_empty()).then(|| Done { cut, text: crate::chunking::join_overlapping(&texts), lang, pieces: texts.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: usize = 512;

    /// Probabilities for `seconds` of speech (0.9) or silence (0.05).
    fn frames(seconds: f64, prob: f32) -> Vec<f32> {
        vec![prob; (seconds * RATE as f64 / FRAME as f64).round() as usize]
    }

    fn secs(samples: usize) -> f64 {
        samples as f64 / RATE as f64
    }

    #[test]
    fn cuts_in_the_middle_of_a_long_pause_after_enough_speech() {
        let mut finder = PauseFinder::new(FRAME);
        assert!(finder.push(&frames(25.0, 0.9)).is_empty());
        let cuts = finder.push(&frames(3.0, 0.05));
        assert_eq!(cuts.len(), 1);
        // Cut as soon as the pause reaches 2 s, in its middle.
        assert!((secs(cuts[0]) - 26.0).abs() < 0.1, "cut at {}", secs(cuts[0]));
    }

    #[test]
    fn short_pauses_soft_speech_and_early_pauses_are_not_cuts() {
        let mut finder = PauseFinder::new(FRAME);
        // A long pause too soon after the start.
        assert!(finder.push(&frames(10.0, 0.9)).is_empty());
        assert!(finder.push(&frames(3.0, 0.05)).is_empty());
        // Breaths between sentences.
        for _ in 0..10 {
            assert!(finder.push(&frames(4.0, 0.9)).is_empty());
            assert!(finder.push(&frames(1.5, 0.05)).is_empty());
        }
        // A "pause" holding quiet speech, as an off-mic voice scores: not silence.
        assert!(finder.push(&frames(3.0, 0.3)).is_empty());
        assert_eq!(finder.push(&frames(2.5, 0.05)).len(), 1);
    }

    #[test]
    fn every_piece_is_at_least_the_minimum_and_a_pause_cuts_once() {
        let mut finder = PauseFinder::new(FRAME);
        let mut cuts = Vec::new();
        for _ in 0..6 {
            cuts.extend(finder.push(&frames(12.0, 0.9)));
            // One long pause, fed in small steps as the recording thread does.
            for step in frames(6.0, 0.05).chunks(8) {
                cuts.extend(finder.push(step));
            }
        }
        assert!(!cuts.is_empty());
        let mut last = 0;
        for cut in cuts {
            assert!(cut - last >= MIN_PIECE, "piece of {} s", secs(cut - last));
            last = cut;
        }
    }
}
