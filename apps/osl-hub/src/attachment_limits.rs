//! Shared native attachment admission limits.

pub const MAX_ATTACHMENT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentLimitError {
    Empty,
    TooLarge { got: u64, max: u64 },
}

pub fn check_attachment_size(size: u64) -> Result<(), AttachmentLimitError> {
    if size == 0 {
        Err(AttachmentLimitError::Empty)
    } else if size > MAX_ATTACHMENT_BYTES {
        Err(AttachmentLimitError::TooLarge {
            got: size,
            max: MAX_ATTACHMENT_BYTES,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_attachment_size_check_accepts_only_the_native_plaintext_window() {
        assert_eq!(check_attachment_size(0), Err(AttachmentLimitError::Empty));
        assert_eq!(check_attachment_size(1), Ok(()));
        assert_eq!(check_attachment_size(MAX_ATTACHMENT_BYTES), Ok(()));
        assert_eq!(
            check_attachment_size(MAX_ATTACHMENT_BYTES + 1),
            Err(AttachmentLimitError::TooLarge {
                got: MAX_ATTACHMENT_BYTES + 1,
                max: MAX_ATTACHMENT_BYTES,
            })
        );
    }
}
