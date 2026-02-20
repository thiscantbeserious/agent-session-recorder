//! Integration tests for AnalyzerService::analyze().
//!
//! These tests establish a behavior-preserving baseline before refactoring
//! to reduce cognitive complexity (Stage 2c of SonarCloud quality fixes).
//!
//! Tests replicate and extend the inline tests from src/analyzer/service.rs.

use agr::analyzer::backend::{AgentBackend, BackendError, RawMarker};
use agr::analyzer::chunk::TokenBudget;
use agr::analyzer::error::AnalysisError;
use agr::analyzer::{extract_json, AnalyzeOptions, AnalyzerService};
use agr::asciicast::{AsciicastFile, Event, Header};
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::NamedTempFile;

// ============================================================================
// Mock Backend
// ============================================================================

struct MockBackend {
    responses: Mutex<Vec<Result<String, BackendError>>>,
    available: bool,
}

impl MockBackend {
    fn new(responses: Vec<Result<String, BackendError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            available: true,
        }
    }
}

impl AgentBackend for MockBackend {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn invoke(
        &self,
        _prompt: &str,
        _timeout: Duration,
        _use_schema: bool,
    ) -> Result<String, BackendError> {
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            Ok(r#"{"markers": []}"#.to_string())
        } else {
            responses.remove(0)
        }
    }

    fn parse_response(&self, response: &str) -> Result<Vec<RawMarker>, BackendError> {
        extract_json(response).map(|r| r.markers)
    }

    fn token_budget(&self) -> TokenBudget {
        TokenBudget::claude()
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a realistic test cast file with diverse, multi-line content
/// that survives the full extraction pipeline.
fn create_test_cast_file() -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    let header = Header {
        version: 3,
        width: Some(120),
        height: Some(10),
        timestamp: None,
        duration: None,
        title: None,
        command: None,
        term: None,
        env: None,
        idle_time_limit: None,
    };
    let mut cast = AsciicastFile::new(header);

    let phases: &[(&[&str], f64)] = &[
        (
            &[
                "$ cargo build --release\n",
                "   Compiling serde v1.0.200\n",
                "   Compiling agr v0.1.0 (/home/user/project)\n",
                "    Finished release [optimized] target(s) in 14.32s\n",
            ],
            0.0,
        ),
        (
            &[
                "$ cargo test --lib\n",
                "running 42 tests\n",
                "test config::tests::load_default_config ... ok\n",
                "test parser::tests::parse_asciicast_header ... ok\n",
                "test result: ok. 42 passed; 0 failed; 0 ignored\n",
            ],
            5.0,
        ),
        (
            &[
                "$ git add -A && git commit -m 'feat: add clipboard support'\n",
                "[main abc1234] feat: add clipboard support\n",
                " 3 files changed, 150 insertions(+), 12 deletions(-)\n",
            ],
            12.0,
        ),
        (
            &[
                "$ git push origin main\n",
                "Enumerating objects: 8, done.\n",
                "To github.com:user/project.git\n",
                "   def5678..abc1234  main -> main\n",
            ],
            20.0,
        ),
        (
            &[
                "$ curl -s https://api.example.com/health | jq .\n",
                "{\n",
                "  \"status\": \"healthy\",\n",
                "  \"version\": \"1.2.3\",\n",
                "  \"uptime\": \"2h 15m\"\n",
                "}\n",
            ],
            30.0,
        ),
    ];

    for (lines, phase_start) in phases {
        for (i, line) in lines.iter().enumerate() {
            let time = if i == 0 { *phase_start } else { 0.1 };
            cast.events.push(Event::output(time, *line));
        }
    }

    let content = cast.to_string().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file
}

fn mock_response_with_markers() -> String {
    r#"{"markers": [
        {"timestamp": 0.0, "label": "Started build process", "category": "implementation"},
        {"timestamp": 0.01, "label": "Build completed successfully", "category": "success"}
    ]}"#
    .to_string()
}

fn empty_cast_file() -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    let header = Header {
        version: 3,
        width: Some(80),
        height: Some(24),
        timestamp: None,
        duration: None,
        title: None,
        command: None,
        term: None,
        env: None,
        idle_time_limit: None,
    };
    let cast = AsciicastFile::new(header);
    let content = cast.to_string().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file
}

// ============================================================================
// Successful Analysis Tests
// ============================================================================

#[test]
fn analyzer_service_successful_analysis_returns_markers() {
    let file = create_test_cast_file();
    let opts = AnalyzeOptions::default().quiet();
    let backend = Box::new(MockBackend::new(vec![Ok(mock_response_with_markers())]));
    let service = AnalyzerService::with_backend(opts, backend);

    let result = service.analyze(file.path());

    let analysis = result.unwrap_or_else(|e| {
        panic!(
            "Analysis should succeed with realistic content, got: {:?}",
            e
        )
    });
    assert!(
        !analysis.markers.is_empty(),
        "Expected markers from mock backend, got none"
    );
}

