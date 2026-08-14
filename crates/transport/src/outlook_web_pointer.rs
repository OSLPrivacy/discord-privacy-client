pub const OUTLOOK_WEB_MAIL_SIZE_LIMIT_BYTES: usize = 14_500_000;
pub const TASK_1240_FILE_SIZE_BYTES: usize = OUTLOOK_WEB_MAIL_SIZE_LIMIT_BYTES + 704_353;

const SPLIT_PROTECTED_FILE_RECORD_FROM_COVER: bool = true;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedPointerFile {
    pub display_name: String,
    pub file_size_bytes: usize,
    pub pointer_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebCoverDraft {
    pub body: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutlookWebPointerSend {
    pub cover_draft: OutlookWebCoverDraft,
    pub file_record: ProtectedPointerFile,
}

impl OutlookWebCoverDraft {
    pub fn byte_len(&self) -> usize {
        self.body.len()
    }
}

pub fn fixture_large_protected_pointer_file() -> ProtectedPointerFile {
    ProtectedPointerFile {
        display_name: "task-1240-large-protected-file.bin".to_owned(),
        file_size_bytes: TASK_1240_FILE_SIZE_BYTES,
        pointer_id: "osl-pointer-task-1240-00000000000000000000000000000000".to_owned(),
    }
}

pub fn send_outlook_web_protected_pointer_file(
    file_record: ProtectedPointerFile,
) -> OutlookWebPointerSend {
    let mut body = format!(
        "OSL protected file pointer\npointer={}\nfile={}\n",
        file_record.pointer_id, file_record.display_name
    );

    if !SPLIT_PROTECTED_FILE_RECORD_FROM_COVER {
        body.push_str("\nprotected-file-record-inline=");
        body.extend(std::iter::repeat('x').take(file_record.file_size_bytes));
    }

    OutlookWebPointerSend {
        cover_draft: OutlookWebCoverDraft { body },
        file_record,
    }
}
