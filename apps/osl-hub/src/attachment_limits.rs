//! Shared native attachment admission limits.

pub const FREE_MAX_ATTACHMENT_BYTES: u64 = 25_000_000;
pub const PRO_MAX_ATTACHMENT_BYTES: u64 = 1_000_000_000;
pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 16;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(result: Result<(), AttachmentLimitError>) -> &'static str {
        if result.is_ok() {
            "accept"
        } else {
            "reject"
        }
    }

    #[test]
    fn task0051_checks_attachment_limits_directly() {
        let cases = [
            (
                "24 MB",
                verdict(check_attachment_request(
                    24_000_000,
                    1,
                    AttachmentAccountTier::Free,
                )),
                "accept",
            ),
            (
                "26 MB",
                verdict(check_attachment_request(
                    26_000_000,
                    1,
                    AttachmentAccountTier::Free,
                )),
                "reject",
            ),
            (
                "999 MB",
                verdict(check_attachment_request(
                    999_000_000,
                    1,
                    AttachmentAccountTier::Pro,
                )),
                "accept",
            ),
            (
                "1.1 GB",
                verdict(check_attachment_request(
                    1_100_000_000,
                    1,
                    AttachmentAccountTier::Pro,
                )),
                "reject",
            ),
            (
                "16 files",
                verdict(check_attachment_request(1, 16, AttachmentAccountTier::Pro)),
                "accept",
            ),
            (
                "17 files",
                verdict(check_attachment_request(1, 17, AttachmentAccountTier::Pro)),
                "reject",
            ),
        ];

        for (label, actual, expected) in cases {
            println!("TASK0051 attachment_limit case={label} result={actual}");
            assert_eq!(actual, expected, "{label} must be {expected}");
        }
    }
}