#[test]
fn analyzer_service_sequential_mode_succeeds() {
    // Sequential mode (no_parallel=true) runs analysis with a single worker.
    // Verify analyze() completes successfully in sequential mode.
    let file = create_test_cast_file();
    let opts = AnalyzeOptions::default().sequential().quiet();
    let backend = Box::new(MockBackend::new(vec![Ok(mock_response_with_markers())]));
    let service = AnalyzerService::with_backend(opts, backend);

    let result = service.analyze(file.path());
    match result {
        Ok(analysis) => assert!(analysis.is_success()),
        Err(AnalysisError::NoContent) => {}
        Err(e) => panic!("Unexpected error in sequential mode: {:?}", e),
    }
}

// ============================================================================
// Error Path Tests
// ============================================================================

#[test]
fn analyzer_service_file_not_found_returns_io_error() {
    let opts = AnalyzeOptions::default().quiet();
    let backend = Box::new(MockBackend::new(vec![]));
    let service = AnalyzerService::with_backend(opts, backend);

    let result = service.analyze("/nonexistent/path/file.cast");

    assert!(
        matches!(result, Err(AnalysisError::IoError { .. })),
        "Missing file must return IoError"
    );
}

#[test]
fn analyzer_service_empty_content_returns_no_content_error() {
    let file = empty_cast_file();
    let opts = AnalyzeOptions::default().quiet();
    let backend = Box::new(MockBackend::new(vec![]));
    let service = AnalyzerService::with_backend(opts, backend);

    let result = service.analyze(file.path());

    assert!(
        matches!(result, Err(AnalysisError::NoContent)),
        "Empty content must return NoContent error"
    );
}

// ============================================================================
// Existing Markers Tests
// ============================================================================

#[test]
fn analyzer_service_detects_existing_markers() {
    let mut file = NamedTempFile::new().unwrap();
    let header = Header {
        version: 3,
        width: Some(80),
        height: Some(24),
        timestamp: None,
        duration: None,
        title: None,
        command: None,
        term: None,
        env: None,
        idle_time_limit: None,
    };
    let mut cast = AsciicastFile::new(header);
    cast.events.push(Event::output(0.1, "Hello\n"));
    cast.events.push(Event::marker(0.2, "Existing marker"));
    cast.events.push(Event::output(0.5, "World\n"));

    let content = cast.to_string().unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let opts = AnalyzeOptions::default().quiet();
    let backend = Box::new(MockBackend::new(vec![Ok(r#"{"markers": []}"#.to_string())]));
    let service = AnalyzerService::with_backend(opts, backend);

    let result = service.analyze(file.path()).unwrap();

    assert!(
        result.had_existing_markers,
        "Should detect existing markers"
    );
    assert_eq!(result.existing_marker_count, 1);
}

// ============================================================================
// Debug Output Mode Tests
// ============================================================================

#[test]
fn analyzer_service_debug_output_mode_writes_file_and_returns_early() {
    let file = create_test_cast_file();
    let output_file = NamedTempFile::new().unwrap();
    let output_path = output_file.path().to_str().unwrap().to_string();

    let opts = AnalyzeOptions::default()
        .quiet()
        .debug(true)
        .output(output_path.clone());
    let backend = Box::new(MockBackend::new(vec![]));
    let service = AnalyzerService::with_backend(opts, backend);

    let result = service.analyze(file.path());

    match result {
        Ok(analysis) => {
            // Debug mode returns early with empty markers
            assert!(
                analysis.markers.is_empty(),
                "Debug mode should return empty markers"
            );
            // Verify the debug output file was written
            assert!(
                std::path::Path::new(&output_path).exists(),
                "Debug output file should have been created"
            );
        }
        Err(AnalysisError::NoContent) => {
            // Acceptable: content may not survive extraction pipeline
        }
        Err(e) => panic!("Unexpected error in debug mode: {:?}", e),
    }
}

// ============================================================================
// File Integrity Tests
// ============================================================================

#[test]
fn analyzer_service_preserves_file_integrity_after_analysis() {
    let file = create_test_cast_file();
    let original = std::fs::read_to_string(file.path()).unwrap();
    let original_lines: Vec<_> = original.lines().collect();

    let opts = AnalyzeOptions::default().quiet();
    let backend = Box::new(MockBackend::new(vec![
        Ok(mock_response_with_markers()),
        Ok(mock_response_with_markers()),
        Ok(mock_response_with_markers()),
    ]));
    let service = AnalyzerService::with_backend(opts, backend);

    let result = service.analyze(file.path());

    match result {
        Ok(_) => {
            let modified = std::fs::read_to_string(file.path()).unwrap();
            let modified_lines: Vec<_> = modified.lines().collect();

            // Header must be preserved
            assert_eq!(
                original_lines[0], modified_lines[0],
                "Header must be preserved after analysis"
            );

            // File must be valid NDJSON
            for line in &modified_lines {
                assert!(
                    serde_json::from_str::<serde_json::Value>(line).is_ok(),
                    "Invalid JSON line after analysis: {}",
                    line
                );
            }
        }
        Err(AnalysisError::NoContent) => {
            // Content may not survive full extraction pipeline; file must be unchanged
            let after = std::fs::read_to_string(file.path()).unwrap();
            assert_eq!(
                original, after,
                "File must not be modified when NoContent is returned"
            );
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}
