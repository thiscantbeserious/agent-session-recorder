//! Sanitizing and truncating names for filenames and directories.

use super::{Config, FALLBACK_NAME, INVALID_CHARS, WINDOWS_RESERVED};
use deunicode::deunicode;

/// Check whether a character is valid for use in filenames.
pub fn is_valid_filename_char(c: char) -> bool {
    !INVALID_CHARS.contains(&c) && !c.is_control()
}

/// Sanitizes a string for use in filenames.
///
/// Applies unicode transliteration, whitespace-to-hyphens, invalid char removal,
/// hyphen collapsing, edge trimming, reserved name handling, and empty fallback.
#[allow(dead_code)]
pub fn sanitize(input: &str, _config: &Config) -> String {
    let ascii = deunicode(input);
    let processed = process_chars(&ascii);
    let trimmed = trim_edges(&processed);
    let final_name = handle_reserved_name(&trimmed);

    if final_name.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        final_name
    }
}

/// Sanitizes a branch name for use in filenames.
///
/// Replaces `/` with `--` to preserve namespace visibility, then removes
/// only filesystem-invalid characters (`INVALID_CHARS`) and control chars.
/// More permissive than `sanitize()` — keeps `#`, `~`, `@`, etc. which
/// are valid on all major filesystems. Does NOT apply length truncation.
#[allow(dead_code)]
pub fn sanitize_branch(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for c in input.chars() {
        if c == '/' {
            result.push_str("--");
        } else if !INVALID_CHARS.contains(&c) && !c.is_control() {
            result.push(c);
        }
    }
    result
}

/// Sanitizes a directory name with length truncation.
///
/// Same as `sanitize()` but also truncates to `config.directory_max_length`.
#[allow(dead_code)]
pub fn sanitize_directory(input: &str, config: &Config) -> String {
    let sanitized = sanitize(input, config);
    truncate_to_length(&sanitized, config.directory_max_length)
}

// ---- Internal helpers ----

/// Processes characters: whitespace to hyphens, remove invalid/brackets, collapse hyphens.
fn process_chars(ascii: &str) -> String {
    let mut result = String::with_capacity(ascii.len());
    let mut last_was_hyphen = false;

    for c in ascii.chars() {
        if c.is_whitespace() {
            if !last_was_hyphen {
                result.push('-');
                last_was_hyphen = true;
            }
        } else if INVALID_CHARS.contains(&c) {
            continue;
        } else if c == '-' {
            if !last_was_hyphen {
                result.push('-');
                last_was_hyphen = true;
            }
        } else if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            result.push(c);
            last_was_hyphen = false;
        } else if c == '(' || c == ')' || c == '[' || c == ']' {
            continue;
        }
    }

    result
}

/// Trims leading and trailing dots, spaces, and hyphens.
fn trim_edges(s: &str) -> String {
    s.trim_matches(|c| c == '.' || c == ' ' || c == '-')
        .to_string()
}

/// Checks if a name is a Windows reserved name and prefixes it if so.
fn handle_reserved_name(name: &str) -> String {
    let base_name = match name.find('.') {
        Some(pos) => &name[..pos],
        None => name,
    };

    let upper = base_name.to_uppercase();
    for reserved in WINDOWS_RESERVED {
        if upper == *reserved {
            return format!("_{}", name);
        }
    }
    name.to_string()
}

/// Vowels used for syllable detection and removal.
const VOWELS: &[char] = &['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U'];

/// Minimum length for first word abbreviation before switching strategies.
const MIN_FIRST_WORD_ABBREV_LEN: usize = 3;

/// Minimum result length for abbreviation fallback.
const MIN_ABBREV_RESULT_LEN: usize = 2;

/// Extracts the first syllable of a word.
///
/// Short words (<=3 chars) returned unchanged. Finds first vowel, collects
/// consonants until next vowel, splits at doubled consonants if found.
fn first_syllable(word: &str) -> &str {
    if word.chars().count() <= 3 {
        return word;
    }

    let chars: Vec<char> = word.chars().collect();

    let first_vowel_idx = match chars.iter().position(|c| VOWELS.contains(c)) {
        Some(idx) => idx,
        None => return word,
    };

    let mut idx = first_vowel_idx + 1;
    let consonant_start = idx;

    while idx < chars.len() && !VOWELS.contains(&chars[idx]) {
        idx += 1;
    }

    if idx >= chars.len() {
        return word;
    }

    let consonant_count = idx - consonant_start;
    let cut_idx = if consonant_count >= 2 {
        let consonants = &chars[consonant_start..idx];
        let mut double_pos = None;
        for i in 0..consonants.len() - 1 {
            if consonants[i] == consonants[i + 1] {
                double_pos = Some(consonant_start + i + 1);
                break;
            }
        }
        double_pos.unwrap_or(idx)
    } else {
        idx
    };

    let byte_idx = word
        .char_indices()
        .nth(cut_idx)
        .map(|(i, _)| i)
        .unwrap_or(word.len());
    &word[..byte_idx]
}

/// Removes vowels from a word, keeping at least the first character.
fn remove_vowels(word: &str) -> String {
    let mut chars = word.chars();
    let mut result = String::with_capacity(word.len());

    if let Some(first) = chars.next() {
        result.push(first);
    }

    for c in chars {
        if !VOWELS.contains(&c) {
            result.push(c);
        }
    }

    result
}

/// Abbreviates the first word using vowel removal when syllable extraction is too short.
fn abbreviate_first_word(word: &str) -> String {
    let syllable = first_syllable(word);
    let syllable_len = syllable.chars().count();

    if syllable_len >= MIN_FIRST_WORD_ABBREV_LEN {
        return syllable.to_string();
    }

    let vowel_removed = remove_vowels(word);
    let vowel_removed_len = vowel_removed.chars().count();

    if vowel_removed_len >= MIN_ABBREV_RESULT_LEN && vowel_removed_len > syllable_len {
        vowel_removed
    } else if syllable_len >= MIN_ABBREV_RESULT_LEN {
        syllable.to_string()
    } else {
        let word_len = word.chars().count();
        if word_len >= MIN_ABBREV_RESULT_LEN {
            word.chars().take(MIN_ABBREV_RESULT_LEN).collect()
        } else if vowel_removed_len >= syllable_len {
            vowel_removed
        } else {
            syllable.to_string()
        }
    }
}

/// Truncates a string to the specified length using smart abbreviation.
///
/// Multi-word strings get syllable-based abbreviation; single words are hard-truncated.
fn truncate_to_length(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        return s.to_string();
    }

    let words: Vec<&str> = s.split(['-', '_', '.']).filter(|w| !w.is_empty()).collect();

    if words.len() <= 1 {
        return s.chars().take(max_len).collect();
    }

    let abbreviated: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                abbreviate_first_word(w)
            } else {
                first_syllable(w).to_string()
            }
        })
        .collect();
    let result = abbreviated.join("-");

    if result.chars().count() <= max_len {
        return result;
    }

    let separator_count = words.len() - 1;
    let available = max_len.saturating_sub(separator_count);
    let chars_per_word = available / words.len();

    let truncated: Vec<String> = abbreviated
        .iter()
        .map(|w| w.chars().take(chars_per_word.max(1)).collect::<String>())
        .collect();

    let joined = truncated.join("-");
    let cleaned = joined.trim_end_matches('-').to_string();

    if cleaned.chars().count() > max_len {
        let truncated: String = cleaned.chars().take(max_len).collect();
        truncated.trim_end_matches('-').to_string()
    } else {
        cleaned
    }
}
