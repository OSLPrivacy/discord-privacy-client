//! Per-device destruction-acknowledgement progress.
//!
//! A destruction request can reach several devices. Acknowledgements are
//! therefore progress, not a single success flag: a sender must be able to
//! report that two of three devices have answered while the third is silent.

use std::collections::HashSet;

use ipc::destruct_ack::DestructionAckKind;

/// An affirmative destruction acknowledgement from one recipient device.
///
/// `outcome` remains attached to the acknowledgement so callers do not lose
/// T2-24's distinction between destroyed, already absent, and never held.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceDestructionAck<'a> {
    pub device_id: &'a str,
    pub outcome: DestructionAckKind,
}

/// Progress reported for a destruction request across its recipient devices.
///
/// This deliberately exposes counts only. A boolean such as `all_acked` would
/// turn a partial response into indistinguishable failure and cannot render the
/// required "2 of 3 devices confirmed" state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DestructionAckRollup {
    pub acknowledged_device_count: usize,
    pub device_count: usize,
}

impl DestructionAckRollup {
    /// Count distinct acknowledged devices from the set the request targeted.
    ///
    /// Replayed acknowledgements and acknowledgements from devices outside the
    /// request do not increase progress. Every explicit T2-24 outcome counts
    /// as a confirmation that its device processed the request; silence is not
    /// represented here and therefore never counts.
    pub fn from_acknowledgements<'a>(
        target_device_ids: impl IntoIterator<Item = &'a str>,
        acknowledgements: impl IntoIterator<Item = DeviceDestructionAck<'a>>,
    ) -> Self {
        let target_device_ids: HashSet<&str> = target_device_ids.into_iter().collect();
        let mut acknowledged_device_ids = HashSet::new();

        for acknowledgement in acknowledgements {
            if target_device_ids.contains(acknowledgement.device_id) {
                acknowledged_device_ids.insert(acknowledgement.device_id);
            }
        }

        Self {
            acknowledged_device_count: acknowledged_device_ids.len(),
            device_count: target_device_ids.len(),
        }
    }

    /// Human-readable progress for the destruction UI.
    pub fn progress_label(self) -> String {
        format!(
            "{} of {} devices confirmed",
            self.acknowledged_device_count, self.device_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tf_25_partial_device_acknowledgements_remain_a_count() {
        let rollup = DestructionAckRollup::from_acknowledgements(
            ["phone", "laptop", "tablet"],
            [
                DeviceDestructionAck {
                    device_id: "phone",
                    outcome: DestructionAckKind::Destroyed,
                },
                DeviceDestructionAck {
                    device_id: "laptop",
                    outcome: DestructionAckKind::AlreadyAbsent,
                },
            ],
        );

        assert_eq!(rollup.acknowledged_device_count, 2);
        assert_eq!(rollup.device_count, 3);
        assert_eq!(rollup.progress_label(), "2 of 3 devices confirmed");
    }

    #[test]
    fn acknowledgements_are_limited_to_distinct_target_devices() {
        let rollup = DestructionAckRollup::from_acknowledgements(
            ["phone", "laptop"],
            [
                DeviceDestructionAck {
                    device_id: "phone",
                    outcome: DestructionAckKind::NeverHeld,
                },
                DeviceDestructionAck {
                    device_id: "phone",
                    outcome: DestructionAckKind::Destroyed,
                },
                DeviceDestructionAck {
                    device_id: "unknown-device",
                    outcome: DestructionAckKind::Destroyed,
                },
            ],
        );

        assert_eq!(rollup.acknowledged_device_count, 1);
        assert_eq!(rollup.device_count, 2);
        assert_eq!(rollup.progress_label(), "1 of 2 devices confirmed");
    }
}
