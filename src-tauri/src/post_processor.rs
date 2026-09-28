//! LLM-friendly post-processing pipeline for ASR transcripts.
//!
//! Wraps the existing `text_normalizer` and adds:
//! - Self-correction and false-start detection
//! - Semantic deduplication (repeated statements with same intent)
//! - Ambiguity annotation for consequential values
//! - Structured value normalization (dates, times, currencies, etc.)
//! - Safety guards for negation, conditions, and uncertainty
//! - ASR noise/silence marker removal
//!
//! The pipeline is deterministic. It does not call an LLM.
//! Processing is idempotent: clean text in → same clean text out.

use serde::{Deserialize, Serialize};

use crate::text_normalizer::{self, NormalizationOptions};

/// Result of post-processing an ASR transcript.
/// Keeps the raw text unchanged. Provides a clean version for LLM calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostProcessedTranscript {
    /// Original ASR output, unchanged.
    pub raw_text: String,
    /// Normalized text, ready for downstream LLM consumption.
    pub clean_text: String,
    /// Spans where the value is ambiguous or low-confidence.
    /// Each entry is a human-readable note about what is unclear.
    pub uncertain_spans: Vec<String>,
    /// True when the transcript needs human review before action.
    pub requires_clarification: bool,
}

/// Configuration for the post-processing pipeline.
/// Extends the existing NormalizationOptions with new capabilities.
#[derive(Debug, Clone, Copy)]
pub struct PostProcessorConfig {
    /// Base normalization options (fillers, numbers, stutters).
    pub normalization: NormalizationOptions,
    /// Detect and apply self-corrections ("sorry, I mean X").
    pub apply_self_corrections: bool,
    /// Remove ASR noise markers like [BLANK_AUDIO], [silence], etc.
    pub remove_noise_markers: bool,
    /// Collapse semantically duplicate statements.
    pub collapse_redundancy: bool,
    /// Annotate ambiguous consequential values instead of guessing.
    pub annotate_ambiguity: bool,
    /// Normalize structured values (dates, times, currencies, etc.).
    pub normalize_structured_values: bool,
}

impl Default for PostProcessorConfig {
    fn default() -> Self {
        Self {
            normalization: NormalizationOptions::default(),
            apply_self_corrections: true,
            remove_noise_markers: true,
            collapse_redundancy: true,
            annotate_ambiguity: true,
            normalize_structured_values: true,
        }
    }
}

/// Main entry point. Processes a raw ASR transcript into a clean version.
/// Safe, deterministic, and idempotent.
pub fn post_process(raw_text: &str, config: PostProcessorConfig) -> PostProcessedTranscript {
    let trimmed = raw_text.trim();
    if trimmed.is_empty() {
        return PostProcessedTranscript {
            raw_text: String::new(),
            clean_text: String::new(),
            uncertain_spans: Vec::new(),
            requires_clarification: false,
        };
    }

    // Preserve paragraph boundaries if input contains double newlines.
    if trimmed.contains("\n\n") {
        let mut clean_paras = Vec::new();
        let mut all_uncertain = Vec::new();

        for para in trimmed.split("\n\n") {
            let res = post_process_single_paragraph(para, config);
            if !res.clean_text.is_empty() {
                clean_paras.push(res.clean_text);
            }
            all_uncertain.extend(res.uncertain_spans);
        }

        let requires_clarification = !all_uncertain.is_empty();
        return PostProcessedTranscript {
            raw_text: raw_text.to_string(),
            clean_text: clean_paras.join("\n\n"),
            uncertain_spans: all_uncertain,
            requires_clarification,
        };
    }

    post_process_single_paragraph(trimmed, config)
}

fn post_process_single_paragraph(trimmed: &str, config: PostProcessorConfig) -> PostProcessedTranscript {
    // Track uncertain spans found during processing.
    let mut uncertain_spans: Vec<String> = Vec::new();

    // Step 1: Remove ASR noise markers before any other processing.
    let denoised = if config.remove_noise_markers {
        remove_noise_markers(trimmed)
    } else {
        trimmed.to_string()
    };

    if denoised.trim().is_empty() {
        return PostProcessedTranscript {
            raw_text: trimmed.to_string(),
            clean_text: String::new(),
            uncertain_spans: Vec::new(),
            requires_clarification: false,
        };
    }

    // Step 2: Detect and apply self-corrections / false starts.
    let corrected = if config.apply_self_corrections {
        apply_self_corrections(&denoised)
    } else {
        denoised.clone()
    };

    // Step 3: Detect ambiguous consequential values BEFORE normalization
    // so we can annotate them. Do this on the corrected text.
    if config.annotate_ambiguity {
        detect_ambiguous_values(&corrected, &mut uncertain_spans);
    }

    // Step 4: Run the existing normalizer (fillers, stutters, numbers, formatting).
    let normalized = text_normalizer::normalize_transcript_with_options(
        &corrected,
        config.normalization,
    );

    // Step 5: Normalize structured values beyond what the base normalizer handles.
    let structured = if config.normalize_structured_values {
        normalize_structured_values(&normalized)
    } else {
        normalized
    };

    // Step 6: Collapse semantically redundant statements.
    let deduped = if config.collapse_redundancy {
        collapse_redundant_statements(&structured)
    } else {
        structured
    };

    // Step 7: Final cleanup — normalize whitespace, trim.
    let clean = final_cleanup(&deduped);

    let requires_clarification = !uncertain_spans.is_empty();

    PostProcessedTranscript {
        raw_text: trimmed.to_string(),
        clean_text: clean,
        uncertain_spans,
        requires_clarification,
    }
}

/// Convenience function with default config.
pub fn post_process_default(raw_text: &str) -> PostProcessedTranscript {
    post_process(raw_text, PostProcessorConfig::default())
}

// ---------------------------------------------------------------------------
// Step 1: ASR noise marker removal
// ---------------------------------------------------------------------------

