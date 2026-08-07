pub const OUTLOOK_DESKTOP_MAIL_SIZE_LIMIT_BYTES: usize = 29 * 1024 * 1024 / 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlookDesktopPointerDelivery {
    pub recipient: String,
    pub transit: &'static str,
    pub cover_draft: String,
    pub protected_pointer: String,
    pub protected_file_record: Vec<u8>,
    pub split_step: &'static str,
}

pub fn send_protected_pointer_file_through_outlook_desktop_fixture(
    recipient: &str,
    protected_pointer: &str,
    protected_file_record: Vec<u8>,
) -> OutlookDesktopPointerDelivery {
    let cover_draft = split_protected_file_into_pointer_cover(protected_pointer);

    OutlookDesktopPointerDelivery {
        recipient: recipient.to_owned(),
        transit: "outlookDesktopProtectedEmail",
        cover_draft,
        protected_pointer: protected_pointer.to_owned(),
        protected_file_record,
        split_step: "enabled",
    }
}

fn split_protected_file_into_pointer_cover(protected_pointer: &str) -> String {
    format!(
        "Outlook desktop pointer cover for task 1289: open the protected file with {protected_pointer}."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task1289_outlook_desktop_pointer_file_ignores_mail_size() {
        let protected_pointer = "osl://protected-file/task-1289/outlook-desktop-pointer";
        let protected_file_record = vec![b'F'; OUTLOOK_DESKTOP_MAIL_SIZE_LIMIT_BYTES + 1];

        let delivery = send_protected_pointer_file_through_outlook_desktop_fixture(
            "alice@outlook-desktop.example.test",
            protected_pointer,
            protected_file_record,
        );

        let cover_draft_bytes = delivery.cover_draft.as_bytes().len();
        let file_record_bytes = delivery.protected_file_record.len();

        println!(
            "TASK1289 outlook_desktop_limit_bytes={} cover_draft_bytes={} file_record_bytes={} protected_pointer={} transit={} split_step={}",
            OUTLOOK_DESKTOP_MAIL_SIZE_LIMIT_BYTES,
            cover_draft_bytes,
            file_record_bytes,
            delivery.protected_pointer,
            delivery.transit,
            delivery.split_step
        );

        assert_eq!(delivery.recipient, "alice@outlook-desktop.example.test");
        assert_eq!(delivery.transit, "outlookDesktopProtectedEmail");
        assert_eq!(delivery.protected_pointer, protected_pointer);
        assert!(delivery.cover_draft.contains(protected_pointer));
        assert!(
            cover_draft_bytes < OUTLOOK_DESKTOP_MAIL_SIZE_LIMIT_BYTES,
            "Outlook desktop cover draft must stay below 14.5 MiB"
        );
        assert!(
            file_record_bytes > OUTLOOK_DESKTOP_MAIL_SIZE_LIMIT_BYTES,
            "protected file record must be larger than 14.5 MiB"
        );
        assert_eq!(delivery.split_step, "enabled");
    }
}
