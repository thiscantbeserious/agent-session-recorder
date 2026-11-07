//! Steps after the stop banner, in the order they must run.

/// Auto push goes after auto-analysis, so the pushed file is the final one.
pub(super) fn after_stop<T>(
    analyze: impl FnOnce(),
    push: impl FnOnce(),
    warn: impl FnOnce() -> T,
) -> T {
    push();
    analyze();
    warn()
}

#[cfg(test)]
mod tests {
    use super::after_stop;
    use std::cell::RefCell;

    #[test]
    fn push_runs_after_analyze() {
        let log = RefCell::new(Vec::new());
        after_stop(
            || log.borrow_mut().push("analyze"),
            || log.borrow_mut().push("push"),
            || log.borrow_mut().push("warn"),
        );
        assert_eq!(*log.borrow(), ["analyze", "push", "warn"]);
    }
}
