#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackgroundMode {
    Idle,
    Busy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackgroundTransition {
    pub from: BackgroundMode,
    pub to: BackgroundMode,
    pub busy_periods: usize,
}

#[derive(Debug, Default)]
pub struct Tracked {
    busy_periods: usize,
    transitions: Vec<BackgroundTransition>,
}

impl Tracked {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mode(&self) -> BackgroundMode {
        if self.busy_periods == 0 {
            BackgroundMode::Idle
        } else {
            BackgroundMode::Busy
        }
    }

    pub fn busy_periods(&self) -> usize {
        self.busy_periods
    }

    pub fn transitions(&self) -> &[BackgroundTransition] {
        &self.transitions
    }

    pub fn track(&mut self) {
        let from = self.mode();
        self.busy_periods = self.busy_periods.saturating_add(1);
        self.record_if_changed(from);
    }

    pub fn untrack(&mut self) {
        let from = self.mode();
        self.busy_periods = self.busy_periods.saturating_sub(1);
        self.record_if_changed(from);
    }

    fn record_if_changed(&mut self, from: BackgroundMode) {
        let to = self.mode();
        if from != to {
            self.transitions.push(BackgroundTransition {
                from,
                to,
                busy_periods: self.busy_periods,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_priority_tracks_busy_period_transitions() {
        let mut tracked = Tracked::new();
        assert_eq!(tracked.mode(), BackgroundMode::Idle);

        tracked.track();
        assert_eq!(tracked.mode(), BackgroundMode::Busy);
        assert_eq!(tracked.busy_periods(), 1);

        tracked.track();
        assert_eq!(
            tracked.transitions(),
            &[BackgroundTransition {
                from: BackgroundMode::Idle,
                to: BackgroundMode::Busy,
                busy_periods: 1,
            }],
            "nested busy work must not create a second Busy transition"
        );

        tracked.untrack();
        assert_eq!(tracked.mode(), BackgroundMode::Busy);
        assert_eq!(tracked.busy_periods(), 1);

        tracked.untrack();
        assert_eq!(tracked.mode(), BackgroundMode::Idle);
        assert_eq!(
            tracked.transitions(),
            &[
                BackgroundTransition {
                    from: BackgroundMode::Idle,
                    to: BackgroundMode::Busy,
                    busy_periods: 1,
                },
                BackgroundTransition {
                    from: BackgroundMode::Busy,
                    to: BackgroundMode::Idle,
                    busy_periods: 0,
                },
            ],
        );

        tracked.untrack();
        assert_eq!(
            tracked.transitions().len(),
            2,
            "extra untrack while idle must not invent another transition"
        );
    }
}
