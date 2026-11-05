use std::time::{Duration, Instant};

const RECOVERY_LIMIT: Duration = Duration::from_secs(15);
const HEALTHY_CAPTURE: Duration = Duration::from_secs(5);

/// A fresh frame, not a successful constructor, establishes recovery. Successful
/// polls after that frame also count as healthy capture: a static desktop need
/// not produce another JPEG. Repeated resets still share one bounded episode.
#[derive(Default)]
pub(crate) struct CaptureRecovery {
    failures: u32,
    started: Option<Instant>,
    healthy_since: Option<Instant>,
    waiting_for_frame: bool,
}
impl CaptureRecovery {
    pub(crate) fn rebuilt(&mut self) {
        if self.started.is_some() {
            self.waiting_for_frame = true;
            self.healthy_since = None;
        }
    }
    pub(crate) fn frame(&mut self, now: Instant) {
        self.waiting_for_frame = false;
        self.healthy(now);
    }
    pub(crate) fn poll(&mut self, now: Instant) {
        if !self.waiting_for_frame {
            self.healthy(now);
        }
    }
    fn healthy(&mut self, now: Instant) {
        let since = *self.healthy_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= HEALTHY_CAPTURE {
            self.failures = 0;
            self.started = None;
        }
    }
    pub(crate) fn timed_out(&self, now: Instant) -> bool {
        self.waiting_for_frame
            && self
                .started
                .is_some_and(|start| now.saturating_duration_since(start) >= RECOVERY_LIMIT)
    }
    pub(crate) fn retry(&mut self, now: Instant) -> Option<Duration> {
        let start = *self.started.get_or_insert(now);
        self.healthy_since = None;
        self.waiting_for_frame = true;
        let remaining = RECOVERY_LIMIT.saturating_sub(now.saturating_duration_since(start));
        if remaining.is_zero() {
            return None;
        }
        self.failures = self.failures.saturating_add(1);
        Some(
            Duration::from_millis(150 * u64::from(self.failures))
                .min(Duration::from_secs(1))
                .min(remaining),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fullscreen_transition_can_recover_after_several_seconds() {
        let mut recovery = CaptureRecovery::default();
        let now = Instant::now();
        for ms in [0, 150, 450, 900, 1500, 2250] {
            let at = now + Duration::from_millis(ms);
            assert!(recovery.retry(at).is_some());
            recovery.rebuilt();
            assert!(!recovery.timed_out(at));
        }
        recovery.frame(now + Duration::from_secs(3));
        assert!(!recovery.timed_out(now + Duration::from_secs(16)));
    }
    #[test]
    fn constructors_without_a_fresh_frame_cannot_wait_forever() {
        let mut recovery = CaptureRecovery::default();
        let now = Instant::now();
        assert!(recovery.retry(now).is_some());
        recovery.rebuilt();
        recovery.poll(now + Duration::from_secs(10));
        assert!(!recovery.timed_out(now + Duration::from_millis(14999)));
        assert!(recovery.timed_out(now + Duration::from_secs(15)));
        assert!(recovery.retry(now + Duration::from_secs(15)).is_none());
    }
    #[test]
    fn continuous_resets_are_bounded_and_backoff_never_exceeds_remaining_time() {
        let mut recovery = CaptureRecovery::default();
        let start = Instant::now();
        let mut elapsed = Duration::ZERO;
        let mut retries = 0;
        while let Some(delay) = recovery.retry(start + elapsed) {
            assert!(delay <= Duration::from_secs(1));
            assert!(delay <= RECOVERY_LIMIT - elapsed);
            elapsed += delay;
            retries += 1;
            assert!(retries < 30);
        }
        assert_eq!(elapsed, RECOVERY_LIMIT);
        assert!(recovery.timed_out(start + elapsed));
    }
    #[test]
    fn one_frame_followed_by_another_reset_does_not_replenish_the_budget() {
        let mut recovery = CaptureRecovery::default();
        let now = Instant::now();
        assert!(recovery.retry(now).is_some());
        for sec in 1..15 {
            let at = now + Duration::from_secs(sec);
            recovery.frame(at);
            assert!(recovery.retry(at).is_some());
        }
        assert!(recovery.retry(now + Duration::from_secs(15)).is_none());
    }
    #[test]
    fn successful_static_polls_after_a_new_frame_replenish_before_another_fullscreen_event() {
        let mut recovery = CaptureRecovery::default();
        let now = Instant::now();
        assert!(recovery.retry(now).is_some());
        recovery.rebuilt();
        recovery.frame(now + Duration::from_secs(2));
        recovery.poll(now + Duration::from_secs(6));
        assert!(recovery.started.is_some());
        recovery.poll(now + Duration::from_secs(7));
        assert!(recovery.started.is_none());
        assert_eq!(recovery.failures, 0);
        assert!(recovery.retry(now + Duration::from_secs(60)).is_some());
        assert!(!recovery.timed_out(now + Duration::from_secs(74)));
        assert!(recovery.timed_out(now + Duration::from_secs(75)));
    }
    #[test]
    fn another_constructor_cannot_reuse_the_old_first_frame_as_health() {
        let mut recovery = CaptureRecovery::default();
        let now = Instant::now();
        assert!(recovery.retry(now).is_some());
        recovery.frame(now + Duration::from_secs(2));
        recovery.rebuilt();
        recovery.poll(now + Duration::from_secs(8));
        assert!(recovery.timed_out(now + Duration::from_secs(15)));
    }
}
