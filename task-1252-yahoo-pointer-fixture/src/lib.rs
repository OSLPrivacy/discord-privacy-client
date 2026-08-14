pub const YAHOO_MAIL_SIZE_LIMIT_BYTES: usize = 18 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YahooPointerDelivery {
    pub recipient: String,
    pub transit: &'static str,
    pub cover_draft: String,
    pub protected_file_record: String,
}

pub fn send_protected_pointer_file_through_yahoo_fixture(
    recipient: &str,
    cover_draft: String,
    protected_file_record: String,
) -> YahooPointerDelivery {
    YahooPointerDelivery {
        recipient: recipient.to_owned(),
        transit: "oslProtectedEmail",
        cover_draft,
        protected_file_record,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task1252_yahoo_pointer_file_ignores_mail_size() {
        let cover =
            "Yahoo pointer cover for task 1252: the draft carries only the protected file pointer."
                .to_owned();
        let protected_file_record = "F".repeat(YAHOO_MAIL_SIZE_LIMIT_BYTES + 1);

        let delivery = send_protected_pointer_file_through_yahoo_fixture(
            "alice@oslprivacy.com",
            cover.clone(),
            protected_file_record,
        );

        let cover_draft_bytes = delivery.cover_draft.as_bytes().len();
        let file_record_bytes = delivery.protected_file_record.as_bytes().len();

        println!(
            "TASK1252 yahoo_limit_bytes={} cover_draft_bytes={} file_record_bytes={} split_step=enabled",
            YAHOO_MAIL_SIZE_LIMIT_BYTES, cover_draft_bytes, file_record_bytes
        );

        assert_eq!(delivery.recipient, "alice@oslprivacy.com");
        assert_eq!(delivery.transit, "oslProtectedEmail");
        assert!(
            cover_draft_bytes < YAHOO_MAIL_SIZE_LIMIT_BYTES,
            "Yahoo cover draft must stay below 18 MiB"
        );
        assert!(
            file_record_bytes > YAHOO_MAIL_SIZE_LIMIT_BYTES,
            "protected file record must be larger than 18 MiB"
        );
        assert_eq!(delivery.cover_draft, cover);
    }
}
