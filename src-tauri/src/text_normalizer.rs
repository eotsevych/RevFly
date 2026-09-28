//! Text normalizer for speech-to-text transcripts.
//! Cleans speech disfluencies (fillers), stutters, formats numbers (ITN), and cleans punctuation.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct NormalizationOptions {
    pub enabled: bool,
    pub remove_fillers: bool,
    pub convert_numbers: bool,
    pub remove_stutters: bool,
}

impl Default for NormalizationOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            remove_fillers: true,
            convert_numbers: true,
            remove_stutters: true,
        }
    }
}

/// Normalizes transcribed speech text according to user options:
/// 1. Removes filler words ("uh", "um", "ah", "er", "erm") if remove_fillers is true.
/// 2. Deduplicates stuttered words ("the the" -> "the") if remove_stutters is true.
/// 3. Converts spoken numbers to digits ("fifteen" -> "15") if convert_numbers is true.
/// 4. Cleans spaces and capitalization.
pub fn normalize_transcript(input: &str) -> String {
    normalize_transcript_with_options(input, NormalizationOptions::default())
}

pub fn normalize_transcript_with_options(input: &str, opts: NormalizationOptions) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() || !opts.enabled {
        return trimmed.to_string();
    }

    // Step 1: Tokenize and conditionally remove filler words
    let tokens = tokenize(trimmed);
    let mut filtered_tokens = Vec::new();

    for token in tokens {
        if opts.remove_fillers && is_filler(&token) {
            continue;
        }
        filtered_tokens.push(token);
    }

    if filtered_tokens.is_empty() {
        return String::new();
    }

    // Step 2: Conditionally deduplicate immediate stutters (words and phrases)
    let deduplicated = if opts.remove_stutters {
        let word_deduped = deduplicate_stutters(&filtered_tokens);
        deduplicate_phrases(&word_deduped)
    } else {
        filtered_tokens
    };

    // Step 3: Conditionally convert number words
    let numbers_converted = if opts.convert_numbers {
        convert_number_words(&deduplicated)
    } else {
        deduplicated
    };

    // Step 4: Reconstruct text and format punctuation & capitalization
    format_sentences(&numbers_converted)
}

/// Token with trailing punctuation attached.
#[derive(Debug, Clone)]
struct WordToken {
    /// Pure lowercase word without punctuation
    cleaned: String,
    /// Preserved original casing
    original: String,
    /// Trailing punctuation (e.g. ",", ".", "?", "!")
    trailing_punct: Option<char>,
    /// Leading punctuation (e.g. quotes, dashes)
    leading_punct: Option<char>,
}

fn is_filler(token: &WordToken) -> bool {
    matches!(
        token.cleaned.as_str(),
        "uh" | "um" | "ah" | "er" | "erm" | "hmm" | "mhm"
    )
}

fn tokenize(text: &str) -> Vec<WordToken> {
    let mut tokens = Vec::new();
    for raw in text.split_whitespace() {
        if raw.is_empty() {
            continue;
        }

        let mut chars = raw.chars().collect::<Vec<_>>();
        let mut leading_punct = None;
        if let Some(&c) = chars.first() {
            if c == '"' || c == '\'' || c == '(' || c == '[' {
                leading_punct = Some(c);
                chars.remove(0);
            }
        }

        let mut trailing_punct = None;
        if let Some(&c) = chars.last() {
            if c == ',' || c == '.' || c == '?' || c == '!' || c == ':' || c == ';' {
                trailing_punct = Some(c);
                chars.pop();
            }
        }

        let core: String = chars.into_iter().collect();
        let cleaned = core
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
            .to_lowercase();

        tokens.push(WordToken {
            cleaned,
            original: core,
            trailing_punct,
            leading_punct,
        });
    }
    tokens
}

fn deduplicate_stutters(tokens: &[WordToken]) -> Vec<WordToken> {
    let mut out: Vec<WordToken> = Vec::with_capacity(tokens.len());
    for token in tokens {
        if let Some(prev) = out.last_mut() {
            if !prev.cleaned.is_empty()
                && prev.cleaned == token.cleaned
                && prev.trailing_punct.is_none()
            {
                // Duplicate word stutter: keep trailing punct if current has one
                if token.trailing_punct.is_some() {
                    prev.trailing_punct = token.trailing_punct;
                }
                continue;
            }
        }
        out.push(token.clone());
    }
    out
}

