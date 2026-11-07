// Frame-time budget for the windowed shell (#74): names the phase of a
// frame that blew the budget, so a stall the reporter sees (and we cannot
// reproduce headlessly) shows up in their log. Provenance: our own design;
// std::time only.

use std::time::{Duration, Instant};

/// A frame slower than this is reported (10 fps; vsync frames are 7-17 ms).
pub const BUDGET: Duration = Duration::from_millis(100);
/// After one report, stay quiet this long so a stuck loop cannot flood.
pub const QUIET: Duration = Duration::from_secs(5);

/// Per-frame phase stopwatch plus the rate-limited verdict.
pub struct FrameWatch {
    start: Instant,
    last: Instant,
    phases: Vec<(&'static str, Duration)>,
    last_report: Option<Instant>,
    slow_frames: u64,
}

impl FrameWatch {
    pub fn new(now: Instant) -> Self {
        FrameWatch { start: now, last: now, phases: Vec::with_capacity(8), last_report: None, slow_frames: 0 }
    }

    /// Open a frame.
    pub fn begin(&mut self, now: Instant) {
        self.start = now;
        self.last = now;
        self.phases.clear();
    }

    /// Close the phase that ran since the previous mark.
    pub fn mark(&mut self, phase: &'static str, now: Instant) {
        self.phases.push((phase, now.saturating_duration_since(self.last)));
        self.last = now;
    }

    /// Close the frame: a warning line when it overran the budget and the
    /// quiet window has passed, naming the slowest phase and every phase.
    pub fn end(&mut self, now: Instant) -> Option<String> {
        let total = now.saturating_duration_since(self.start);
        if total <= BUDGET {
            return None;
        }
        self.slow_frames = self.slow_frames.saturating_add(1);
        if self.last_report.is_some_and(|t| now.saturating_duration_since(t) < QUIET) {
            return None;
        }
        self.last_report = Some(now);
        let (worst, wt) = self
            .phases
            .iter()
            .copied()
            .max_by_key(|(_, d)| *d)
            .unwrap_or(("unmarked", total));
        let all: Vec<String> = self.phases.iter().map(|(p, d)| format!("{p} {} ms", d.as_millis())).collect();
        Some(format!(
            "gpu: slow frame {} ms (budget {} ms, {} slow so far) - slowest phase \"{worst}\" {} ms [{}]",
            total.as_millis(),
            BUDGET.as_millis(),
            self.slow_frames,
            wt.as_millis(),
            all.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_fast_frame_says_nothing() {
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        w.begin(t0);
        w.mark("ui", t0 + ms(5));
        w.mark("present", t0 + ms(16));
        assert_eq!(w.end(t0 + ms(16)), None);
    }

    #[test]
    fn a_slow_frame_names_its_slowest_phase() {
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        w.begin(t0);
        w.mark("ui", t0 + ms(4));
        w.mark("acquire", t0 + ms(1504));
        w.mark("present", t0 + ms(1510));
        let line = w.end(t0 + ms(1510)).expect("over budget");
        assert!(line.contains("1510 ms"), "{line}");
        assert!(line.contains("slowest phase \"acquire\" 1500 ms"), "{line}");
        assert!(line.contains("ui 4 ms"), "{line}");
    }

    #[test]
    fn reports_are_rate_limited_but_counted() {
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        let frame = |w: &mut FrameWatch, at: Instant| {
            w.begin(at);
            w.mark("ui", at + ms(200));
            w.end(at + ms(200))
        };
        assert!(frame(&mut w, t0).is_some());
        assert!(frame(&mut w, t0 + ms(300)).is_none(), "inside the quiet window");
        let again = frame(&mut w, t0 + QUIET + ms(400)).expect("quiet window over");
        assert!(again.contains("3 slow so far"), "{again}");
    }
}
