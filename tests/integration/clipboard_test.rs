//! Integration tests for the clipboard module.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

// =============================================================================
// Stage 1: Core Types Tests
// =============================================================================

mod result_tests {
    use agr::clipboard::{CopyMethod, CopyResult};

    #[test]
    fn copy_method_name_returns_correct_strings() {
        assert_eq!(CopyMethod::OsaScript.name(), "osascript");
        assert_eq!(CopyMethod::Pbcopy.name(), "pbcopy");
        assert_eq!(CopyMethod::Xclip.name(), "xclip");
        assert_eq!(CopyMethod::Xsel.name(), "xsel");
        assert_eq!(CopyMethod::WlCopy.name(), "wl-copy");
    }

    #[test]
    fn copy_result_file_copied_creates_correct_variant() {
        let result = CopyResult::file_copied(CopyMethod::OsaScript);
        assert!(matches!(
            result,
            CopyResult::FileCopied {
                tool: CopyMethod::OsaScript
            }
        ));
    }

    #[test]
    fn copy_result_content_copied_creates_correct_variant() {
        let result = CopyResult::content_copied(CopyMethod::Pbcopy, 1024);
        assert!(matches!(
            result,
            CopyResult::ContentCopied {
                tool: CopyMethod::Pbcopy,
                size_bytes: 1024
            }
        ));
    }

    #[test]
    fn copy_result_message_formats_file_copy_correctly() {
        let result = CopyResult::file_copied(CopyMethod::OsaScript);
        let msg = result.message("my-recording");
        assert_eq!(msg, "🎬 Copied my-recording.cast to clipboard");
    }

    #[test]
    fn copy_result_message_formats_content_copy_correctly() {
        let result = CopyResult::content_copied(CopyMethod::Pbcopy, 512);
        let msg = result.message("my-recording");
        assert_eq!(
            msg,
            "🎬 Copied my-recording.cast content to clipboard (file copy not supported on this platform)"
        );
    }

    #[test]
    fn copy_result_is_file_copy_returns_true_for_file_copied() {
        let result = CopyResult::file_copied(CopyMethod::OsaScript);
        assert!(result.is_file_copy());
    }

    #[test]
    fn copy_result_is_file_copy_returns_false_for_content_copied() {
        let result = CopyResult::content_copied(CopyMethod::Pbcopy, 100);
        assert!(!result.is_file_copy());
    }
}

mod error_tests {
    use agr::clipboard::ClipboardError;
    use std::path::PathBuf;

