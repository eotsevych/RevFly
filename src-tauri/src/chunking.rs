//! Long-form chunking: audio over 40 s is transcribed in chunks cut in the middle of pauses,
//! because models lose accuracy past 30-40 s of audio in one run. Audio of 40 s or less is
//! never cut, and nothing overlaps: every cut falls in silence, so no word is split or doubled.
//!
//! The cut plan is Handy's (MIT, github.com/cjpais/Handy, `src-tauri/src/audio_toolkit/chunking.rs`),
//! with its rules and Silero thresholds unchanged. Durations are in samples of 16 kHz audio.

use std::ops::Range;
use std::sync::OnceLock;

use regex::Regex;

const RATE: usize = 16000;

const TARGET_SECONDS: f64 = 30.0;
/// Audio up to this many samples is transcribed in one piece; longer audio is cut.
pub const MAX_CHUNK_SAMPLES: usize = 40 * RATE;
const MIN_CHUNK_SAMPLES: usize = 5 * RATE;

// Silero's `get_speech_timestamps` defaults. Stricter than `vad::timestamp_config`, which is
// tuned to keep soft speech when trimming; here a pause has to be a real one.
const THRESHOLD: f32 = 0.5;
const NEG_THRESHOLD: f32 = THRESHOLD - 0.15;
const MIN_SPEECH_SAMPLES: usize = 250 * RATE / 1000;
const MIN_SILENCE_SAMPLES: usize = 100 * RATE / 1000;
const SPEECH_PAD_SAMPLES: usize = 30 * RATE / 1000;

/// How a chunk ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ending {
    /// Cut in the middle of a pause this long.
    Pause { gap_ms: u64 },
    /// 40 s of speech with no usable pause.
    HardCut,
    /// The end of the audio.
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub range: Range<usize>,
    pub ending: Ending,
}

/// The whole of `range` as one chunk.
pub fn whole(range: Range<usize>) -> Vec<Chunk> {
    vec![Chunk { range, ending: Ending::End }]
}

/// Chunks of the audio from `from` to `samples`, planned from the speech probability of every
/// `frame_samples`-sample frame from the start of the audio. In order, without gaps.
pub fn plan(probs: &[f32], frame_samples: usize, samples: usize, from: usize) -> Vec<Chunk> {
    if samples - from <= MAX_CHUNK_SAMPLES {
        return whole(from..samples);
    }
    let segments = speech_segments(probs, frame_samples, samples);
    let mut chunks = Vec::new();
    let mut start = from;
    for (cut, gap_seconds) in plan_cuts(&cut_candidates(&segments, samples), samples, from) {
        let ending = gap_seconds.map_or(Ending::HardCut, |gap| Ending::Pause { gap_ms: (gap * 1000.0) as u64 });
        chunks.push(Chunk { range: start..cut, ending });
        start = cut;
    }
    chunks.push(Chunk { range: start..samples, ending: Ending::End });
    chunks
}

/// Speech segments, in samples, from per-frame speech probabilities: Silero's
/// `get_speech_timestamps` without a maximum speech length. `samples` is the
/// length of the audio the probabilities score.
fn speech_segments(probs: &[f32], frame_samples: usize, samples: usize) -> Vec<Range<usize>> {
    let mut segments: Vec<Range<usize>> = Vec::new();
    let mut triggered = false;
    let mut start = 0;
    let mut temp_end: Option<usize> = None;
    for (i, &prob) in probs.iter().enumerate() {
        let at = i * frame_samples;
        if prob >= THRESHOLD {
            temp_end = None;
            if !triggered {
                triggered = true;
                start = at;
            }
            continue;
        }
        if prob < NEG_THRESHOLD && triggered {
            let end = *temp_end.get_or_insert(at);
            if at - end < MIN_SILENCE_SAMPLES {
                continue;
            }
            if end - start > MIN_SPEECH_SAMPLES {
                segments.push(start..end);
            }
            triggered = false;
            temp_end = None;
        }
    }
    if triggered && samples.saturating_sub(start) > MIN_SPEECH_SAMPLES {
        segments.push(start..samples);
    }

    for i in 0..segments.len() {
        if i == 0 {
            segments[0].start = segments[0].start.saturating_sub(SPEECH_PAD_SAMPLES);
        }
        if i + 1 == segments.len() {
            segments[i].end = (segments[i].end + SPEECH_PAD_SAMPLES).min(samples);
            continue;
        }
        let silence = segments[i + 1].start - segments[i].end;
        let pad = if silence < 2 * SPEECH_PAD_SAMPLES { silence / 2 } else { SPEECH_PAD_SAMPLES };
        segments[i].end = (segments[i].end + pad).min(samples);
        segments[i + 1].start = segments[i + 1].start.saturating_sub(pad);
    }
    segments
}

