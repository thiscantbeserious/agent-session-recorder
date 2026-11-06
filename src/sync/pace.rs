//! Pacing for mutating GitHub calls, pure with an injected clock and sleeper.

use std::time::Duration;

/// Mutating calls stay at least this far apart, which is at most 60 per minute.
pub const MIN_GAP: Duration = Duration::from_secs(1);

/// Waits before a mutating call so two calls are at least `MIN_GAP` apart.
/// There is no call budget, a real rate limit stops the run as `BackendError::RateLimited`.
pub struct Pacer<C, S> {
    clock: C,
    sleeper: S,
    last: Option<Duration>,
}

impl<C: FnMut() -> Duration, S: FnMut(Duration)> Pacer<C, S> {
    /// `clock` is a monotonic time since any fixed start, `sleeper` blocks for the given time.
    pub fn new(clock: C, sleeper: S) -> Self {
        Self {
            clock,
            sleeper,
            last: None,
        }
    }

    /// Call right before each mutating call.
    pub fn wait(&mut self) {
        let now = (self.clock)();
        if let Some(last) = self.last {
            let wait = MIN_GAP.saturating_sub(now.saturating_sub(last));
            if !wait.is_zero() {
                (self.sleeper)(wait);
            }
        }
        self.last = Some((self.clock)());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn secs(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn calls_are_at_least_one_second_apart() {
        let now = Cell::new(secs(5000));
        let slept = RefCell::new(Vec::new());
        let mut pacer = Pacer::new(
            || now.get(),
            |d| {
                slept.borrow_mut().push(d);
                now.set(now.get() + d);
            },
        );
        pacer.wait();
        assert!(slept.borrow().is_empty(), "first call does not wait");
        now.set(now.get() + secs(300));
        pacer.wait();
        assert_eq!(*slept.borrow(), vec![secs(700)]);
        now.set(now.get() + secs(2500));
        pacer.wait();
        assert_eq!(slept.borrow().len(), 1, "a slow gap needs no wait");
        pacer.wait();
        assert_eq!(*slept.borrow(), vec![secs(700), secs(1000)]);
    }
}