/// Removes consecutive repeated phrases (2-3 word sequences).
/// Example: "update the report update the report" → "update the report"
/// Only removes when no punctuation separates the phrases.
fn deduplicate_phrases(tokens: &[WordToken]) -> Vec<WordToken> {
    if tokens.len() < 4 {
        return tokens.to_vec();
    }

    let mut out = tokens.to_vec();

    // Check phrase lengths 3, then 2 (longest first to catch wider repeats).
    for phrase_len in (2..=3).rev() {
        let mut result: Vec<WordToken> = Vec::with_capacity(out.len());
        let mut i = 0;

        while i < out.len() {
            // Check if tokens[i..i+phrase_len] == tokens[i+phrase_len..i+2*phrase_len]
            if i + 2 * phrase_len <= out.len() {
                let mut is_repeat = true;

                // The last token of the first phrase must NOT have sentence-ending
                // punctuation (which would mean separate sentences).
                if let Some(tp) = out[i + phrase_len - 1].trailing_punct {
                    if matches!(tp, '.' | '?' | '!' | ';') {
                        is_repeat = false;
                    }
                }

                if is_repeat {
                    for k in 0..phrase_len {
                        if out[i + k].cleaned != out[i + phrase_len + k].cleaned {
                            is_repeat = false;
                            break;
                        }
                    }
                }

                if is_repeat {
                    // Keep the second occurrence (it may have better punctuation).
                    for k in 0..phrase_len {
                        result.push(out[i + phrase_len + k].clone());
                    }
                    i += 2 * phrase_len;
                    continue;
                }
            }

            result.push(out[i].clone());
            i += 1;
        }

        out = result;
    }

    out
}

/// Converts number words (e.g., "fifteen" -> "15", "twenty one" -> "21")
fn convert_number_words(tokens: &[WordToken]) -> Vec<WordToken> {
    let number_map: HashMap<&str, i64> = [
        ("zero", 0),
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("eleven", 11),
        ("twelve", 12),
        ("thirteen", 13),
        ("fourteen", 14),
        ("fifteen", 15),
        ("sixteen", 16),
        ("seventeen", 17),
        ("eighteen", 18),
        ("nineteen", 19),
        ("twenty", 20),
        ("thirty", 30),
        ("forty", 40),
        ("fifty", 50),
        ("sixty", 60),
        ("seventy", 70),
        ("eighty", 80),
        ("ninety", 90),
        ("hundred", 100),
        ("thousand", 1000),
        ("million", 1_000_000),
    ]
    .iter()
    .cloned()
    .collect();

    let ordinal_map: HashMap<&str, &str> = [
        ("first", "1st"),
        ("second", "2nd"),
        ("third", "3rd"),
        ("fourth", "4th"),
        ("fifth", "5th"),
        ("sixth", "6th"),
        ("seventh", "7th"),
        ("eighth", "8th"),
        ("ninth", "9th"),
        ("tenth", "10th"),
    ]
    .iter()
    .cloned()
    .collect();

    let mut out: Vec<WordToken> = Vec::with_capacity(tokens.len());
    let mut i = 0;

    while i < tokens.len() {
        let token = &tokens[i];

        // Check ordinal first
        if let Some(&ord) = ordinal_map.get(token.cleaned.as_str()) {
            out.push(WordToken {
                cleaned: ord.to_string(),
                original: ord.to_string(),
                trailing_punct: token.trailing_punct,
                leading_punct: token.leading_punct,
            });
            i += 1;
            continue;
        }

        // Check cardinal number
        if let Some(&val) = number_map.get(token.cleaned.as_str()) {
            // See if we have compound numbers (e.g. "twenty five", "one hundred")
            let mut total = val;
            let mut last_idx = i;
            let mut trailing = token.trailing_punct;

            if token.trailing_punct.is_none() && i + 1 < tokens.len() {
                let next = &tokens[i + 1];
                if let Some(&next_val) = number_map.get(next.cleaned.as_str()) {
                    if (val >= 20 && val <= 90 && next_val < 10) || next_val == 100 || next_val == 1000 {
                        if next_val == 100 || next_val == 1000 {
                            total = val * next_val;
                        } else {
                            total = val + next_val;
                        }
                        last_idx = i + 1;
                        trailing = next.trailing_punct;
                    }
                }
            }

            let num_str = total.to_string();
            out.push(WordToken {
                cleaned: num_str.clone(),
                original: num_str,
                trailing_punct: trailing,
                leading_punct: token.leading_punct,
            });
            i = last_idx + 1;
            continue;
        }

        out.push(token.clone());
        i += 1;
    }

    out
}

