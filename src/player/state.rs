//! Player state management
//!
//! Contains the central `PlaybackState` struct that holds all playback state,
//! as well as shared types used across player modules.

use std::time::Instant;

/// Result of processing an input event.
///
/// This enum is returned by input handlers to signal control flow
/// decisions to the main loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputResult {
    /// Continue normal playback/rendering
    Continue,
    /// Exit the player normally
    Quit,
    /// Reserved for future `agr ls` integration where player returns selected file
    QuitWithFile,
}

/// Marker information for the progress bar.
///
/// Tracks the cumulative time and label for each marker in the recording.
#[derive(Debug, Clone)]
pub struct MarkerPosition {
    /// Cumulative time when the marker occurs
    pub time: f64,
    /// Marker label (from the cast file)
    pub label: String,
}

/// Central playback state for the native player.
///
/// This struct contains all state needed for playback, rendering,
/// and input handling. It is passed to various modules as needed.
///
/// Some fields are private with validated setter methods to ensure
/// invariants are maintained (e.g., time values are non-negative,
/// indices are within bounds).
#[derive(Debug)]
pub struct PlaybackState {
    // === Playback timing (guarded) ===
    /// Current event index in the cast file (private, use getter/setter)
    event_idx: usize,
    /// Current playback time in seconds (private, use getter/setter)
    current_time: f64,
    /// Cumulative time at current event index (private, use getter/setter)
    cumulative_time: f64,
    /// Time offset for seeking (private, use getter/setter)
    time_offset: f64,

    // === Playback timing (public) ===
    /// Whether playback is paused
    pub paused: bool,
    /// Playback speed multiplier (1.0 = normal)
    pub speed: f64,
    /// Wall clock time when playback started/resumed
    pub start_time: Instant,

    // === UI modes ===
    /// Whether help overlay is visible
    pub show_help: bool,
    /// Whether viewport mode is active (arrow keys scroll instead of seek)
    pub viewport_mode: bool,
    /// Whether free mode is active (line-by-line navigation)
    pub free_mode: bool,

    // === Free mode state (guarded) ===
    /// Current highlighted line in free mode (private, use getter/setter)
    free_line: usize,

    // === Free mode state (public) ===
    /// Previous highlighted line (for partial updates)
    pub prev_free_line: usize,
    /// True if only free_line changed (enables partial update optimization)
    pub free_line_only: bool,

    // === Viewport state (guarded) ===
    /// Vertical scroll offset into buffer (private, use getter/setter)
    view_row_offset: usize,
    /// Horizontal scroll offset into buffer (private, use getter/setter)
    view_col_offset: usize,

    // === Viewport state (public) ===
    /// Current terminal width
    pub term_cols: u16,
    /// Current terminal height
    pub term_rows: u16,
    /// Number of visible content rows (term_rows - status_lines)
    pub view_rows: usize,
    /// Number of visible content columns
    pub view_cols: usize,

    // === Rendering flags ===
    /// True when screen needs to be redrawn
    pub needs_render: bool,
}

impl PlaybackState {
    /// Number of status/chrome lines (separator + progress + status bar)
    pub const STATUS_LINES: u16 = 3;

    /// Create a new PlaybackState with default values.
    ///
    /// # Arguments
    /// * `term_cols` - Terminal width in columns
    /// * `term_rows` - Terminal height in rows
    pub fn new(term_cols: u16, term_rows: u16) -> Self {
        let view_rows = (term_rows.saturating_sub(Self::STATUS_LINES)) as usize;
        let view_cols = term_cols as usize;

        Self {
            // Playback timing
            paused: false,
            speed: 1.0,
            event_idx: 0,
            current_time: 0.0,
            cumulative_time: 0.0,
            start_time: Instant::now(),
            time_offset: 0.0,

            // UI modes
            show_help: false,
            viewport_mode: false,
            free_mode: false,

            // Free mode state
            free_line: 0,
            prev_free_line: 0,
            free_line_only: false,

            // Viewport state
            term_cols,
            term_rows,
            view_rows,
            view_cols,
            view_row_offset: 0,
            view_col_offset: 0,

            // Rendering flags
            needs_render: true,
        }
    }

