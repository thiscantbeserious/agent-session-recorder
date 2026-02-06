//! Resize stress test for diagnosing scroll bugs
//!
//! This test processes the resize_stress_excerpt.cast fixture through
//! the TerminalBuffer and captures snapshots at key points to diagnose
//! visual corruption during rapid terminal resize events.

use agr::asciicast::AsciicastFile;
use agr::terminal::TerminalBuffer;
use std::path::Path;

/// Process events from a cast file through a TerminalBuffer, applying resizes.
/// Returns snapshots at specified event indices.
fn process_cast_with_snapshots(
    cast: &AsciicastFile,
    snapshot_indices: &[usize],
) -> Vec<(usize, u32, u32, String)> {
    let (initial_cols, initial_rows) = cast.terminal_size();
    let mut buffer = TerminalBuffer::new(initial_cols as usize, initial_rows as usize);
    let mut snapshots = Vec::new();
    let mut current_cols = initial_cols;
    let mut current_rows = initial_rows;

    for (idx, event) in cast.events.iter().enumerate() {
        if event.is_output() {
            buffer.process(&event.data, None);
        } else if let Some((cols, rows)) = event.parse_resize() {
            buffer.resize(cols as usize, rows as usize);
            current_cols = cols;
            current_rows = rows;
        }

        if snapshot_indices.contains(&idx) {
            snapshots.push((idx, current_cols, current_rows, buffer.to_string()));
        }
    }

    snapshots
}

#[test]
fn analyze_resize_event_distribution() {
    let path = Path::new("tests/fixtures/resize_stress_excerpt.cast");
    if !path.exists() {
        eprintln!("Skipping test: fixture file not found");
        return;
    }

    let cast = AsciicastFile::parse(path).expect("Failed to parse cast file");

    // Count resize events
    let total_resizes = cast.events.iter().filter(|e| e.is_resize()).count();
    let total_outputs = cast.events.iter().filter(|e| e.is_output()).count();

    println!("Total events: {}", cast.events.len());
    println!("Resize events: {}", total_resizes);
    println!("Output events: {}", total_outputs);

    // Find the maximum resize burst (consecutive resizes without output)
    let mut max_burst = 0;
    let mut current_burst = 0;
    for event in &cast.events {
        if event.is_resize() {
            current_burst += 1;
            max_burst = max_burst.max(current_burst);
        } else {
            current_burst = 0;
        }
    }
    println!("Max consecutive resizes: {}", max_burst);

    // Show size progression
    let (initial_cols, _) = cast.terminal_size();
    let mut current_cols = initial_cols;
    let mut min_cols = initial_cols;
    let mut max_cols = initial_cols;
    for event in &cast.events {
        if let Some((cols, _)) = event.parse_resize() {
            current_cols = cols;
            min_cols = min_cols.min(cols);
            max_cols = max_cols.max(cols);
        }
    }
    println!(
        "Column range: {} -> {} (min: {}, max: {})",
        initial_cols, current_cols, min_cols, max_cols
    );
}

#[test]
fn snapshot_terminal_state_at_resize_bursts() {
    let path = Path::new("tests/fixtures/resize_stress_excerpt.cast");
    if !path.exists() {
        eprintln!("Skipping test: fixture file not found");
        return;
    }

    let cast = AsciicastFile::parse(path).expect("Failed to parse cast file");

    // Snapshot indices around problematic areas (based on fixture analysis):
    // - Line 94 (idx 93): Last event before first resize
    // - Line 95 (idx 94): First resize
    // - Line 96-104 (idx 95-103): Large output batch after resize
    // - Line 105-108 (idx 104-107): Resize burst (241->238)
    // - Line 109 (idx 108): Output after resize burst
    let snapshot_indices = vec![
        93,  // Before first resize
        95,  // After first resize + output
        103, // End of first output batch
        108, // After resize burst + next output
        128, // After long resize sequence
        145, // Middle of processing
    ];

    let snapshots = process_cast_with_snapshots(&cast, &snapshot_indices);

    for (idx, cols, rows, content) in &snapshots {
        println!("\n=== Event {} ({}x{}) ===", idx, cols, rows);
        // Show first 20 lines
        for (i, line) in content.lines().take(20).enumerate() {
            println!("{:2}: {}", i, line);
        }
        if content.lines().count() > 20 {
            println!("... ({} more lines)", content.lines().count() - 20);
        }
    }

    // Create insta snapshots for regression testing
    for (idx, cols, rows, content) in &snapshots {
        let snapshot_name = format!("resize_stress_event_{}_{cols}x{rows}", idx);
        insta::with_settings!({
            snapshot_path => "snapshots/player"
        }, {
            insta::assert_snapshot!(snapshot_name, content.clone());
        });
    }
}

