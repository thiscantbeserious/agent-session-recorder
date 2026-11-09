//! The live progress line of one running session, pure text, the command does the printing.

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
        let _ = (verb, head);
        String::new()
    }

    /// New percent, `None` when it did not change or no session runs.
    pub fn percent(&mut self, percent: u8) -> Option<String> {
        let _ = percent;
        None
    }

    /// Replaces the live line with the final one, ends the line.
    pub fn finish(&mut self, line: &str) -> String {
        let _ = line;
        String::new()
    }

    /// Wipes the live line without a final one, `None` when nothing is shown.
    pub fn clear(&mut self) -> Option<String> {
        None
    }
}

/// `[i/n] path size`, the part of the live line that stays while the percent moves.
pub fn live_head(path: &str, size: u64, i: usize, n: usize) -> String {
    let _ = (path, size, i, n);
    String::new()
}

#[cfg(test)]
mod tests {
    use super::{live_head, LiveLine};

    #[test]
    fn head_names_index_path_and_size() {
        assert_eq!(
            live_head("claude/x.cast.gz", 49_700_000, 12, 721),
            "[12/721] claude/x.cast.gz 47.4 MiB"
        );
    }

    #[test]
    fn start_shows_zero_and_the_percent_redraws_in_place() {
        let mut live = LiveLine::default();
        let head = "[12/721] claude/x.cast.gz 47.4 MiB";
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
}
