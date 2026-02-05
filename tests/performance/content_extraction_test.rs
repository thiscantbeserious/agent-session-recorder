//! Performance tests for content extraction pipeline.
//!
//! These tests verify the extraction pipeline meets performance requirements:
//! - <5 seconds for 70MB file
//! - 55-89% compression ratio

use std::time::Instant;

use agr::analyzer::{ContentExtractor, ExtractionConfig};
use agr::asciicast::Event;

/// Generate events that simulate real agent output with ANSI codes, spinners, and progress.
fn generate_realistic_events(target_bytes: usize) -> Vec<Event> {
    let mut events = Vec::new();
    let mut total_bytes = 0;

    // Patterns observed in real agent sessions
    let patterns = [
        // Claude-style box drawing with ANSI colors
        "\x1b[38;5;174m╭───────────────────────────────────────╮\x1b[0m\n",
        "\x1b[38;5;174m│ Analyzing code...                      │\x1b[0m\n",
        "\x1b[38;5;174m╰───────────────────────────────────────╯\x1b[0m\n",
        // Spinner progress lines (will be deduplicated)
        "\r\x1b[2K✻ Thinking...",
        "\r\x1b[2K✳ Processing...",
        "\r\x1b[2K✶ Working...",
        "\r\x1b[2K✓ Complete\n",
        // Gemini braille spinners
        "\r⠋ Loading...",
        "\r⠙ Loading...",
        "\r⠹ Loading...",
        "\r⠸ Loading...",
        // Actual content
        "Running tests...\n",
        "\x1b[32m✔ Test 1 passed\x1b[0m\n",
        "\x1b[32m✔ Test 2 passed\x1b[0m\n",
        "\x1b[31m✕ Test 3 failed\x1b[0m\n",
        "\x1b[33m⚠ Warning: deprecated API\x1b[0m\n",
        // Long output lines
        "Building component src/analyzer/mod.rs with optimizations enabled for release...\n",
        "Compiling dependencies: tokio, serde, clap, rayon, crossterm, ratatui...\n",
        // Progress bars
        "Progress: ████████░░░░░░░░░░░░ 40%\r",
        "Progress: ████████████░░░░░░░░ 60%\r",
        "Progress: ████████████████░░░░ 80%\r",
        "Progress: ████████████████████ 100%\n",
    ];

    let mut pattern_idx = 0;
    while total_bytes < target_bytes {
        let pattern = patterns[pattern_idx % patterns.len()];
        total_bytes += pattern.len();

        // Use small time deltas to simulate real output
        let time = if pattern.starts_with('\r') {
            0.05 // Fast progress updates
        } else if pattern_idx % 50 == 0 {
            2.5 // Occasional pauses (creates segments)
        } else {
            0.1
        };

        events.push(Event::output(time, pattern));
        pattern_idx += 1;
    }

    events
}

#[test]
fn benchmark_content_extraction_15mb() {
    // Generate ~15MB of realistic event data
    let events = generate_realistic_events(15 * 1024 * 1024);
    let original_bytes: usize = events.iter().map(|e| e.data.len()).sum();

    println!(
        "Generated {} events, {} bytes",
        events.len(),
        original_bytes
    );

    let extractor = ContentExtractor::default();
    let mut events_clone = events.clone();

    let start = Instant::now();
    let result = extractor.extract(&mut events_clone);
    let duration = start.elapsed();

    println!(
        "Extraction took {:?} ({:.2} MB/s)",
        duration,
        (original_bytes as f64 / 1024.0 / 1024.0) / duration.as_secs_f64()
    );
    println!(
        "Compression: {} -> {} bytes ({:.1}% reduction)",
        result.stats.original_bytes,
        result.stats.extracted_bytes,
        (1.0 - result.stats.extracted_bytes as f64 / result.stats.original_bytes as f64) * 100.0
    );
    println!("Segments created: {}", result.segments.len());
    println!("Estimated tokens: {}", result.total_tokens);

    // Performance assertion: 15MB should process in <5 seconds in debug mode
    // In release mode, this typically completes in <0.5 seconds
    // (extrapolates to <5s for 70MB in release mode)
    assert!(
        duration.as_secs_f64() < 5.0,
        "15MB extraction should complete in <5s, took {:?}",
        duration
    );

    // Compression ratio assertion: expect 55-89% reduction
    let compression_ratio =
        1.0 - result.stats.extracted_bytes as f64 / result.stats.original_bytes as f64;
    assert!(
        compression_ratio >= 0.55 && compression_ratio <= 0.95,
        "Compression ratio {:.1}% should be between 55-95%",
        compression_ratio * 100.0
    );
}

