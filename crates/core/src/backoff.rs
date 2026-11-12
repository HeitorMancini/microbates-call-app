use std::time::Duration;

/// Exponential backoff used between reconnection attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
    pub max_attempts: u32,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(500),
            max: Duration::from_secs(8),
            max_attempts: 6,
        }
    }
}

impl Backoff {
    /// Delay before the attempt with the given zero-based index.
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 1u32.checked_shl(attempt).unwrap_or(u32::MAX);
        self.initial.saturating_mul(factor).min(self.max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_until_capped() {
        let b = Backoff {
            initial: Duration::from_millis(100),
            max: Duration::from_millis(700),
            max_attempts: 10,
        };
        let delays: Vec<_> = (0..5).map(|a| b.delay(a).as_millis()).collect();
        assert_eq!(delays, [100, 200, 400, 700, 700]);
    }

    #[test]
    fn huge_attempt_does_not_overflow() {
        assert_eq!(Backoff::default().delay(200), Backoff::default().max);
    }
}
