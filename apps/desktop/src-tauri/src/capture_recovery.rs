use std::time::{Duration, Instant};

/// A frame, not a successful constructor, is recovery evidence. Continuous resets
/// cannot keep a session retrying forever; intermittent healthy capture replenishes it.
#[derive(Default)]
pub(crate) struct CaptureRecovery {
    failures: u32,
    healthy_since: Option<Instant>,
}
impl CaptureRecovery {
    pub(crate) fn frame(&mut self, now: Instant) {
        let since = *self.healthy_since.get_or_insert(now);
        if now.duration_since(since) >= Duration::from_secs(5) {
            self.failures = 0;
        }
    }
    pub(crate) fn retry(&mut self) -> Option<Duration> {
        self.healthy_since = None;
        self.failures += 1;
        (self.failures <= 3).then(|| Duration::from_millis(150 * u64::from(self.failures)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constructor_success_or_one_frame_cannot_reset_the_budget() {
        let mut recovery = CaptureRecovery::default();
        let now = Instant::now();
        for _ in 0..3 {
            assert!(recovery.retry().is_some());
            recovery.frame(now);
        }
        assert!(recovery.retry().is_none());
    }
    #[test]
    fn sustained_new_frames_replenish_the_budget() {
        let mut recovery = CaptureRecovery::default();
        assert!(recovery.retry().is_some());
        let now = Instant::now();
        recovery.frame(now);
        recovery.frame(now + Duration::from_secs(5));
        for _ in 0..3 {
            assert!(recovery.retry().is_some());
        }
        assert!(recovery.retry().is_none());
    }
}
