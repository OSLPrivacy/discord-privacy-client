#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentAccountTier {
    Free,
    Pro,
}

impl AttachmentAccountTier {
    pub const fn max_bytes_per_file(self) -> u64 {
        match self {
            Self::Free => FREE_MAX_ATTACHMENT_BYTES,
            Self::Pro => PRO_MAX_ATTACHMENT_BYTES,
        }
    }
}

pub const FREE_MAX_ATTACHMENT_BYTES: u64 = 25 * 1024 * 1024;
pub const PRO_MAX_ATTACHMENT_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentLimitError {
    InvalidFileSize,
    FileTooLarge {
        tier: AttachmentAccountTier,
        bytes: u64,
        max: u64,
    },
    InvalidFileCount,
    TooManyFiles {
        count: usize,
        max: usize,
    },
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

pub fn check_attachment_count(file_count: usize) -> Result<(), AttachmentLimitError> {
    if file_count == 0 {
        return Err(AttachmentLimitError::InvalidFileCount);
    }
    if file_count > MAX_ATTACHMENTS_PER_MESSAGE {
        return Err(AttachmentLimitError::TooManyFiles {
            count: file_count,
            max: MAX_ATTACHMENTS_PER_MESSAGE,
        });
    }
    Ok(())
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

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn task0051_checks_attachment_limits_directly() {
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
