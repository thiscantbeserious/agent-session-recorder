//! The live line of `agr sync push` and `pull` on stderr, only on a terminal.

use std::io::{IsTerminal, Write};
use std::sync::{Arc, Mutex, PoisonError};

use agr::sync::format::{live_head, progress_line, LiveLine, LoadLine};
use agr::sync::{Event, ProgressSink};
use agr::theme::current_theme;

/// Shared between the observer of the engine and the percent sink of the backend.
pub struct Live {
    tty: bool,
    verb: &'static str,
    done: &'static str,
    line: Arc<Mutex<LiveLine>>,
    load: Mutex<LoadLine>,
}

fn write(text: &str) {
    let mut err = std::io::stderr();
    let _ = err.write_all(text.as_bytes());
    let _ = err.flush();
}

impl Live {
    /// `verb` is `uploading` or `downloading`, `done` the past tense of the command.
    pub fn new(verb: &'static str, done: &'static str) -> Self {
        Self {
            tty: std::io::stderr().is_terminal(),
            verb,
            done,
            line: Arc::default(),
            load: Mutex::default(),
        }
    }

    /// The sink for the backend, `None` off a terminal so nothing is parsed or drawn.
    pub fn sink(&self) -> Option<ProgressSink> {
        let line = Arc::clone(&self.line);
        self.tty.then(|| -> ProgressSink {
            Arc::new(move |percent| {
                let drawn = line
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .percent(percent);
                drawn.iter().for_each(|text| write(text));
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
        cleared.iter().for_each(|text| write(text));
    }

    /// Wipes the loading line, the normal output follows on a clean row.
    fn end_load(&self) {
        let cleared = self
            .load
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        cleared.iter().for_each(|text| write(text));
    }

    fn show_load(&self, event: &Event<'_>) -> bool {
        let mut load = self.load.lock().unwrap_or_else(PoisonError::into_inner);
        match event {
            Event::Listing => write(&load.listing()),
            Event::Scanning { done, total } => write(&load.scanning(*done, *total)),
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
                write(&line.start(self.verb, &live_head(path, *size, *i, *n)));
            }
            Event::Finished { item, i, n } => {
                let plain = progress_line(self.done, item, *i, *n);
                let painted = current_theme().secondary_text(&plain);
                write(&line.finish(&plain).replacen(&plain, &painted, 1));
            }
            Event::Listing | Event::Scanning { .. } => {}
        }
    }
}
