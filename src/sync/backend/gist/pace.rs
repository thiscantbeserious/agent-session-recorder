//! One clock for the sync code, monotonic for pacing and unix seconds for commit dates,
//! plus the pacer for mutating GitHub calls.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::sync::backend::BackendError;

/// Mutating calls stay at least this far apart, which is at most 60 per minute.
pub(super) const MIN_GAP: Duration = Duration::from_secs(1);

/// Time source and sleeper, `&self` so tests can keep their state behind a mutex.
pub trait Clock: Send + Sync {
    /// Monotonic time since any fixed start.
    fn mono(&self) -> Duration;
    /// Unix seconds, an error when the system clock is before 1970.
    fn unix_secs(&self) -> Result<u64, BackendError>;
    fn sleep(&self, time: Duration);
}

/// The real clock, the only wall clock and the only sleep of the sync code.
pub(super) struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub(super) fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn mono(&self) -> Duration {
        self.start.elapsed()
    }

    fn unix_secs(&self) -> Result<u64, BackendError> {
        secs_since_epoch(SystemTime::now())
    }

    fn sleep(&self, time: Duration) {
        std::thread::sleep(time);
    }
}

fn secs_since_epoch(time: SystemTime) -> Result<u64, BackendError> {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .as_ref()
        .map(std::time::Duration::as_secs)
        .map_err(|_| BackendError::Other("the system clock is before 1970".into()))
}

/// Waits before a mutating call so two calls are at least `MIN_GAP` apart.
/// There is no call budget, a real rate limit stops the run as `BackendError::RateLimited`.
pub(super) struct Pacer {
    clock: Arc<dyn Clock>,
    last: Option<Duration>,
}

impl Pacer {
    pub(super) fn new(clock: Arc<dyn Clock>) -> Self {
        Self { clock, last: None }
    }

    /// Call right before each mutating call.
    pub(super) fn wait(&mut self) {
        let now = self.clock.mono();
        if let Some(last) = self.last {
            let wait = MIN_GAP.saturating_sub(now.saturating_sub(last));
            if !wait.is_zero() {
                self.clock.sleep(wait);
            }
        }
        self.last = Some(self.clock.mono());
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Stateful fake, `sleep` advances `mono` and is recorded.
    pub struct FakeClock {
        state: Mutex<(Duration, Vec<Duration>)>,
        unix: u64,
    }

    impl FakeClock {
        pub fn new(mono: Duration, unix: u64) -> Self {
            Self {
                state: Mutex::new((mono, Vec::new())),
                unix,
            }
        }

        pub fn advance(&self, by: Duration) {
            self.state.lock().unwrap().0 += by;
        }

        pub fn slept(&self) -> Vec<Duration> {
            self.state.lock().unwrap().1.clone()
        }
    }

    impl Clock for FakeClock {
        fn mono(&self) -> Duration {
            self.state.lock().unwrap().0
        }

        fn unix_secs(&self) -> Result<u64, BackendError> {
            Ok(self.unix)
        }

        fn sleep(&self, time: Duration) {
            let mut state = self.state.lock().unwrap();
            state.0 += time;
            state.1.push(time);
        }
    }

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn calls_are_at_least_one_second_apart() {
        let clock = Arc::new(FakeClock::new(ms(5000), 0));
        let mut pacer = Pacer::new(clock.clone());
        pacer.wait();
        assert!(clock.slept().is_empty(), "first call does not wait");
        clock.advance(ms(300));
        pacer.wait();
        assert_eq!(clock.slept(), vec![ms(700)]);
        clock.advance(ms(2500));
        pacer.wait();
        assert_eq!(clock.slept().len(), 1, "a slow gap needs no wait");
        pacer.wait();
        assert_eq!(clock.slept(), vec![ms(700), ms(1000)]);
    }

    #[test]
    fn a_pre_epoch_clock_is_an_error_not_1970() {
        let before = SystemTime::UNIX_EPOCH - Duration::from_secs(5);
        assert!(secs_since_epoch(before).is_err());
        let after = SystemTime::UNIX_EPOCH + Duration::from_secs(7);
        assert_eq!(secs_since_epoch(after), Ok(7));
    }
}
