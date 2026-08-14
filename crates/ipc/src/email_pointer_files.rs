pub const GMAIL_COVER_DRAFT_LIMIT_BYTES: u64 = 18 * 1024 * 1024;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GmailFixtureProtectedFile {
    pub display_name: String,
    pub size_bytes: u64,
    pub pointer_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GmailProtectedPointerDelivery {
    pub cover_draft: String,
    pub cover_draft_bytes: u64,
    pub protected_file_record_bytes: u64,
    pub split_step: &'static str,
}

pub fn send_gmail_fixture_protected_pointer_file(
    file: GmailFixtureProtectedFile,
) -> Result<GmailProtectedPointerDelivery, String> {
    gmail_fixture_pointer_file_delivery(file, true)
}

fn gmail_fixture_pointer_file_delivery(
    file: GmailFixtureProtectedFile,
    split_protected_file_record: bool,
) -> Result<GmailProtectedPointerDelivery, String> {
    if file.display_name.is_empty()
        || file.display_name.len() > 180
        || file
            .display_name
            .bytes()
            .any(|byte| byte < 0x20 || byte == b'/' || byte == b'\\')
        || file.pointer_id.is_empty()
        || file.pointer_id.len() > 180
        || file.size_bytes == 0
    {
        return Err("Gmail protected pointer file fixture is invalid".to_owned());
    }

    let cover_draft = format!(
        "OSL protected file pointer: {} ({})",
        file.display_name, file.pointer_id
    );
    let visible_cover_bytes = cover_draft.as_bytes().len() as u64;
    let counted_cover_bytes = if split_protected_file_record {
        visible_cover_bytes
    } else {
        visible_cover_bytes.saturating_add(file.size_bytes)
    };
    if counted_cover_bytes > GMAIL_COVER_DRAFT_LIMIT_BYTES {
        return Err(format!(
            "Gmail cover draft exceeds 18 MB: cover_draft_bytes={} protected_file_record_bytes={}",
            counted_cover_bytes, file.size_bytes
        ));
    }

    Ok(GmailProtectedPointerDelivery {
        cover_draft,
        cover_draft_bytes: visible_cover_bytes,
        protected_file_record_bytes: file.size_bytes,
        split_step: "protected_file_record",
    })
}
