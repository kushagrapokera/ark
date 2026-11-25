//! Shared, bounded caret blink clock for every editable surface.

use std::time::{Duration, Instant};

pub const BLINK_PERIOD: Duration = Duration::from_millis(530);

#[derive(Debug)]
pub struct CaretBlink {
    visible: bool,
    due: Option<Instant>,
}

impl Default for CaretBlink {
    fn default() -> Self {
        Self {
            visible: true,
            due: None,
        }
    }
}

impl CaretBlink {
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn due(&self) -> Option<Instant> {
        self.due
    }

    pub fn reset(&mut self, now: Instant) {
        self.visible = true;
        self.due = Some(now + BLINK_PERIOD);
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let Some(due) = self.due else {
            return false;
        };
        if now < due {
            return false;
        }
        self.visible = !self.visible;
        self.due = Some(now + BLINK_PERIOD);
        true
    }

    pub fn stop(&mut self) {
        self.visible = true;
        self.due = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_shows_the_caret_and_schedules_the_next_blink() {
        let now = Instant::now();
        let mut blink = CaretBlink::default();
        blink.reset(now);
        assert!(blink.visible());
        assert_eq!(blink.due(), Some(now + BLINK_PERIOD));
    }

    #[test]
    fn tick_toggles_only_when_due() {
        let now = Instant::now();
        let mut blink = CaretBlink::default();
        blink.reset(now);
        assert!(!blink.tick(now + BLINK_PERIOD - Duration::from_nanos(1)));
        assert!(blink.tick(now + BLINK_PERIOD));
        assert!(!blink.visible());
        assert_eq!(blink.due(), Some(now + 2 * BLINK_PERIOD));
        assert!(blink.tick(now + 2 * BLINK_PERIOD));
        assert!(blink.visible());
    }

    #[test]
    fn stop_clears_the_deadline() {
        let mut blink = CaretBlink::default();
        blink.reset(Instant::now());
        blink.stop();
        assert!(blink.visible());
        assert!(blink.due().is_none());
    }
}