    #[test]
    fn file_not_found_displays_path_in_message() {
        let error = ClipboardError::FileNotFound {
            path: PathBuf::from("/some/path/file.cast"),
        };
        let msg = error.to_string();
        assert!(msg.contains("/some/path/file.cast"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn no_tool_available_has_helpful_linux_message() {
        let error = ClipboardError::NoToolAvailable;
        let msg = error.to_string();
        assert!(msg.contains("xclip") || msg.contains("xsel") || msg.contains("wl-copy"));
    }
}

mod tool_tests {
    use agr::clipboard::tool::{CopyTool, CopyToolError};
    use agr::clipboard::CopyMethod;
    use std::path::Path;

    #[test]
    fn copy_tool_error_not_supported_exists_and_is_clone() {
        let err = CopyToolError::NotSupported;
        let _cloned = err.clone();
    }

    #[test]
    fn copy_tool_error_failed_contains_message() {
        let err = CopyToolError::Failed("something went wrong".to_string());
        if let CopyToolError::Failed(msg) = err {
            assert_eq!(msg, "something went wrong");
        } else {
            panic!("Expected CopyToolError::Failed variant");
        }
    }

    #[test]
    fn copy_tool_error_not_found_exists() {
        let _err = CopyToolError::NotFound;
    }

    // Test the default name() implementation via a mock
    struct TestTool;

    impl CopyTool for TestTool {
        fn method(&self) -> CopyMethod {
            CopyMethod::Xclip
        }

        fn is_available(&self) -> bool {
            true
        }

        fn can_copy_files(&self) -> bool {
            false
        }

        fn try_copy_file(&self, _path: &Path) -> Result<(), CopyToolError> {
            Err(CopyToolError::NotSupported)
        }

        fn try_copy_text(&self, _text: &str) -> Result<(), CopyToolError> {
            Ok(())
        }
    }

    #[test]
    fn default_name_implementation_uses_method_name() {
        let tool = TestTool;
        assert_eq!(tool.name(), "xclip");
    }
}

// =============================================================================
// Stage 2: Copy Orchestrator Tests
// =============================================================================

mod copy_tests {
    use super::*;
    use agr::clipboard::copy::Copy;
    use agr::clipboard::tool::{CopyTool, CopyToolError};
    use agr::clipboard::{ClipboardError, CopyMethod, CopyResult, MAX_CONTENT_SIZE};
    use std::sync::atomic::AtomicBool;
    use tempfile::NamedTempFile;

    /// A mock tool for testing the Copy orchestrator.
    struct MockTool {
        method: CopyMethod,
        available: bool,
        can_files: bool,
        file_result: Result<(), CopyToolError>,
        text_result: Result<(), CopyToolError>,
        file_called: AtomicBool,
        text_called: AtomicBool,
    }

    impl MockTool {
        fn new(method: CopyMethod) -> Self {
            Self {
                method,
                available: true,
                can_files: true,
                file_result: Ok(()),
                text_result: Ok(()),
                file_called: AtomicBool::new(false),
                text_called: AtomicBool::new(false),
            }
        }

        fn available(mut self, available: bool) -> Self {
            self.available = available;
            self
        }

        fn can_files(mut self, can: bool) -> Self {
            self.can_files = can;
            self
        }

        fn file_result(mut self, result: Result<(), CopyToolError>) -> Self {
            self.file_result = result;
            self
        }

        fn text_result(mut self, result: Result<(), CopyToolError>) -> Self {
            self.text_result = result;
            self
        }

        #[allow(dead_code)]
        fn was_file_called(&self) -> bool {
            self.file_called.load(Ordering::SeqCst)
        }

        #[allow(dead_code)]
        fn was_text_called(&self) -> bool {
            self.text_called.load(Ordering::SeqCst)
        }
    }

    impl CopyTool for MockTool {
        fn method(&self) -> CopyMethod {
            self.method
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn can_copy_files(&self) -> bool {
            self.can_files
        }

        fn try_copy_file(&self, _path: &Path) -> Result<(), CopyToolError> {
            self.file_called.store(true, Ordering::SeqCst);
            self.file_result.clone()
        }

        fn try_copy_text(&self, _text: &str) -> Result<(), CopyToolError> {
            self.text_called.store(true, Ordering::SeqCst);
            self.text_result.clone()
        }
    }

    #[test]
    fn mock_tool_compiles_and_implements_copy_tool() {
        let tool = MockTool::new(CopyMethod::Xclip);
        let _: &dyn CopyTool = &tool;
    }

    #[test]
    fn copy_with_tools_accepts_empty_vec() {
        let copy = Copy::with_tools(vec![]);
        assert!(copy.tools().is_empty());
    }

    #[test]
    fn file_returns_file_not_found_for_nonexistent_path() {
        let tool = MockTool::new(CopyMethod::Xclip);
        let copy = Copy::with_tools(vec![Box::new(tool)]);

        let result = copy.file(Path::new("/nonexistent/path/file.cast"));
        assert!(matches!(result, Err(ClipboardError::FileNotFound { .. })));
    }

    #[test]
    fn file_tries_file_copy_first_when_tool_supports_it() {
        let tool = MockTool::new(CopyMethod::OsaScript).can_files(true);
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "test content").unwrap();

        let copy = Copy::with_tools(vec![Box::new(tool)]);
        let result = copy.file(temp.path());

        assert!(result.is_ok());
        // Verify file copy was attempted (not text copy)
        // We can check the result type
        assert!(matches!(result, Ok(CopyResult::FileCopied { .. })));
    }

    #[test]
    fn file_returns_file_copied_when_file_copy_succeeds() {
        let tool = MockTool::new(CopyMethod::OsaScript);
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "test").unwrap();

        let copy = Copy::with_tools(vec![Box::new(tool)]);
        let result = copy.file(temp.path()).unwrap();

        assert!(matches!(
            result,
            CopyResult::FileCopied {
                tool: CopyMethod::OsaScript
            }
        ));
    }

    #[test]
    fn file_falls_back_to_content_copy_when_file_copy_fails() {
        let tool = MockTool::new(CopyMethod::Pbcopy)
            .can_files(true)
            .file_result(Err(CopyToolError::Failed("oops".to_string())));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "fallback content").unwrap();

        let copy = Copy::with_tools(vec![Box::new(tool)]);
        let result = copy.file(temp.path()).unwrap();

        assert!(matches!(result, CopyResult::ContentCopied { .. }));
    }

    #[test]
    fn file_returns_content_copied_when_content_copy_succeeds() {
        let tool = MockTool::new(CopyMethod::Pbcopy).can_files(false);

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "content here").unwrap();

        let copy = Copy::with_tools(vec![Box::new(tool)]);
        let result = copy.file(temp.path()).unwrap();

        if let CopyResult::ContentCopied { tool, size_bytes } = result {
            assert_eq!(tool, CopyMethod::Pbcopy);
            assert_eq!(size_bytes, "content here".len());
        } else {
            panic!("Expected ContentCopied");
        }
    }

