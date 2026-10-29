//! Auto-lock timing. Pure state machine so it can be tested without timers;
//! a Tauri background task drives it.

use std::time::{Duration, Instant};

/// Seconds remaining before the vault locks. May be negative when idle.
pub type Remaining = i64;

/// Emitted when `remaining <= WARNING_WINDOW` and not yet emitted.
pub const WARNING_WINDOW: i64 = 60;

#[derive(Debug, Clone)]
pub struct AutoLock {
    timeout: Duration,
    last_activity: Instant,
    warning_sent: bool,
}

/// What the background task should do on this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// Nothing to do.
    Idle,
    /// Emit `vault://lock-warning { seconds }` with the given remaining secs.
    Warn(i64),
    /// Emit `vault://locked` and lock.
    Lock,
}

impl AutoLock {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            last_activity: Instant::now(),
            warning_sent: false,
        }
    }

    /// Resets the idle timer (user activity).
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
        self.warning_sent = false;
    }

    /// Changes the timeout; the timer restarts from now.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
        self.touch();
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Seconds until the warning/lock, floored at 0.
    pub fn remaining(&self, now: Instant) -> Remaining {
        let elapsed = now.saturating_duration_since(self.last_activity);
        let timeout = self.timeout.as_secs() as i64;
        timeout - elapsed.as_secs() as i64
    }

    /// Advances the state machine.
    pub fn tick(&mut self, now: Instant) -> Tick {
        let elapsed = now.saturating_duration_since(self.last_activity);
        if elapsed >= self.timeout {
            return Tick::Lock;
        }
        let remaining = self.remaining(now);
        if remaining <= WARNING_WINDOW && !self.warning_sent {
            self.warning_sent = true;
            return Tick::Warn(remaining);
        }
        Tick::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_then_locks() {
        let mut a = AutoLock::new(Duration::from_secs(300));
        let start = a.last_activity;
        assert_eq!(a.tick(start), Tick::Idle);
        assert_eq!(a.tick(start + Duration::from_secs(200)), Tick::Idle);
        assert_eq!(a.tick(start + Duration::from_secs(241)), Tick::Warn(59));
        assert_eq!(a.tick(start + Duration::from_secs(250)), Tick::Idle);
        assert_eq!(a.tick(start + Duration::from_secs(300)), Tick::Lock);
    }

    #[test]
    fn touch_resets_warning() {
        let mut a = AutoLock::new(Duration::from_secs(120));
        let start = a.last_activity;
        assert_eq!(a.tick(start + Duration::from_secs(70)), Tick::Warn(50));
        a.touch();
        assert_eq!(a.tick(Instant::now()), Tick::Idle);
        let now = Instant::now();
        assert_eq!(a.remaining(now), 120);
    }

    #[test]
    fn short_timeout_warns_immediately() {
        let mut a = AutoLock::new(Duration::from_secs(60));
        assert_eq!(a.tick(a.last_activity), Tick::Warn(60));
    }

    #[test]
    fn set_timeout_restarts() {
        let mut a = AutoLock::new(Duration::from_secs(300));
        a.set_timeout(Duration::from_secs(90));
        assert_eq!(a.tick(a.last_activity), Tick::Idle);
        assert_eq!(a.remaining(a.last_activity), 90);
    }
}
