//! Shared native attachment admission limits.

pub const FREE_MAX_ATTACHMENT_BYTES: u64 = 25 * 1024 * 1024;
pub const PRO_MAX_ATTACHMENT_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ATTACHMENT_BYTES: u64 = PRO_MAX_ATTACHMENT_BYTES;
pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentAccountTier {
    Free,
    Pro,
}

/// Every account tier that the shipping desktop can independently discover.
/// Keep this closed inventory next to the admission limits so a new tier
/// cannot silently inherit another tier's attachment path.
pub const SHIPPING_ATTACHMENT_TIERS: [AttachmentAccountTier; 2] =
    [AttachmentAccountTier::Free, AttachmentAccountTier::Pro];

impl AttachmentAccountTier {
    pub const fn max_attachment_bytes(self) -> u64 {
        match self {
            Self::Free => FREE_MAX_ATTACHMENT_BYTES,
            Self::Pro => PRO_MAX_ATTACHMENT_BYTES,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Pro => "Pro",
        }
    }

    pub const fn wire_label(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Pro => "pro",
        }
    }
}

/// Resolve the account tier from the same native license state used by the
/// rest of the shipping application. `PaidOfflineGrace` remains Pro because
/// the entitlement layer already treats it as paid-equivalent.
pub fn shipping_account_tier(state: &ipc::AppState) -> AttachmentAccountTier {
    if ipc::tier_gate::is_paid_equivalent(state) {
        AttachmentAccountTier::Pro
    } else {
        AttachmentAccountTier::Free
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentLimitError {
    Empty,
    TooLarge { got: u64, max: u64 },
    EmptyBatch,
    TooManyFiles { got: usize, max: usize },
}

pub fn check_attachment_size(
    size: u64,
    tier: AttachmentAccountTier,
) -> Result<(), AttachmentLimitError> {
    let max = tier.max_attachment_bytes();
    if size == 0 {
        Err(AttachmentLimitError::Empty)
    } else if size > max {
        Err(AttachmentLimitError::TooLarge { got: size, max })
    } else {
        Ok(())
    }
}

pub fn check_attachment_count(count: usize) -> Result<(), AttachmentLimitError> {
    if count == 0 {
        Err(AttachmentLimitError::EmptyBatch)
    } else if count > MAX_ATTACHMENTS_PER_MESSAGE {
        Err(AttachmentLimitError::TooManyFiles {
            got: count,
            max: MAX_ATTACHMENTS_PER_MESSAGE,
        })
    } else {
        Ok(())
    }
}

pub fn check_attachment_request(
    size: u64,
    count: usize,
    tier: AttachmentAccountTier,
) -> Result<(), AttachmentLimitError> {
    check_attachment_size(size, tier)?;
    check_attachment_count(count)
}

pub fn attachment_limit_command(
    tier: AttachmentAccountTier,
    bytes_per_file: u64,
    file_count: usize,
) -> &'static str {
    match check_attachment_request(bytes_per_file, file_count, tier) {
        Ok(()) => "accept",
        Err(_) => "reject",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_attachment_size_check_uses_the_account_tier_window() {
        assert_eq!(
            check_attachment_size(0, AttachmentAccountTier::Free),
            Err(AttachmentLimitError::Empty)
        );
        assert_eq!(
            check_attachment_size(1, AttachmentAccountTier::Free),
            Ok(())
        );
        assert_eq!(
            check_attachment_size(FREE_MAX_ATTACHMENT_BYTES, AttachmentAccountTier::Free),
            Ok(())
        );
        assert_eq!(
            check_attachment_size(FREE_MAX_ATTACHMENT_BYTES + 1, AttachmentAccountTier::Free),
            Err(AttachmentLimitError::TooLarge {
                got: FREE_MAX_ATTACHMENT_BYTES + 1,
                max: FREE_MAX_ATTACHMENT_BYTES,
            })
        );
        assert_eq!(
            check_attachment_size(PRO_MAX_ATTACHMENT_BYTES, AttachmentAccountTier::Pro),
            Ok(())
        );
        assert_eq!(
            check_attachment_size(PRO_MAX_ATTACHMENT_BYTES + 1, AttachmentAccountTier::Pro),
            Err(AttachmentLimitError::TooLarge {
                got: PRO_MAX_ATTACHMENT_BYTES + 1,
                max: PRO_MAX_ATTACHMENT_BYTES,
            })
        );
    }

    #[test]
    fn task0051_checks_attachment_limits_directly() {
        const MIB: u64 = 1024 * 1024;
        let cases = [
            ("24 MB", AttachmentAccountTier::Free, 24 * MIB, 1, "accept"),
            ("26 MB", AttachmentAccountTier::Free, 26 * MIB, 1, "reject"),
            ("999 MB", AttachmentAccountTier::Pro, 999 * MIB, 1, "accept"),
            (
                "1.1 GB",
                AttachmentAccountTier::Pro,
                1127 * MIB,
                1,
                "reject",
            ),
            (
                "16 files",
                AttachmentAccountTier::Free,
                1 * MIB,
                16,
                "accept",
            ),
            (
                "17 files",
                AttachmentAccountTier::Free,
                1 * MIB,
                17,
                "reject",
            ),
        ];

        for (label, tier, bytes_per_file, file_count, expected) in cases {
            let actual = attachment_limit_command(tier, bytes_per_file, file_count);
            println!("TASK0051 attachment_limit case={label} result={actual}");
            assert_eq!(actual, expected, "{label} must be {expected}");
        }
    }
}
