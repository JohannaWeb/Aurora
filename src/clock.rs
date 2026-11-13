//! The clock the JS event loop runs on.
//!
//! Normally this is the real monotonic clock. Under `AURORA_REPLAY` it is
//! virtual: time stands still while code runs and moves only when the event
//! loop waits, and [`sleep`] advances it instantly instead of blocking. A
//! replay then fires the same timers in the same order however fast the
//! machine is.
//!
//! An interactive window cannot stand still, so the window calls
//! [`follow_real_time`] when it opens; from then on virtual time advances
//! with real time from wherever the headless boot left it.

use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

struct Virtual {
    /// Virtual `now()` is `base + elapsed` (plus real time since `real_since`
    /// once following real time).
    base: Instant,
    elapsed: Duration,
    real_since: Option<Instant>,
}

static VIRTUAL: LazyLock<Option<Mutex<Virtual>>> = LazyLock::new(|| {
    crate::fetch::replay::start_time_ms().map(|_| {
        Mutex::new(Virtual {
            base: Instant::now(),
            elapsed: Duration::ZERO,
            real_since: None,
        })
    })
});

fn with_virtual<T>(f: impl FnOnce(&mut Virtual) -> T) -> Option<T> {
    VIRTUAL
        .as_ref()
        .map(|clock| f(&mut clock.lock().unwrap_or_else(PoisonError::into_inner)))
}

impl Virtual {
    fn now(&self) -> Instant {
        let real = self.real_since.map_or(Duration::ZERO, |since| since.elapsed());
        self.base + self.elapsed + real
    }

    /// Advance by `duration` unless following real time; returns whether it did.
    fn sleep(&mut self, duration: Duration) -> bool {
        if self.real_since.is_none() {
            self.elapsed += duration;
            true
        } else {
            false
        }
    }

    fn follow_real_time(&mut self) {
        if self.real_since.is_none() {
            self.real_since = Some(Instant::now());
        }
    }
}

/// The current time on the event-loop clock.
pub fn now() -> Instant {
    with_virtual(|clock| clock.now()).unwrap_or_else(Instant::now)
}

/// Wait for `duration`: blocks on the real clock, advances the virtual one.
pub fn sleep(duration: Duration) {
    if with_virtual(|clock| clock.sleep(duration)) != Some(true) {
        std::thread::sleep(duration);
    }
}

/// Whether the clock is virtual and not following real time, i.e. time
/// moves only when the event loop waits.
pub fn is_standing() -> bool {
    with_virtual(|clock| clock.real_since.is_none()).unwrap_or(false)
}

/// From now on, advance the virtual clock with real time. No effect on the
/// real clock or when already following real time.
pub fn follow_real_time() {
    with_virtual(Virtual::follow_real_time);
}

/// The real instant at which the event-loop clock will read `deadline`, for
/// waiting on it with an OS timer.
pub fn to_real(deadline: Instant) -> Instant {
    let real_now = Instant::now();
    let clock_now = now();
    if deadline >= clock_now {
        real_now + (deadline - clock_now)
    } else {
        real_now.checked_sub(clock_now - deadline).unwrap_or(real_now)
    }
}

/// How far each script read moves a standing virtual clock, so a page that
/// spins on `Date.now()` still sees time pass. Read counts are deterministic,
/// so this keeps replays reproducible.
const SCRIPT_READ_STEP: Duration = Duration::from_micros(1);

/// Milliseconds the virtual clock has run, as seen by a script; 0 on the real
/// clock. Each call advances a standing clock by [`SCRIPT_READ_STEP`].
pub fn script_elapsed_ms() -> f64 {
    with_virtual(|clock| {
        clock.sleep(SCRIPT_READ_STEP);
        (clock.now() - clock.base).as_secs_f64() * 1000.0
    })
    .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing() -> Virtual {
        Virtual { base: Instant::now(), elapsed: Duration::ZERO, real_since: None }
    }

    #[test]
    fn virtual_time_stands_still_and_sleep_advances_it_exactly() {
        let mut clock = standing();
        let start = clock.now();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(clock.now(), start);
        assert!(clock.sleep(Duration::from_millis(250)));
        assert_eq!(clock.now() - start, Duration::from_millis(250));
    }

    #[test]
    fn following_real_time_keeps_virtual_progress_and_stops_teleporting() {
        let mut clock = standing();
        clock.sleep(Duration::from_secs(10));
        clock.follow_real_time();
        assert!(!clock.sleep(Duration::from_secs(10)));
        std::thread::sleep(Duration::from_millis(5));
        let ahead = clock.now() - clock.base;
        assert!(ahead >= Duration::from_millis(10_005) && ahead < Duration::from_secs(11));
    }
}
