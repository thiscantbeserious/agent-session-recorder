//! User-friendly error handling for analysis operations.
//!
//! This module provides:
//! - `AnalysisError` - enum with all failure modes and Display trait
//! - Clear, user-friendly error messages (no stack traces)
//!
//! # Error Categories
//!
//! - Agent availability errors (CLI not found)
//! - Timeout errors (agent took too long)
//! - Parse errors (invalid JSON response)
//! - Chunk-level failures
//! - Rate limiting

use crate::analyzer::backend::{AgentType, BackendError};
use std::fmt;
use std::time::Duration;

/// Error type for analysis operations.
///
/// All variants include user-friendly messages suitable for CLI output.
#[derive(Debug)]
pub enum AnalysisError {
    /// Agent CLI is not available on the system.
    AgentNotAvailable {
        /// The agent type that was requested
        agent: AgentType,
    },

    /// Agent timed out while processing a chunk.
    AgentTimeout {
        /// Chunk that timed out
        chunk_id: usize,
        /// Timeout duration
        timeout_secs: u64,
    },

    /// Failed to parse JSON from agent response.
    JsonParseError {
        /// Chunk that failed to parse
        chunk_id: usize,
        /// Truncated response for debugging
        response_preview: String,
    },

    /// A single chunk failed to analyze.
    ChunkFailed {
        /// Chunk that failed
        chunk_id: usize,
        /// Human-readable reason
        reason: String,
    },

    /// All chunks failed to analyze.
    AllChunksFailed {
        /// Number of chunks that failed
        total_chunks: usize,
        /// Errors from each chunk
        errors: Vec<(usize, String)>,
    },

    /// Rate limited by the agent.
    RateLimited {
        /// Suggested retry delay (if provided)
        retry_after: Option<Duration>,
        /// Human-readable message
        message: String,
    },

    /// IO error reading/writing files.
    IoError {
        /// Description of what operation failed
        operation: String,
        /// The underlying error message
        message: String,
    },

    /// No content to analyze after extraction.
    NoContent,
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalysisError::AgentNotAvailable { agent } => {
                write!(
                    f,
                    "Agent '{}' is not available. Please install the {} CLI and ensure it's in your PATH.",
                    agent,
                    agent.command_name()
                )
            }
            AnalysisError::AgentTimeout {
                chunk_id,
                timeout_secs,
            } => {
                write!(
                    f,
                    "Chunk {} timed out after {} seconds. Consider increasing the timeout or reducing chunk size.",
                    chunk_id, timeout_secs
                )
            }
            AnalysisError::JsonParseError {
                chunk_id,
                response_preview,
            } => {
                write!(
                    f,
                    "Failed to parse response for chunk {}. Response preview: {}",
                    chunk_id, response_preview
                )
            }
            AnalysisError::ChunkFailed { chunk_id, reason } => {
                write!(f, "Chunk {} failed: {}", chunk_id, reason)
            }
            AnalysisError::AllChunksFailed {
                total_chunks,
                errors,
            } => {
                write!(f, "All {} chunks failed to analyze.\n", total_chunks)?;
                for (chunk_id, error) in errors.iter().take(5) {
                    write!(f, "  - Chunk {}: {}\n", chunk_id, error)?;
                }
                if errors.len() > 5 {
                    write!(f, "  ... and {} more errors", errors.len() - 5)?;
                }
                Ok(())
            }
            AnalysisError::RateLimited {
                retry_after,
                message,
            } => {
                if let Some(duration) = retry_after {
                    write!(
                        f,
                        "Rate limited: {}. Retry after {} seconds.",
                        message,
                        duration.as_secs()
                    )
                } else {
                    write!(f, "Rate limited: {}. Please wait before retrying.", message)
                }
            }
            AnalysisError::IoError { operation, message } => {
                write!(f, "IO error during {}: {}", operation, message)
            }
            AnalysisError::NoContent => {
                write!(
                    f,
                    "No content to analyze. The recording may be empty or contain only noise."
                )
            }
        }
    }
}

impl std::error::Error for AnalysisError {}

impl AnalysisError {
    /// Create from a BackendError with chunk context.
    pub fn from_backend_error(chunk_id: usize, error: &BackendError) -> Self {
        match error {
            BackendError::NotAvailable(cmd) => {
                // Try to parse agent type from command name
                let agent = match cmd.as_str() {
                    "claude" => AgentType::Claude,
                    "codex" => AgentType::Codex,
                    "gemini" => AgentType::Gemini,
                    _ => AgentType::Claude, // Default
                };
                AnalysisError::AgentNotAvailable { agent }
            }
            BackendError::Timeout(duration) => AnalysisError::AgentTimeout {
                chunk_id,
                timeout_secs: duration.as_secs(),
            },
            BackendError::RateLimited(info) => AnalysisError::RateLimited {
                retry_after: info.retry_after,
                message: info.message.clone(),
            },
            BackendError::JsonParse(_) | BackendError::JsonExtraction { .. } => {
                let preview = match error {
                    BackendError::JsonExtraction { response } => truncate_response(response, 100),
                    _ => "Invalid JSON".to_string(),
                };
                AnalysisError::JsonParseError {
                    chunk_id,
                    response_preview: preview,
                }
            }
            BackendError::ExitCode { code, stderr } => AnalysisError::ChunkFailed {
                chunk_id,
                reason: format!("Exit code {}: {}", code, truncate_response(stderr, 100)),
            },
            BackendError::Io(e) => AnalysisError::ChunkFailed {
                chunk_id,
                reason: format!("IO error: {}", e),
            },
        }
    }

