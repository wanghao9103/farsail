use farsail_media::ViewportRequest;
use std::time::{Duration, Instant};

const EDGES: [u16; 8] = [480, 640, 800, 960, 1280, 1600, 1920, 2560];
const RELAY_START: u16 = 960;
pub(super) const SLOW_BODY: Duration = Duration::from_millis(150);

/// One instance per outgoing session, created only after the peer advertises FSZ0.
pub(super) struct AdaptiveViewport {
    maximum: u16,
    direct: bool,
    request: ViewportRequest,
    confirmed: bool,
    last_sent: Instant,
    last_sample: Option<Instant>,
    slow_since: Option<Instant>,
    healthy_since: Option<Instant>,
}

impl AdaptiveViewport {
    pub(super) fn new(maximum: u16, now: Instant) -> Self {
        Self {
            maximum: maximum.clamp(320, 2560),
            direct: false,
            request: ViewportRequest {
                generation: 0,
                edge: 0,
            },
            confirmed: false,
            last_sent: now,
            last_sample: None,
            slow_since: None,
            healthy_since: None,
        }
    }

    pub(super) fn confirm(&mut self, reply: ViewportRequest) {
        if reply == self.request {
            self.confirmed = true;
        }
    }

    pub(super) fn next(
        &mut self,
        path: (&str, Option<u64>),
        body_time: Option<Duration>,
        now: Instant,
    ) -> Option<ViewportRequest> {
        let direct = path.0 == "direct";
        let edge = self.request.edge;
        let mut next = edge;
        if self.request.generation == 0 {
            next = if direct {
                self.maximum
            } else {
                self.maximum.min(RELAY_START)
            };
        } else if self.direct != direct {
            self.slow_since = None;
            self.healthy_since = None;
            if !direct {
                next = edge.min(RELAY_START);
            }
        } else if self.confirmed
            && let Some(elapsed) = body_time
        {
            // Time spent waiting for a screen change must not accumulate as
            // congestion or recovery, even across a long idle period.
            if self
                .last_sample
                .is_none_or(|last| now.duration_since(last) > elapsed + Duration::from_secs(2))
            {
                self.slow_since = None;
                self.healthy_since = None;
            }
            self.last_sample = Some(now);
            let slow = elapsed >= SLOW_BODY || path.1.is_some_and(|rtt| rtt >= 250);
            let healthy =
                elapsed <= Duration::from_millis(65) && path.1.is_some_and(|rtt| rtt <= 140);
            if slow {
                self.healthy_since = None;
                let since = *self.slow_since.get_or_insert(now);
                if now.duration_since(since) >= Duration::from_secs(2) {
                    next = EDGES
                        .iter()
                        .rev()
                        .copied()
                        .find(|&v| v < edge)
                        .unwrap_or(edge);
                }
            } else if healthy {
                self.slow_since = None;
                let since = *self.healthy_since.get_or_insert(now);
                if now.duration_since(since) >= Duration::from_secs(10) {
                    next = EDGES
                        .iter()
                        .copied()
                        .find(|&v| v > edge)
                        .unwrap_or(edge)
                        .min(self.maximum);
                }
            } else {
                self.slow_since = None;
                self.healthy_since = None;
            }
        }
        self.direct = direct;
        if next != edge {
            self.request = ViewportRequest {
                generation: self.request.generation + 1,
                edge: next,
            };
            self.confirmed = false;
            self.slow_since = None;
            self.healthy_since = None;
        } else if self.confirmed || now.duration_since(self.last_sent) < Duration::from_secs(3) {
            return None;
        }
        // Re-send the same generation if its response was lost.
        self.last_sent = now;
        Some(self.request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_caps_the_edge_and_only_matching_replies_complete_a_request() {
        let start = Instant::now();
        let mut p = AdaptiveViewport::new(1500, start);
        let direct = p.next(("direct", Some(10)), None, start).unwrap();
        assert_eq!(direct.edge, 1500);
        let relay = p.next(("relay", Some(50)), None, start).unwrap();
        assert_eq!((relay.generation, relay.edge), (direct.generation + 1, 960));
        p.confirm(direct);
        p.confirm(ViewportRequest { edge: 640, ..relay });
        assert!(
            p.next(("relay", Some(50)), None, start + Duration::from_secs(2))
                .is_none()
        );
        assert_eq!(
            p.next(("relay", Some(50)), None, start + Duration::from_secs(3)),
            Some(relay)
        );
        p.confirm(relay);
        assert!(
            p.next(("relay", None), None, start + Duration::from_secs(180))
                .is_none()
        );
    }

    #[test]
    fn adaptive_edges_need_sustained_samples_and_respect_screen_size() {
        let start = Instant::now();
        let mut p = AdaptiveViewport::new(1100, start);
        let first = p.next(("relay", None), None, start).unwrap();
        p.confirm(first);
        let sample = |p: &mut AdaptiveViewport, sec, rtt, body| {
            p.next(
                ("relay", Some(rtt)),
                Some(Duration::from_millis(body)),
                start + Duration::from_secs(sec),
            )
        };
        assert!(sample(&mut p, 0, 500, 15).is_none());
        assert!(sample(&mut p, 1, 25, 15).is_none());
        assert!(sample(&mut p, 3, 500, 15).is_none());
        // Idle time and a missing RTT are insufficient evidence for recovery.
        assert!(sample(&mut p, 45, 500, 15).is_none());
        let lower = sample(&mut p, 47, 500, 15).unwrap();
        assert_eq!(lower.edge, 800);
        p.confirm(lower);
        for t in 48..65 {
            assert!(
                p.next(
                    ("relay", None),
                    Some(Duration::ZERO),
                    start + Duration::from_secs(t)
                )
                .is_none()
            );
        }
        for t in 65..75 {
            assert!(sample(&mut p, t, 80, 30).is_none());
        }
        let up = sample(&mut p, 75, 80, 30).unwrap();
        assert_eq!(up.edge, 960);
        p.confirm(up);
        for t in 76..86 {
            assert!(sample(&mut p, t, 80, 30).is_none());
        }
        let max = sample(&mut p, 86, 80, 30).unwrap();
        assert_eq!(max.edge, 1100);
        p.confirm(max);
        for t in 87..100 {
            assert!(sample(&mut p, t, 80, 30).is_none());
        }
        for t in 100..130 {
            if let Some(step) = sample(&mut p, t, 25, 230) {
                p.confirm(step);
            }
        }
        assert_eq!(p.request.edge, 480);
    }
}