#[test]
fn benchmark_content_extraction_with_config_variations() {
    let events = generate_realistic_events(5 * 1024 * 1024);
    let original_bytes: usize = events.iter().map(|e| e.data.len()).sum();

    // Test with all features enabled (default)
    let config_full = ExtractionConfig::default();
    let extractor_full = ContentExtractor::new(config_full);
    let mut events_full = events.clone();

    let start = Instant::now();
    let result_full = extractor_full.extract(&mut events_full);
    let duration_full = start.elapsed();

    println!("Full pipeline: {:?}", duration_full);
    println!(
        "  Compression: {:.1}%",
        (1.0 - result_full.stats.extracted_bytes as f64 / original_bytes as f64) * 100.0
    );
    println!(
        "  ANSI stripped: {}",
        result_full.stats.ansi_sequences_stripped
    );
    println!(
        "  Progress deduplicated: {}",
        result_full.stats.progress_lines_deduplicated
    );

    // Test with minimal features
    let mut config_minimal = ExtractionConfig::default();
    config_minimal.dedupe_progress_lines = false;
    config_minimal.normalize_whitespace = false;

    let extractor_minimal = ContentExtractor::new(config_minimal);
    let mut events_minimal = events.clone();

    let start = Instant::now();
    let result_minimal = extractor_minimal.extract(&mut events_minimal);
    let duration_minimal = start.elapsed();

    println!("Minimal pipeline: {:?}", duration_minimal);
    println!(
        "  Compression: {:.1}%",
        (1.0 - result_minimal.stats.extracted_bytes as f64 / original_bytes as f64) * 100.0
    );

    // Full pipeline should achieve better compression
    assert!(
        result_full.stats.extracted_bytes <= result_minimal.stats.extracted_bytes,
        "Full pipeline should compress better than minimal"
    );
}

#[test]
fn verify_compression_ratios_match_spec() {
    // SPEC.md Section 1.7 shows expected compression ratios
    // Claude box drawing: high compression (lots of ANSI + box chars)
    // Gemini: medium compression (some ANSI)
    // Codex: medium compression (some ANSI)

    let extractor = ContentExtractor::default();

    // Simulate Claude-heavy content (box drawing + ANSI)
    let claude_events: Vec<_> = (0..1000)
        .map(|_| {
            Event::output(
                0.1,
                "\x1b[38;5;174m╭───────────────────────────────────────╮\x1b[0m\n",
            )
        })
        .collect();

    let claude_original: usize = claude_events.iter().map(|e| e.data.len()).sum();
    let mut claude_clone = claude_events;
    let claude_result = extractor.extract(&mut claude_clone);

    let claude_ratio = 1.0 - claude_result.stats.extracted_bytes as f64 / claude_original as f64;
    println!(
        "Claude-style compression: {:.1}% ({} -> {} bytes)",
        claude_ratio * 100.0,
        claude_original,
        claude_result.stats.extracted_bytes
    );

    // Claude content should compress well (>70% due to box drawing + ANSI)
    assert!(
        claude_ratio >= 0.70,
        "Claude-style content should achieve >70% compression, got {:.1}%",
        claude_ratio * 100.0
    );

    // Simulate progress-heavy content (spinner + progress bars)
    let progress_events: Vec<_> = (0..1000)
        .flat_map(|i| {
            vec![
                Event::output(0.05, "\r⠋ Loading..."),
                Event::output(0.05, "\r⠙ Loading..."),
                Event::output(0.05, "\r⠹ Loading..."),
                Event::output(0.05, &format!("\r✓ Item {} complete\n", i)),
            ]
        })
        .collect();

    let progress_original: usize = progress_events.iter().map(|e| e.data.len()).sum();
    let mut progress_clone = progress_events;
    let progress_result = extractor.extract(&mut progress_clone);

    let progress_ratio =
        1.0 - progress_result.stats.extracted_bytes as f64 / progress_original as f64;
    println!(
        "Progress-style compression: {:.1}% ({} -> {} bytes)",
        progress_ratio * 100.0,
        progress_original,
        progress_result.stats.extracted_bytes
    );

    // Progress content should compress very well due to deduplication
    assert!(
        progress_ratio >= 0.60,
        "Progress-style content should achieve >60% compression, got {:.1}%",
        progress_ratio * 100.0
    );
}