    /// Check if this error is retriable.
    pub fn is_retriable(&self) -> bool {
        matches!(
            self,
            AnalysisError::AgentTimeout { .. }
                | AnalysisError::RateLimited { .. }
                | AnalysisError::ChunkFailed { .. }
        )
    }

    /// Check if this error indicates rate limiting.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, AnalysisError::RateLimited { .. })
    }

    /// Get suggested retry delay for rate limiting.
    pub fn retry_after(&self) -> Option<Duration> {
        if let AnalysisError::RateLimited { retry_after, .. } = self {
            *retry_after
        } else {
            None
        }
    }
}

/// Truncate a response string for display.
fn truncate_response(response: &str, max_len: usize) -> String {
    let trimmed = response.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::backend::RateLimitInfo;

    // ============================================
    // AnalysisError Display Tests
    // ============================================

    #[test]
    fn agent_not_available_message() {
        let err = AnalysisError::AgentNotAvailable {
            agent: AgentType::Claude,
        };

        let msg = format!("{}", err);
        assert!(msg.contains("Claude"));
        assert!(msg.contains("not available"));
        assert!(msg.contains("claude")); // CLI command
    }

    #[test]
    fn agent_timeout_message() {
        let err = AnalysisError::AgentTimeout {
            chunk_id: 2,
            timeout_secs: 60,
        };

        let msg = format!("{}", err);
        assert!(msg.contains("Chunk 2"));
        assert!(msg.contains("60 seconds"));
        assert!(msg.contains("timed out"));
    }

    #[test]
    fn json_parse_error_message() {
        let err = AnalysisError::JsonParseError {
            chunk_id: 1,
            response_preview: "not valid json".to_string(),
        };

        let msg = format!("{}", err);
        assert!(msg.contains("chunk 1"));
        assert!(msg.contains("parse"));
        assert!(msg.contains("not valid json"));
    }

    #[test]
    fn chunk_failed_message() {
        let err = AnalysisError::ChunkFailed {
            chunk_id: 3,
            reason: "Connection refused".to_string(),
        };

        let msg = format!("{}", err);
        assert!(msg.contains("Chunk 3"));
        assert!(msg.contains("Connection refused"));
    }

    #[test]
    fn all_chunks_failed_message() {
        let err = AnalysisError::AllChunksFailed {
            total_chunks: 4,
            errors: vec![
                (0, "Timeout".to_string()),
                (1, "Rate limited".to_string()),
                (2, "Parse error".to_string()),
                (3, "Unknown error".to_string()),
            ],
        };

        let msg = format!("{}", err);
        assert!(msg.contains("4 chunks"));
        assert!(msg.contains("Chunk 0: Timeout"));
        assert!(msg.contains("Chunk 1: Rate limited"));
    }

    #[test]
    fn all_chunks_failed_truncates_errors() {
        let errors: Vec<(usize, String)> = (0..10).map(|i| (i, format!("Error {}", i))).collect();

        let err = AnalysisError::AllChunksFailed {
            total_chunks: 10,
            errors,
        };

        let msg = format!("{}", err);
        assert!(msg.contains("Chunk 4")); // Shows first 5
        assert!(!msg.contains("Chunk 5")); // Doesn't show 6th
        assert!(msg.contains("5 more errors")); // Shows count of remaining
    }

    #[test]
    fn rate_limited_with_retry_after() {
        let err = AnalysisError::RateLimited {
            retry_after: Some(Duration::from_secs(45)),
            message: "Too many requests".to_string(),
        };

        let msg = format!("{}", err);
        assert!(msg.contains("Rate limited"));
        assert!(msg.contains("Too many requests"));
        assert!(msg.contains("45 seconds"));
    }

    #[test]
    fn rate_limited_without_retry_after() {
        let err = AnalysisError::RateLimited {
            retry_after: None,
            message: "Quota exceeded".to_string(),
        };

        let msg = format!("{}", err);
        assert!(msg.contains("Rate limited"));
        assert!(msg.contains("Quota exceeded"));
        assert!(msg.contains("wait before retrying"));
    }

    #[test]
    fn io_error_message() {
        let err = AnalysisError::IoError {
            operation: "reading cast file".to_string(),
            message: "file not found".to_string(),
        };

        let msg = format!("{}", err);
        assert!(msg.contains("reading cast file"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn no_content_message() {
        let err = AnalysisError::NoContent;

        let msg = format!("{}", err);
        assert!(msg.contains("No content"));
        assert!(msg.contains("empty"));
    }

    // ============================================
    // from_backend_error Tests
    // ============================================

    #[test]
    fn from_backend_error_not_available() {
        let backend_err = BackendError::NotAvailable("claude".to_string());
        let err = AnalysisError::from_backend_error(0, &backend_err);

        assert!(matches!(
            err,
            AnalysisError::AgentNotAvailable {
                agent: AgentType::Claude
            }
        ));
    }

    #[test]
    fn from_backend_error_timeout() {
        let backend_err = BackendError::Timeout(Duration::from_secs(120));
        let err = AnalysisError::from_backend_error(5, &backend_err);

        match err {
            AnalysisError::AgentTimeout {
                chunk_id,
                timeout_secs,
            } => {
                assert_eq!(chunk_id, 5);
                assert_eq!(timeout_secs, 120);
            }
            _ => panic!("Expected AgentTimeout"),
        }
    }

    #[test]
    fn from_backend_error_rate_limited() {
        let backend_err = BackendError::RateLimited(RateLimitInfo {
            retry_after: Some(Duration::from_secs(30)),
            message: "Rate limited".to_string(),
        });
        let err = AnalysisError::from_backend_error(0, &backend_err);

        match err {
            AnalysisError::RateLimited {
                retry_after,
                message,
            } => {
                assert_eq!(retry_after, Some(Duration::from_secs(30)));
                assert_eq!(message, "Rate limited");
            }
            _ => panic!("Expected RateLimited"),
        }
    }

    #[test]
    fn from_backend_error_json_extraction() {
        let backend_err = BackendError::JsonExtraction {
            response: "This is not JSON, it's a very long response that should be truncated for display purposes.".to_string(),
        };
        let err = AnalysisError::from_backend_error(2, &backend_err);

        match err {
            AnalysisError::JsonParseError {
                chunk_id,
                response_preview,
            } => {
                assert_eq!(chunk_id, 2);
                assert!(response_preview.len() <= 103); // 100 + "..."
            }
            _ => panic!("Expected JsonParseError"),
        }
    }

    #[test]
    fn from_backend_error_exit_code() {
        let backend_err = BackendError::ExitCode {
            code: 1,
            stderr: "Command failed".to_string(),
        };
        let err = AnalysisError::from_backend_error(3, &backend_err);

        match err {
            AnalysisError::ChunkFailed { chunk_id, reason } => {
                assert_eq!(chunk_id, 3);
                assert!(reason.contains("Exit code 1"));
            }
            _ => panic!("Expected ChunkFailed"),
        }
    }

    // ============================================
    // Helper Method Tests
    // ============================================

    #[test]
    fn is_retriable_timeout() {
        let err = AnalysisError::AgentTimeout {
            chunk_id: 0,
            timeout_secs: 60,
        };
        assert!(err.is_retriable());
    }

    #[test]
    fn is_retriable_rate_limited() {
        let err = AnalysisError::RateLimited {
            retry_after: None,
            message: "".to_string(),
        };
        assert!(err.is_retriable());
    }

    #[test]
    fn is_retriable_chunk_failed() {
        let err = AnalysisError::ChunkFailed {
            chunk_id: 0,
            reason: "".to_string(),
        };
        assert!(err.is_retriable());
    }

    #[test]
    fn is_not_retriable_agent_not_available() {
        let err = AnalysisError::AgentNotAvailable {
            agent: AgentType::Claude,
        };
        assert!(!err.is_retriable());
    }

    #[test]
    fn is_not_retriable_all_chunks_failed() {
        let err = AnalysisError::AllChunksFailed {
            total_chunks: 1,
            errors: vec![],
        };
        assert!(!err.is_retriable());
    }

    #[test]
    fn is_rate_limited_true() {
        let err = AnalysisError::RateLimited {
            retry_after: None,
            message: "".to_string(),
        };
        assert!(err.is_rate_limited());
    }

    #[test]
    fn is_rate_limited_false() {
        let err = AnalysisError::AgentTimeout {
            chunk_id: 0,
            timeout_secs: 60,
        };
        assert!(!err.is_rate_limited());
    }

    #[test]
    fn retry_after_returns_duration() {
        let err = AnalysisError::RateLimited {
            retry_after: Some(Duration::from_secs(45)),
            message: "".to_string(),
        };
        assert_eq!(err.retry_after(), Some(Duration::from_secs(45)));
    }

    #[test]
    fn retry_after_returns_none_for_other_errors() {
        let err = AnalysisError::AgentTimeout {
            chunk_id: 0,
            timeout_secs: 60,
        };
        assert_eq!(err.retry_after(), None);
    }

    // ============================================
    // truncate_response Tests
    // ============================================

    #[test]
    fn truncate_short_response() {
        let result = truncate_response("short", 100);
        assert_eq!(result, "short");
    }

    #[test]
    fn truncate_long_response() {
        let long = "a".repeat(200);
        let result = truncate_response(&long, 50);
        assert!(result.len() <= 53); // 50 + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn truncate_trims_whitespace() {
        let result = truncate_response("  trimmed  ", 100);
        assert_eq!(result, "trimmed");
    }
}
