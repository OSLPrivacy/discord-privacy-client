use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES};

const GMX_MAIL_SIZE_LIMIT_BYTES: usize = 50 * 1024 * 1024;
const PROTECTED_FILE_RECORD_BYTES: usize = 51 * 1024 * 1024;
const DISABLE_GMX_POINTER_FILE_SPLIT: bool = false;

struct ProtectedFileRecord {
    pointer: [u8; TOKEN_ID_BYTES],
    bytes: Vec<u8>,
}

struct GmxPointerFileFixtureSend {
    cover_draft: String,
    file_record: ProtectedFileRecord,
    decoded_pointer_matches: bool,
}

fn send_protected_pointer_file_through_gmx_fixture() -> GmxPointerFileFixtureSend {
    let cipher = ConversationCipher::from_salt(b"task-1264-gmx-mail-size-fixture");
    let detect_key = b"task-1264-gmx-detect-key";
    let pointer = [0x64; TOKEN_ID_BYTES];
    let file_record = ProtectedFileRecord {
        pointer,
        bytes: vec![b'f'; PROTECTED_FILE_RECORD_BYTES],
    };
    let mut cover_draft = encode_token(&cipher, detect_key, &file_record.pointer);

    if DISABLE_GMX_POINTER_FILE_SPLIT {
        cover_draft.push('\n');
        cover_draft.push_str(std::str::from_utf8(&file_record.bytes).expect("fixture ascii"));
    }

    let decoded_pointer_matches =
        decode_token(&cipher, detect_key, &cover_draft) == Some(file_record.pointer);

    GmxPointerFileFixtureSend {
        cover_draft,
        file_record,
        decoded_pointer_matches,
    }
}

#[test]
fn task_1264_gmx_pointer_file_cover_ignores_file_record_size() {
    let sent = send_protected_pointer_file_through_gmx_fixture();
    let cover_draft_bytes = sent.cover_draft.len();
    let file_record_bytes = sent.file_record.bytes.len();
    let cover_draft_under_mail_limit = cover_draft_bytes < GMX_MAIL_SIZE_LIMIT_BYTES;
    let file_record_over_mail_limit = file_record_bytes > GMX_MAIL_SIZE_LIMIT_BYTES;

    println!(
        "TASK1264 carrier=gmx split_step_enabled={} cover_draft_bytes={} mail_limit_bytes={} file_record_bytes={} cover_draft_under_50mb={} file_record_over_50mb={} decoded_pointer_matches={}",
        !DISABLE_GMX_POINTER_FILE_SPLIT,
        cover_draft_bytes,
        GMX_MAIL_SIZE_LIMIT_BYTES,
        file_record_bytes,
        cover_draft_under_mail_limit,
        file_record_over_mail_limit,
        sent.decoded_pointer_matches,
    );

    assert!(
        cover_draft_under_mail_limit,
        "GMX cover draft must stay below 50 MB because it carries only the pointer"
    );
    assert!(
        file_record_over_mail_limit,
        "protected file record must prove the sent file is larger than 50 MB"
    );
    assert!(
        sent.decoded_pointer_matches,
        "the GMX fixture must send a recoverable protected pointer"
    );
}
