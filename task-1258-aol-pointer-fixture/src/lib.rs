pub const AOL_MAIL_SIZE_LIMIT_BYTES: usize = 18 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AolPointerDelivery {
    pub recipient: String,
    pub transit: &'static str,
    pub cover_draft: String,
    pub protected_pointer: String,
    pub protected_file_record: Vec<u8>,
}

pub fn send_protected_pointer_file_through_aol_fixture(
    recipient: &str,
    protected_pointer: &str,
    protected_file_record: Vec<u8>,
) -> AolPointerDelivery {
    AolPointerDelivery {
        recipient: recipient.to_owned(),
        transit: "oslProtectedEmail",
        cover_draft: format!(
            "AOL pointer cover for task 1258: open the protected file with {protected_pointer}."
        ),
        protected_pointer: protected_pointer.to_owned(),
        protected_file_record,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task1258_aol_pointer_file_ignores_mail_size() {
        let protected_pointer = "osl://protected-file/task-1258/aol-pointer";
        let protected_file_record = vec![b'F'; AOL_MAIL_SIZE_LIMIT_BYTES + 1];

        let delivery = send_protected_pointer_file_through_aol_fixture(
            "alice@aol.example.test",
            protected_pointer,
            protected_file_record,
        );

        let cover_draft_bytes = delivery.cover_draft.as_bytes().len();
        let file_record_bytes = delivery.protected_file_record.len();

        println!(
            "TASK1258 aol_limit_bytes={} cover_draft_bytes={} file_record_bytes={} protected_pointer={} transit={}",
            AOL_MAIL_SIZE_LIMIT_BYTES,
            cover_draft_bytes,
            file_record_bytes,
            delivery.protected_pointer,
            delivery.transit
        );

        assert_eq!(delivery.recipient, "alice@aol.example.test");
        assert_eq!(delivery.transit, "oslProtectedEmail");
        assert_eq!(delivery.protected_pointer, protected_pointer);
        assert!(delivery.cover_draft.contains(protected_pointer));
        assert!(
            cover_draft_bytes < AOL_MAIL_SIZE_LIMIT_BYTES,
            "AOL cover draft must stay below 18 MiB"
        );
        assert!(
            file_record_bytes > AOL_MAIL_SIZE_LIMIT_BYTES,
            "protected file record must be larger than 18 MiB"
        );
    }
}