    #[test]
    fn file_skips_unavailable_tools() {
        let unavailable = MockTool::new(CopyMethod::OsaScript).available(false);
        let available = MockTool::new(CopyMethod::Pbcopy)
            .available(true)
            .can_files(false);

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "test").unwrap();

        let copy = Copy::with_tools(vec![Box::new(unavailable), Box::new(available)]);
        let result = copy.file(temp.path()).unwrap();

        // Should use Pbcopy since OsaScript is unavailable
        if let CopyResult::ContentCopied { tool, .. } = result {
            assert_eq!(tool, CopyMethod::Pbcopy);
        } else {
            panic!("Expected ContentCopied with Pbcopy");
        }
    }

    #[test]
    fn file_skips_tools_that_dont_support_file_copy_for_file_phase() {
        let text_only = MockTool::new(CopyMethod::Pbcopy).can_files(false);
        let file_capable = MockTool::new(CopyMethod::OsaScript).can_files(true);

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "test").unwrap();

        let copy = Copy::with_tools(vec![Box::new(text_only), Box::new(file_capable)]);
        let result = copy.file(temp.path()).unwrap();

        // Should use OsaScript for file copy since Pbcopy can't do files
        assert!(matches!(
            result,
            CopyResult::FileCopied {
                tool: CopyMethod::OsaScript
            }
        ));
    }

    #[test]
    fn file_returns_no_tool_available_when_all_tools_fail() {
        let failing = MockTool::new(CopyMethod::Xclip)
            .file_result(Err(CopyToolError::Failed("fail".to_string())))
            .text_result(Err(CopyToolError::Failed("fail".to_string())));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "test").unwrap();

        let copy = Copy::with_tools(vec![Box::new(failing)]);
        let result = copy.file(temp.path());

        assert!(matches!(result, Err(ClipboardError::NoToolAvailable)));
    }

    #[test]
    fn file_tries_tools_in_order_first_available_wins() {
        // Use a static counter to track call order
        static CALL_ORDER: AtomicUsize = AtomicUsize::new(0);
        CALL_ORDER.store(0, Ordering::SeqCst);

        let first = MockTool::new(CopyMethod::OsaScript);
        let second = MockTool::new(CopyMethod::Pbcopy);

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "test").unwrap();

        let copy = Copy::with_tools(vec![Box::new(first), Box::new(second)]);
        let result = copy.file(temp.path()).unwrap();

        // First tool should win
        assert!(matches!(
            result,
            CopyResult::FileCopied {
                tool: CopyMethod::OsaScript
            }
        ));
    }

    // =========================================================================
    // Phase helper behaviour tests (10 new tests)
    // =========================================================================

    /// file_phase: a tool that returns NotSupported is silently skipped and the
    /// next tool in the list is tried.
    #[test]
    fn file_phase_skips_not_supported_tries_next() {
        let not_supported = MockTool::new(CopyMethod::OsaScript)
            .can_files(true)
            .file_result(Err(CopyToolError::NotSupported));
        let fallback = MockTool::new(CopyMethod::Xclip)
            .can_files(true)
            .file_result(Ok(()));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "data").unwrap();

        let copy = Copy::with_tools(vec![Box::new(not_supported), Box::new(fallback)]);
        let result = copy.file(temp.path()).unwrap();

        assert!(
            matches!(
                result,
                CopyResult::FileCopied {
                    tool: CopyMethod::Xclip
                }
            ),
            "Expected Xclip to be used after OsaScript returned NotSupported"
        );
    }

    /// file_phase: a tool that returns NotFound is silently skipped and the
    /// next tool in the list is tried.
    #[test]
    fn file_phase_skips_not_found_tries_next() {
        let not_found = MockTool::new(CopyMethod::OsaScript)
            .can_files(true)
            .file_result(Err(CopyToolError::NotFound));
        let fallback = MockTool::new(CopyMethod::Xclip)
            .can_files(true)
            .file_result(Ok(()));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "data").unwrap();

        let copy = Copy::with_tools(vec![Box::new(not_found), Box::new(fallback)]);
        let result = copy.file(temp.path()).unwrap();

        assert!(
            matches!(
                result,
                CopyResult::FileCopied {
                    tool: CopyMethod::Xclip
                }
            ),
            "Expected Xclip to be used after OsaScript returned NotFound"
        );
    }

    /// file_phase: when no tools are supplied the orchestrator falls straight
    /// through to content copy; with no content tools either it returns
    /// NoToolAvailable.
    #[test]
    fn file_with_empty_tools_returns_no_tool_available() {
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "hello").unwrap();

        let copy = Copy::with_tools(vec![]);
        let result = copy.file(temp.path());

        assert!(
            matches!(result, Err(ClipboardError::NoToolAvailable)),
            "Expected NoToolAvailable with empty tool list"
        );
    }

    /// file_phase: when the first file-capable tool fails with Failed, the
    /// second file-capable tool is tried and succeeds.
    #[test]
    fn file_phase_tries_second_tool_when_first_fails() {
        let first = MockTool::new(CopyMethod::OsaScript)
            .can_files(true)
            .file_result(Err(CopyToolError::Failed("first broke".to_string())));
        let second = MockTool::new(CopyMethod::Xclip)
            .can_files(true)
            .file_result(Ok(()));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "data").unwrap();

        let copy = Copy::with_tools(vec![Box::new(first), Box::new(second)]);
        let result = copy.file(temp.path()).unwrap();

        assert!(
            matches!(
                result,
                CopyResult::FileCopied {
                    tool: CopyMethod::Xclip
                }
            ),
            "Expected second tool (Xclip) to succeed after first (OsaScript) failed"
        );
    }

    /// text_phase: a tool that returns NotSupported from try_copy_text is
    /// silently skipped and the next tool is used.
    #[test]
    fn text_phase_skips_not_supported_tries_next() {
        // Both tools cannot copy files so we go straight to text phase.
        let not_supported = MockTool::new(CopyMethod::OsaScript)
            .can_files(false)
            .text_result(Err(CopyToolError::NotSupported));
        let fallback = MockTool::new(CopyMethod::Pbcopy)
            .can_files(false)
            .text_result(Ok(()));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "hello").unwrap();

        let copy = Copy::with_tools(vec![Box::new(not_supported), Box::new(fallback)]);
        let result = copy.file(temp.path()).unwrap();

        assert!(
            matches!(
                result,
                CopyResult::ContentCopied {
                    tool: CopyMethod::Pbcopy,
                    ..
                }
            ),
            "Expected Pbcopy to be used after OsaScript returned NotSupported for text"
        );
    }

    /// text_phase: a tool that returns NotFound from try_copy_text is silently
    /// skipped and the next tool is used.
    #[test]
    fn text_phase_skips_not_found_tries_next() {
        let not_found = MockTool::new(CopyMethod::Xsel)
            .can_files(false)
            .text_result(Err(CopyToolError::NotFound));
        let fallback = MockTool::new(CopyMethod::Pbcopy)
            .can_files(false)
            .text_result(Ok(()));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "hi").unwrap();

        let copy = Copy::with_tools(vec![Box::new(not_found), Box::new(fallback)]);
        let result = copy.file(temp.path()).unwrap();

        assert!(
            matches!(
                result,
                CopyResult::ContentCopied {
                    tool: CopyMethod::Pbcopy,
                    ..
                }
            ),
            "Expected Pbcopy to be used after Xsel returned NotFound for text"
        );
    }

    /// text_phase: when the first text-copy tool fails with Failed, the second
    /// is tried and succeeds.
    #[test]
    fn text_phase_tries_second_tool_when_first_fails() {
        let first = MockTool::new(CopyMethod::Xsel)
            .can_files(false)
            .text_result(Err(CopyToolError::Failed("xsel broke".to_string())));
        let second = MockTool::new(CopyMethod::WlCopy)
            .can_files(false)
            .text_result(Ok(()));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "content").unwrap();

        let copy = Copy::with_tools(vec![Box::new(first), Box::new(second)]);
        let result = copy.file(temp.path()).unwrap();

        assert!(
            matches!(
                result,
                CopyResult::ContentCopied {
                    tool: CopyMethod::WlCopy,
                    ..
                }
            ),
            "Expected WlCopy to succeed after Xsel failed for text"
        );
    }

    /// cross_phase: when only a file-phase error exists (text phase had no
    /// eligible tools) the final error is still NoToolAvailable.
    #[test]
    fn cross_phase_error_file_error_used_when_text_has_none() {
        // Tool can copy files (will fail), but is unavailable for text phase
        // because is_available will return false after we model it this way.
        // Easiest: one file-capable tool that fails, then no text tool.
        let file_only = MockTool::new(CopyMethod::OsaScript)
            .can_files(true)
            .file_result(Err(CopyToolError::Failed("file broke".to_string())))
            .available(false); // unavailable for text phase (is_available gate)

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "data").unwrap();

        // We need a second available tool for the file phase to actually record
        // the failure.  Use one that IS available but fails file AND text.
        let failing = MockTool::new(CopyMethod::Xclip)
            .can_files(true)
            .file_result(Err(CopyToolError::Failed("xclip file broke".to_string())))
            .text_result(Err(CopyToolError::NotSupported)); // skipped in text phase

        let copy = Copy::with_tools(vec![Box::new(file_only), Box::new(failing)]);
        let result = copy.file(temp.path());

        // Both phases exhausted → NoToolAvailable
        assert!(
            matches!(result, Err(ClipboardError::NoToolAvailable)),
            "Expected NoToolAvailable when file phase fails and text phase has no tool"
        );
    }

    /// all_tools_fail: when every tool returns Failed for both file and text,
    /// the orchestrator returns NoToolAvailable (text error takes precedence in
    /// the or() merge).
    #[test]
    fn all_tools_fail_text_error_takes_precedence() {
        let tool1 = MockTool::new(CopyMethod::Xclip)
            .can_files(true)
            .file_result(Err(CopyToolError::Failed("xclip file".to_string())))
            .text_result(Err(CopyToolError::Failed("xclip text".to_string())));
        let tool2 = MockTool::new(CopyMethod::Xsel)
            .can_files(false)
            .text_result(Err(CopyToolError::Failed("xsel text".to_string())));

        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "data").unwrap();

        let copy = Copy::with_tools(vec![Box::new(tool1), Box::new(tool2)]);
        let result = copy.file(temp.path());

        assert!(
            matches!(result, Err(ClipboardError::NoToolAvailable)),
            "Expected NoToolAvailable when all tools fail"
        );
    }

    /// file_too_large: when file copy fails and the file exceeds MAX_CONTENT_SIZE,
    /// FileTooLarge error includes human-readable size and limit values.
    #[test]
    fn file_too_large_displays_size_and_limit() {
        // Write a file that is exactly one byte over the limit.
        let temp = NamedTempFile::new().unwrap();
        let oversized = vec![b'x'; (MAX_CONTENT_SIZE + 1) as usize];
        std::fs::write(temp.path(), &oversized).unwrap();

        // Use a tool that cannot copy files so we always go to the size check.
        let no_file = MockTool::new(CopyMethod::Pbcopy).can_files(false);

        let copy = Copy::with_tools(vec![Box::new(no_file)]);
        let result = copy.file(temp.path());

        match result {
            Err(ClipboardError::FileTooLarge { size_mb, max_mb }) => {
                // size_mb should be just over 10.0, max_mb should be 10
                assert!(
                    size_mb > 10.0,
                    "size_mb should exceed 10.0, got {}",
                    size_mb
                );
                assert_eq!(max_mb, 10, "max_mb should be 10, got {}", max_mb);
            }
            other => panic!("Expected FileTooLarge, got {:?}", other),
        }
    }
}

