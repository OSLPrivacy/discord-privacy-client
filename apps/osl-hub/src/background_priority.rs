use std::collections::BTreeSet;

/// Effective scheduling posture for background work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackgroundMode {
    Idle,
    Busy,
}

/// One logical piece of background work that can keep the app in busy mode.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Tracked {
    id: String,
}

impl Tracked {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl From<&str> for Tracked {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Tracked {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// A real effective-mode change caused by tracking or untracking work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackgroundTransition {
    pub from: BackgroundMode,
    pub to: BackgroundMode,
}

/// Tracks whether any background work is currently active.
#[derive(Clone, Debug, Default)]
pub struct BackgroundPriority {
    tracked: BTreeSet<Tracked>,
}

impl BackgroundPriority {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mode(&self) -> BackgroundMode {
        if self.tracked.is_empty() {
            BackgroundMode::Idle
        } else {
            BackgroundMode::Busy
        }
    }

    pub fn tracked_count(&self) -> usize {
        self.tracked.len()
    }

    pub fn is_tracked(&self, tracked: &Tracked) -> bool {
        self.tracked.contains(tracked)
    }

    pub fn track(&mut self, tracked: impl Into<Tracked>) -> Option<BackgroundTransition> {
        let before = self.mode();
        self.tracked.insert(tracked.into());
        self.transition_from(before)
    }

    pub fn untrack(&mut self, tracked: &Tracked) -> Option<BackgroundTransition> {
        let before = self.mode();
        self.tracked.remove(tracked);
        self.transition_from(before)
    }

    fn transition_from(&self, before: BackgroundMode) -> Option<BackgroundTransition> {
        let after = self.mode();
        (before != after).then_some(BackgroundTransition {
            from: before,
            to: after,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_priority_tracks_busy_period_transitions() {
        let mut priority = BackgroundPriority::new();
        let sync = Tracked::new("sync");
        let sweep = Tracked::new("sweep");

        assert_eq!(priority.mode(), BackgroundMode::Idle);
        assert_eq!(
            priority.track(sync.clone()),
            Some(BackgroundTransition {
                from: BackgroundMode::Idle,
                to: BackgroundMode::Busy,
            })
        );
        assert_eq!(priority.mode(), BackgroundMode::Busy);
        assert!(priority.is_tracked(&sync));

        assert_eq!(priority.track(sweep.clone()), None);
        assert_eq!(priority.track(sync.clone()), None);
        assert_eq!(priority.tracked_count(), 2);
        assert_eq!(priority.mode(), BackgroundMode::Busy);

        assert_eq!(priority.untrack(&sync), None);
        assert_eq!(priority.mode(), BackgroundMode::Busy);
        assert_eq!(priority.tracked_count(), 1);

        assert_eq!(
            priority.untrack(&sweep),
            Some(BackgroundTransition {
                from: BackgroundMode::Busy,
                to: BackgroundMode::Idle,
            })
        );
        assert_eq!(priority.mode(), BackgroundMode::Idle);
        assert_eq!(priority.untrack(&sweep), None);
    }
}
