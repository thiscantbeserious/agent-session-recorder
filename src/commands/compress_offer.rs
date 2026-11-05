//! Once-per-version offer to compress plain sessions, shown before list, status and cleanup.

use std::io::IsTerminal;

use anyhow::Result;
use humansize::{format_size, BINARY};

use agr::config::state::State;
use agr::storage::compress::{self, CompressPlan};
use agr::{Config, StorageManager};

use super::compress::{confirm_and_run, preview_lines, print_lines};
use super::prompt::should_proceed;

/// Offer only on a terminal, with plain sessions left, once per agr version.
pub fn should_offer(
    is_tty: bool,
    state_version: Option<&str>,
    current_version: &str,
    plain_count: usize,
) -> bool {
    is_tty && plain_count > 0 && state_version != Some(current_version)
}

/// Offer the migration on an interactive run, never when stdin or stdout is piped.
#[cfg(not(tarpaulin_include))]
pub fn maybe_offer() -> Result<()> {
    let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if !is_tty {
        return Ok(());
    }
    let storage = StorageManager::new(Config::load()?);
    let plan = compress::plan(&storage, None)?;
    let ask = |q: &str| should_proceed(q, false);
    offer_flow(
        is_tty,
        &plan,
        State::load(),
        env!("CARGO_PKG_VERSION"),
        State::save,
        ask,
    )
}

/// Gate, show the preview, remember the version, then run exactly like `agr compress`.
pub fn offer_flow(
    is_tty: bool,
    plan: &CompressPlan,
    state: State,
    version: &str,
    save: impl FnOnce(&State) -> Result<()>,
    ask: impl FnOnce(&str) -> Result<bool>,
) -> Result<()> {
    let seen = state.compress_offer_version.as_deref();
    if !should_offer(is_tty, seen, version, plan.files.len()) {
        return Ok(());
    }
    print_lines(&["Plain sessions can be compressed to save disk space.".into()]);
    print_lines(&preview_lines(plan));
    let remembered = State {
        compress_offer_version: Some(version.to_string()),
    };
    if let Err(e) = save(&remembered) {
        eprintln!("Warning: could not remember the offer: {e:#}");
    }
    confirm_and_run(plan, ask)
}

/// "N plain, ~X saved by agr compress" for the status output, none without plain sessions.
pub fn savings_line(plan: &CompressPlan) -> Option<String> {
    if plan.files.is_empty() {
        return None;
    }
    let saved = plan.total_size().saturating_sub(plan.total_projected());
    let n = plan.files.len();
    Some(format!(
        "{n} plain, ~{} saved by agr compress",
        format_size(saved, BINARY)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agr::storage::compress::PlannedFile;
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;

    const V: &str = "0.3.0";

    #[test]
    fn gate_truth_table() {
        assert!(should_offer(true, None, V, 1));
        assert!(should_offer(true, Some("0.2.0"), V, 3));
        assert!(!should_offer(false, None, V, 1), "no tty");
        assert!(!should_offer(true, Some(V), V, 1), "same version");
        assert!(!should_offer(true, None, V, 0), "no plain sessions");
        assert!(!should_offer(false, Some(V), V, 0));
    }

    fn plan_of(n: usize) -> CompressPlan {
        let files = (0..n)
            .map(|i| PlannedFile {
                path: PathBuf::from(format!("/nonexistent/a/{i}.cast")),
                agent: "a".into(),
                size: 4096,
                projected: 1024,
            })
            .collect();
        CompressPlan {
            files,
            live: vec![],
        }
    }

    /// Run the flow with a fixed answer, returns (saved versions, questions asked).
    fn flow(plan: &CompressPlan, state: State, answer: bool) -> (Vec<String>, usize) {
        let saved = RefCell::new(Vec::new());
        let asked = Cell::new(0);
        offer_flow(
            true,
            plan,
            state,
            V,
            |s| {
                saved.borrow_mut().extend(s.compress_offer_version.clone());
                Ok(())
            },
            |_| {
                asked.set(asked.get() + 1);
                Ok(answer)
            },
        )
        .unwrap();
        (saved.into_inner(), asked.get())
    }

    #[test]
    fn accept_and_decline_both_save_the_current_version() {
        for answer in [true, false] {
            let (saved, asked) = flow(&plan_of(1), State::default(), answer);
            assert_eq!(saved, vec![V.to_string()], "answer {answer}");
            assert_eq!(asked, 1);
        }
    }

    #[test]
    fn second_call_with_saved_version_does_not_offer() {
        let state = State {
            compress_offer_version: Some(V.into()),
        };
        let (saved, asked) = flow(&plan_of(1), state, true);
        assert!(saved.is_empty());
        assert_eq!(asked, 0);
    }

    #[test]
    fn no_tty_never_asks_and_never_saves() {
        let mut touched = false;
        offer_flow(
            false,
            &plan_of(1),
            State::default(),
            V,
            |_| {
                touched = true;
                Ok(())
            },
            |_| panic!("must not ask without a tty"),
        )
        .unwrap();
        assert!(!touched);
    }

    #[test]
    fn savings_line_reports_count_and_projected_savings() {
        let line = savings_line(&plan_of(2));
        assert_eq!(
            line.as_deref(),
            Some("2 plain, ~6 KiB saved by agr compress")
        );
        assert_eq!(savings_line(&plan_of(0)), None);
    }
}