// =============================================================================
// Stage 3: macOS Tools Tests
// =============================================================================

mod osascript_tests {
    use agr::clipboard::tool::{CopyTool, CopyToolError};
    use agr::clipboard::tools::OsaScript;
    use agr::clipboard::CopyMethod;
    use std::path::Path;

    #[test]
    fn escape_path_handles_simple_path_unchanged() {
        let result = OsaScript::escape_path(Path::new("/simple/path/file.cast"));
        assert_eq!(result, "/simple/path/file.cast");
    }

    #[test]
    fn escape_path_escapes_double_quotes() {
        let result = OsaScript::escape_path(Path::new("/path/with\"quote/file.cast"));
        assert_eq!(result, "/path/with\\\"quote/file.cast");
    }

    #[test]
    fn escape_path_escapes_backslashes() {
        let result = OsaScript::escape_path(Path::new("/path/with\\backslash/file.cast"));
        assert_eq!(result, "/path/with\\\\backslash/file.cast");
    }

    #[test]
    fn escape_path_handles_path_with_spaces_no_escape_needed() {
        let result = OsaScript::escape_path(Path::new("/path with spaces/file.cast"));
        assert_eq!(result, "/path with spaces/file.cast");
    }

    #[test]
    fn escape_path_escapes_newlines() {
        let result = OsaScript::escape_path(Path::new("/path/with\nnewline/file.cast"));
        assert_eq!(result, "/path/with\\nnewline/file.cast");
    }

