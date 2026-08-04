//! Per-device fan-out for view-once ciphertext.
//!
//! A single-fetch reservation is a property of one blob.  Sharing that blob
//! between devices would let device A's fetch make device B permanently unable
//! to open the content.  This small, dependency-free planner makes the only
//! valid shape explicit: one independently prepared copy per recipient device.

use std::collections::BTreeSet;

/// An opaque local device routing target.  It is not sent to cipher-store:
/// storage receives only the independently generated blob pointer/capability.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct RecipientDeviceTarget {
    pub recipient_id: String,
    pub device_id: String,
}

/// One independently prepared ciphertext copy for one recipient device.
pub struct DeviceFanoutCopy<T> {
    pub target: RecipientDeviceTarget,
    pub copy: T,
}

/// Prepare exactly one independent copy for every recipient device.
///
/// Duplicate targets are refused instead of silently coalesced: coalescing is
/// the tempting but incorrect "one blob for all devices" implementation this
/// boundary exists to prevent.
pub fn fan_out_view_once<T, F>(
    targets: Vec<RecipientDeviceTarget>,
    mut prepare: F,
) -> Result<Vec<DeviceFanoutCopy<T>>, String>
where
    F: FnMut(&RecipientDeviceTarget) -> Result<T, String>,
{
    if targets.is_empty() {
        return Err("A view-once attachment needs at least one recipient device".to_owned());
    }

    let mut seen = BTreeSet::new();
    let mut copies = Vec::with_capacity(targets.len());
    for target in targets {
        if target.recipient_id.is_empty()
            || target.device_id.is_empty()
            || target.recipient_id.chars().any(char::is_control)
            || target.device_id.chars().any(char::is_control)
        {
            return Err("A view-once recipient device target is invalid".to_owned());
        }
        if !seen.insert(target.clone()) {
            return Err("A view-once recipient device was listed more than once".to_owned());
        }
        let copy = prepare(&target)?;
        copies.push(DeviceFanoutCopy { target, copy });
    }
    Ok(copies)
}

#[cfg(test)]
mod tests {
    use super::{fan_out_view_once, RecipientDeviceTarget};

    #[test]
    fn tf_52_two_devices_get_two_independent_view_once_copies() {
        let targets = vec![
            RecipientDeviceTarget {
                recipient_id: "recipient".to_owned(),
                device_id: "device-a".to_owned(),
            },
            RecipientDeviceTarget {
                recipient_id: "recipient".to_owned(),
                device_id: "device-b".to_owned(),
            },
        ];
        let mut next_copy = 0_u8;
        let copies = fan_out_view_once(targets, |_| {
            next_copy += 1;
            Ok::<_, String>(format!("independent-blob-{next_copy}"))
        })
        .expect("each device receives a copy");

        assert_eq!(copies.len(), 2);
        assert_eq!(copies[0].target.device_id, "device-a");
        assert_eq!(copies[1].target.device_id, "device-b");
        assert_ne!(copies[0].copy, copies[1].copy);
    }

    #[test]
    fn duplicate_device_cannot_be_silently_collapsed_into_one_blob() {
        let target = RecipientDeviceTarget {
            recipient_id: "recipient".to_owned(),
            device_id: "device-a".to_owned(),
        };
        assert!(fan_out_view_once(vec![target.clone(), target], |_| Ok::<_, String>(())).is_err());
    }
}
