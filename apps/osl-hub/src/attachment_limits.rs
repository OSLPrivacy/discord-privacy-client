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

impl AttachmentAccountTier {
    pub const fn max_attachment_bytes(self) -> u64 {
        self.max_bytes_per_file()
    }

    pub const fn max_bytes_per_file(self) -> u64 {
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
    TooLarge {
        got: u64,
        max: u64,
    },
    EmptyBatch,
    TooManyFiles {
        got: usize,
        max: usize,
    },
    InvalidFileSize,
    FileTooLarge {
        tier: AttachmentAccountTier,
        bytes: u64,
        max: u64,
    },
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

pub fn check_attachment_limits(
    tier: AttachmentAccountTier,
    bytes_per_file: u64,
    file_count: usize,
) -> Result<(), AttachmentLimitError> {
    if bytes_per_file == 0 {
        return Err(AttachmentLimitError::InvalidFileSize);
    }
    let max = tier.max_bytes_per_file();
    if bytes_per_file > max {
        return Err(AttachmentLimitError::FileTooLarge {
            tier,
            bytes: bytes_per_file,
            max,
        });
    }
    check_attachment_count(file_count)
}

pub fn attachment_limit_command(
    tier: AttachmentAccountTier,
    bytes_per_file: u64,
    file_count: usize,
) -> &'static str {
    match check_attachment_limits(tier, bytes_per_file, file_count) {
        Ok(()) => "accept",
        Err(_) => "reject",
    }
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
            (
                "24 MB",
                verdict(check_attachment_request(
                    24 * MIB,
                    1,
                    AttachmentAccountTier::Free,
                )),
                "accept",
            ),
            (
                "26 MB",
                verdict(check_attachment_request(
                    26 * MIB,
                    1,
                    AttachmentAccountTier::Free,
                )),
                "reject",
            ),
            (
                "999 MB",
                verdict(check_attachment_request(
                    999 * MIB,
                    1,
                    AttachmentAccountTier::Pro,
                )),
                "accept",
            ),
            (
                "1.1 GB",
                verdict(check_attachment_request(
                    1127 * MIB,
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
