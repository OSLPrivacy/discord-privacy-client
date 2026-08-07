//! Shared native attachment admission limits.

pub const FREE_MAX_ATTACHMENT_BYTES: u64 = 25_000_000;
pub const PRO_MAX_ATTACHMENT_BYTES: u64 = 1_000_000_000;
pub const MAX_ATTACHMENT_BYTES: u64 = PRO_MAX_ATTACHMENT_BYTES;
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

    pub const fn max_bytes_per_file(self) -> u64 {
        self.max_attachment_bytes()
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

pub fn check_attachment_limits(
    tier: AttachmentAccountTier,
    bytes_per_file: u64,
    file_count: usize,
) -> Result<(), AttachmentLimitError> {
    check_attachment_request(bytes_per_file, file_count, tier)
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

fn grouped_decimal(value: u64) -> String {
    let text = value.to_string();
    let first_group = text.len() % 3;
    let mut grouped = String::with_capacity(text.len() + text.len() / 3);
    let mut index = 0;
    if first_group != 0 {
        grouped.push_str(&text[..first_group]);
        index = first_group;
    }
    while index < text.len() {
        if !grouped.is_empty() {
            grouped.push(',');
        }
        grouped.push_str(&text[index..index + 3]);
        index += 3;
    }
    grouped
}

fn size_with_exact_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 && bytes % 1_000_000_000 == 0 {
        format!(
            "{} GB ({} bytes)",
            bytes / 1_000_000_000,
            grouped_decimal(bytes)
        )
    } else if bytes >= 1_000_000 && bytes % 1_000_000 == 0 {
        format!(
            "{} MB ({} bytes)",
            bytes / 1_000_000,
            grouped_decimal(bytes)
        )
    } else {
        format!("{} bytes", grouped_decimal(bytes))
    }
}

pub fn free_too_large_attachment_message(size_bytes: u64) -> String {
    format!(
        "This file is {}. Free limit is 25 MB (25,000,000 bytes). Pro limit is 1 GB (1,000,000,000 bytes). Upgrade to Pro to send this file.",
        size_with_exact_bytes(size_bytes)
    )
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
    fn task_0048_free_too_large_message_names_size_limits_and_upgrade() {
        let message = free_too_large_attachment_message(26_000_000);
        println!("TASK0048_MESSAGE {message}");
        assert_eq!(
            message,
            "This file is 26 MB (26,000,000 bytes). Free limit is 25 MB (25,000,000 bytes). Pro limit is 1 GB (1,000,000,000 bytes). Upgrade to Pro to send this file."
        );
        assert_eq!(message.matches("Upgrade").count(), 1);
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
