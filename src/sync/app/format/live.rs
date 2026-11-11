//! The live progress line of one running session and the loading line, `Live` draws them on a
//! writer and only when told it is a terminal.

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use crate::sync::engine::observe::Event;
use crate::sync::store::gist::ProgressSink;
use crate::theme::cli::{colors_enabled, paint_if};
use crate::theme::current_theme;

/// What to write for the session in flight, redraws start with `\r` and pad over the old text.
#[derive(Debug, Default)]
pub struct LiveLine {
    head: String,
    verb: String,
    percent: Option<u8>,
    width: usize,
}

impl LiveLine {
    /// A session starts, `verb` is `uploading` or `downloading`.
    pub fn start(&mut self, verb: &str, head: &str) -> String {
        self.head = head.into();
        self.verb = verb.into();
        self.width = 0;
        self.percent = Some(0);
        self.draw(0)
    }

    /// New percent, `None` when it did not change or no session runs.
    pub fn percent(&mut self, percent: u8) -> Option<String> {
        match self.percent {
            Some(old) if old != percent => Some(self.draw(percent)),
            _ => None,
        }
    }

    /// Replaces the live line with the final one, ends the line.
    pub fn finish(&mut self, line: &str) -> String {
        let live = self.percent.take().is_some();
        let pad = if live { self.pad(line) } else { String::new() };
        let lead = if live { "\r" } else { "" };
        self.width = 0;
        format!("{lead}{line}{pad}\n")
    }

    /// Wipes the live line without a final one, `None` when nothing is shown.
    pub fn clear(&mut self) -> Option<String> {
        self.percent.take()?;
        let blank = " ".repeat(self.width);
        self.width = 0;
        Some(format!("\r{blank}\r"))
    }

    fn draw(&mut self, percent: u8) -> String {
        self.percent = Some(percent);
        let text = format!("{}, {} {percent}%", self.head, self.verb);
        let out = format!("\r{text}{}", self.pad(&text));
        self.width = self.width.max(text.chars().count());
        out
    }

    /// Spaces that cover what is still on screen behind `text`.
    fn pad(&self, text: &str) -> String {
        " ".repeat(self.width.saturating_sub(text.chars().count()))
    }
}

/// The one line shown while the engine lists the remote and hashes local sessions.
#[derive(Debug, Default)]
pub struct LoadLine {
    width: usize,
    shown: bool,
}

impl LoadLine {
    /// The remote list is being fetched.
    pub fn listing(&mut self) -> String {
        self.draw("Listing remote sessions...")
    }

    /// `done` of `total` local sessions looked at.
    pub fn scanning(&mut self, done: usize, total: usize) -> String {
        self.draw(&format!("Scanning local sessions {done}/{total}"))
    }

    /// Wipes the line back to the full width it ever had, `None` when nothing is shown.
    pub fn clear(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.shown) {
            return None;
        }
        let blank = " ".repeat(std::mem::take(&mut self.width));
        Some(format!("\r{blank}\r"))
    }

    fn draw(&mut self, text: &str) -> String {
        let pad = " ".repeat(self.width.saturating_sub(text.chars().count()));
        self.width = self.width.max(text.chars().count());
        self.shown = true;
        format!("\r{text}{pad}")
    }
}

type Out = Arc<Mutex<Box<dyn Write + Send>>>;

/// Shared between the observer of the engine and the percent sink of the store.
pub struct Live {
    tty: bool,
    color: bool,
    verb: &'static str,
    done: &'static str,
    out: Out,
    line: Arc<Mutex<LiveLine>>,
    load: Mutex<LoadLine>,
}