/// Reconstructs sentences with proper spacing, trailing punctuation, and capitalization.
fn format_sentences(tokens: &[WordToken]) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;

    for (idx, token) in tokens.iter().enumerate() {
        if idx > 0 {
            result.push(' ');
        }

        if let Some(lp) = token.leading_punct {
            result.push(lp);
        }

        let word = if capitalize_next {
            capitalize_word(&token.original)
        } else {
            token.original.clone()
        };
        result.push_str(&word);

        if let Some(tp) = token.trailing_punct {
            result.push(tp);
            if tp == '.' || tp == '?' || tp == '!' {
                capitalize_next = true;
            } else {
                capitalize_next = false;
            }
        } else {
            capitalize_next = false;
        }
    }

    // Ensure sentence ends with a punctuation mark if long enough
    let trimmed = result.trim();
    if !trimmed.is_empty() {
        let last_ch = trimmed.chars().last().unwrap_or(' ');
        if !matches!(last_ch, '.' | '?' | '!' | ':' | ';') {
            result.push('.');
        }
    }

    result
}

fn capitalize_word(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_1_filler_removal() {
        let input = "Uh okay I like uh the design but uh now we need to prepare it for the development first of all the settings uh view uh should be wrapped as the uh desktop app not the web and yeah let's put everything into the separate uh file folder so it will be easier to investigate where the required uh design is placed.";
        let normalized = normalize_transcript(input);
        assert!(!normalized.contains(" uh "));
        assert!(!normalized.contains("Uh "));
        assert!(normalized.starts_with("Okay"));
        assert!(normalized.contains("like the design"));
        assert!(normalized.contains("settings view should"));
    }

    #[test]
    fn test_case_2_number_conversion() {
        let input = "Imagine you have only fifteen minutes and you need to provide the comprehensive test results";
        let normalized = normalize_transcript(input);
        assert_eq!(
            normalized,
            "Imagine you have only 15 minutes and you need to provide the comprehensive test results."
        );
    }

    #[test]
    fn test_stutter_deduplication() {
        let input = "wrapped as the the desktop app";
        let normalized = normalize_transcript(input);
        assert_eq!(normalized, "Wrapped as the desktop app.");
    }

    #[test]
    fn test_verbatim_mode_disabled() {
        let input = "Uh okay I like uh the design fifteen minutes the the";
        let opts = NormalizationOptions {
            enabled: false,
            remove_fillers: false,
            convert_numbers: false,
            remove_stutters: false,
        };
        let raw = normalize_transcript_with_options(input, opts);
        assert_eq!(raw, input);
    }

    #[test]
    fn test_keep_fillers_only() {
        let input = "Uh okay I have fifteen apples";
        let opts = NormalizationOptions {
            enabled: true,
            remove_fillers: false,
            convert_numbers: true,
            remove_stutters: true,
        };
        let res = normalize_transcript_with_options(input, opts);
        assert!(res.contains("15"));
        assert!(res.contains("Uh") || res.contains("uh"));
    }

    #[test]
    fn test_phrase_repetition_removal() {
        let input = "update the report update the report";
        let normalized = normalize_transcript(input);
        assert_eq!(normalized, "Update the report.");
    }

    #[test]
    fn test_phrase_repetition_with_single_word_repeat() {
        // Both single-word and phrase-level dedup should work together.
        let input = "the the update the report update the report";
        let normalized = normalize_transcript(input);
        assert_eq!(normalized, "The update the report.");
    }
}