/// A place a chunk may end: the middle of a pause.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CutCandidate {
    cut: usize,
    gap_seconds: f64,
}

/// One candidate in the middle of each pause, lead-in and tail included.
fn cut_candidates(segments: &[Range<usize>], samples: usize) -> Vec<CutCandidate> {
    let mut candidates = Vec::new();
    let mut silence_start = 0;
    for segment in segments.iter().cloned().chain(std::iter::once(samples..samples)) {
        if segment.start > silence_start {
            candidates.push(CutCandidate {
                cut: (silence_start + segment.start) / 2,
                gap_seconds: (segment.start - silence_start) as f64 / RATE as f64,
            });
        }
        silence_start = silence_start.max(segment.end);
    }
    candidates
}

/// Greedy, left to right from `from`: each cut is the best-scoring pause
/// 5-40 s after the previous one, preferring long pauses near 30 s and ones
/// that leave at least 5 s after them. With no pause, it is a hard cut at 40 s,
/// or halfway when that would leave under 5 s. Audio of 40 s or less is never
/// cut. `candidates` must be in order. Each cut comes with its pause's length,
/// `None` for a hard cut.
fn plan_cuts(candidates: &[CutCandidate], samples: usize, from: usize) -> Vec<(usize, Option<f64>)> {
    let mut cuts = Vec::new();
    let mut start = from;
    // Each window starts after the last, so earlier candidates never return.
    let mut first = 0;
    while samples - start > MAX_CHUNK_SAMPLES {
        while first < candidates.len() && candidates[first].cut < start + MIN_CHUNK_SAMPLES {
            first += 1;
        }
        // A pause that leaves under 5 s is only used when no other pause will do.
        let mut best: Option<(CutCandidate, f64)> = None;
        let mut fallback: Option<(CutCandidate, f64)> = None;
        for candidate in candidates[first..]
            .iter()
            .take_while(|candidate| candidate.cut <= start + MAX_CHUNK_SAMPLES)
        {
            let position = (candidate.cut - start) as f64 / RATE as f64;
            let off_target = if position > TARGET_SECONDS {
                0.1 * (position - TARGET_SECONDS)
            } else {
                0.05 * (TARGET_SECONDS - position)
            };
            let score = candidate.gap_seconds.min(1.0) - off_target;
            if samples - candidate.cut >= MIN_CHUNK_SAMPLES && best.map_or(true, |(_, best)| score > best) {
                best = Some((*candidate, score));
            }
            if fallback.map_or(true, |(_, fallback)| score > fallback) {
                fallback = Some((*candidate, score));
            }
        }
        let hard_cut = if samples - start < MAX_CHUNK_SAMPLES + MIN_CHUNK_SAMPLES {
            start + (samples - start) / 2
        } else {
            start + MAX_CHUNK_SAMPLES
        };
        let cut = best.or(fallback).map_or((hard_cut, None), |(c, _)| (c.cut, Some(c.gap_seconds)));
        start = cut.0;
        cuts.push(cut);
    }
    cuts
}

/// Scripts written without spaces between words, and CJK punctuation.
fn is_unspaced(c: char) -> bool {
    static UNSPACED: OnceLock<Regex> = OnceLock::new();
    UNSPACED
        .get_or_init(|| {
            Regex::new(
                r"[\p{Han}\p{Hiragana}\p{Katakana}\p{Bopomofo}\p{Thai}\p{Lao}\p{Khmer}\p{Myanmar}\x{3000}-\x{303f}\x{ff00}-\x{ffef}]",
            )
            .unwrap()
        })
        .is_match(c.encode_utf8(&mut [0; 4]))
}