fn put(out: &Out, text: &str) {
    let mut out = out.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

impl Live {
    /// On stderr, `verb` is `uploading` or `downloading`, `done` the past tense of the command.
    /// Colour follows `NO_COLOR`.
    pub fn stderr(verb: &'static str, done: &'static str) -> Self {
        use std::io::IsTerminal;
        let tty = std::io::stderr().is_terminal();
        let no_color = std::env::var("NO_COLOR").ok();
        let color = colors_enabled(tty, no_color.as_deref());
        Self::with(verb, done, (tty, color), Box::new(std::io::stderr()))
    }

    /// Writes to `out`, nothing at all unless `tty`, colour only when `color`.
    pub fn with(
        verb: &'static str,
        done: &'static str,
        (tty, color): (bool, bool),
        out: Box<dyn Write + Send>,
    ) -> Self {
        Self {
            tty,
            color,
            verb,
            done,
            out: Arc::new(Mutex::new(out)),
            line: Arc::default(),
            load: Mutex::default(),
        }
    }

    /// The sink for the store, `None` off a terminal so nothing is parsed or drawn.
    pub fn sink(&self) -> Option<ProgressSink> {
        let (line, out) = (Arc::clone(&self.line), Arc::clone(&self.out));
        self.tty.then(|| -> ProgressSink {
            Arc::new(move |percent| {
                let drawn = line
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .percent(percent);
                drawn.iter().for_each(|text| put(&out, text));
            })
        })
    }

    /// The observer of the engine, draws the live line and the final line per session.
    pub fn observer(&self) -> impl FnMut(&Event<'_>) + '_ {
        move |event| {
            if self.tty {
                self.show(event);
            }
        }
    }

    /// Draws only the loading line, for the dry-run plan whose items are not acted on.
    pub fn loader(&self) -> impl FnMut(&Event<'_>) + '_ {
        move |event| {
            if self.tty {
                self.show_load(event);
            }
        }
    }

    /// Wipes a half drawn line, for runs that end before the final line of a session.
    pub fn end(&self) {
        self.end_load();
        let cleared = self
            .line
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        cleared.iter().for_each(|text| put(&self.out, text));
    }

    /// Wipes the loading line, the normal output follows on a clean row.
    fn end_load(&self) {
        let cleared = self
            .load
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        cleared.iter().for_each(|text| put(&self.out, text));
    }

    fn show_load(&self, event: &Event<'_>) -> bool {
        let mut load = self.load.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            Event::Listing => put(&self.out, &load.listing()),
            Event::Scanning { done, total } => put(&self.out, &load.scanning(*done, *total)),
            _ => return false,
        }
        true
    }

    fn show(&self, event: &Event<'_>) {
        if self.show_load(event) {
            return;
        }
        self.end_load();
        let mut line = self.line.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            Event::Started { path, size, i, n } => {
                put(
                    &self.out,
                    &line.start(self.verb, &live_head(path, *size, *i, *n)),
                );
            }
            Event::Finished { item, i, n } => {
                let plain = super::progress_line(self.done, item, *i, *n);
                let theme = current_theme();
                let painted = paint_if(self.color, theme.text_secondary, &plain);
                put(
                    &self.out,
                    &line.finish(&plain).replacen(&plain, &painted, 1),
                );
            }
            Event::Listing | Event::Scanning { .. } => {}
        }
    }
}

/// `[i/n] path size`, the part of the live line that stays while the percent moves.
pub fn live_head(path: &str, size: u64, i: usize, n: usize) -> String {
    format!("[{i}/{n}] {path} {}", super::size(size))
}

#[cfg(test)]
mod tests {
    use super::{live_head, LiveLine, LoadLine};

    #[test]
    fn head_names_index_path_and_size() {
        assert_eq!(
            live_head("claude/x.cast.gz", 49_700_000, 12, 721),
            "[12/721] claude/x.cast.gz 47.40 MiB"
        );
    }

    #[test]
    fn start_shows_zero_and_the_percent_redraws_in_place() {
        let mut live = LiveLine::default();
        let head = "[12/721] claude/x.cast.gz 47.40 MiB";
        assert_eq!(
            live.start("uploading", head),
            format!("\r{head}, uploading 0%")
        );
        assert_eq!(
            live.percent(45).unwrap(),
            format!("\r{head}, uploading 45%")
        );
    }

    #[test]
    fn an_unchanged_percent_draws_nothing() {
        let mut live = LiveLine::default();
        live.start("uploading", "[1/2] a 1 B");
        assert!(live.percent(0).is_none());
        assert!(live.percent(7).is_some());
        assert!(live.percent(7).is_none());
    }

    #[test]
    fn a_percent_without_a_running_session_draws_nothing() {
        assert!(LiveLine::default().percent(50).is_none());
    }

    #[test]
    fn finish_pads_over_the_live_text_and_ends_the_line() {
        let mut live = LiveLine::default();
        let head = "[1/2] claude/x.cast.gz 1 MiB";
        live.start("uploading", head);
        live.percent(100);
        let live_width = format!("{head}, uploading 100%").chars().count();
        let done = "[1/2] pushed claude/x.cast.gz (1 MiB)";
        let out = live.finish(done);
        assert_eq!(
            out,
            format!(
                "\r{done}{}\n",
                " ".repeat(live_width.saturating_sub(done.chars().count()))
            )
        );
        assert!(live.clear().is_none());
    }

    #[test]
    fn finish_without_a_live_line_is_a_plain_line() {
        assert_eq!(
            LiveLine::default().finish("[1/1] skipped a"),
            "[1/1] skipped a\n"
        );
    }

    #[test]
    fn clear_wipes_a_live_line_and_never_contains_escape_codes() {
        let mut live = LiveLine::default();
        let shown = live.start("downloading", "[1/1] a 1 B");
        let wipe = live.clear().unwrap();
        assert!(wipe.starts_with('\r') && wipe.ends_with('\r'), "{wipe:?}");
        assert_eq!(wipe.trim_matches('\r').trim(), "");
        assert!(!shown.contains('\u{1b}') && !wipe.contains('\u{1b}'));
        assert!(live.clear().is_none());
    }

    #[test]
    fn load_line_names_listing_and_the_scan_count() {
        let mut load = LoadLine::default();
        assert_eq!(load.listing(), "\rListing remote sessions...");
        assert_eq!(load.scanning(312, 727), "\rScanning local sessions 312/727");
    }

    #[test]
    fn load_line_pads_a_shorter_redraw_over_the_old_text() {
        let mut load = LoadLine::default();
        load.scanning(10, 727);
        let short = load.scanning(9, 9);
        assert_eq!(short.trim_start_matches('\r').chars().count(), 30);
    }

    #[test]
    fn load_line_clear_returns_the_full_width_and_never_has_escape_codes() {
        let mut load = LoadLine::default();
        assert!(load.clear().is_none());
        let shown = load.scanning(312, 727);
        let wipe = load.clear().unwrap();
        let width = shown.trim_start_matches('\r').chars().count();
        assert_eq!(wipe, format!("\r{}\r", " ".repeat(width)));
        assert!(!shown.contains('\u{1b}') && !wipe.contains('\u{1b}'));
        assert!(load.clear().is_none());
    }
}