#[test]
fn test_rapid_resize_maintains_content_integrity() {
    // Simplified test: create buffer, write content, rapid resize, verify
    let mut buffer = TerminalBuffer::new(100, 24);

    // Write some content
    buffer.process("Line 1: Hello World\r\n", None);
    buffer.process("Line 2: Test Content\r\n", None);
    buffer.process("Line 3: More Text\r\n", None);

    let _before_resize = buffer.to_string();

    // Rapid resize sequence (100 -> 80 -> 60 -> 40 -> 60 -> 80 -> 100)
    buffer.resize(80, 24);
    buffer.resize(60, 24);
    buffer.resize(40, 24);
    buffer.resize(60, 24);
    buffer.resize(80, 24);
    buffer.resize(100, 24);

    let after_resize = buffer.to_string();

    // Content should be preserved (truncated lines restored)
    // Note: truncation is expected for columns that went below content width
    assert!(
        after_resize.contains("Line 1:"),
        "Line 1 prefix should be preserved"
    );
    assert!(
        after_resize.contains("Line 2:"),
        "Line 2 prefix should be preserved"
    );
    assert!(
        after_resize.contains("Line 3:"),
        "Line 3 prefix should be preserved"
    );
}

#[test]
fn test_resize_with_cursor_positioning() {
    let mut buffer = TerminalBuffer::new(100, 24);

    // Fill first few lines
    buffer.process("AAAAAAAAAA\r\n", None);
    buffer.process("BBBBBBBBBB\r\n", None);
    buffer.process("CCCCCCCCCC\r\n", None);

    // Resize to smaller
    buffer.resize(50, 24);

    // Now do cursor positioning that would have been valid at 100 cols
    // Move cursor to column 40 (still valid at 50), row 1
    buffer.process("\x1b[2;41H", None); // Row 2, Column 41
    buffer.process("X", None);

    let output = buffer.to_string();
    let lines: Vec<&str> = output.lines().collect();

    // X should be at row 1 (0-indexed), around column 40
    assert!(lines.len() >= 2, "Should have at least 2 lines");
    assert!(
        lines[1].contains("X"),
        "X should be on line 2: got '{}'",
        lines[1]
    );
}

#[test]
fn test_resize_clamps_cursor() {
    let mut buffer = TerminalBuffer::new(100, 24);

    // Position cursor at far right
    buffer.process("\x1b[1;90H", None); // Row 1, Column 90
    assert_eq!(buffer.cursor_col(), 89); // 0-indexed

    // Resize to smaller
    buffer.resize(50, 24);

    // Cursor should be clamped to new bounds
    assert_eq!(
        buffer.cursor_col(),
        49,
        "Cursor col should be clamped to 49 (width-1)"
    );

    // Write something - at col 49, "TEST" will wrap: 'T' at col 49, "EST" on next row
    buffer.process("TEST", None);

    let output = buffer.to_string();

    // Text wraps correctly: 'T' at end of row 0, 'EST' at start of row 1
    assert!(output.contains('T'), "T should be visible on row 0");
    assert!(
        output.lines().nth(1).is_some_and(|l| l.starts_with("EST")),
        "EST should wrap to row 1, got: '{}'",
        output
    );
}
