//! Diff-style preview of config migration changes.

use agr::theme::ansi;

/// Print a diff-style preview of the config changes.
///
/// Shows lines that contain added fields with a green `+` prefix.
/// For new files, shows all content as additions.
pub(super) fn print_diff_preview(new_content: &str, added_fields: &[String], is_new_file: bool) {
    // Build a set of full field paths (section.key) for accurate matching
    let added_field_set: std::collections::HashSet<&str> =
        added_fields.iter().map(|s| s.as_str()).collect();

    let mut current_section = String::new();
    let mut section_has_additions = false;
    let mut pending_section_header: Option<String> = None;

    for line in new_content.lines() {
        let trimmed = line.trim();

        // Track section headers - handle standard [section], but skip [[arrays]] and [a.b.c] dotted
        if let Some(section_name) = parse_simple_section_header(trimmed) {
            handle_section_header_line(
                line,
                section_name,
                added_fields,
                is_new_file,
                &mut current_section,
                &mut section_has_additions,
                &mut pending_section_header,
            );
            continue;
        }

        // Check if this line is a field assignment
        if trimmed.contains('=') {
            print_field_line(
                line,
                trimmed,
                &current_section,
                &added_field_set,
                is_new_file,
                section_has_additions,
                &mut pending_section_header,
            );
        } else if is_new_file && !trimmed.is_empty() {
            // For new files, show comments too
            if let Some(header) = pending_section_header.take() {
                println!("{}+{} {}{}", ansi::GREEN, ansi::RESET, ansi::GREEN, header);
            }
            println!("{}+ {}{}", ansi::GREEN, line, ansi::RESET);
        }
    }
}

/// Update state for a section header line and queue it for deferred printing.
fn handle_section_header_line(
    line: &str,
    section_name: &str,
    added_fields: &[String],
    is_new_file: bool,
    current_section: &mut String,
    section_has_additions: &mut bool,
    pending_section_header: &mut Option<String>,
) {
    let is_added_section = added_fields
        .iter()
        .any(|f| f.starts_with(&format!("{}.", section_name)));

    *current_section = section_name.to_string();
    *section_has_additions = is_added_section;

    if is_new_file || is_added_section {
        *pending_section_header = Some(line.to_string());
    } else {
        *pending_section_header = None;
    }
}

/// Print a field assignment line, with green prefix if it is added or the file is new.
fn print_field_line(
    line: &str,
    trimmed: &str,
    current_section: &str,
    added_field_set: &std::collections::HashSet<&str>,
    is_new_file: bool,
    section_has_additions: bool,
    pending_section_header: &mut Option<String>,
) {
    let eq_pos = match trimmed.find('=') {
        Some(pos) => pos,
        None => return,
    };
    let key = trimmed[..eq_pos].trim();
    let full_path = if current_section.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", current_section, key)
    };
    let is_added = added_field_set.contains(full_path.as_str());

    if is_new_file || is_added {
        if let Some(header) = pending_section_header.take() {
            println!("{}+{} {}{}", ansi::GREEN, ansi::RESET, ansi::GREEN, header);
        }
        println!("{}+ {}{}", ansi::GREEN, line, ansi::RESET);
    } else if section_has_additions {
        // Show context: flush pending section header once we know there are additions
        if let Some(header) = pending_section_header.take() {
            println!("  {}", header);
        }
        // Skip showing existing fields to keep diff focused
    }
}

/// Parse a simple TOML section header like `[section]`.
///
/// Returns None for:
/// - Array of tables: `[[array]]`
/// - Dotted keys: `[a.b.c]`
/// - Non-section lines
fn parse_simple_section_header(line: &str) -> Option<&str> {
    let trimmed = line.trim();

    // Must start with [ and end with ]
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }

    // Skip array of tables [[...]]
    if trimmed.starts_with("[[") {
        return None;
    }

    // Extract content between brackets
    let inner = &trimmed[1..trimmed.len() - 1];

    // Skip dotted section names like [a.b.c] - we only handle simple [section]
    if inner.contains('.') {
        return None;
    }

    // Skip empty section names
    if inner.trim().is_empty() {
        return None;
    }

    Some(inner.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- handle_section_header_line ----

    #[test]
    fn handle_section_header_with_additions_queues_pending_header() {
        let mut current_section = String::new();
        let mut section_has_additions = false;
        let mut pending: Option<String> = None;

        handle_section_header_line(
            "[analysis]",
            "analysis",
            &["analysis.workers".to_string()],
            false,
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        assert_eq!(current_section, "analysis");
        assert!(section_has_additions);
        assert_eq!(pending, Some("[analysis]".to_string()));
    }

    #[test]
    fn handle_section_header_no_additions_clears_pending() {
        let mut current_section = "old".to_string();
        let mut section_has_additions = true;
        let mut pending: Option<String> = Some("[old]".to_string());

        handle_section_header_line(
            "[recording]",
            "recording",
            &["analysis.workers".to_string()],
            false,
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        assert_eq!(current_section, "recording");
        assert!(!section_has_additions);
        assert_eq!(pending, None);
    }

    #[test]
    fn handle_section_header_new_file_always_queues() {
        let mut current_section = String::new();
        let mut section_has_additions = false;
        let mut pending: Option<String> = None;

        handle_section_header_line(
            "[shell]",
            "shell",
            &[],  // no additions
            true, // is_new_file = true
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        // new file => pending header is set even without additions
        assert_eq!(pending, Some("[shell]".to_string()));
    }

    #[test]
    fn handle_section_header_always_updates_current_section() {
        let mut current_section = "old_section".to_string();
        let mut section_has_additions = false;
        let mut pending: Option<String> = None;

        handle_section_header_line(
            "[storage]",
            "storage",
            &[],
            false,
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        assert_eq!(current_section, "storage");
    }

    #[test]
    fn handle_section_header_prefix_no_false_positive() {
        // "anal" is a prefix of "analysis" but should NOT match "analysis.workers"
        let mut current_section = String::new();
        let mut section_has_additions = false;
        let mut pending: Option<String> = None;

        handle_section_header_line(
            "[anal]",
            "anal",
            &["analysis.workers".to_string()],
            false,
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        // "anal.workers" is not an entry in added_fields, so no additions
        assert!(!section_has_additions);
        assert_eq!(pending, None);
    }

    // ---- parse_simple_section_header ----

    #[test]
    fn parse_simple_section_header_valid() {
        assert_eq!(parse_simple_section_header("[analysis]"), Some("analysis"));
    }

    #[test]
    fn parse_simple_section_header_array_of_tables_returns_none() {
        assert_eq!(parse_simple_section_header("[[agents]]"), None);
    }

    #[test]
    fn parse_simple_section_header_dotted_returns_none() {
        assert_eq!(parse_simple_section_header("[a.b.c]"), None);
    }

    #[test]
    fn parse_simple_section_header_not_a_section_returns_none() {
        assert_eq!(parse_simple_section_header("workers = 4"), None);
    }

    #[test]
    fn parse_simple_section_header_empty_brackets_returns_none() {
        assert_eq!(parse_simple_section_header("[]"), None);
    }

    #[test]
    fn parse_simple_section_header_trims_whitespace() {
        assert_eq!(
            parse_simple_section_header("[ recording ]"),
            Some("recording")
        );
    }
}
