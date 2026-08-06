use rusqlite::{params, Connection};

pub const SERVICE_BURN_SELECTION_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS service_burn_sender_messages (
    owner_osl_user_id TEXT NOT NULL,
    service_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    service_message_id TEXT NOT NULL,
    authored_by_self INTEGER NOT NULL CHECK (authored_by_self IN (0, 1)),
    created_at_unix_ms INTEGER,
    PRIMARY KEY (owner_osl_user_id, service_id, account_id, service_message_id)
);

CREATE INDEX IF NOT EXISTS idx_service_burn_sender_messages_service
    ON service_burn_sender_messages(owner_osl_user_id, service_id, authored_by_self);
"#;

pub const SELECT_SERVICE_BURN_SENDER_MESSAGES_SQL: &str = r#"
SELECT service_id, service_message_id
  FROM service_burn_sender_messages
 WHERE owner_osl_user_id = ?1
   AND service_id = ?2
   AND authored_by_self = 1
 ORDER BY created_at_unix_ms ASC, service_message_id ASC
"#;

pub const SELECT_SERVICE_ACCOUNT_BURN_SENDER_MESSAGES_SQL: &str = r#"
SELECT service_id, service_message_id
  FROM service_burn_sender_messages
 WHERE owner_osl_user_id = ?1
   AND service_id = ?2
   AND account_id = ?3
   AND authored_by_self = 1
 ORDER BY created_at_unix_ms ASC, service_message_id ASC
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceBurnSenderMessage {
    pub service_id: String,
    pub service_message_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceBurnActionResult {
    pub service_id: String,
    pub account_id: String,
    pub scope_choice: String,
    pub selected_message_count: usize,
    pub local_removal_count: usize,
    pub remote_removal_count: usize,
    pub remaining_local_count: usize,
    pub equal_removal_counts: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceBurnSenderMessageRecord {
    pub owner_osl_user_id: String,
    pub service_id: String,
    pub account_id: String,
    pub service_message_id: String,
    pub authored_by_self: bool,
    pub created_at_unix_ms: Option<i64>,
}

pub fn install_service_burn_selection_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SERVICE_BURN_SELECTION_SCHEMA)
}

pub fn record_service_burn_sender_message(
    conn: &Connection,
    record: &ServiceBurnSenderMessageRecord,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO service_burn_sender_messages \
            (owner_osl_user_id, service_id, account_id, service_message_id, \
             authored_by_self, created_at_unix_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(owner_osl_user_id, service_id, account_id, service_message_id) \
         DO UPDATE SET \
            authored_by_self = excluded.authored_by_self, \
            created_at_unix_ms = excluded.created_at_unix_ms",
        params![
            record.owner_osl_user_id,
            record.service_id,
            record.account_id,
            record.service_message_id,
            if record.authored_by_self { 1i64 } else { 0i64 },
            record.created_at_unix_ms,
        ],
    )?;
    Ok(())
}

pub fn select_service_burn_sender_messages(
    conn: &Connection,
    owner_osl_user_id: &str,
    service_id: &str,
) -> rusqlite::Result<Vec<ServiceBurnSenderMessage>> {
    let mut stmt = conn.prepare(SELECT_SERVICE_BURN_SENDER_MESSAGES_SQL)?;
    let rows = stmt.query_map(params![owner_osl_user_id, service_id], |row| {
        Ok(ServiceBurnSenderMessage {
            service_id: row.get(0)?,
            service_message_id: row.get(1)?,
        })
    })?;

    let mut selected = Vec::new();
    for row in rows {
        selected.push(row?);
    }
    Ok(selected)
}

pub fn select_service_account_burn_sender_messages(
    conn: &Connection,
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
) -> rusqlite::Result<Vec<ServiceBurnSenderMessage>> {
    let mut stmt = conn.prepare(SELECT_SERVICE_ACCOUNT_BURN_SENDER_MESSAGES_SQL)?;
    let rows = stmt.query_map(params![owner_osl_user_id, service_id, account_id], |row| {
        Ok(ServiceBurnSenderMessage {
            service_id: row.get(0)?,
            service_message_id: row.get(1)?,
        })
    })?;

    let mut selected = Vec::new();
    for row in rows {
        selected.push(row?);
    }
    Ok(selected)
}

