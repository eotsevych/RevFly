//! Confidential text masking and replacement engine.
//!
//! Supports:
//! - Exact word matching (case-insensitive, ignoring register)
//! - Multi-word phrase matching
//! - Fuzzy percentage similarity matching (e.g. 90% match threshold)
//! - Custom replacements (e.g. "John -> [NAME]")
//! - Optional filters for emails, phone numbers, and credit cards

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Individual masking or replacement rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskRule {
    /// Target pattern or word to find.
    pub pattern: String,
    /// Replacement string. If empty, falls back to the default mask (e.g. "***").
    pub replacement: String,
}

/// Configuration for confidential text masking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskConfig {
    /// Whether masking is active.
    pub enabled: bool,
    /// Custom word and phrase rules.
    pub rules: Vec<MaskRule>,
    /// Match similarity threshold (50 to 100). 100 means exact match only.
    pub threshold: u32,
    /// Default mask token used when no specific replacement is given.
    pub default_mask: String,
    /// Automatically redact email addresses.
    pub mask_emails: bool,
    /// Automatically redact phone numbers.
    pub mask_phones: bool,
    /// Automatically redact credit card numbers.
    pub mask_cards: bool,
}

impl Default for MaskConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rules: Vec::new(),
            threshold: 90,
            default_mask: "***".to_string(),
            mask_emails: false,
            mask_phones: false,
            mask_cards: false,
        }
    }
}

/// Computes the Levenshtein edit distance between two character sequences.
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let s1_chars: Vec<char> = s1.chars().flat_map(|c| c.to_lowercase()).collect();
    let s2_chars: Vec<char> = s2.chars().flat_map(|c| c.to_lowercase()).collect();
    let len1 = s1_chars.len();
    let len2 = s2_chars.len();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut dp = vec![vec![0; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        dp[i][0] = i;
    }
    for j in 0..=len2 {
        dp[0][j] = j;
    }

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[len1][len2]
}

/// Computes the similarity percentage ratio between two strings (0.0 to 1.0).
pub fn similarity_ratio(s1: &str, s2: &str) -> f64 {
    let s1_lower: String = s1.chars().flat_map(|c| c.to_lowercase()).collect();
    let s2_lower: String = s2.chars().flat_map(|c| c.to_lowercase()).collect();

    if s1_lower == s2_lower {
        return 1.0;
    }

    let d = levenshtein_distance(&s1_lower, &s2_lower);
    let max_len = s1_lower.chars().count().max(s2_lower.chars().count());
    if max_len == 0 {
        return 1.0;
    }

    1.0 - (d as f64 / max_len as f64)
}

/// Parses a multiline or comma-delimited string into a list of [`MaskRule`] items.
///
/// Supported formats:
/// - `John -> [NAME]`
/// - `SecretKey = [REDACTED]`
/// - `Eugene` (defaults to default_mask)
pub fn parse_rules(input: &str, default_mask: &str) -> Vec<MaskRule> {
    let mut rules = Vec::new();

    for line in input.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() || trimmed_line.starts_with('#') || trimmed_line.starts_with("//") {
            continue;
        }

        // Check if line has multiple comma-separated items without arrow syntax
        if !trimmed_line.contains("->") && !trimmed_line.contains('=') && trimmed_line.contains(',') {
            for part in trimmed_line.split(',') {
                let p = part.trim();
                if !p.is_empty() {
                    rules.push(MaskRule {
                        pattern: p.to_string(),
                        replacement: default_mask.to_string(),
                    });
                }
            }
            continue;
        }

        if let Some((pat, rep)) = trimmed_line.split_once("->") {
            let p = pat.trim();
            let r = rep.trim();
            if !p.is_empty() {
                rules.push(MaskRule {
                    pattern: p.to_string(),
                    replacement: if r.is_empty() { default_mask.to_string() } else { r.to_string() },
                });
            }
        } else if let Some((pat, rep)) = trimmed_line.split_once('=') {
            let p = pat.trim();
            let r = rep.trim();
            if !p.is_empty() {
                rules.push(MaskRule {
                    pattern: p.to_string(),
                    replacement: if r.is_empty() { default_mask.to_string() } else { r.to_string() },
                });
            }
        } else {
            rules.push(MaskRule {
                pattern: trimmed_line.to_string(),
                replacement: default_mask.to_string(),
            });
        }
    }

    rules
}

static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();
static CARD_REGEX: OnceLock<Regex> = OnceLock::new();
static PHONE_REGEX: OnceLock<Regex> = OnceLock::new();
static TOKEN_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_email_regex() -> &'static Regex {
    EMAIL_REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").unwrap()
    })
}

fn get_card_regex() -> &'static Regex {
    CARD_REGEX.get_or_init(|| {
        Regex::new(r"\b(?:\d{4}[ -]?){3}\d{4}\b").unwrap()
    })
}

