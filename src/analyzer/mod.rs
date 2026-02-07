//! Content extraction and analysis pipeline for AI agent sessions.
//!
//! This module provides the infrastructure for extracting meaningful content
//! from asciicast recordings for LLM analysis. The pipeline strips ANSI codes,
//! deduplicates progress output, and creates segments with token estimates.
//!
//! # Design Philosophy
//!
//! The extraction pipeline is designed for efficiency with large files (100MB+):
//! - **Single-pass processing**: Content cleaning uses a state machine to avoid
//!   multiple passes over the data
//! - **In-place mutation**: Uses the existing Transform trait for memory efficiency
//! - **Semantic preservation**: Preserves meaningful characters like checkmarks
//!   while stripping visual-only decorations
//!
//! # Module Structure
//!
//! - [`config`] - Pipeline configuration
//! - [`transforms`] - Individual content cleaning transforms
//! - [`extractor`] - Pipeline orchestration and segment creation
//! - [`types`] - Data structures for analysis content and segments
//! - [`chunk`] - Token budget and chunking for parallel analysis
//! - [`backend`] - Agent backend implementations (Strategy pattern)
//! - [`worker`] - Parallel execution using Rayon
//! - [`progress`] - Progress reporting for analysis
//! - [`result`] - Result aggregation and marker writing
//! - [`service`] - AnalyzerService facade (main entry point)

pub mod backend;
pub mod chunk;
mod config;
pub mod error;
mod extractor;
pub mod progress;
mod prompt;
pub mod result;
mod service;
pub mod tracker;
mod transforms;
mod types;
pub mod worker;

// Re-export main types from backend
pub use backend::{
    extract_json, AgentBackend, AgentType, AnalysisResponse, BackendError, BackendResult,
    ClaudeBackend, CodexBackend, GeminiBackend, MarkerCategory, RateLimitInfo, RawMarker,
};

// Re-export chunk types (AgentType moved to backend)
pub use chunk::{AnalysisChunk, ChunkCalculator, ChunkConfig, TimeRange, TokenBudget};

// Re-export other types
pub use crate::config::{AgentAnalysisConfig, AnalysisConfig};
pub use config::ExtractionConfig;
pub use extractor::ContentExtractor;
pub use progress::DefaultProgressReporter;
pub use transforms::{
    ContentCleaner, DeduplicateProgressLines, FilterEmptyEvents, NormalizeWhitespace,
};
pub use types::{AnalysisContent, AnalysisSegment, ExtractionStats, TokenEstimator};
pub use worker::{
    ChunkResult, ParallelExecutor, ProgressReporter, RetryExecutor, WorkerConfig, WorkerScaler,
};

// Re-export result types
pub use result::{
    resolve_timestamp, AggregationReport, MarkerWriter, ResultAggregator, ValidatedMarker,
    WriteReport,
};

// Re-export error types
pub use error::AnalysisError;

// Re-export tracker types
pub use tracker::{ChunkUsage, RetryCoordinator, RetryPolicy, TokenTracker, UsageSummary};

// Re-export service types (main entry point)
pub use prompt::build_analyze_prompt;
pub use service::{AnalysisResult, AnalyzeOptions, AnalyzerService};