pub fn burn_selected_service_sender_messages(
    conn: &Connection,
    state: &ipc::state::AppState,
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    scope_choice: &str,
) -> Result<ServiceBurnActionResult, String> {
    let selected = select_service_account_burn_sender_messages(
        conn,
        owner_osl_user_id,
        service_id,
        account_id,
    )
    .map_err(|error| format!("OSL: service burn selection failed: {error}"))?;
    let selected_message_ids: Vec<String> = selected
        .iter()
        .map(|message| message.service_message_id.clone())
        .collect();
    let selected_message_count = selected_message_ids.len();
    let action = ipc::commands::cmd_osl_burn_sender_message_records_choice(
        state,
        scope_choice,
        selected_message_ids,
    )?;

    Ok(ServiceBurnActionResult {
        service_id: service_id.to_owned(),
        account_id: account_id.to_owned(),
        scope_choice: scope_choice.to_owned(),
        selected_message_count,
        local_removal_count: action.local_removal_count,
        remote_removal_count: action.remote_removal_count,
        remaining_local_count: action.remaining_local_count,
        equal_removal_counts: action.equal_removal_counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use ipc::state::AppState;
    use keystore::{canonical_burn_bytes, BurnScope, KeyServerClient};
    use serde_json::Value;
    use store::{MessageStore, StoredMessage};
    use tempfile::TempDir;

    const OWNER: &str = "owner-0533";
    const DISCORD_ACCOUNT: &str = "discord-acct-0533";
    const TELEGRAM_ACCOUNT: &str = "telegram-acct-0533";
    const TASK_0534_OWNER: &str = "owner-0534";
    const TASK_0534_SERVICE: &str = "discord";
    const TASK_0534_ACCOUNT: &str = "discord-acct-0534";
    const TASK_0534_OTHER_ACCOUNT: &str = "discord-acct-0534-other";
    const TASK_0534_CHANNEL: &str = "task0534-channel";
    const TASK_0534_SECRET: &[u8; 32] = &[0x34; 32];

    fn row(
        service_id: &str,
        account_id: &str,
        service_message_id: &str,
        authored_by_self: bool,
        created_at_unix_ms: i64,
    ) -> ServiceBurnSenderMessageRecord {
        ServiceBurnSenderMessageRecord {
            owner_osl_user_id: OWNER.to_owned(),
            service_id: service_id.to_owned(),
            account_id: account_id.to_owned(),
            service_message_id: service_message_id.to_owned(),
            authored_by_self,
            created_at_unix_ms: Some(created_at_unix_ms),
        }
    }

    #[test]
    fn direct_query_selects_only_sender_messages_for_named_service() {
        let conn = Connection::open_in_memory().unwrap();
        install_service_burn_selection_schema(&conn).unwrap();
        for record in [
            row("discord", DISCORD_ACCOUNT, "1180000000000000001", true, 1),
            row("discord", DISCORD_ACCOUNT, "1180000000000000002", true, 2),
            row("discord", DISCORD_ACCOUNT, "1180000000000000003", true, 3),
            row("discord", DISCORD_ACCOUNT, "1180000000000000004", true, 4),
            row("discord", DISCORD_ACCOUNT, "1180000000000000099", false, 5),
            row("telegram", TELEGRAM_ACCOUNT, "telegram-0533-1", true, 6),
            row("telegram", TELEGRAM_ACCOUNT, "telegram-0533-2", true, 7),
        ] {
            record_service_burn_sender_message(&conn, &record).unwrap();
        }

        let selected = select_service_burn_sender_messages(&conn, OWNER, "discord").unwrap();
        let discord_ids: Vec<_> = selected
            .iter()
            .filter(|message| message.service_id == "discord")
            .map(|message| message.service_message_id.as_str())
            .collect();
        let telegram_count = selected
            .iter()
            .filter(|message| message.service_id == "telegram")
            .count();

        println!(
            "direct_query=SELECT_SERVICE_BURN_SENDER_MESSAGES_SQL service=discord discord_count={} telegram_count={} discord_ids={discord_ids:?}",
            discord_ids.len(),
            telegram_count
        );

        assert_eq!(
            discord_ids,
            vec![
                "1180000000000000001",
                "1180000000000000002",
                "1180000000000000003",
                "1180000000000000004",
            ]
        );
        assert_eq!(telegram_count, 0);
        assert!(selected
            .iter()
            .all(|message| message.service_id == "discord"));
    }

    fn task0534_row(
        account_id: &str,
        service_message_id: &str,
        authored_by_self: bool,
        created_at_unix_ms: i64,
    ) -> ServiceBurnSenderMessageRecord {
        ServiceBurnSenderMessageRecord {
            owner_osl_user_id: TASK_0534_OWNER.to_owned(),
            service_id: TASK_0534_SERVICE.to_owned(),
            account_id: account_id.to_owned(),
            service_message_id: service_message_id.to_owned(),
            authored_by_self,
            created_at_unix_ms: Some(created_at_unix_ms),
        }
    }

    fn task0534_message(id: &str, body: &str, at: i64) -> StoredMessage {
        StoredMessage {
            discord_message_id: id.to_owned(),
            channel_id: TASK_0534_CHANNEL.to_owned(),
            sender_discord_id: TASK_0534_OWNER.to_owned(),
            sender_osl_user_id: TASK_0534_OWNER.to_owned(),
            plaintext: body.to_owned(),
            decrypted_at: at,
            burned: false,
        }
    }

    fn state_with_store(dir: &std::path::Path) -> AppState {
        let state = AppState::new();
        let store = MessageStore::open(dir, TASK_0534_SECRET).expect("open message store");
        *state.message_store.lock().unwrap() = Some(store);
        state
    }

    struct SignedBurnServer {
        base_url: String,
        removed: Arc<Mutex<BTreeSet<String>>>,
        requests: Arc<Mutex<Vec<String>>>,
        server: JoinHandle<()>,
    }

    impl SignedBurnServer {
        fn start(
            expected_content_ids: Vec<String>,
            user_id: String,
            public_key: crypto::ed25519::PublicKey,
        ) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback burn server");
            let address = listener.local_addr().expect("read loopback burn address");
            let expected: BTreeSet<_> = expected_content_ids.into_iter().collect();
            let removed = Arc::new(Mutex::new(expected.clone()));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let removed_for_server = Arc::clone(&removed);
            let requests_for_server = Arc::clone(&requests);
            let server = thread::spawn(move || {
                for _ in 0..expected.len() {
                    let (mut stream, _) = listener.accept().expect("accept burn request");
                    let request = read_request(&mut stream);
                    let request_text = String::from_utf8_lossy(&request);
                    let (head, body) = request_text
                        .split_once("\r\n\r\n")
                        .expect("request has headers and body");
                    assert!(
                        head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1"),
                        "remote burn must call the wrapped-key delete endpoint"
                    );
                    let burn = serde_json::from_str::<Value>(body).expect("burn request is JSON");
                    assert_eq!(burn["scope"], "single");
                    assert_eq!(burn["user_id"], user_id);
                    let content_id = burn["target_content_id"]
                        .as_str()
                        .expect("single-scope burn names a content id");
                    let timestamp_ms = burn["timestamp_ms"]
                        .as_i64()
                        .expect("signed burn carries a timestamp");
                    let request_id = burn["request_id"]
                        .as_str()
                        .expect("signed burn carries a request id");
                    let signature_b64 = burn["burn_signature_b64"]
                        .as_str()
                        .expect("signed burn carries a signature");
                    let signature_bytes =
                        STANDARD.decode(signature_b64).expect("signature is base64");
                    let signature_array: [u8; crypto::ed25519::SIGNATURE_SIZE] =
                        signature_bytes.try_into().expect("signature is 64 bytes");
                    let signature = crypto::ed25519::Signature::from_bytes(signature_array);
                    let canonical = canonical_burn_bytes(
                        &user_id,
                        timestamp_ms,
                        request_id,
                        &BurnScope::Single {
                            content_id: content_id.to_owned(),
                        },
                    );
                    assert!(
                        crypto::ed25519::verify(&public_key, &canonical, &signature).unwrap(),
                        "remote burn request signature must verify"
                    );
                    requests_for_server
                        .lock()
                        .expect("lock requests")
                        .push(content_id.to_owned());
                    let deleted = expected.contains(content_id)
                        && removed_for_server
                            .lock()
                            .expect("lock removed set")
                            .remove(content_id);
                    let body = format!(
                        r#"{{"scope":"single","deleted_count":{}}}"#,
                        u32::from(deleted)
                    );
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                    .expect("respond to burn request");
                }
            });
            Self {
                base_url: format!("http://{address}"),
                removed,
                requests,
                server,
            }
        }

        fn join(self) -> (Vec<String>, usize) {
            self.server.join().expect("burn server exits cleanly");
            let requests = self.requests.lock().expect("lock requests").clone();
            let remaining = self.removed.lock().expect("lock removed set").len();
            (requests, remaining)
        }
    }

    fn read_request(stream: &mut TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            let read = stream.read(&mut chunk).expect("read burn request");
            assert_ne!(read, 0, "burn request ended before its body arrived");
            request.extend_from_slice(&chunk[..read]);
            let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..headers_end]);
            let content_length = headers
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .expect("burn request has content length")
                .parse::<usize>()
                .expect("content length is numeric");
            if request.len() >= headers_end + 4 + content_length {
                return request;
            }
        }
    }

    #[test]
    fn service_burn_passes_selected_records_to_chosen_both_sides_action() {
        let conn = Connection::open_in_memory().unwrap();
        install_service_burn_selection_schema(&conn).unwrap();
        let selected = vec!["118000000000005341".to_string()];
        for record in [
            task0534_row(TASK_0534_ACCOUNT, &selected[0], true, 1),
            task0534_row(TASK_0534_ACCOUNT, "118000000000005342", false, 2),
            task0534_row(TASK_0534_OTHER_ACCOUNT, "118000000000005343", true, 3),
            ServiceBurnSenderMessageRecord {
                owner_osl_user_id: TASK_0534_OWNER.to_owned(),
                service_id: "telegram".to_owned(),
                account_id: "telegram-acct-0534".to_owned(),
                service_message_id: "telegram-message-0534".to_owned(),
                authored_by_self: true,
                created_at_unix_ms: Some(4),
            },
        ] {
            record_service_burn_sender_message(&conn, &record).unwrap();
        }

        let temp = TempDir::new().unwrap();
        let state = state_with_store(temp.path());
        let identity = keystore::identity_from_entropy([0x53; 16], TASK_0534_OWNER.to_owned());
        let public_key = identity.ed25519_public;
        state.install_identity(identity);
        let server =
            SignedBurnServer::start(selected.clone(), TASK_0534_OWNER.to_owned(), public_key);
        *state.keyserver.lock().unwrap() =
            Some(KeyServerClient::new(&server.base_url).expect("install loopback keyserver"));

        {
            let guard = state.message_store.lock().unwrap();
            let store = guard.as_ref().expect("message store installed");
            store
                .put(&task0534_message(
                    &selected[0],
                    "TASK0534 selected service sender record",
                    1_900_000_534,
                ))
                .unwrap();
            store
                .put(&task0534_message(
                    "118000000000005342",
                    "TASK0534 not authored by self",
                    1_900_000_535,
                ))
                .unwrap();
            store
                .put(&task0534_message(
                    "118000000000005343",
                    "TASK0534 other service account survivor",
                    1_900_000_536,
                ))
                .unwrap();
        }

        let result = burn_selected_service_sender_messages(
            &conn,
            &state,
            TASK_0534_OWNER,
            TASK_0534_SERVICE,
            TASK_0534_ACCOUNT,
            "both-sides",
        )
        .unwrap();
        let (remote_requests, remote_remaining) = server.join();

        assert_eq!(result.service_id, TASK_0534_SERVICE);
        assert_eq!(result.selected_message_count, 1);
        assert_eq!(result.local_removal_count, 1);
        assert_eq!(result.remote_removal_count, 1);
        assert!(result.equal_removal_counts);
        assert_eq!(remote_requests, selected);
        assert_eq!(remote_remaining, 0);

        println!(
            "TASK0534 service={} scope_choice={} selected_message_count={} local_removal_count={} remote_removal_count={} equal_removal_counts={}",
            result.service_id,
            result.scope_choice,
            result.selected_message_count,
            result.local_removal_count,
            result.remote_removal_count,
            result.equal_removal_counts
        );
    }
}