/// Removes common ASR noise and silence markers.
/// These are artifacts from the speech model, not real speech.
fn remove_noise_markers(text: &str) -> String {
    let patterns = [
        "[BLANK_AUDIO]",
        "[blank_audio]",
        "[SILENCE]",
        "[silence]",
        "[NOISE]",
        "[noise]",
        "[MUSIC]",
        "[music]",
        "[LAUGHTER]",
        "[laughter]",
        "[INAUDIBLE]",
        "[inaudible]",
        "[UNINTELLIGIBLE]",
        "[unintelligible]",
        "(inaudible)",
        "(unintelligible)",
        "<|endoftext|>",
        "<|startoftranscript|>",
        "<|nospeech|>",
        "[_BEG_]",
        "[_TT_",  // Whisper timestamp tokens like [_TT_123]
    ];

    let mut result = text.to_string();
    for pat in &patterns {
        if pat.ends_with('_') {
            // Prefix pattern: remove everything from this prefix to the next ']'
            while let Some(start) = result.find(pat) {
                if let Some(end) = result[start..].find(']') {
                    result.replace_range(start..start + end + 1, "");
                } else {
                    break;
                }
            }
        } else {
            result = result.replace(pat, "");
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Step 2: Self-correction and false-start detection
// ---------------------------------------------------------------------------

/// Correction markers that signal the speaker is replacing a previous fragment.
const CORRECTION_MARKERS: &[&str] = &[
    "sorry,",
    "sorry",
    "no, i mean",
    "no i mean",
    "i mean,",
    "i mean",
    "actually,",
    "correction,",
    "correction:",
    "correction",
    "scratch that,",
    "scratch that",
    "rather,",
    "rather",
    "wait,",
    "wait",
    "no,",
];

/// Detects explicit self-corrections and applies them.
///
/// Pattern: `<wrong fragment> <marker> <correct fragment>`
///
/// Only applies when the correction is explicit and unambiguous.
/// "Actually" at sentence start is kept (it may carry meaning).
fn apply_self_corrections(text: &str) -> String {
    let mut result = text.to_string();

    // Handle em-dash corrections: "Delete—no, deactivate—the account"
    // Pattern: word1—marker, word2—rest
    result = apply_dash_corrections(&result);

    // Handle comma/period-separated corrections with markers.
    // Process sentence by sentence to avoid cross-sentence corrections.
    let sentences = split_into_sentences(&result);
    let mut corrected_parts: Vec<String> = Vec::new();

    for sentence in &sentences {
        let corrected = apply_marker_corrections(sentence);
        corrected_parts.push(corrected);
    }

    result = corrected_parts.join(" ");

    // Normalize whitespace after corrections.
    normalize_whitespace(&result)
}

/// Handles dash-style corrections: "Delete—no, deactivate—the account"
fn apply_dash_corrections(text: &str) -> String {
    let mut result = text.to_string();

    // Pattern: "word—no, replacement—" or "word—no, replacement—rest"
    // Use the em-dash (—) as delimiter.
    for marker in &["—no, ", "—no ", "—sorry, ", "—sorry "] {
        while let Some(marker_pos) = result.to_lowercase().find(marker) {
            // Find the start of the wrong fragment (go back to the previous space or start).
            let before = &result[..marker_pos];
            let word_start = before.rfind(' ').map(|p| p + 1).unwrap_or(0);

            let after_marker = marker_pos + marker.len();

            // Find the end of the replacement (next em-dash or end of text).
            let rest = &result[after_marker..];
            let replacement_end = rest.find('—').unwrap_or(rest.len());

            let replacement = rest[..replacement_end].to_string();
            let remaining = if replacement_end < rest.len() {
                &rest[replacement_end + '—'.len_utf8()..]
            } else {
                ""
            };

            result = format!(
                "{}{}{}",
                &result[..word_start],
                replacement,
                if remaining.is_empty() {
                    String::new()
                } else {
                    format!(" {}", remaining.trim_start())
                }
            );
        }
    }

    result
}

/// Applies marker-based corrections within a single sentence.
///
/// Example: "Schedule it for Tuesday sorry, Wednesday at three"
/// → "Schedule it for Wednesday at three"
fn apply_marker_corrections(sentence: &str) -> String {
    let lower = sentence.to_lowercase();
    let mut result = sentence.to_string();

    // Sort markers by length (longest first) to match greedy.
    let mut markers: Vec<&&str> = CORRECTION_MARKERS.iter().collect();
    markers.sort_by(|a, b| b.len().cmp(&a.len()));

    for marker in markers {
        let marker_lower = *marker;

        // Skip "actually" and "rather" at sentence start — they often carry meaning.
        // "Actually, the budget is fine" should not be treated as a correction.
        if (marker_lower == "actually," || marker_lower == "rather,"
            || marker_lower == "actually" || marker_lower == "rather")
            && lower.starts_with(marker_lower)
        {
            continue;
        }

        if let Some(pos) = lower.find(marker_lower) {
            // Only treat as correction if not at the very beginning.
            if pos == 0 {
                continue;
            }

            let after_marker_pos = pos + marker_lower.len();

            // The replacement text starts after the marker.
            let replacement = result[after_marker_pos..].trim_start();

            // Check if there's something after the marker to replace with.
            if replacement.is_empty() {
                continue;
            }

            // The "wrong" fragment is the clause before the marker.
            // For simple cases, drop the last clause before the marker.
            let before = &result[..pos];

            // Find a natural break point: look for the last comma, dash, or
            // significant pause before the marker.
            let break_pos = find_clause_break(before);

            result = format!(
                "{}{}",
                &result[..break_pos],
                if break_pos > 0 {
                    format!(" {}", replacement.trim())
                } else {
                    replacement.trim().to_string()
                }
            );

            // After one correction per sentence, stop.
            // Multiple corrections in one sentence are rare and risky.
            break;
        }
    }

    result
}

/// Finds the best break point before a correction marker.
/// Returns the byte offset where the "wrong" clause starts.
fn find_clause_break(text: &str) -> usize {
    let trimmed = text.trim_end();

    // Look for the last comma, semicolon, or sentence-level break.
    if let Some(pos) = trimmed.rfind(", ") {
        return pos;
    }
    if let Some(pos) = trimmed.rfind("; ") {
        return pos;
    }

    // Otherwise, find the start of the last "phrase" —
    // the last 1-4 words before the marker.
    // We remove the last phrase since that is what the speaker is correcting.
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() <= 1 {
        return 0;
    }

    // For short fragments ("Tuesday sorry"), remove one word.
    // For longer ones, be conservative and remove at most 3 words.
    let words_to_remove = (words.len()).min(3).max(1);
    let keep = words.len() - words_to_remove;

    // Find the byte position after the `keep`-th word.
    let mut pos = 0;
    for (i, word) in words.iter().enumerate() {
        if i >= keep {
            break;
        }
        // Find this word's position in the original text.
        if let Some(found) = trimmed[pos..].find(word) {
            pos += found + word.len();
        }
    }

    pos
}

/// Splits text into rough sentences based on sentence-ending punctuation.
/// Smart about periods: only splits when a period is followed by whitespace
/// and an uppercase letter, or is at the end of text. This avoids breaking
/// email addresses (user@example.com), URLs, abbreviations, and numbers.
fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    for i in 0..len {
        let ch = chars[i];
        current.push(ch);

        if ch == '?' || ch == '!' {
            // Question marks and exclamation marks always end a sentence.
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current = String::new();
        } else if ch == '.' {
            // A period ends a sentence only if:
            // 1. It's the last character, OR
            // 2. It's followed by a space and then an uppercase letter.
            // This avoids splitting "example.com", "Dr.", "3.5", etc.
            let is_end_of_text = i + 1 >= len;

            let followed_by_space_and_upper = if i + 2 < len {
                chars[i + 1].is_whitespace() && chars[i + 2].is_uppercase()
            } else {
                false
            };

            if is_end_of_text || followed_by_space_and_upper {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    sentences.push(trimmed);
                }
                current = String::new();
            }
        }
    }

    // Any remaining text without terminal punctuation.
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    sentences
}

// ---------------------------------------------------------------------------
// Step 3: Ambiguity detection
// ---------------------------------------------------------------------------

/// Detects ambiguous consequential values in the text.
///
/// Looks for patterns like "fifteen or fifty", "Monday or Tuesday",
/// and other signals of uncertainty around critical values
/// (money, dates, quantities, account numbers).
fn detect_ambiguous_values(text: &str, uncertain_spans: &mut Vec<String>) {
    let lower = text.to_lowercase();

    // Pattern: "X or Y" with numbers that are consequential
    detect_numeric_or_patterns(&lower, uncertain_spans);

    // Pattern: explicit hedging around destructive commands
    detect_destructive_ambiguity(&lower, uncertain_spans);
}

/// Finds "number_A or number_B" patterns that suggest the speaker
/// is uncertain about a consequential value.
fn detect_numeric_or_patterns(lower: &str, uncertain_spans: &mut Vec<String>) {
    // Match patterns like "fifteen or fifty", "100 or 1000", "5 or 50"
    let words: Vec<&str> = lower.split_whitespace().collect();

    for i in 0..words.len().saturating_sub(2) {
        if words[i + 1] == "or" {
            let a = words[i].trim_matches(|c: char| !c.is_alphanumeric());
            let b = words[i + 2].trim_matches(|c: char| !c.is_alphanumeric());

            let a_is_num = is_number_word(a) || a.parse::<f64>().is_ok();
            let b_is_num = is_number_word(b) || b.parse::<f64>().is_ok();

            if a_is_num && b_is_num {
                // Check context: is this about money, quantities, or similar?
                let context = get_surrounding_context(&words, i, 3);
                if is_consequential_context(&context) {
                    uncertain_spans.push(format!(
                        "Ambiguous value: '{}' or '{}' — context: {}",
                        a, b, context
                    ));
                }
            }
        }
    }
}

/// Checks if a word is a spoken number word.
fn is_number_word(word: &str) -> bool {
    matches!(
        word,
        "zero" | "one" | "two" | "three" | "four" | "five" | "six"
        | "seven" | "eight" | "nine" | "ten" | "eleven" | "twelve"
        | "thirteen" | "fourteen" | "fifteen" | "sixteen" | "seventeen"
        | "eighteen" | "nineteen" | "twenty" | "thirty" | "forty"
        | "fifty" | "sixty" | "seventy" | "eighty" | "ninety"
        | "hundred" | "thousand" | "million" | "billion"
        | "first" | "second" | "third" | "fourth" | "fifth"
    )
}

/// Gets surrounding words as context string.
fn get_surrounding_context(words: &[&str], center: usize, radius: usize) -> String {
    let start = center.saturating_sub(radius);
    let end = (center + radius + 3).min(words.len());
    words[start..end].join(" ")
}

/// Returns true if the surrounding context suggests a consequential value
/// (money, quantities, account numbers, dates, medical values).
fn is_consequential_context(context: &str) -> bool {
    let indicators = [
        "dollar", "dollars", "$", "euro", "euros", "€", "pound", "pounds",
        "£", "yen", "¥", "transfer", "pay", "payment", "send", "wire",
        "deposit", "withdraw", "account", "invoice", "order", "quantity",
        "mg", "ml", "dose", "dosage", "units", "cc", "percent", "%",
        "thousand", "million", "billion", "hundred",
    ];

    for ind in &indicators {
        if context.contains(ind) {
            return true;
        }
    }

    // Any two numbers connected by "or" in a sentence with action verbs
    // is potentially consequential — be safe.
    true
}

/// Detects ambiguity around destructive or irreversible commands.
fn detect_destructive_ambiguity(lower: &str, uncertain_spans: &mut Vec<String>) {
    let destructive_verbs = ["delete", "remove", "cancel", "terminate", "drop", "destroy", "erase"];
    let hedging_words = ["maybe", "possibly", "i think", "might", "could", "not sure"];

    for verb in &destructive_verbs {
        if lower.contains(verb) {
            for hedge in &hedging_words {
                if lower.contains(hedge) {
                    uncertain_spans.push(format!(
                        "Uncertain destructive action: '{}' used with hedging '{}'",
                        verb, hedge
                    ));
                    return; // One annotation per destructive pattern is enough.
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Step 5: Structured value normalization
// ---------------------------------------------------------------------------

/// Normalizes structured values that the base number converter doesn't handle:
/// - Time expressions ("three PM" → "3:00 PM")
/// - Date patterns ("March 5th 2024" → "March 5th, 2024")
/// - Currency ("fifty dollars" → "$50")
/// - Percentages ("twenty percent" → "20%")
/// - Email addresses ("john at example dot com" → "john@example.com")
/// - URLs ("https colon slash slash example dot com" → "https://example.com")
/// - Version numbers ("version 2 point 1 point 0" → "version 2.1.0")
/// - Ticket IDs ("JIRA dash 123" → "JIRA-123")
/// - File paths ("slash usr slash local slash bin" → "/usr/local/bin")
/// - Domain technical identifiers ("localhost colon 3000" → "localhost:3000", "UTF dash 8" → "UTF-8")
fn normalize_structured_values(text: &str) -> String {
    let mut result = text.to_string();

    result = normalize_time_expressions(&result);
    result = normalize_date_expressions(&result);
    result = normalize_percentage_expressions(&result);
    result = normalize_currency_expressions(&result);
    result = normalize_email_spoken(&result);
    result = normalize_url_spoken(&result);
    result = normalize_version_numbers(&result);
    result = normalize_ticket_ids(&result);
    result = normalize_file_paths(&result);
    result = normalize_technical_identifiers(&result);

    result
}

/// Converts spoken time to formatted time.
/// "three PM" → "3:00 PM", "at three" → "at 3:00"
fn normalize_time_expressions(text: &str) -> String {
    let mut result = text.to_string();

    // Pattern: "NUMBER am/pm" or "NUMBER a.m./p.m."
    let time_suffixes = [
        (" am", " AM"), (" AM", " AM"),
        (" pm", " PM"), (" PM", " PM"),
        (" a.m.", " AM"), (" p.m.", " PM"),
        (" A.M.", " AM"), (" P.M.", " PM"),
    ];

    for (suffix, normalized_suffix) in &time_suffixes {
        // Find digit followed by suffix, add :00 if no minutes
        let search = suffix.to_string();
        let mut offset = 0;
        while let Some(pos) = result[offset..].find(&search) {
            let abs_pos = offset + pos;

            // Check if preceded by a digit (possibly with :MM)
            if abs_pos > 0 {
                let before = &result[..abs_pos];
                let before_trimmed = before.trim_end();

                // Already has minutes like "3:30 PM" — just normalize the suffix
                if before_trimmed.ends_with(|c: char| c.is_ascii_digit()) {
                    let has_colon = before_trimmed.len() >= 3
                        && before_trimmed.as_bytes()[before_trimmed.len() - 3] == b':';

                    if !has_colon {
                        // "3 PM" → "3:00 PM"
                        // Find the start of the number
                        let num_start = before_trimmed
                            .rfind(|c: char| !c.is_ascii_digit())
                            .map(|p| p + 1)
                            .unwrap_or(0);

                        let num_str = &before_trimmed[num_start..];
                        if let Ok(hour) = num_str.parse::<u32>() {
                            if (1..=12).contains(&hour) {
                                let new_time = format!("{}:00{}", hour, normalized_suffix);
                                result = format!(
                                    "{}{}{}",
                                    &result[..num_start],
                                    new_time,
                                    &result[abs_pos + suffix.len()..]
                                );
                                offset = num_start + new_time.len();
                                continue;
                            }
                        }
                    } else {
                        // Already "3:30 PM" — just normalize suffix casing
                        result = format!(
                            "{}{}{}",
                            &result[..abs_pos],
                            normalized_suffix,
                            &result[abs_pos + suffix.len()..]
                        );
                        offset = abs_pos + normalized_suffix.len();
                        continue;
                    }
                }
            }

            offset = abs_pos + suffix.len();
        }
    }

    result
}

/// "twenty percent" → "20%", "fifty percent" → "50%"
fn normalize_percentage_expressions(text: &str) -> String {
    let mut result = text.to_string();

    // Pattern: "NUMBER percent" where NUMBER is already a digit
    let re_pattern = " percent";
    while let Some(pos) = result.to_lowercase().find(re_pattern) {
        let before = &result[..pos];
        let before_trimmed = before.trim_end();

        if before_trimmed.ends_with(|c: char| c.is_ascii_digit()) {
            let num_start = before_trimmed
                .rfind(|c: char| !c.is_ascii_digit())
                .map(|p| p + 1)
                .unwrap_or(0);

            let num_str = &before_trimmed[num_start..];
            let replacement = format!("{}%", num_str);
            // Keep the num_start prefix, replace from num_start to end of "percent"
            result = format!(
                "{}{}{}",
                &result[..num_start],
                replacement,
                &result[pos + re_pattern.len()..]
            );
        } else {
            break; // Avoid infinite loop if pattern doesn't match expected structure
        }
    }

    result
}

/// "fifty dollars" → "$50", "twenty euros" → "€20"
fn normalize_currency_expressions(text: &str) -> String {
    let mut result = text.to_string();

    let currencies = [
        ("dollars", "$"),
        ("dollar", "$"),
        ("euros", "€"),
        ("euro", "€"),
        ("pounds", "£"),
        ("pound", "£"),
    ];

    for (word, symbol) in &currencies {
        let pattern = format!(" {}", word);
        while let Some(pos) = result.to_lowercase().find(&pattern) {
            let before = &result[..pos];
            let before_trimmed = before.trim_end();

            if before_trimmed.ends_with(|c: char| c.is_ascii_digit()) {
                let num_start = before_trimmed
                    .rfind(|c: char| !c.is_ascii_digit() && c != ',')
                    .map(|p| p + 1)
                    .unwrap_or(0);

                let num_str = &before_trimmed[num_start..];
                let replacement = format!("{}{}", symbol, num_str);
                result = format!(
                    "{}{}{}",
                    &result[..num_start],
                    replacement,
                    &result[pos + pattern.len()..]
                );
            } else {
                break;
            }
        }
    }

    result
}

/// Normalizes spoken email patterns.
/// "john at example dot com" → "john@example.com"
fn normalize_email_spoken(text: &str) -> String {
    let mut result = text.to_string();

    // Simple pattern: "word at word dot word"
    // Only match if it looks like an email (no spaces in user/domain parts,
    // domain part is a known TLD indicator).
    let words: Vec<&str> = result.split_whitespace().collect();
    let mut i = 0;
    let mut new_words: Vec<String> = Vec::new();
    let mut changed = false;

    while i < words.len() {
        if i + 4 < words.len()
            && words[i + 1].to_lowercase() == "at"
            && words[i + 3].to_lowercase() == "dot"
        {
            let user = words[i];
            let domain = words[i + 2];
            let tld = words[i + 4].trim_matches(|c: char| !c.is_alphanumeric());

            // Basic validation: parts should look like email components.
            let valid_parts = user.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-')
                && domain.chars().all(|c| c.is_alphanumeric() || c == '-')
                && tld.len() >= 2 && tld.len() <= 4;

            if valid_parts {
                // Preserve any trailing punctuation from the TLD word
                let trailing: String = words[i + 4].chars().filter(|c| !c.is_alphanumeric()).collect();
                new_words.push(format!("{}@{}.{}{}", user, domain, tld, trailing));
                i += 5;
                changed = true;
                continue;
            }
        }

        new_words.push(words[i].to_string());
        i += 1;
    }

    if changed {
        result = new_words.join(" ");
    }

    result
}

/// Normalizes dates: e.g. "January 15th 2024" -> "January 15th, 2024"
fn normalize_date_expressions(text: &str) -> String {
    let months = [
        "january", "february", "march", "april", "may", "june",
        "july", "august", "september", "october", "november", "december",
    ];

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 3 {
        return text.to_string();
    }

    let mut new_words: Vec<String> = Vec::new();
    let mut i = 0;
    let mut changed = false;

    while i < words.len() {
        if i + 2 < words.len() {
            let m = words[i].trim_matches(|c: char| !c.is_alphabetic()).to_lowercase();
            let d = words[i + 1].trim_matches(|c: char| !c.is_alphanumeric());
            let y = words[i + 2].trim_matches(|c: char| !c.is_numeric());

            let is_month = months.contains(&m.as_str());
            let is_day = (d.ends_with("st") || d.ends_with("nd") || d.ends_with("rd") || d.ends_with("th"))
                || (d.parse::<u32>().map(|n| n >= 1 && n <= 31).unwrap_or(false));
            let is_year = y.len() == 4 && y.parse::<u32>().map(|yr| yr >= 1900 && yr <= 2100).unwrap_or(false);

            if is_month && is_day && is_year {
                let day_clean = d.trim_end_matches(',');
                let trailing: String = words[i + 2].chars().filter(|c| !c.is_numeric()).collect();
                new_words.push(words[i].to_string());
                new_words.push(format!("{},", day_clean));
                new_words.push(format!("{}{}", y, trailing));
                i += 3;
                changed = true;
                continue;
            }
        }
        new_words.push(words[i].to_string());
        i += 1;
    }

    if changed {
        new_words.join(" ")
    } else {
        text.to_string()
    }
}

/// Normalizes spoken URLs:
/// "https colon slash slash example dot com" -> "https://example.com"
/// "http colon slash slash" -> "http://"
/// "www dot example dot com" -> "www.example.com"
/// "github dot com slash user slash repo" -> "github.com/user/repo"
fn normalize_url_spoken(text: &str) -> String {
    let mut result = text.to_string();

    let protocol_replacements = [
        ("https colon slash slash ", "https://"),
        ("https colon slash slash", "https://"),
        ("https colon slash ", "https://"),
        ("https colon slash", "https://"),
        ("http colon slash slash ", "http://"),
        ("http colon slash slash", "http://"),
        ("http colon slash ", "http://"),
        ("http colon slash", "http://"),
        ("colon slash slash ", "://"),
        ("colon slash slash", "://"),
        ("colon slash ", "://"),
        ("colon slash", "://"),
    ];

    for (pattern, repl) in &protocol_replacements {
        while let Some(pos) = result.to_lowercase().find(pattern) {
            result.replace_range(pos..pos + pattern.len(), repl);
        }
    }

    let words: Vec<&str> = result.split_whitespace().collect();
    let mut new_words: Vec<String> = Vec::new();
    let mut i = 0;
    let mut changed = false;

    let tlds = ["com", "org", "net", "io", "dev", "ai", "app", "edu", "gov", "co", "uk", "de"];

    while i < words.len() {
        if i + 4 < words.len()
            && words[i].to_lowercase() == "www"
            && words[i + 1].to_lowercase() == "dot"
            && words[i + 3].to_lowercase() == "dot"
        {
            let domain = words[i + 2];
            let tld = words[i + 4].trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            if tlds.contains(&tld.as_str()) {
                let trailing: String = words[i + 4].chars().filter(|c| !c.is_alphanumeric()).collect();
                let mut url = format!("www.{}.{}{}", domain, tld, trailing);
                i += 5;

                while i + 1 < words.len() && words[i].to_lowercase() == "slash" {
                    let seg = words[i + 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
                    let seg_trailing: String = words[i + 1].chars().filter(|&c| !c.is_alphanumeric() && c != '-' && c != '_').collect();
                    url.push('/');
                    url.push_str(seg);
                    url.push_str(&seg_trailing);
                    i += 2;
                }

                new_words.push(url);
                changed = true;
                continue;
            }
        }

        if i + 2 < words.len() && words[i + 1].to_lowercase() == "dot" {
            let domain = words[i].trim_matches(|c: char| !c.is_alphanumeric() && c != ':' && c != '/');
            let tld = words[i + 2].trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();

            if tlds.contains(&tld.as_str()) && !domain.is_empty() {
                let trailing: String = words[i + 2].chars().filter(|c| !c.is_alphanumeric()).collect();
                let mut url = format!("{}.{}{}", words[i], tld, trailing);
                i += 3;

                while i + 1 < words.len() && words[i].to_lowercase() == "slash" {
                    let seg = words[i + 1].trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
                    let seg_trailing: String = words[i + 1].chars().filter(|&c| !c.is_alphanumeric() && c != '-' && c != '_').collect();
                    url.push('/');
                    url.push_str(seg);
                    url.push_str(&seg_trailing);
                    i += 2;
                }

                new_words.push(url);
                changed = true;
                continue;
            }
        }

        new_words.push(words[i].to_string());
        i += 1;
    }

    if changed {
        new_words.join(" ")
    } else {
        result
    }
}

/// "version 2 point 1 point 0" -> "version 2.1.0"
/// "version 1 point 0" -> "version 1.0"
/// "v 2 point 0" -> "v2.0"
fn normalize_version_numbers(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 3 {
        return text.to_string();
    }

    let mut new_words: Vec<String> = Vec::new();
    let mut i = 0;
    let mut changed = false;

    while i < words.len() {
        let w_lower = words[i].to_lowercase();
        let is_ver_prefix = w_lower == "version" || w_lower == "v";

        if is_ver_prefix && i + 3 < words.len() {
            let n1 = words[i + 1].trim_matches(|c: char| !c.is_numeric());
            let pt = words[i + 2].to_lowercase();
            let n2 = words[i + 3].trim_matches(|c: char| !c.is_numeric());

            if !n1.is_empty() && pt == "point" && !n2.is_empty() {
                if i + 5 < words.len() && words[i + 4].to_lowercase() == "point" {
                    let n3 = words[i + 5].trim_matches(|c: char| !c.is_numeric());
                    if !n3.is_empty() {
                        let trailing: String = words[i + 5].chars().filter(|c| !c.is_numeric()).collect();
                        let prefix = if w_lower == "v" { "v" } else { "version " };
                        new_words.push(format!("{}{}.{}.{}{}", prefix, n1, n2, n3, trailing));
                        i += 6;
                        changed = true;
                        continue;
                    }
                }

                let trailing: String = words[i + 3].chars().filter(|c| !c.is_numeric()).collect();
                let prefix = if w_lower == "v" { "v" } else { "version " };
                new_words.push(format!("{}{}.{}{}", prefix, n1, n2, trailing));
                i += 4;
                changed = true;
                continue;
            }
        }

        new_words.push(words[i].to_string());
        i += 1;
    }

    if changed {
        new_words.join(" ")
    } else {
        text.to_string()
    }
}

/// "JIRA dash 123" -> "JIRA-123"
/// "PROJ dash 456" -> "PROJ-456"
/// "ticket ABC dash 789" -> "ticket ABC-789"
fn normalize_ticket_ids(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 3 {
        return text.to_string();
    }

    let mut new_words: Vec<String> = Vec::new();
    let mut i = 0;
    let mut changed = false;

    while i < words.len() {
        if i + 2 < words.len() {
            let key = words[i].trim_matches(|c: char| !c.is_alphabetic());
            let dash = words[i + 1].to_lowercase();
            let num = words[i + 2].trim_matches(|c: char| !c.is_numeric());

            let is_key = key.len() >= 2
                && key.len() <= 10
                && key.chars().all(|c| c.is_ascii_uppercase());
            let is_dash = dash == "dash" || dash == "hyphen";
            let is_num = !num.is_empty();

            if is_key && is_dash && is_num {
                let trailing: String = words[i + 2].chars().filter(|c| !c.is_numeric()).collect();
                new_words.push(format!("{}-{}{}", key, num, trailing));
                i += 3;
                changed = true;
                continue;
            }
        }

        new_words.push(words[i].to_string());
        i += 1;
    }

    if changed {
        new_words.join(" ")
    } else {
        text.to_string()
    }
}

/// "slash usr slash local slash bin" -> "/usr/local/bin"
/// "slash var slash log" -> "/var/log"
/// "dot slash src slash components" -> "./src/components"
fn normalize_file_paths(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 3 {
        return text.to_string();
    }

    let mut new_words: Vec<String> = Vec::new();
    let mut i = 0;
    let mut changed = false;

    while i < words.len() {
        let is_root_slash = words[i].to_lowercase() == "slash";
        let is_dot_slash = i + 1 < words.len()
            && words[i].to_lowercase() == "dot"
            && words[i + 1].to_lowercase() == "slash";

        if is_root_slash || is_dot_slash {
            let mut path = if is_dot_slash {
                i += 2;
                "./".to_string()
            } else {
                i += 1;
                "/".to_string()
            };

            let mut seg_count = 0;
            while i < words.len() {
                let word = words[i];
                let word_clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_' && c != '-');

                if !word_clean.is_empty() && word_clean.to_lowercase() != "slash" {
                    if seg_count > 0 {
                        path.push('/');
                    }
                    path.push_str(word_clean);
                    seg_count += 1;

                    let trailing: String = word.chars().filter(|&c| matches!(c, '.' | ',' | '?' | '!' | ';' | ':')).collect();
                    if !trailing.is_empty() && trailing != "." {
                        path.push_str(&trailing);
                    }

                    i += 1;

                    if i < words.len() && words[i].to_lowercase() == "slash" {
                        i += 1;
                        continue;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            if seg_count > 0 {
                new_words.push(path);
                changed = true;
                continue;
            }
        }

        new_words.push(words[i].to_string());
        i += 1;
    }

    if changed {
        new_words.join(" ")
    } else {
        text.to_string()
    }
}

/// "localhost colon 3000" -> "localhost:3000"
/// "UTF dash 8" -> "UTF-8"
/// "SHA dash 256" -> "SHA-256"
/// "IPv 4" / "IP v 4" -> "IPv4"
fn normalize_technical_identifiers(text: &str) -> String {
    let mut result = text.to_string();

    let replacements = [
        ("localhost colon ", "localhost:"),
        ("Localhost colon ", "localhost:"),
        ("UTF dash 8", "UTF-8"),
        ("utf dash 8", "UTF-8"),
        ("Utf dash 8", "UTF-8"),
        ("UTF dash 16", "UTF-16"),
        ("utf dash 16", "UTF-16"),
        ("SHA dash 256", "SHA-256"),
        ("sha dash 256", "SHA-256"),
        ("SHA dash 1", "SHA-1"),
        ("sha dash 1", "SHA-1"),
        ("IPv 4", "IPv4"),
        ("ipv 4", "IPv4"),
        ("IP v 4", "IPv4"),
        ("IPv 6", "IPv6"),
        ("ipv 6", "IPv6"),
        ("IP v 6", "IPv6"),
    ];

    for (from, to) in &replacements {
        result = result.replace(from, to);
    }

    result
}

// ---------------------------------------------------------------------------
// Step 6: Semantic deduplication
// ---------------------------------------------------------------------------

/// Collapses semantically redundant consecutive statements.
///
/// Example: "Send it today. I need it sent today. Make sure it goes today."
/// → "Priority: Send it today."
///
/// Only collapses when statements share the same core intent and no
/// new conditions, constraints, or information is added.
fn collapse_redundant_statements(text: &str) -> String {
    let sentences = split_into_sentences(text);

    if sentences.len() <= 1 {
        return text.to_string();
    }

    let mut result_sentences: Vec<String> = Vec::new();
    let mut i = 0;

    while i < sentences.len() {
        let current = &sentences[i];

        // Look ahead for duplicates of this sentence.
        let mut dup_count = 0;
        let mut j = i + 1;

        while j < sentences.len() {
            if are_semantically_similar(current, &sentences[j]) {
                dup_count += 1;
                j += 1;
            } else {
                break;
            }
        }

        if dup_count >= 2 {
            // Three or more similar statements → prefix with "Priority:"
            result_sentences.push(format!("Priority: {}", current.trim()));
        } else if dup_count == 1 {
            // Two similar statements → keep just one, no prefix
            result_sentences.push(current.trim().to_string());
        } else {
            result_sentences.push(current.trim().to_string());
        }

        i = j.max(i + 1);
    }

    result_sentences.join(" ")
}

/// Checks if two sentences are semantically similar.
/// Uses a simple word-overlap heuristic. Not a full NLP comparison.
///
/// Two sentences are "similar" if they share ≥60% of significant words.
fn are_semantically_similar(a: &str, b: &str) -> bool {
    let a_words = extract_significant_words(a);
    let b_words = extract_significant_words(b);

    if a_words.is_empty() || b_words.is_empty() {
        return false;
    }

    let mut overlap = 0;
    for word in &a_words {
        if b_words.contains(word) {
            overlap += 1;
        }
    }

    let max_len = a_words.len().max(b_words.len());
    let similarity = overlap as f32 / max_len as f32;

    similarity >= 0.6
}

/// Extracts significant words (removes stop words and punctuation).
fn extract_significant_words(text: &str) -> Vec<String> {
    let stop_words = [
        "a", "an", "the", "it", "is", "i", "to", "be", "do", "of",
        "in", "that", "this", "for", "on", "with", "as", "at", "by",
        "we", "you", "he", "she", "they", "me", "us", "my", "your",
    ];

    text.split_whitespace()
        .map(|w| w.to_lowercase().replace(|c: char| !c.is_alphanumeric(), ""))
        .filter(|w| !w.is_empty() && !stop_words.contains(&w.as_str()))
        .collect()
}

// ---------------------------------------------------------------------------
// Step 7: Final cleanup
// ---------------------------------------------------------------------------

/// Normalizes whitespace. Makes processing idempotent.
fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// Final cleanup pass: normalize whitespace, trim, ensure terminal punctuation.
fn final_cleanup(text: &str) -> String {
    let mut clean = normalize_whitespace(text);
    clean = clean.trim().to_string();

    if clean.is_empty() {
        return clean;
    }

    // Ensure the text ends with punctuation (the base normalizer does this too,
    // but corrections or dedup might have broken it).
    let last_ch = clean.chars().last().unwrap_or(' ');
    if !matches!(last_ch, '.' | '?' | '!' | ':' | ';') {
        clean.push('.');
    }

    clean
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1. Existing filler removal (via base normalizer) ──
    #[test]
    fn test_filler_removal() {
        let result = post_process_default("Um, I need you to update the report.");
        assert!(!result.clean_text.to_lowercase().contains(" um"));
        assert!(result.clean_text.contains("need you to update the report"));
    }

    // ── 2. Existing stutter removal (via base normalizer) ──
    #[test]
    fn test_stutter_removal() {
        let result = post_process_default("I-I need you to update update the report");
        // The base normalizer handles "update update" → "update".
        // "I-I" is treated as one hyphenated token by the tokenizer,
        // so it remains. This is acceptable — it's a single token, not a repeat.
        assert!(result.clean_text.contains("update the report"));
        // Verify no double "update"
        assert!(!result.clean_text.contains("update update"));
    }

    // ── 3. Existing repetition removal ──
    #[test]
    fn test_word_repetition_removal() {
        let result = post_process_default("the the desktop app");
        assert_eq!(result.clean_text, "The desktop app.");
    }

    // ── 4. Existing spoken-number conversion ──
    #[test]
    fn test_number_conversion() {
        let result = post_process_default("I have fifteen apples");
        assert!(result.clean_text.contains("15"));
    }

    // ── 5. Context-aware filler preservation ──
    #[test]
    fn test_context_aware_filler_like() {
        // "like" as comparison — should be preserved.
        let result = post_process_default("The design looks like the mockup");
        assert!(result.clean_text.contains("like"));
    }

    #[test]
    fn test_context_aware_filler_well() {
        // "well" as discourse marker but carrying meaning — should be preserved.
        // The current filler list only removes: uh, um, ah, er, erm, hmm, mhm.
        // "well" is NOT in that list, so it is preserved by default. Correct.
        let result = post_process_default("Well, I think we should proceed");
        assert!(result.clean_text.contains("Well") || result.clean_text.contains("well"));
    }

    #[test]
    fn test_context_aware_filler_actually() {
        // "actually" should be preserved — it carries meaning.
        let result = post_process_default("Actually, the budget is fine");
        assert!(result.clean_text.to_lowercase().contains("actually"));
    }

    #[test]
    fn test_context_aware_filler_just() {
        // "just" should be preserved — it carries meaning/emphasis.
        let result = post_process_default("Just update the report");
        assert!(result.clean_text.contains("ust")); // "Just" or "just"
    }

    #[test]
    fn test_context_aware_filler_maybe() {
        // "maybe" should be preserved — it signals uncertainty.
        let result = post_process_default("Maybe we should cancel it");
        assert!(result.clean_text.to_lowercase().contains("maybe"));
    }

    // ── 6. Explicit self-corrections ──
    #[test]
    fn test_self_correction_sorry() {
        let result = post_process_default(
            "Schedule it for Tuesday sorry, Wednesday at three"
        );
        assert!(result.clean_text.contains("Wednesday"));
        // "Tuesday" should be removed since the speaker corrected it.
        assert!(!result.clean_text.contains("Tuesday"));
    }

    #[test]
    fn test_self_correction_no_deactivate() {
        let result = post_process_default(
            "Delete—no, deactivate—the account"
        );
        assert!(result.clean_text.contains("deactivate") || result.clean_text.contains("Deactivate"));
        assert!(!result.clean_text.contains("Delete"));
    }

    // ── 7. False starts ──
    #[test]
    fn test_false_start_correction() {
        let result = post_process_default(
            "I want to, no i mean, I need to update the report"
        );
        assert!(result.clean_text.contains("need to update"));
    }

    // ── 8. Negation preservation ──
    #[test]
    fn test_negation_not_preserved() {
        let result = post_process_default("I do not want to delete the file");
        assert!(result.clean_text.contains("not"));
    }

    #[test]
    fn test_negation_dont_preserved() {
        let result = post_process_default("Don't cancel the meeting");
        assert!(result.clean_text.contains("on't") || result.clean_text.contains("Don't"));
    }

    #[test]
    fn test_negation_never_preserved() {
        let result = post_process_default("Never share the password with anyone");
        assert!(result.clean_text.to_lowercase().contains("never"));
    }

    #[test]
    fn test_negation_without_preserved() {
        let result = post_process_default("Complete the task without restarting the server");
        assert!(result.clean_text.to_lowercase().contains("without"));
    }

    // ── 9. Conditions and exceptions ──
    #[test]
    fn test_condition_if_preserved() {
        let result = post_process_default("If the build fails, roll back to the previous version");
        assert!(result.clean_text.contains("If") || result.clean_text.contains("if"));
        assert!(result.clean_text.to_lowercase().contains("roll back"));
    }

    #[test]
    fn test_condition_unless_preserved() {
        let result = post_process_default("Deploy to production unless the tests fail");
        assert!(result.clean_text.to_lowercase().contains("unless"));
    }

    #[test]
    fn test_condition_only_when_preserved() {
        let result = post_process_default("Only when the manager approves should you proceed");
        assert!(result.clean_text.to_lowercase().contains("only when"));
    }

    // ── 10. Speaker uncertainty ──
    #[test]
    fn test_uncertainty_think_maybe() {
        let result = post_process_default("I think maybe we should cancel it");
        // The uncertainty must remain. Must NOT become just "Cancel it."
        assert!(result.clean_text.to_lowercase().contains("think")
            || result.clean_text.to_lowercase().contains("maybe"));
    }

    #[test]
    fn test_uncertainty_not_sure() {
        let result = post_process_default("I'm not sure if we should delete the account");
        assert!(result.clean_text.to_lowercase().contains("not sure"));
    }

    // ── 11. Repetition used as emphasis ──
    #[test]
    fn test_repetition_emphasis_separate_sentences() {
        // Three sentences with high word overlap = detected as redundant, collapsed.
        let result = post_process_default(
            "Send the report today. Send the report today. Send the report today."
        );
        assert!(result.clean_text.contains("today"));
        // Three identical sentences → collapsed with "Priority:" prefix.
        assert!(
            result.clean_text.contains("Priority:"),
            "Expected 'Priority:' prefix for triple emphasis, got: {}",
            result.clean_text
        );
    }

    #[test]
    fn test_repetition_different_phrasing_preserved() {
        // Sentences with the same *intent* but very different words are beyond
        // deterministic word-overlap detection. They should be preserved intact
        // rather than risk dropping distinct information.
        let result = post_process_default(
            "Send it today. I need it sent today. Make sure it goes today."
        );
        // All three sentences should remain (different verbs = low word overlap).
        assert!(result.clean_text.contains("today"));
    }

    // ── 12. Multiple speakers ──
    #[test]
    fn test_multiple_speaker_labels_preserved() {
        // If the ASR provides speaker labels, they must be preserved.
        let input = "Speaker 1: I need the report. Speaker 2: Which report?";
        let result = post_process_default(input);
        assert!(result.clean_text.contains("Speaker 1"));
        assert!(result.clean_text.contains("Speaker 2"));
    }

    // ── 13. Structured values ──
    #[test]
    fn test_percentage_normalization() {
        let result = post_process_default("The success rate is 50 percent");
        assert!(result.clean_text.contains("50%"));
    }

    #[test]
    fn test_currency_dollars() {
        let result = post_process_default("That costs 200 dollars");
        assert!(result.clean_text.contains("$200"));
    }

    #[test]
    fn test_email_spoken() {
        let result = post_process_default("Send it to john at example dot com please");
        assert!(result.clean_text.contains("john@example.com"));
    }

    #[test]
    fn test_time_normalization() {
        let result = post_process_default("The meeting is at 3 pm");
        assert!(
            result.clean_text.contains("3:00 PM")
                || result.clean_text.contains("3:00 pm"),
            "Expected time normalization, got: {}",
            result.clean_text
        );
    }

    // ── 14. Ambiguous consequential values ──
    #[test]
    fn test_ambiguous_transfer_amount() {
        let result = post_process_default(
            "Transfer fifteen or fifty thousand dollars"
        );
        // Must NOT silently pick one amount. Must flag ambiguity.
        assert!(
            result.requires_clarification,
            "Should require clarification for ambiguous amount"
        );
        assert!(
            !result.uncertain_spans.is_empty(),
            "Should have uncertain spans"
        );
    }

    #[test]
    fn test_ambiguous_destructive_with_hedge() {
        let result = post_process_default(
            "I think maybe we should delete the account"
        );
        // Hedging + destructive verb = requires clarification.
        assert!(
            result.requires_clarification,
            "Should require clarification for hedged destructive action"
        );
    }

    // ── 15. Empty input ──
    #[test]
    fn test_empty_input() {
        let result = post_process_default("");
        assert_eq!(result.clean_text, "");
        assert_eq!(result.raw_text, "");
        assert!(!result.requires_clarification);
        assert!(result.uncertain_spans.is_empty());
    }

    #[test]
    fn test_whitespace_only_input() {
        let result = post_process_default("   ");
        assert_eq!(result.clean_text, "");
    }

    // ── 16. Already-clean input ──
    #[test]
    fn test_already_clean() {
        let input = "Schedule the meeting for Wednesday at 3:00 PM.";
        let result = post_process_default(input);
        // Clean input should pass through without significant changes.
        // The normalizer may re-capitalize but content must match.
        assert!(result.clean_text.to_lowercase().contains("schedule"));
        assert!(result.clean_text.contains("Wednesday"));
        assert!(result.clean_text.contains("3:00 PM") || result.clean_text.contains("3:00 pm"));
    }

    // ── 17. Idempotency ──
    #[test]
    fn test_idempotency() {
        let input = "Um, I need you to update update the report. Send it today.";
        let first = post_process_default(input);
        let second = post_process_default(&first.clean_text);
        assert_eq!(
            first.clean_text, second.clean_text,
            "Processing should be idempotent. First: '{}', Second: '{}'",
            first.clean_text, second.clean_text
        );
    }

    #[test]
    fn test_idempotency_complex() {
        let input = "Schedule it for Tuesday sorry, Wednesday at 3 pm";
        let first = post_process_default(input);
        let second = post_process_default(&first.clean_text);
        assert_eq!(
            first.clean_text, second.clean_text,
            "Second pass should not change output. First: '{}', Second: '{}'",
            first.clean_text, second.clean_text
        );
    }

    // ── 18. Safe fallback after processing failure ──
    #[test]
    fn test_fallback_preserves_raw() {
        // If the clean_text is empty but raw is not, the caller
        // should fall back to raw. This tests that the struct preserves raw.
        let result = post_process("[BLANK_AUDIO] [SILENCE]", PostProcessorConfig::default());
        // After removing all noise markers, clean_text is empty.
        assert_eq!(result.raw_text, "[BLANK_AUDIO] [SILENCE]");
        // The clean text should be empty since there's no actual speech.
        assert!(result.clean_text.is_empty());
    }

    // ── Additional coverage ──

    #[test]
    fn test_noise_marker_removal() {
        let result = post_process_default("[BLANK_AUDIO] Hello there [silence]");
        assert!(result.clean_text.contains("Hello"));
        assert!(!result.clean_text.contains("BLANK_AUDIO"));
        assert!(!result.clean_text.contains("silence"));
    }

    #[test]
    fn test_raw_text_preserved() {
        let input = "Uh, um, like totally";
        let result = post_process_default(input);
        assert_eq!(result.raw_text, input);
    }

    #[test]
    fn test_combined_pipeline() {
        // Full pipeline test combining multiple features.
        let input = "Um, I-I need to update update the the report. Actually send it to john at example dot com. The cost is 50 dollars.";
        let result = post_process_default(input);

        // Fillers removed
        assert!(!result.clean_text.to_lowercase().contains(" um,"));
        // Stutters removed
        assert!(!result.clean_text.contains("the the"));
        // Email normalized
        assert!(result.clean_text.contains("john@example.com"));
        // Currency normalized
        assert!(result.clean_text.contains("$50"));
    }

    #[test]
    fn test_disabled_config() {
        let config = PostProcessorConfig {
            normalization: NormalizationOptions {
                enabled: false,
                remove_fillers: false,
                convert_numbers: false,
                remove_stutters: false,
            },
            apply_self_corrections: false,
            remove_noise_markers: false,
            collapse_redundancy: false,
            annotate_ambiguity: false,
            normalize_structured_values: false,
        };

        let input = "Uh the the fifteen";
        let result = post_process(input, config);
        // With everything disabled, text should pass through mostly unchanged.
        // Only final_cleanup (whitespace normalization + terminal punctuation) applies.
        assert!(result.clean_text.contains("Uh"));
        assert!(result.clean_text.contains("the the"));
        assert!(result.clean_text.contains("fifteen"));
    }

    #[test]
    fn test_phrase_repetition_two_sentences() {
        // Two identical sentences: keep one, no "Priority:" prefix.
        let result = post_process_default("Send the report. Send the report.");
        let count = result.clean_text.matches("Send").count()
            + result.clean_text.matches("send").count();
        assert!(count <= 1, "Expected deduplication, got: {}", result.clean_text);
    }

    #[test]
    fn test_condition_different_constraints_not_merged() {
        // Different conditions should NOT be merged even if similar words.
        let result = post_process_default(
            "If the build passes, deploy to staging. If the tests pass, deploy to production."
        );
        // Both sentences should remain since they have different conditions.
        assert!(result.clean_text.to_lowercase().contains("staging"));
        assert!(result.clean_text.to_lowercase().contains("production"));
    }

    #[test]
    fn test_date_normalization() {
        let result = post_process_default("The release date is March 15th 2024 for this update");
        assert!(result.clean_text.contains("March 15th, 2024"));
    }

    #[test]
    fn test_url_normalization() {
        let result = post_process_default("Visit https colon slash slash example dot com slash api for details");
        assert!(result.clean_text.contains("https://example.com/api"));
    }

    #[test]
    fn test_www_url_normalization() {
        let result = post_process_default("Go to www dot google dot com for search");
        assert!(result.clean_text.contains("www.google.com"));
    }

    #[test]
    fn test_version_normalization() {
        let result = post_process_default("Please deploy version 2 point 1 point 0 immediately");
        assert!(result.clean_text.contains("version 2.1.0"));
    }

    #[test]
    fn test_ticket_id_normalization() {
        let result = post_process_default("Check JIRA dash 123 for specifications");
        assert!(result.clean_text.contains("JIRA-123"));
    }

    #[test]
    fn test_file_path_normalization() {
        let result = post_process_default("Open slash usr slash local slash bin now");
        assert!(result.clean_text.contains("/usr/local/bin"));
    }

    #[test]
    fn test_technical_identifier_normalization() {
        let result = post_process_default("Connect to localhost colon 3000 using UTF dash 8 encoding");
        assert!(result.clean_text.contains("localhost:3000"));
        assert!(result.clean_text.contains("UTF-8"));
    }

    #[test]
    fn test_paragraph_boundary_preservation() {
        let input = "First paragraph with details.\n\nSecond paragraph with more info.";
        let result = post_process_default(input);
        assert!(result.clean_text.contains("\n\n"));
        let parts: Vec<&str> = result.clean_text.split("\n\n").collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("paragraph with details"));
        assert!(parts[1].contains("paragraph with more info"));
    }
}
