use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::osl_mail::{
    open_osl_mail_sealed_body, provision, send, OslMailSealedEnvelope, OslMailState,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;

#[test]
fn task4323_osl_mail_send_uploads_fetchable_sealed_body() {
    let (base_url, captured_rx) = spawn_mail_service();
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("keyserver.json"),
        format!(r#"{{"base_url":"{base_url}"}}"#),
    )
    .unwrap();
    keystore::set_base_dir_override(Some(temp.path().to_path_buf()));
    keystore::set_active_account_dir(None);

    let identity = keystore::generate_identity("task-4323-sender".to_owned());
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(identity.clone());
    let state = OslMailState::default();

    let provisioned = provision(&core, &state, "task4323sender".to_owned()).unwrap();
    assert_eq!(
        provisioned.address.as_deref(),
        Some("task4323sender@oslprivacy.com")
    );

    let recipient = "receiver@oslprivacy.com";
    let subject = "OSL protected message";
    let typed = "TASK4323 one sealed OSL Mail message body";
    let receipt = send(
        &core,
        &state,
        recipient.to_owned(),
        subject.to_owned(),
        typed.to_owned(),
    )
    .unwrap();
    let upload = captured_rx.recv().unwrap();
    keystore::set_base_dir_override(None);

    let fetched_body = STANDARD
        .decode(upload["ciphertext_b64"].as_str().unwrap())
        .unwrap();
    let envelope: OslMailSealedEnvelope =
        serde_json::from_value(upload["envelope"].clone()).unwrap();
    let opened =
        open_osl_mail_sealed_body(&identity, recipient, subject, &envelope, &fetched_body).unwrap();
    let readable_count = count_readable_ascii(&fetched_body);
    let subject_fp = sha256_hex(subject.as_bytes());
    let body_fp = sha256_hex(typed.as_bytes());
    let recipient_fp = sha256_hex(recipient.as_bytes());

    println!(
        "TASK4323 finish_line sealed_body_len={} message_len={} opened=\"{}\" readable_count={} subject_sha256={} body_sha256={} recipient_sha256={} receipt_message_id={}",
        fetched_body.len(),
        typed.as_bytes().len(),
        opened,
        readable_count,
        envelope.subject_sha256,
        envelope.body_sha256,
        envelope.recipient_sha256,
        receipt.client_message_id
    );

    assert_eq!(envelope.version, 2);
    assert_eq!(fetched_body.len(), typed.as_bytes().len());
    assert_eq!(envelope.sealed_body_len, typed.as_bytes().len());
    assert_eq!(opened, typed);
    assert_eq!(readable_count, 0);
    assert_eq!(envelope.subject_sha256, subject_fp);
    assert_eq!(envelope.body_sha256, body_fp);
    assert_eq!(envelope.recipient_sha256, recipient_fp);
    assert_eq!(upload["recipient_key_fingerprint"], recipient_fp);
}

fn count_readable_ascii(upload: &[u8]) -> usize {
    upload
        .iter()
        .filter(|byte| byte.is_ascii_graphic() || **byte == b' ')
        .count()
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn spawn_mail_service() -> (String, mpsc::Receiver<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for request_index in 0..4 {
            let mut stream = listener.accept().unwrap().0;
            let request = read_http_request(&mut stream);
            match request_index {
                0 | 2 => {
                    assert!(request.starts_with("GET /v1/mail/capabilities "));
                    write_json(
                        &mut stream,
                        r#"{"version":1,"addressDomain":"oslprivacy.com","oslToOslE2ee":true}"#,
                    );
                }
                1 => {
                    assert!(request.starts_with("POST /v1/mail/address "));
                    write_json(
                        &mut stream,
                        r#"{"address":"task4323sender@oslprivacy.com","username":"task4323sender","user_id":"task-4323-sender","state":"active"}"#,
                    );
                }
                3 => {
                    assert!(request.starts_with("POST /v1/mail/send/osl "));
                    let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
                    tx.send(serde_json::from_str(body).unwrap()).unwrap();
                    write_json(
                        &mut stream,
                        r#"{"message_id":"task-4323-message","accepted":true}"#,
                    );
                }
                _ => unreachable!(),
            }
        }
    });
    (format!("http://{address}"), rx)
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buffer = [0u8; 8192];
    let mut bytes = Vec::new();
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "connection closed before request completed");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_len = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .or_else(|| {
                    headers
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                })
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_len {
                return String::from_utf8_lossy(&bytes).to_string();
            }
        }
    }
}

fn write_json(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.as_bytes().len(),
        body
    )
    .unwrap();
}