/// Chunk texts in order: a space between two chunks unless either side is
/// unspaced script.
pub fn join_transcripts<S: AsRef<str>>(texts: &[S]) -> String {
    let mut joined = String::new();
    let mut last: Option<char> = None;
    for text in texts {
        let text = text.as_ref().trim();
        let Some(first) = text.chars().next() else {
            continue;
        };
        if last.is_some_and(|last| !is_unspaced(last) && !is_unspaced(first)) {
            joined.push(' ');
        }
        joined.push_str(text);
        last = text.chars().last();
    }
    joined
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: usize = 512;

    fn s(seconds: f64) -> usize {
        (seconds * RATE as f64).round() as usize
    }

    /// Frame probabilities: speech everywhere except the given pauses, in seconds.
    fn probs_with_pauses(seconds: f64, pauses: &[(f64, f64)]) -> Vec<f32> {
        (0..s(seconds) / FRAME)
            .map(|i| {
                let at = (i * FRAME) as f64 / RATE as f64;
                if pauses.iter().any(|&(start, end)| at >= start && at < end) { 0.0 } else { 1.0 }
            })
            .collect()
    }

    /// Where chunks end, except the last, in tenths of a second.
    fn plan_secs(seconds: f64, pauses: &[(f64, f64)]) -> Vec<f64> {
        let chunks = plan(&probs_with_pauses(seconds, pauses), FRAME, s(seconds), 0);
        chunks[..chunks.len() - 1]
            .iter()
            .map(|c| (c.range.end as f64 / RATE as f64 * 10.0).round() / 10.0)
            .collect()
    }

    #[test]
    fn cuts_prefer_long_pauses_near_30s() {
        assert!(plan_secs(40.0, &[(20.0, 21.0)]).is_empty(), "40 s or less is never cut");
        // A 1 s pause at 28-29 s beats a short one right at 30 s; 41.5 s remain, so cut again.
        assert_eq!(plan_secs(70.0, &[(28.0, 29.0), (30.0, 30.2), (55.0, 56.0)]), vec![28.5, 55.5]);
        // Without any pause, cuts are hard 40 s ones.
        assert_eq!(plan_secs(100.0, &[]), vec![40.0, 80.0]);
        // A pause under 5 s in is never used.
        assert_eq!(plan_secs(60.0, &[(2.0, 3.0)]), vec![40.0]);
    }

    #[test]
    fn cuts_avoid_leaving_a_short_last_chunk() {
        // The pause at 38 s would leave 4 s, so the weaker one at 10 s wins.
        assert_eq!(plan_secs(42.0, &[(9.8, 10.2), (37.5, 38.5)]), vec![10.0]);
        // Without a pause, a short rest is split evenly instead of cut at 40 s.
        assert_eq!(plan_secs(84.0, &[]), vec![40.0, 62.0]);
        // A pause that leaves a short rest still beats cutting mid-speech.
        assert_eq!(plan_secs(40.5, &[(38.8, 39.2)]), vec![39.0]);
    }

    #[test]
    fn chunks_cover_the_audio_from_the_start_point_and_say_how_they_end() {
        // Leading silence already trimmed off: planning starts at 3 s.
        let probs = probs_with_pauses(103.0, &[(0.0, 3.0), (31.0, 32.0)]);
        let chunks = plan(&probs, FRAME, s(103.0), s(3.0));
        assert_eq!(chunks.first().unwrap().range.start, s(3.0));
        assert_eq!(chunks.last().unwrap().range.end, s(103.0));
        for pair in chunks.windows(2) {
            assert_eq!(pair[0].range.end, pair[1].range.start, "chunks must be contiguous");
        }
        assert!(matches!(chunks[0].ending, Ending::Pause { gap_ms } if (900..=1100).contains(&gap_ms)));
        assert_eq!(chunks[1].ending, Ending::HardCut, "no pause after 32 s");
        assert_eq!(chunks.last().unwrap().ending, Ending::End);
        assert!(chunks.iter().all(|c| c.range.len() <= MAX_CHUNK_SAMPLES));
    }

    #[test]
    fn joins_put_spaces_only_between_spaced_scripts() {
        assert_eq!(join_transcripts(&[" Hello there. ", "", "General Kenobi."]), "Hello there. General Kenobi.");
        assert_eq!(join_transcripts(&["今天天气很好。", "我们去公园吧。"]), "今天天气很好。我们去公园吧。");
        assert_eq!(join_transcripts(&["I said", "你好"]), "I said你好");
        assert_eq!(join_transcripts(&["Привіт.", "Як справи?"]), "Привіт. Як справи?");
    }
}