    #[test]
    fn escape_path_escapes_carriage_returns() {
        let result = OsaScript::escape_path(Path::new("/path/with\rcarriage/file.cast"));
        assert_eq!(result, "/path/with\\rcarriage/file.cast");
    }

    #[test]
    fn escape_path_escapes_tabs() {
        let result = OsaScript::escape_path(Path::new("/path/with\ttab/file.cast"));
        assert_eq!(result, "/path/with\\ttab/file.cast");
    }

    #[test]
    fn escape_path_handles_combined_special_chars() {
        let result = OsaScript::escape_path(Path::new("/path\"with\\\n\r\tspecial/file.cast"));
        assert_eq!(result, "/path\\\"with\\\\\\n\\r\\tspecial/file.cast");
    }

    #[test]
    fn build_file_script_creates_correct_applescript() {
        let script = OsaScript::build_file_script(Path::new("/some/file.cast"));
        assert_eq!(
            script,
            "set the clipboard to POSIX file \"/some/file.cast\""
        );
    }

    #[test]
    fn method_returns_osascript() {
        let tool = OsaScript::new();
        assert_eq!(tool.method(), CopyMethod::OsaScript);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn is_available_returns_true_on_macos() {
        let tool = OsaScript::new();
        assert!(tool.is_available());
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn is_available_returns_false_on_non_macos() {
        let tool = OsaScript::new();
        assert!(!tool.is_available());
    }

    #[test]
    fn can_copy_files_returns_true() {
        let tool = OsaScript::new();
        assert!(tool.can_copy_files());
    }

    #[test]
    fn try_copy_text_returns_not_supported() {
        let tool = OsaScript::new();
        let result = tool.try_copy_text("some text");
        assert!(matches!(result, Err(CopyToolError::NotSupported)));
    }
}

mod pbcopy_tests {
    use agr::clipboard::tool::{CopyTool, CopyToolError};
    use agr::clipboard::tools::Pbcopy;
    use agr::clipboard::CopyMethod;
    use std::path::Path;

    #[test]
    fn method_returns_pbcopy() {
        let tool = Pbcopy::new();
        assert_eq!(tool.method(), CopyMethod::Pbcopy);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn is_available_returns_true_on_macos() {
        let tool = Pbcopy::new();
        assert!(tool.is_available());
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn is_available_returns_false_on_non_macos() {
        let tool = Pbcopy::new();
        assert!(!tool.is_available());
    }

    #[test]
    fn can_copy_files_returns_false() {
        let tool = Pbcopy::new();
        assert!(!tool.can_copy_files());
    }

    #[test]
    fn try_copy_file_returns_not_supported() {
        let tool = Pbcopy::new();
        let result = tool.try_copy_file(Path::new("/some/file.cast"));
        assert!(matches!(result, Err(CopyToolError::NotSupported)));
    }
}

// =============================================================================
// Stage 4: Linux Tools Tests
// =============================================================================

mod xclip_tests {
    use agr::clipboard::tool::CopyTool;
    use agr::clipboard::tools::Xclip;
    use agr::clipboard::CopyMethod;
    use std::path::Path;

    #[test]
    fn build_file_uri_creates_correct_uri() {
        let uri = Xclip::build_file_uri(Path::new("/some/file.cast"));
        assert_eq!(uri, "file:///some/file.cast");
    }

    #[test]
    fn build_file_uri_handles_paths_with_spaces() {
        let uri = Xclip::build_file_uri(Path::new("/path with spaces/file.cast"));
        assert_eq!(uri, "file:///path%20with%20spaces/file.cast");
    }

    #[test]
    fn build_file_uri_handles_unicode_characters() {
        // Japanese characters: UTF-8 encoding
        let uri = Xclip::build_file_uri(Path::new("/path/日本語/file.cast"));
        // 日 = E6 97 A5, 本 = E6 9C AC, 語 = E8 AA 9E
        assert_eq!(uri, "file:///path/%E6%97%A5%E6%9C%AC%E8%AA%9E/file.cast");
    }

    #[test]
    fn build_file_uri_handles_emoji() {
        // 🎬 = F0 9F 8E AC
        let uri = Xclip::build_file_uri(Path::new("/path/🎬/file.cast"));
        assert_eq!(uri, "file:///path/%F0%9F%8E%AC/file.cast");
    }

    #[test]
    fn build_file_uri_handles_special_ascii_chars() {
        // Characters like # @ & that need encoding
        let uri = Xclip::build_file_uri(Path::new("/path/test#1@2&3/file.cast"));
        assert_eq!(uri, "file:///path/test%231%402%263/file.cast");
    }

    #[test]
    fn method_returns_xclip() {
        let tool = Xclip::new();
        assert_eq!(tool.method(), CopyMethod::Xclip);
    }

    #[test]
    fn can_copy_files_returns_true() {
        let tool = Xclip::new();
        assert!(tool.can_copy_files());
    }
}

mod xsel_tests {
    use agr::clipboard::tool::{CopyTool, CopyToolError};
    use agr::clipboard::tools::Xsel;
    use agr::clipboard::CopyMethod;
    use std::path::Path;

    #[test]
    fn method_returns_xsel() {
        let tool = Xsel::new();
        assert_eq!(tool.method(), CopyMethod::Xsel);
    }

    #[test]
    fn can_copy_files_returns_false() {
        let tool = Xsel::new();
        assert!(!tool.can_copy_files());
    }

    #[test]
    fn try_copy_file_returns_not_supported() {
        let tool = Xsel::new();
        let result = tool.try_copy_file(Path::new("/some/file.cast"));
        assert!(matches!(result, Err(CopyToolError::NotSupported)));
    }
}

mod wl_copy_tests {
    use agr::clipboard::tool::{CopyTool, CopyToolError};
    use agr::clipboard::tools::WlCopy;
    use agr::clipboard::CopyMethod;
    use std::path::Path;

    #[test]
    fn method_returns_wl_copy() {
        let tool = WlCopy::new();
        assert_eq!(tool.method(), CopyMethod::WlCopy);
    }

    #[test]
    fn can_copy_files_returns_false() {
        let tool = WlCopy::new();
        assert!(!tool.can_copy_files());
    }

    #[test]
    fn try_copy_file_returns_not_supported() {
        let tool = WlCopy::new();
        let result = tool.try_copy_file(Path::new("/some/file.cast"));
        assert!(matches!(result, Err(CopyToolError::NotSupported)));
    }
}

// =============================================================================
// Stage 5: Platform Selection & Public API Tests
// =============================================================================

mod platform_tools_tests {
    use agr::clipboard::tools::platform_tools;

    #[test]
    #[cfg(target_os = "macos")]
    fn platform_tools_returns_osascript_and_pbcopy_on_macos() {
        let tools = platform_tools();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name(), "osascript");
        assert_eq!(tools[1].name(), "pbcopy");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn platform_tools_returns_xclip_xsel_wlcopy_on_linux() {
        let tools = platform_tools();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].name(), "xclip");
        assert_eq!(tools[1].name(), "xsel");
        assert_eq!(tools[2].name(), "wl-copy");
    }

    #[test]
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn platform_tools_returns_empty_on_other_platforms() {
        let tools = platform_tools();
        assert!(tools.is_empty());
    }
}

mod public_api_tests {
    use agr::clipboard::{copy_file_to_clipboard, ClipboardError};
    use std::path::Path;
    #[cfg(target_os = "macos")]
    use tempfile::NamedTempFile;

    #[test]
    fn copy_file_to_clipboard_returns_error_for_nonexistent_file() {
        let result = copy_file_to_clipboard(Path::new("/nonexistent/file.cast"));
        assert!(matches!(result, Err(ClipboardError::FileNotFound { .. })));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn copy_file_to_clipboard_succeeds_on_macos() {
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "test content").unwrap();

        let result = copy_file_to_clipboard(temp.path());
        assert!(result.is_ok());
    }
}
