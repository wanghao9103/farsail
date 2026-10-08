use farsail_windows::ForegroundWindow;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct InputRecovery {
    failed: Option<ForegroundWindow>,
    candidate: Option<(ForegroundWindow, Instant)>,
    prepared: Option<ForegroundWindow>,
    last_probe: Option<Instant>,
}
impl InputRecovery {
    pub fn arm(&mut self, failed: Option<ForegroundWindow>) {
        *self = Self {
            failed,
            ..Self::default()
        };
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    fn discard_candidate(&mut self) {
        self.candidate = None;
        self.prepared = None;
    }
    fn sample(
        &mut self,
        current: Option<ForegroundWindow>,
        now: Instant,
    ) -> Option<ForegroundWindow> {
        let Some(current) = current.filter(|f| f.window != 0 && f.process_id != 0) else {
            self.discard_candidate();
            return None;
        };
        let failed = self.failed.filter(|f| f.window != 0 && f.process_id != 0)?;
        if current == failed {
            self.discard_candidate();
            return None;
        }
        // Every observed change resets stability, even inside the probe throttle.
        if self
            .candidate
            .is_none_or(|(previous, _)| previous != current)
        {
            self.candidate = Some((current, now));
            self.prepared = None;
        }
        if self
            .last_probe
            .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(500))
        {
            return None;
        }
        self.last_probe = Some(now);
        let (_, since) = self.candidate?;
        (now.saturating_duration_since(since) >= Duration::from_millis(250)).then_some(current)
    }
    // Caller owns the input lock. These closures are synchronous: no receive
    // future is cancelled and no input/grant is replayed to test recovery.
    // Preparing keeps control paused; only an ordered viewer acknowledgement
    // may complete recovery after all lower input sequences have been discarded.
    pub fn prepare<E>(
        &mut self,
        current: Option<ForegroundWindow>,
        now: Instant,
        authorised: impl Fn() -> bool,
        ready: impl Fn(ForegroundWindow) -> bool,
        release: impl FnOnce() -> Result<(), E>,
    ) -> Option<ForegroundWindow> {
        if !authorised() {
            self.discard_candidate();
            return None;
        }
        let candidate = self.sample(current, now)?;
        if !ready(candidate) || release().is_err() {
            self.prepared = None;
            return None;
        }
        if !authorised() || !ready(candidate) {
            self.discard_candidate();
            return None;
        }
        self.prepared = Some(candidate);
        Some(candidate)
    }

