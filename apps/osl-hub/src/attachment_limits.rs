//! Shared native attachment admission limits.

pub const FREE_MAX_ATTACHMENT_BYTES: u64 = 50 * 1024 * 1024;
pub const PRO_MAX_ATTACHMENT_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_ATTACHMENT_BYTES: u64 = PRO_MAX_ATTACHMENT_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentAccountTier {
    Free,
    Pro,
}

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentLimitError {
    Empty,
    TooLarge { got: u64, max: u64 },
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
}