    /// Handle terminal resize event.
    ///
    /// Updates viewport dimensions and clamps scroll offsets to valid range.
    ///
    /// # Arguments
    /// * `new_cols` - New terminal width
    /// * `new_rows` - New terminal height
    /// * `buf_cols` - Current buffer width (for clamping)
    /// * `buf_rows` - Current buffer height (for clamping)
    pub fn handle_resize(
        &mut self,
        new_cols: u16,
        new_rows: u16,
        buf_cols: usize,
        buf_rows: usize,
    ) {
        self.term_cols = new_cols;
        self.term_rows = new_rows;
        self.view_rows = (new_rows.saturating_sub(Self::STATUS_LINES)) as usize;
        self.view_cols = new_cols as usize;

        // Clamp viewport offset to valid range
        let max_row_offset = buf_rows.saturating_sub(self.view_rows);
        let max_col_offset = buf_cols.saturating_sub(self.view_cols);
        self.view_row_offset = self.view_row_offset.min(max_row_offset);
        self.view_col_offset = self.view_col_offset.min(max_col_offset);

        self.needs_render = true;
    }

    /// Toggle pause state and reset timing if resuming.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        if !self.paused {
            // Exit free mode when resuming playback
            self.free_mode = false;
            // Reset timing when resuming
            self.start_time = Instant::now();
            self.time_offset = self.current_time;
        }
        self.needs_render = true;
    }

    /// Fixed speed steps for clean playback speed values.
    /// Using fixed steps prevents floating point drift when adjusting speed up and down.
    const SPEED_STEPS: &'static [f64] = &[0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0];

    /// Increase playback speed to the next fixed step (max 16x).
    ///
    /// Uses fixed speed steps to prevent floating point drift.
    /// If current speed isn't exactly a step value, snaps to the nearest higher step.
    pub fn speed_up(&mut self) {
        // Find the next speed step higher than current
        for &step in Self::SPEED_STEPS {
            if step > self.speed + f64::EPSILON {
                self.speed = step;
                self.needs_render = true;
                return;
            }
        }
        // Already at or above max speed
        self.speed = 16.0;
        self.needs_render = true;
    }

    /// Decrease playback speed to the next fixed step (min 0.25x).
    ///
    /// Uses fixed speed steps to prevent floating point drift.
    /// If current speed isn't exactly a step value, snaps to the nearest lower step.
    pub fn speed_down(&mut self) {
        // Find the next speed step lower than current
        for &step in Self::SPEED_STEPS.iter().rev() {
            if step < self.speed - f64::EPSILON {
                self.speed = step;
                self.needs_render = true;
                return;
            }
        }
        // Already at or below min speed
        self.speed = 0.25;
        self.needs_render = true;
    }

    /// Toggle help overlay visibility.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        self.needs_render = true;
    }

    /// Toggle viewport mode.
    pub fn toggle_viewport_mode(&mut self) {
        self.viewport_mode = !self.viewport_mode;
        if self.viewport_mode {
            self.free_mode = false; // Exit free mode when entering viewport mode
        }
        self.needs_render = true;
    }

    /// Toggle free mode (pauses playback automatically).
    ///
    /// # Arguments
    /// * `cursor_row` - Current cursor row to start highlight at
    pub fn toggle_free_mode(&mut self, cursor_row: usize) {
        self.free_mode = !self.free_mode;
        if self.free_mode {
            self.viewport_mode = false; // Exit viewport mode when entering free mode
            self.paused = true; // Enforce pause in free mode
            self.free_line = cursor_row;
        }
        self.needs_render = true;
    }

    /// Exit current mode (viewport or free) or quit.
    ///
    /// Returns true if a mode was exited, false if should quit.
    pub fn exit_mode_or_quit(&mut self) -> bool {
        if self.viewport_mode {
            self.viewport_mode = false;
            self.needs_render = true;
            true
        } else if self.free_mode {
            self.free_mode = false;
            self.needs_render = true;
            true
        } else {
            false // Should quit
        }
    }

    // === Getters for guarded fields ===

    /// Get the current playback time in seconds.
    #[inline]
    pub fn current_time(&self) -> f64 {
        self.current_time
    }

    /// Get the time offset (added to elapsed wall time).
    #[inline]
    pub fn time_offset(&self) -> f64 {
        self.time_offset
    }

    /// Get the current event index.
    #[inline]
    pub fn event_idx(&self) -> usize {
        self.event_idx
    }

    /// Get the cumulative time at current event index.
    #[inline]
    pub fn cumulative_time(&self) -> f64 {
        self.cumulative_time
    }

    /// Get the current highlighted line in free mode.
    #[inline]
    pub fn free_line(&self) -> usize {
        self.free_line
    }

    /// Get the vertical scroll offset into buffer.
    #[inline]
    pub fn view_row_offset(&self) -> usize {
        self.view_row_offset
    }

    /// Get the horizontal scroll offset into buffer.
    #[inline]
    pub fn view_col_offset(&self) -> usize {
        self.view_col_offset
    }

    // === Setters with validation ===

    /// Set current playback time, clamped to valid range [0.0, max_time].
    pub fn set_current_time(&mut self, time: f64, max_time: f64) {
        self.current_time = time.clamp(0.0, max_time);
    }

    /// Set time offset, clamped to >= 0.0.
    pub fn set_time_offset(&mut self, offset: f64) {
        self.time_offset = offset.max(0.0);
    }

    /// Set event index, clamped to valid range [0, max_idx].
    pub fn set_event_idx(&mut self, idx: usize, max_idx: usize) {
        self.event_idx = idx.min(max_idx);
    }

    /// Set cumulative time, clamped to >= 0.0.
    pub fn set_cumulative_time(&mut self, time: f64) {
        self.cumulative_time = time.max(0.0);
    }

    /// Set event index and cumulative time together (common operation after seeking).
    pub fn set_event_position(&mut self, idx: usize, cumulative: f64, max_idx: usize) {
        self.event_idx = idx.min(max_idx);
        self.cumulative_time = cumulative.max(0.0);
    }

    /// Set free line, clamped to buffer bounds.
    /// Also updates prev_free_line to track the previous value.
    pub fn set_free_line(&mut self, line: usize, max_line: usize) {
        self.prev_free_line = self.free_line;
        self.free_line = line.min(max_line);
    }

    /// Set view row offset, clamped to valid range [0, max_offset].
    pub fn set_view_row_offset(&mut self, offset: usize, max_offset: usize) {
        self.view_row_offset = offset.min(max_offset);
    }

    /// Set view col offset, clamped to valid range [0, max_offset].
    pub fn set_view_col_offset(&mut self, offset: usize, max_offset: usize) {
        self.view_col_offset = offset.min(max_offset);
    }

    /// Increment event index by 1 if below max.
    /// Returns true if incremented, false if already at max.
    pub fn increment_event_idx(&mut self, max_idx: usize) -> bool {
        if self.event_idx < max_idx {
            self.event_idx += 1;
            true
        } else {
            false
        }
    }

    /// Add to cumulative time (always non-negative result).
    pub fn add_cumulative_time(&mut self, delta: f64) {
        self.cumulative_time = (self.cumulative_time + delta).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_has_correct_defaults() {
        let state = PlaybackState::new(80, 27);

        assert!(!state.paused);
        assert_eq!(state.speed, 1.0);
        assert_eq!(state.event_idx(), 0);
        assert_eq!(state.current_time(), 0.0);
        assert!(!state.show_help);
        assert!(!state.viewport_mode);
        assert!(!state.free_mode);
        assert_eq!(state.view_rows, 24); // 27 - 3 status lines
        assert_eq!(state.view_cols, 80);
        assert!(state.needs_render);
    }

    #[test]
    fn handle_resize_updates_dimensions() {
        let mut state = PlaybackState::new(80, 27);
        state.handle_resize(120, 40, 100, 50);

        assert_eq!(state.term_cols, 120);
        assert_eq!(state.term_rows, 40);
        assert_eq!(state.view_rows, 37); // 40 - 3
        assert_eq!(state.view_cols, 120);
    }

    #[test]
    fn handle_resize_clamps_offset() {
        let mut state = PlaybackState::new(80, 27);
        state.set_view_row_offset(100, 200);
        state.set_view_col_offset(100, 200);

        state.handle_resize(80, 27, 30, 30);

        // Offset should be clamped: 30 - 24 = 6 max row, 30 - 80 = 0 max col
        assert!(state.view_row_offset() <= 6);
        assert_eq!(state.view_col_offset(), 0);
    }

    #[test]
    fn toggle_pause_resets_timing() {
        let mut state = PlaybackState::new(80, 27);
        state.paused = true;
        state.set_current_time(10.0, 100.0);
        state.free_mode = true;

        state.toggle_pause();

        assert!(!state.paused);
        assert!(!state.free_mode); // Exited free mode
        assert_eq!(state.time_offset(), 10.0); // Preserved current time
    }

    #[test]
    fn speed_up_uses_fixed_steps() {
        let mut state = PlaybackState::new(80, 27);
        assert_eq!(state.speed, 1.0);
        state.speed_up();
        assert_eq!(state.speed, 2.0);
        state.speed_up();
        assert_eq!(state.speed, 4.0);
        state.speed_up();
        assert_eq!(state.speed, 8.0);
        state.speed_up();
        assert_eq!(state.speed, 16.0);
    }

    #[test]
    fn speed_up_maxes_at_16() {
        let mut state = PlaybackState::new(80, 27);
        state.speed = 15.0;
        state.speed_up();
        assert_eq!(state.speed, 16.0);
        // Already at max
        state.speed_up();
        assert_eq!(state.speed, 16.0);
    }

    #[test]
    fn speed_down_uses_fixed_steps() {
        let mut state = PlaybackState::new(80, 27);
        assert_eq!(state.speed, 1.0);
        state.speed_down();
        assert_eq!(state.speed, 0.5);
        state.speed_down();
        assert_eq!(state.speed, 0.25);
    }

    #[test]
    fn speed_down_mins_at_0_25() {
        let mut state = PlaybackState::new(80, 27);
        state.speed = 0.3;
        state.speed_down();
        assert_eq!(state.speed, 0.25);
        // Already at min
        state.speed_down();
        assert_eq!(state.speed, 0.25);
    }

    #[test]
    fn speed_up_to_max_and_back_returns_to_1x() {
        // This is the main bug fix test: speed should return to exactly 1.0
        // after going up to 16x and back down
        let mut state = PlaybackState::new(80, 27);
        assert_eq!(state.speed, 1.0);

        // Speed up: 1.0 -> 2.0 -> 4.0 -> 8.0 -> 16.0
        state.speed_up();
        state.speed_up();
        state.speed_up();
        state.speed_up();
        assert_eq!(state.speed, 16.0);

        // Speed down: 16.0 -> 8.0 -> 4.0 -> 2.0 -> 1.0
        state.speed_down();
        state.speed_down();
        state.speed_down();
        state.speed_down();
        assert_eq!(state.speed, 1.0); // Must be EXACTLY 1.0, not 0.9-something
    }

    #[test]
    fn speed_down_to_min_and_back_returns_to_1x() {
        // Speed should return to exactly 1.0 after going down to 0.25x and back up
        let mut state = PlaybackState::new(80, 27);
        assert_eq!(state.speed, 1.0);

        // Speed down: 1.0 -> 0.5 -> 0.25
        state.speed_down();
        state.speed_down();
        assert_eq!(state.speed, 0.25);

        // Speed up: 0.25 -> 0.5 -> 1.0
        state.speed_up();
        state.speed_up();
        assert_eq!(state.speed, 1.0); // Must be EXACTLY 1.0
    }

    #[test]
    fn rapid_speed_changes_no_drift() {
        // Test rapid speed changes don't cause drift
        let mut state = PlaybackState::new(80, 27);

        // Many rapid up/down cycles
        for _ in 0..10 {
            state.speed_up();
            state.speed_down();
        }
        assert_eq!(state.speed, 1.0);

        // Many rapid down/up cycles
        for _ in 0..10 {
            state.speed_down();
            state.speed_up();
        }
        assert_eq!(state.speed, 1.0);
    }

    #[test]
    fn speed_stays_within_bounds() {
        let mut state = PlaybackState::new(80, 27);

        // Try to exceed max
        for _ in 0..20 {
            state.speed_up();
        }
        assert_eq!(state.speed, 16.0);

        // Try to go below min
        for _ in 0..20 {
            state.speed_down();
        }
        assert_eq!(state.speed, 0.25);
    }

    #[test]
    fn speed_snaps_to_nearest_step_on_speed_up() {
        let mut state = PlaybackState::new(80, 27);
        // Set speed to a non-step value
        state.speed = 1.7;
        state.speed_up();
        assert_eq!(state.speed, 2.0); // Should snap to next higher step
    }

    #[test]
    fn speed_snaps_to_nearest_step_on_speed_down() {
        let mut state = PlaybackState::new(80, 27);
        // Set speed to a non-step value
        state.speed = 1.7;
        state.speed_down();
        assert_eq!(state.speed, 1.0); // Should snap to next lower step
    }

    #[test]
    fn toggle_free_mode_enables_and_pauses() {
        let mut state = PlaybackState::new(80, 27);
        state.viewport_mode = true;

        state.toggle_free_mode(5);

        assert!(state.free_mode);
        assert!(state.paused);
        assert!(!state.viewport_mode);
        assert_eq!(state.free_line(), 5);
    }

    #[test]
    fn toggle_viewport_mode_exits_free_mode() {
        let mut state = PlaybackState::new(80, 27);
        state.free_mode = true;

        state.toggle_viewport_mode();

        assert!(state.viewport_mode);
        assert!(!state.free_mode);
    }

    #[test]
    fn exit_mode_exits_viewport_first() {
        let mut state = PlaybackState::new(80, 27);
        state.viewport_mode = true;

        assert!(state.exit_mode_or_quit()); // Should return true (mode exited)
        assert!(!state.viewport_mode);
    }

    #[test]
    fn exit_mode_exits_free_mode() {
        let mut state = PlaybackState::new(80, 27);
        state.free_mode = true;

        assert!(state.exit_mode_or_quit());
        assert!(!state.free_mode);
    }

    #[test]
    fn exit_mode_returns_false_when_no_mode() {
        let mut state = PlaybackState::new(80, 27);
        assert!(!state.exit_mode_or_quit()); // Should quit
    }

    #[test]
    fn input_result_enum_variants() {
        assert_eq!(InputResult::Continue, InputResult::Continue);
        assert_ne!(InputResult::Quit, InputResult::Continue);
        assert_ne!(InputResult::QuitWithFile, InputResult::Quit);
    }

    #[test]
    fn marker_position_stores_data() {
        let marker = MarkerPosition {
            time: 5.5,
            label: "Test marker".to_string(),
        };
        assert_eq!(marker.time, 5.5);
        assert_eq!(marker.label, "Test marker");
    }

    // === Guard tests ===

    #[test]
    fn set_current_time_clamps_negative_to_zero() {
        let mut state = PlaybackState::new(80, 27);
        state.set_current_time(-5.0, 100.0);
        assert_eq!(state.current_time(), 0.0);
    }

    #[test]
    fn set_current_time_clamps_to_max() {
        let mut state = PlaybackState::new(80, 27);
        state.set_current_time(150.0, 100.0);
        assert_eq!(state.current_time(), 100.0);
    }

    #[test]
    fn set_current_time_allows_valid_value() {
        let mut state = PlaybackState::new(80, 27);
        state.set_current_time(50.0, 100.0);
        assert_eq!(state.current_time(), 50.0);
    }

    #[test]
    fn set_time_offset_clamps_negative_to_zero() {
        let mut state = PlaybackState::new(80, 27);
        state.set_time_offset(-10.0);
        assert_eq!(state.time_offset(), 0.0);
    }

    #[test]
    fn set_time_offset_allows_positive() {
        let mut state = PlaybackState::new(80, 27);
        state.set_time_offset(25.0);
        assert_eq!(state.time_offset(), 25.0);
    }

    #[test]
    fn set_event_idx_clamps_to_max() {
        let mut state = PlaybackState::new(80, 27);
        state.set_event_idx(100, 50);
        assert_eq!(state.event_idx(), 50);
    }

    #[test]
    fn set_event_idx_allows_valid_value() {
        let mut state = PlaybackState::new(80, 27);
        state.set_event_idx(25, 50);
        assert_eq!(state.event_idx(), 25);
    }

    #[test]
    fn set_cumulative_time_clamps_negative_to_zero() {
        let mut state = PlaybackState::new(80, 27);
        state.set_cumulative_time(-5.0);
        assert_eq!(state.cumulative_time(), 0.0);
    }

    #[test]
    fn set_event_position_clamps_both_values() {
        let mut state = PlaybackState::new(80, 27);
        state.set_event_position(100, -5.0, 50);
        assert_eq!(state.event_idx(), 50);
        assert_eq!(state.cumulative_time(), 0.0);
    }

    #[test]
    fn set_free_line_clamps_to_max() {
        let mut state = PlaybackState::new(80, 27);
        state.set_free_line(100, 23);
        assert_eq!(state.free_line(), 23);
    }

    #[test]
    fn set_free_line_updates_prev_free_line() {
        let mut state = PlaybackState::new(80, 27);
        state.set_free_line(5, 100);
        state.set_free_line(10, 100);
        assert_eq!(state.free_line(), 10);
        assert_eq!(state.prev_free_line, 5);
    }

    #[test]
    fn set_view_row_offset_clamps_to_max() {
        let mut state = PlaybackState::new(80, 27);
        state.set_view_row_offset(100, 50);
        assert_eq!(state.view_row_offset(), 50);
    }

    #[test]
    fn set_view_col_offset_clamps_to_max() {
        let mut state = PlaybackState::new(80, 27);
        state.set_view_col_offset(100, 30);
        assert_eq!(state.view_col_offset(), 30);
    }

    #[test]
    fn set_view_offsets_allow_valid_values() {
        let mut state = PlaybackState::new(80, 27);
        state.set_view_row_offset(10, 50);
        state.set_view_col_offset(5, 30);
        assert_eq!(state.view_row_offset(), 10);
        assert_eq!(state.view_col_offset(), 5);
    }

    #[test]
    fn increment_event_idx_increments_when_below_max() {
        let mut state = PlaybackState::new(80, 27);
        state.set_event_idx(5, 100);
        assert!(state.increment_event_idx(100));
        assert_eq!(state.event_idx(), 6);
    }

    #[test]
    fn increment_event_idx_returns_false_at_max() {
        let mut state = PlaybackState::new(80, 27);
        state.set_event_idx(100, 100);
        assert!(!state.increment_event_idx(100));
        assert_eq!(state.event_idx(), 100);
    }

    #[test]
    fn add_cumulative_time_adds_positive() {
        let mut state = PlaybackState::new(80, 27);
        state.set_cumulative_time(5.0);
        state.add_cumulative_time(3.0);
        assert_eq!(state.cumulative_time(), 8.0);
    }

    #[test]
    fn add_cumulative_time_clamps_result_to_zero() {
        let mut state = PlaybackState::new(80, 27);
        state.set_cumulative_time(5.0);
        state.add_cumulative_time(-10.0);
        assert_eq!(state.cumulative_time(), 0.0);
    }
}