    // The caller validates the acknowledgement's input generation and owns the
    // input lock. A stale or unprepared witness cannot clear the armed failure.
    pub fn acknowledge<E>(
        &mut self,
        expected: ForegroundWindow,
        authorised: impl Fn() -> bool,
        ready: impl Fn(ForegroundWindow) -> bool,
        release: impl FnOnce() -> Result<(), E>,
    ) -> bool {
        if self.failed.is_none()
            || self.prepared != Some(expected)
            || self.candidate.map(|(window, _)| window) != Some(expected)
        {
            return false;
        }
        if !authorised() || !ready(expected) {
            self.discard_candidate();
            return false;
        }
        if release().is_err() {
            self.prepared = None;
            return false;
        }
        if !authorised() || !ready(expected) {
            self.discard_candidate();
            return false;
        }
        self.clear();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const A: ForegroundWindow = ForegroundWindow {
        window: 1,
        process_id: 10,
    };
    const B: ForegroundWindow = ForegroundWindow {
        window: 2,
        process_id: 20,
    };
    const C: ForegroundWindow = ForegroundWindow {
        window: 3,
        process_id: 30,
    };

    fn primed(start: Instant) -> InputRecovery {
        let mut p = InputRecovery::default();
        p.arm(Some(A));
        assert_eq!(
            p.prepare(
                Some(B),
                start,
                || true,
                |_| true,
                || -> Result<(), ()> { panic!("first sample cannot release") }
            ),
            None
        );
        p
    }
    fn prepared(start: Instant) -> InputRecovery {
        let mut p = primed(start);
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_millis(600),
                || true,
                |_| true,
                || Ok::<(), ()>(()),
            ),
            Some(B)
        );
        assert_eq!(p.failed, Some(A));
        p
    }

    #[test]
    fn unchanged_or_unknown_foreground_never_prepares() {
        let start = Instant::now();
        for current in [
            None,
            Some(A),
            Some(ForegroundWindow {
                window: 0,
                process_id: 0,
            }),
        ] {
            let mut p = InputRecovery::default();
            p.arm(Some(A));
            for n in 0..4 {
                assert_eq!(
                    p.prepare(
                        current,
                        start + Duration::from_secs(n),
                        || true,
                        |_| true,
                        || -> Result<(), ()> { panic!("no changed witness") },
                    ),
                    None
                );
            }
        }
        for failed in [
            None,
            Some(ForegroundWindow {
                window: 0,
                process_id: 0,
            }),
        ] {
            let mut p = InputRecovery::default();
            p.arm(failed);
            assert_eq!(
                p.prepare(
                    Some(B),
                    start + Duration::from_secs(2),
                    || true,
                    |_| true,
                    || -> Result<(), ()> { panic!("unknown failure context") },
                ),
                None
            );
        }
    }

    #[test]
    fn preparing_keeps_failure_armed_and_acknowledgement_clears_once() {
        let start = Instant::now();
        let mut p = primed(start);
        let releases = Cell::new(0);
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_millis(600),
                || true,
                |_| true,
                || {
                    releases.set(releases.get() + 1);
                    Ok::<(), ()>(())
                },
            ),
            Some(B)
        );
        assert_eq!(p.failed, Some(A));
        assert_eq!(p.prepared, Some(B));
        assert!(p.acknowledge(
            B,
            || true,
            |_| true,
            || {
                releases.set(releases.get() + 1);
                Ok::<(), ()>(())
            }
        ));
        assert_eq!(p.failed, None);
        assert!(!p.acknowledge(
            B,
            || true,
            |_| true,
            || -> Result<(), ()> { panic!("an acknowledgement cannot be reused") }
        ));
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(2),
                || true,
                |_| true,
                || -> Result<(), ()> { panic!("successful recovery is no longer armed") },
            ),
            None
        );
        assert_eq!(releases.get(), 2);
    }

    #[test]
    fn unsafe_context_cannot_prepare_or_release() {
        let start = Instant::now();
        let mut p = primed(start);
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(1),
                || true,
                |_| false,
                || -> Result<(), ()> { panic!("higher, unknown or inactive context") },
            ),
            None
        );
        assert_eq!(p.failed, Some(A));
    }

    #[test]
    fn prepare_cleanup_failure_requires_a_later_safe_prepare() {
        let start = Instant::now();
        let mut p = primed(start);
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(1),
                || true,
                |_| true,
                || Err::<(), ()>(()),
            ),
            None
        );
        assert_eq!(p.failed, Some(A));
        assert!(!p.acknowledge(
            B,
            || true,
            |_| true,
            || -> Result<(), ()> { panic!("cleanup failure did not prepare a witness") }
        ));
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(2),
                || true,
                |_| true,
                || Ok::<(), ()>(()),
            ),
            Some(B)
        );
        assert!(p.acknowledge(B, || true, |_| true, || Ok::<(), ()>(())));
    }

    #[test]
    fn prepare_rechecks_authority_and_context_after_cleanup() {
        let start = Instant::now();
        let mut p = primed(start);
        let active = Cell::new(true);
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(1),
                || active.get(),
                |_| true,
                || {
                    active.set(false);
                    Ok::<(), ()>(())
                },
            ),
            None
        );
        assert_eq!(p.failed, Some(A));
        assert_eq!(p.prepared, None);
        let mut p = primed(start);
        let reads = Cell::new(0);
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(1),
                || true,
                |_| {
                    reads.set(reads.get() + 1);
                    reads.get() == 1
                },
                || Ok::<(), ()>(()),
            ),
            None
        );
        assert_eq!(p.prepared, None);
    }

    #[test]
    fn disabled_scope_cannot_probe_or_release() {
        let start = Instant::now();
        let mut p = primed(start);
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(1),
                || false,
                |_| panic!("ended, view-only or sharing-disabled scope"),
                || -> Result<(), ()> { panic!("unauthorised release") },
            ),
            None
        );
        let mut p = prepared(start);
        assert!(!p.acknowledge(
            B,
            || false,
            |_| panic!("unauthorised acknowledgement probe"),
            || -> Result<(), ()> { panic!("unauthorised acknowledgement release") },
        ));
        assert_eq!(p.failed, Some(A));
    }

    #[test]
    fn null_transition_invalidates_pending_ack_and_requires_stability_again() {
        let start = Instant::now();
        let mut p = prepared(start);
        assert_eq!(
            p.prepare(
                None,
                start + Duration::from_millis(700),
                || true,
                |_| true,
                || -> Result<(), ()> { panic!("null") },
            ),
            None
        );
        assert!(!p.acknowledge(
            B,
            || true,
            |_| true,
            || -> Result<(), ()> { panic!("null invalidated the pending witness") }
        ));
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(1),
                || true,
                |_| true,
                || -> Result<(), ()> { panic!("new first sample") },
            ),
            None
        );
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(2),
                || true,
                |_| true,
                || Ok::<(), ()>(()),
            ),
            Some(B)
        );
    }

    #[test]
    fn stale_or_unprepared_acknowledgement_cannot_release() {
        let start = Instant::now();
        let mut p = primed(start);
        assert!(!p.acknowledge(
            B,
            || true,
            |_| true,
            || -> Result<(), ()> { panic!("first sample did not prepare") }
        ));
        let mut p = prepared(start);
        assert!(!p.acknowledge(
            C,
            || true,
            |_| true,
            || -> Result<(), ()> { panic!("stale acknowledgement witness") }
        ));
        assert_eq!(p.failed, Some(A));
        assert_eq!(p.prepared, Some(B));
        assert!(p.acknowledge(B, || true, |_| true, || Ok::<(), ()>(())));
    }

    #[test]
    fn acknowledgement_rechecks_context_before_cleanup() {
        let start = Instant::now();
        let mut p = prepared(start);
        assert!(!p.acknowledge(
            B,
            || true,
            |_| false,
            || -> Result<(), ()> { panic!("context changed before acknowledgement cleanup") }
        ));
        assert_eq!(p.failed, Some(A));
        assert_eq!(p.prepared, None);
    }

    #[test]
    fn acknowledgement_cleanup_failure_keeps_failure_and_requires_reprepare() {
        let start = Instant::now();
        let mut p = prepared(start);
        assert!(!p.acknowledge(B, || true, |_| true, || Err::<(), ()>(())));
        assert_eq!(p.failed, Some(A));
        assert_eq!(p.prepared, None);
        assert!(!p.acknowledge(
            B,
            || true,
            |_| true,
            || -> Result<(), ()> {
                panic!("failed cleanup invalidated the pending acknowledgement")
            }
        ));
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_secs(2),
                || true,
                |_| true,
                || Ok::<(), ()>(()),
            ),
            Some(B)
        );
        assert!(p.acknowledge(B, || true, |_| true, || Ok::<(), ()>(())));
    }

    #[test]
    fn acknowledgement_rechecks_authority_and_context_after_cleanup() {
        let start = Instant::now();
        let mut p = prepared(start);
        let active = Cell::new(true);
        assert!(!p.acknowledge(
            B,
            || active.get(),
            |_| true,
            || {
                active.set(false);
                Ok::<(), ()>(())
            }
        ));
        assert_eq!(p.failed, Some(A));
        assert_eq!(p.prepared, None);
        let mut p = prepared(start);
        let reads = Cell::new(0);
        assert!(!p.acknowledge(
            B,
            || true,
            |_| {
                reads.set(reads.get() + 1);
                reads.get() == 1
            },
            || Ok::<(), ()>(()),
        ));
        assert_eq!(p.failed, Some(A));
        assert_eq!(p.prepared, None);
    }

    #[test]
    fn explicit_clear_cancels_pending_acknowledgement() {
        let mut p = prepared(Instant::now());
        p.clear();
        assert!(!p.acknowledge(
            B,
            || true,
            |_| true,
            || -> Result<(), ()> { panic!("explicit clear cancelled pending acknowledgement") }
        ));
    }

    #[test]
    fn every_observed_b_c_b_change_resets_stability_before_throttling() {
        let start = Instant::now();
        let mut p = primed(start);
        for (current, milliseconds) in [(C, 400), (B, 500)] {
            assert_eq!(
                p.prepare(
                    Some(current),
                    start + Duration::from_millis(milliseconds),
                    || true,
                    |_| true,
                    || -> Result<(), ()> { panic!("intervening change reset stability") },
                ),
                None
            );
        }
        assert_eq!(
            p.prepare(
                Some(B),
                start + Duration::from_millis(1100),
                || true,
                |_| true,
                || Ok::<(), ()>(()),
            ),
            Some(B)
        );
    }
}
