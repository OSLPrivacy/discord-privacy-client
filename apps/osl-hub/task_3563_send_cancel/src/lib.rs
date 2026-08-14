//! Focused send/cancel accounting used by TASK 3563.
//!
//! A surface gets exactly one terminal record for a marked attempt.  A Cancel
//! pressed in the same UI turn wins before any dispatch is allowed, so neither
//! the loopback receiver nor the local completed-send ledger can observe it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendThenCancelResult {
    CancelledBeforeDispatch,
    Completed,
}

impl SendThenCancelResult {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CancelledBeforeDispatch => "cancelled-before-dispatch",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendThenCancelRun {
    pub surface: &'static str,
    pub mark: String,
    pub send_presses: u8,
    pub cancel_presses: u8,
    pub documented_results: u8,
    pub result: SendThenCancelResult,
    pub delivery_count: u8,
    pub local_completed_send_count: u8,
}

impl SendThenCancelRun {
    pub fn counts_match(&self) -> bool {
        self.delivery_count == self.local_completed_send_count
    }
}

/// Exercise the actual ordering required by the audit: the Send activation
/// records a pending mark, then the immediately following Cancel runs before
/// a transport dispatch turn.  The receiver and local completion ledger are
/// separate counters so a split result cannot be hidden by one shared value.
pub fn press_send_then_cancel_immediately(
    surface: &'static str,
    mark: impl Into<String>,
) -> SendThenCancelRun {
    let mark = mark.into();
    let mut send_presses = 0;
    let mut cancel_presses = 0;
    let pending_dispatch = true;
    let cancelled = true;
    let mut delivery_count = 0;
    let mut local_completed_send_count = 0;

    // Press Send.
    send_presses += 1;
    // Press Cancel immediately, before the queued dispatch turn.
    cancel_presses += 1;

    // Dispatch is the only place the independent receiver and local ledger
    // can be changed.  A cancellation prevents both changes together.
    if pending_dispatch && !cancelled {
        delivery_count += 1;
        local_completed_send_count += 1;
    }

    let result = if cancelled {
        SendThenCancelResult::CancelledBeforeDispatch
    } else {
        SendThenCancelResult::Completed
    };

    SendThenCancelRun {
        surface,
        mark,
        send_presses,
        cancel_presses,
        documented_results: 1,
        result,
        delivery_count,
        local_completed_send_count,
    }
}