fn get_phone_regex() -> &'static Regex {
    PHONE_REGEX.get_or_init(|| {
        Regex::new(r"(?:\+?\d{1,3}[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b").unwrap()
    })
}

fn get_token_regex() -> &'static Regex {
    TOKEN_REGEX.get_or_init(|| {
        Regex::new(r"([\p{L}\p{N}_]+|[^\p{L}\p{N}_\s]+|\s+)").unwrap()
    })
}

/// Applies confidential masking and replacement to the given text according to config.
pub fn apply_masking(text: &str, config: &MaskConfig) -> String {
    if !config.enabled || text.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();

    // 1. Built-in sensitive data masking
    if config.mask_cards {
        result = get_card_regex().replace_all(&result, "****-****-****-****").to_string();
    }
    if config.mask_emails {
        result = get_email_regex().replace_all(&result, "[EMAIL]").to_string();
    }
    if config.mask_phones {
        result = get_phone_regex().replace_all(&result, "[PHONE]").to_string();
    }

    if config.rules.is_empty() {
        return result;
    }

    // 2. Multi-word phrase rules (e.g. "John Doe", "Secret Project Alpha")
    let mut multi_word_rules: Vec<&MaskRule> = config
        .rules
        .iter()
        .filter(|r| r.pattern.contains(char::is_whitespace))
        .collect();
    multi_word_rules.sort_by(|a, b| b.pattern.len().cmp(&a.pattern.len()));

    for rule in multi_word_rules {
        let pattern_escaped = regex::escape(&rule.pattern);
        if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", pattern_escaped)) {
            result = re.replace_all(&result, rule.replacement.as_str()).to_string();
        }
    }

    // 3. Single-word rules (Exact + Fuzzy matching)
    let single_word_rules: Vec<&MaskRule> = config
        .rules
        .iter()
        .filter(|r| !r.pattern.contains(char::is_whitespace))
        .collect();

    if single_word_rules.is_empty() {
        return result;
    }

    let threshold_f64 = config.threshold.clamp(50, 100) as f64;
    let mut token_pieces = Vec::new();

    for cap in get_token_regex().captures_iter(&result) {
        let token = cap.get(0).unwrap().as_str();

        // Check if token is a word/alphanumeric sequence
        let is_word = token.chars().any(|c| c.is_alphanumeric());

        if !is_word {
            token_pieces.push(token.to_string());
            continue;
        }

        let token_lower: String = token.chars().flat_map(|c| c.to_lowercase()).collect();
        let token_len = token_lower.chars().count();

        let mut matched_replacement = None;

        // Exact match check first (100% case-insensitive)
        for rule in &single_word_rules {
            let rule_lower: String = rule.pattern.chars().flat_map(|c| c.to_lowercase()).collect();
            if token_lower == rule_lower {
                matched_replacement = Some(rule.replacement.clone());
                break;
            }
        }

        // Fuzzy match check if exact match not found and threshold < 100
        if matched_replacement.is_none() && config.threshold < 100 {
            for rule in &single_word_rules {
                let rule_lower: String = rule.pattern.chars().flat_map(|c| c.to_lowercase()).collect();
                let rule_len = rule_lower.chars().count();

                // Skip fuzzy comparison for very short words (< 3 chars) to avoid false positives
                if rule_len < 3 || token_len < 3 {
                    continue;
                }

                // Skip if length difference is too large for the threshold
                let len_diff = (token_len as isize - rule_len as isize).unsigned_abs();
                if len_diff > 3 {
                    continue;
                }

                let ratio = similarity_ratio(&token_lower, &rule_lower) * 100.0;
                if ratio >= threshold_f64 {
                    matched_replacement = Some(rule.replacement.clone());
                    break;
                }
            }
        }

        if let Some(rep) = matched_replacement {
            token_pieces.push(rep);
        } else {
            token_pieces.push(token.to_string());
        }
    }

    token_pieces.join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_similarity_ratio_calculation() {
        assert_eq!(similarity_ratio("hello", "hello"), 1.0);
        assert_eq!(similarity_ratio("Hello", "hello"), 1.0);
        assert_eq!(similarity_ratio("JOHN", "john"), 1.0);

        // 1 edit out of 10 = 90%
        let ratio_10 = similarity_ratio("projectone", "projectane");
        assert!((ratio_10 - 0.9).abs() < 0.01);

        // Cyrillic
        assert_eq!(similarity_ratio("Олександр", "олександр"), 1.0);
    }

    #[test]
    fn test_exact_word_masking_case_insensitive() {
        let config = MaskConfig {
            enabled: true,
            rules: vec![
                MaskRule {
                    pattern: "John".to_string(),
                    replacement: "[NAME]".to_string(),
                },
                MaskRule {
                    pattern: "SecretKey".to_string(),
                    replacement: "***".to_string(),
                },
            ],
            threshold: 100,
            default_mask: "***".to_string(),
            ..Default::default()
        };

        let input = "Hello john, your secretkey is active. JOHN confirmed.";
        let out = apply_masking(input, &config);
        assert_eq!(out, "Hello [NAME], your *** is active. [NAME] confirmed.");
    }

    #[test]
    fn test_fuzzy_matching_90_percent() {
        let config = MaskConfig {
            enabled: true,
            rules: vec![
                MaskRule {
                    pattern: "Alexander".to_string(),
                    replacement: "[REDACTED]".to_string(),
                },
                MaskRule {
                    pattern: "Confidential".to_string(),
                    replacement: "***".to_string(),
                },
            ],
            threshold: 90, // 90% threshold
            default_mask: "***".to_string(),
            ..Default::default()
        };

        // "Alexandr" has 8 chars, "Alexander" has 9 chars. Distance is 1. Similarity is 8/9 = 88.8% -> won't match at 90
        // "Alexanderr" has 10 chars, "Alexander" has 9 chars. Distance 1. Similarity = 9/10 = 90.0% -> matches at 90!
        let input1 = "Contact Alexanderr about the project.";
        let out1 = apply_masking(input1, &config);
        assert_eq!(out1, "Contact [REDACTED] about the project.");

        // "Confidentials" has 13 chars, distance 1 -> 12/13 = 92.3% -> matches at 90!
        let input2 = "This is Confidentials information.";
        let out2 = apply_masking(input2, &config);
        assert_eq!(out2, "This is *** information.");
    }

    #[test]
    fn test_fuzzy_rejection_below_threshold() {
        let config = MaskConfig {
            enabled: true,
            rules: vec![MaskRule {
                pattern: "Apple".to_string(),
                replacement: "***".to_string(),
            }],
            threshold: 90,
            default_mask: "***".to_string(),
            ..Default::default()
        };

        // "Apply" vs "Apple": 1 char diff in 5 chars = 80% similarity -> below 90% -> should NOT match
        let input = "Please apply today.";
        let out = apply_masking(input, &config);
        assert_eq!(out, "Please apply today.");
    }

    #[test]
    fn test_multi_word_phrase_masking() {
        let config = MaskConfig {
            enabled: true,
            rules: vec![MaskRule {
                pattern: "Secret Project Alpha".to_string(),
                replacement: "[PROJECT]".to_string(),
            }],
            threshold: 100,
            default_mask: "***".to_string(),
            ..Default::default()
        };

        let input = "We are discussing secret project alpha with stakeholders.";
        let out = apply_masking(input, &config);
        assert_eq!(out, "We are discussing [PROJECT] with stakeholders.");
    }

    #[test]
    fn test_parse_rules() {
        let input = "John -> [NAME]\nAlex\nEugene = [DEV]\nSecret1, Secret2";
        let rules = parse_rules(input, "***");
        assert_eq!(rules.len(), 5);
        assert_eq!(rules[0].pattern, "John");
        assert_eq!(rules[0].replacement, "[NAME]");
        assert_eq!(rules[1].pattern, "Alex");
        assert_eq!(rules[1].replacement, "***");
        assert_eq!(rules[2].pattern, "Eugene");
        assert_eq!(rules[2].replacement, "[DEV]");
        assert_eq!(rules[3].pattern, "Secret1");
        assert_eq!(rules[4].pattern, "Secret2");
    }

    #[test]
    fn test_cyrillic_masking() {
        let config = MaskConfig {
            enabled: true,
            rules: vec![
                MaskRule {
                    pattern: "Євген".to_string(),
                    replacement: "[ІМ'Я]".to_string(),
                },
                MaskRule {
                    pattern: "Секрет".to_string(),
                    replacement: "***".to_string(),
                },
            ],
            threshold: 90,
            default_mask: "***".to_string(),
            ..Default::default()
        };

        let input = "Привіт, євген! Це великий секрет?";
        let out = apply_masking(input, &config);
        assert_eq!(out, "Привіт, [ІМ'Я]! Це великий ***?");
    }

    #[test]
    fn test_sensitive_patterns() {
        let config = MaskConfig {
            enabled: true,
            rules: vec![],
            threshold: 100,
            default_mask: "***".to_string(),
            mask_emails: true,
            mask_phones: true,
            mask_cards: true,
        };

        let input = "Email me at dev@example.com or call 555-123-4567. Card: 4111-2222-3333-4444.";
        let out = apply_masking(input, &config);
        assert_eq!(
            out,
            "Email me at [EMAIL] or call [PHONE]. Card: ****-****-****-****."
        );
    }
}
