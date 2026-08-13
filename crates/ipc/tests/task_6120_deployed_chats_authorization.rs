//! TASK 6120 — prove deployed OSL Chats authorization against hostile clients.
//!
//! This check never calls an authorization function. It deploys the shipping
//! `osl-chats-service` binary into a throwaway install prefix, starts it as a
//! separate process on a real TCP port, installs five independently keyed
//! `osl-chats-client` copies (enclave owner, ordinary member, limited-channel
//! member, excluded member, never-member), and then drives every request by
//! executing an installed client, which signs it with a secret this process
//! never sees.
//!
//! What it observes is therefore two things, neither of them self-authored:
//!
//!   * the raw bytes each client actually received off the socket, status line
//!     and headers included, saved by the client itself, and
//!   * the service's own decision journal and rights snapshot, read off disk
//!     from the service's data directory — written by the service process, not
//!     by any client and not by this checker.
//!
//! Every frozen identifier is fixed below before the service is deployed.
//!
//! Starvation knobs (`TASK6120_STARVE_*`) exist so the break-it sibling can
//! show that removing a role, an endpoint, an identifier, the authorized
//! controls, the hostile requests, the captured bytes or the independent
//! service read makes this check exit non-zero naming what went missing. They
//! remove work from the checker; they never relax the service.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::Engine as _;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Frozen identifiers. Fixed before the run; the service is seeded from them and
// every authorized answer is compared against them exactly.
// ---------------------------------------------------------------------------

const ENCLAVE: &str = "enclave-6120-frozen-a91f";
const OPEN_CHANNEL: &str = "channel-6120-open-10a0";
const RESTRICTED_CHANNEL: &str = "channel-6120-limited-20b0";
const OPEN_THREAD: &str = "thread-6120-open-31c1";
const CHILD_THREAD: &str = "thread-6120-child-30c0";
const MSG_OPEN: &str = "message-6120-open-40d0";
const MSG_OPEN_THREAD: &str = "message-6120-openthread-70a1";
const MSG_RESTRICTED: &str = "message-6120-restricted-50e0";
const MSG_CHILD: &str = "message-6120-thread-60f0";

const OWNER_NAME: &str = "Ada TASK6120";
const MEMBER_NAME: &str = "Ben TASK6120";
const LIMITED_NAME: &str = "Cy TASK6120";
const EXCLUDED_NAME: &str = "Del TASK6120";
const NEVER_NAME: &str = "Eve TASK6120";

const ROLES: [(&str, &str); 5] = [
    ("owner", OWNER_NAME),
    ("member", MEMBER_NAME),
    ("limited", LIMITED_NAME),
    ("excluded", EXCLUDED_NAME),
    ("never", NEVER_NAME),
];

const OPS: [&str; 8] = [
    "role.grant",
    "roster.list",
    "channel.list",
    "history.read",
    "history.search",
    "history.sync",
    "history.subscribe",
    "blob.fetch",
];

const FROZEN_MEMBER_NAMES: [&str; 5] = [
    OWNER_NAME,
    MEMBER_NAME,
    LIMITED_NAME,
    EXCLUDED_NAME,
    NEVER_NAME,
];
const FROZEN_CHANNEL_THREAD_IDS: [&str; 4] =
    [OPEN_CHANNEL, RESTRICTED_CHANNEL, OPEN_THREAD, CHILD_THREAD];
const FROZEN_MESSAGE_IDS: [&str; 4] = [MSG_OPEN, MSG_OPEN_THREAD, MSG_RESTRICTED, MSG_CHILD];

/// The frozen identifiers an authorized control has to hand back exactly.
const FROZEN_IDENTIFIERS: [&str; 12] = [
    ENCLAVE,
    OPEN_CHANNEL,
    RESTRICTED_CHANNEL,
    OPEN_THREAD,
    CHILD_THREAD,
    MSG_OPEN,
    MSG_OPEN_THREAD,
    MSG_RESTRICTED,
    MSG_CHILD,
    OWNER_NAME,
    MEMBER_NAME,
    LIMITED_NAME,
];

fn ciphertext_of(message_id: &str) -> Vec<u8> {
    Sha256::digest(format!("TASK6120/ciphertext/{message_id}").as_bytes()).to_vec()
}

fn ciphertext_b64(message_id: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(ciphertext_of(message_id))
}

// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Plan {
    label: &'static str,
    op: &'static str,
    hostile: bool,
    body: serde_json::Value,
    /// `None` for an authorized control; the exact refusal code otherwise.
    expect_code: Option<&'static str>,
    /// Exact list the response must carry, as (json field, extracted ids).
    exact: Option<(&'static str, Vec<String>)>,
    must_contain: Vec<String>,
    must_not_contain: Vec<String>,
    forge_key_of: Option<&'static str>,
    tamper: bool,
    audit_label: &'static str,
}

impl Plan {
    fn allow(label: &'static str, op: &'static str, body: serde_json::Value) -> Self {
        Self {
            label,
            op,
            hostile: false,
            body,
            expect_code: None,
            exact: None,
            must_contain: Vec::new(),
            must_not_contain: Vec::new(),
            forge_key_of: None,
            tamper: false,
            audit_label: label,
        }
    }

    fn refuse(
        label: &'static str,
        op: &'static str,
        body: serde_json::Value,
        code: &'static str,
    ) -> Self {
        let mut plan = Self::allow(label, op, body);
        plan.hostile = true;
        plan.expect_code = Some(code);
        plan
    }

    fn exact(mut self, field: &'static str, ids: &[&str]) -> Self {
        self.exact = Some((field, ids.iter().map(|id| (*id).to_owned()).collect()));
        self
    }

    fn contains(mut self, tokens: &[&str]) -> Self {
        self.must_contain
            .extend(tokens.iter().map(|token| (*token).to_owned()));
        self
    }

    fn absent(mut self, tokens: &[&str]) -> Self {
        self.must_not_contain
            .extend(tokens.iter().map(|token| (*token).to_owned()));
        self
    }
}

fn plans() -> Vec<Plan> {
    let open_cipher = ciphertext_b64(MSG_OPEN);
    let restricted_cipher = ciphertext_b64(MSG_RESTRICTED);
    let child_cipher = ciphertext_b64(MSG_CHILD);

    let mut plans = vec![
        // ---- authorized controls: the enclave owner ----
        Plan::allow(
            "owner",
            "role.grant",
            serde_json::json!({"target": LIMITED_NAME, "right": "invite"}),
        )
        .exact("rights", &["read", "send", "invite"])
        .contains(&[LIMITED_NAME, ENCLAVE]),
        Plan::allow("owner", "roster.list", serde_json::json!({}))
            .exact("members", &[OWNER_NAME, MEMBER_NAME, LIMITED_NAME])
            .absent(&[EXCLUDED_NAME, NEVER_NAME]),
        Plan::allow(
            "owner",
            "roster.list",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
        )
        .exact("members", &[OWNER_NAME, LIMITED_NAME])
        .absent(&[MEMBER_NAME, EXCLUDED_NAME, NEVER_NAME]),
        Plan::allow("owner", "channel.list", serde_json::json!({}))
            .exact("channels", &[OPEN_CHANNEL, RESTRICTED_CHANNEL]),
        Plan::allow(
            "owner",
            "history.read",
            serde_json::json!({"channelId": OPEN_CHANNEL}),
        )
        .exact("messages", &[MSG_OPEN])
        .contains(&[&open_cipher]),
        Plan::allow(
            "owner",
            "history.read",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
        )
        .exact("messages", &[MSG_RESTRICTED])
        .contains(&[&restricted_cipher]),
        Plan::allow(
            "owner",
            "history.read",
            serde_json::json!({"threadId": CHILD_THREAD}),
        )
        .exact("messages", &[MSG_CHILD])
        .contains(&[CHILD_THREAD, RESTRICTED_CHANNEL, &child_cipher]),
        Plan::allow(
            "owner",
            "history.search",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL, "needle": "6120"}),
        )
        .exact("messageIds", &[MSG_RESTRICTED, MSG_CHILD]),
        Plan::allow(
            "owner",
            "history.sync",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL, "since": 0}),
        )
        .exact("messages", &[MSG_RESTRICTED, MSG_CHILD]),
        Plan::allow(
            "owner",
            "history.subscribe",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
        )
        .exact("events", &[MSG_RESTRICTED, MSG_CHILD]),
        Plan::allow(
            "owner",
            "blob.fetch",
            serde_json::json!({"messageId": MSG_RESTRICTED}),
        )
        .contains(&[MSG_RESTRICTED, &restricted_cipher]),
        Plan::allow(
            "owner",
            "blob.fetch",
            serde_json::json!({"messageId": MSG_CHILD}),
        )
        .contains(&[MSG_CHILD, &child_cipher]),
        // ---- authorized controls: an ordinary member ----
        Plan::allow("member", "roster.list", serde_json::json!({}))
            .exact("members", &[OWNER_NAME, MEMBER_NAME, LIMITED_NAME])
            .absent(&[EXCLUDED_NAME, NEVER_NAME]),
        Plan::allow(
            "member",
            "roster.list",
            serde_json::json!({"channelId": OPEN_CHANNEL}),
        )
        .exact("members", &[OWNER_NAME, MEMBER_NAME, LIMITED_NAME]),
        Plan::allow("member", "channel.list", serde_json::json!({}))
            .exact("channels", &[OPEN_CHANNEL])
            .absent(&[RESTRICTED_CHANNEL]),
        Plan::allow(
            "member",
            "history.read",
            serde_json::json!({"channelId": OPEN_CHANNEL}),
        )
        .exact("messages", &[MSG_OPEN])
        .contains(&[&open_cipher]),
        Plan::allow(
            "member",
            "history.read",
            serde_json::json!({"threadId": OPEN_THREAD}),
        )
        .exact("messages", &[MSG_OPEN_THREAD])
        .contains(&[OPEN_THREAD]),
        Plan::allow(
            "member",
            "history.search",
            serde_json::json!({"channelId": OPEN_CHANNEL, "needle": "6120"}),
        )
        .exact("messageIds", &[MSG_OPEN, MSG_OPEN_THREAD]),
        Plan::allow(
            "member",
            "history.sync",
            serde_json::json!({"channelId": OPEN_CHANNEL, "since": 0}),
        )
        .exact("messages", &[MSG_OPEN, MSG_OPEN_THREAD]),
        Plan::allow(
            "member",
            "history.subscribe",
            serde_json::json!({"channelId": OPEN_CHANNEL}),
        )
        .exact("events", &[MSG_OPEN, MSG_OPEN_THREAD]),
        Plan::allow(
            "member",
            "blob.fetch",
            serde_json::json!({"messageId": MSG_OPEN}),
        )
        .contains(&[MSG_OPEN, &open_cipher]),
        // ---- authorized controls: the limited-channel member ----
        Plan::allow("limited", "roster.list", serde_json::json!({}))
            .exact("members", &[OWNER_NAME, MEMBER_NAME, LIMITED_NAME]),
        Plan::allow(
            "limited",
            "roster.list",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
        )
        .exact("members", &[OWNER_NAME, LIMITED_NAME]),
        Plan::allow("limited", "channel.list", serde_json::json!({}))
            .exact("channels", &[OPEN_CHANNEL, RESTRICTED_CHANNEL]),
        Plan::allow(
            "limited",
            "history.read",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
        )
        .exact("messages", &[MSG_RESTRICTED]),
        Plan::allow(
            "limited",
            "history.read",
            serde_json::json!({"channelId": OPEN_CHANNEL}),
        )
        .exact("messages", &[MSG_OPEN]),
        Plan::allow(
            "limited",
            "history.read",
            serde_json::json!({"threadId": CHILD_THREAD}),
        )
        .exact("messages", &[MSG_CHILD]),
        Plan::allow(
            "limited",
            "history.search",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL, "needle": "6120"}),
        )
        .exact("messageIds", &[MSG_RESTRICTED, MSG_CHILD]),
        Plan::allow(
            "limited",
            "history.sync",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL, "since": 0}),
        )
        .exact("messages", &[MSG_RESTRICTED, MSG_CHILD]),
        Plan::allow(
            "limited",
            "history.subscribe",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
        )
        .exact("events", &[MSG_RESTRICTED, MSG_CHILD]),
        Plan::allow(
            "limited",
            "blob.fetch",
            serde_json::json!({"messageId": MSG_CHILD}),
        )
        .contains(&[MSG_CHILD, &child_cipher]),
    ];

    // ---- hostile: self-raising rights, from every installed role ----
    for (label, person) in ROLES {
        plans.push(Plan::refuse(
            label,
            "role.grant",
            serde_json::json!({"target": person, "right": "remove-people"}),
            "self-role-raise-refused",
        ));
    }

    // ---- hostile: an ordinary member reaching into the limited channel ----
    plans.extend([
        Plan::refuse(
            "member",
            "roster.list",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
            "restricted-history-refused",
        ),
        Plan::refuse(
            "member",
            "history.read",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
            "restricted-history-refused",
        ),
        Plan::refuse(
            "member",
            "history.search",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL, "needle": "6120"}),
            "restricted-history-refused",
        ),
        Plan::refuse(
            "member",
            "history.sync",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL, "since": 0}),
            "restricted-history-refused",
        ),
        Plan::refuse(
            "member",
            "history.subscribe",
            serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
            "restricted-history-refused",
        ),
        Plan::refuse(
            "member",
            "history.read",
            serde_json::json!({"threadId": CHILD_THREAD}),
            "thread-inheritance-refused",
        ),
        Plan::refuse(
            "member",
            "blob.fetch",
            serde_json::json!({"messageId": MSG_RESTRICTED}),
            "known-id-fetch-refused",
        ),
        Plan::refuse(
            "member",
            "blob.fetch",
            serde_json::json!({"messageId": MSG_CHILD}),
            "known-id-fetch-refused",
        ),
    ]);

    // ---- hostile: the excluded member and the never-member, everywhere ----
    for label in ["excluded", "never"] {
        plans.extend([
            Plan::refuse(label, "roster.list", serde_json::json!({}), "directory-refused"),
            Plan::refuse(
                label,
                "roster.list",
                serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
                "directory-refused",
            ),
            Plan::refuse(label, "channel.list", serde_json::json!({}), "directory-refused"),
            Plan::refuse(
                label,
                "history.read",
                serde_json::json!({"channelId": OPEN_CHANNEL}),
                "directory-refused",
            ),
            Plan::refuse(
                label,
                "history.read",
                serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
                "directory-refused",
            ),
            Plan::refuse(
                label,
                "history.read",
                serde_json::json!({"threadId": CHILD_THREAD}),
                "directory-refused",
            ),
            Plan::refuse(
                label,
                "history.search",
                serde_json::json!({"channelId": RESTRICTED_CHANNEL, "needle": "6120"}),
                "directory-refused",
            ),
            Plan::refuse(
                label,
                "history.sync",
                serde_json::json!({"channelId": RESTRICTED_CHANNEL, "since": 0}),
                "directory-refused",
            ),
            Plan::refuse(
                label,
                "history.subscribe",
                serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
                "directory-refused",
            ),
            Plan::refuse(
                label,
                "blob.fetch",
                serde_json::json!({"messageId": MSG_OPEN}),
                "directory-refused",
            ),
            Plan::refuse(
                label,
                "blob.fetch",
                serde_json::json!({"messageId": MSG_RESTRICTED}),
                "directory-refused",
            ),
            Plan::refuse(
                label,
                "blob.fetch",
                serde_json::json!({"messageId": MSG_CHILD}),
                "directory-refused",
            ),
        ]);
    }

    // ---- hostile: the signatures themselves ----
    let mut forged = Plan::refuse(
        "never",
        "roster.list",
        serde_json::json!({}),
        "bad-signature",
    );
    forged.forge_key_of = Some("owner");
    forged.audit_label = "owner";
    plans.push(forged);

    let mut tampered = Plan::refuse(
        "excluded",
        "history.read",
        serde_json::json!({"channelId": RESTRICTED_CHANNEL}),
        "bad-signature",
    );
    tampered.tamper = true;
    plans.push(tampered);

    plans
}

// ---------------------------------------------------------------------------

fn body_of(raw: &[u8]) -> &[u8] {
    let marker = b"\r\n\r\n";
    raw.windows(marker.len())
        .position(|window| window == marker)
        .map(|at| &raw[at + marker.len()..])
        .unwrap_or(&[])
}

fn count_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn ids_from(value: &serde_json::Value, field: &str) -> Vec<String> {
    let Some(array) = value.get(field).and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .map(|item| match item {
            serde_json::Value::String(text) => text.clone(),
            other => other
                .get("channelId")
                .or_else(|| other.get("messageId"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        })
        .collect()
}

fn wait_for_port(ready_file: &Path) -> Option<u16> {
    for _ in 0..600 {
        if let Ok(text) = std::fs::read_to_string(ready_file) {
            if let Ok(port) = text.trim().parse::<u16>() {
                if port != 0 {
                    return Some(port);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    None
}

fn healthz(port: u16) -> String {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return String::new();
    };
    let request = format!("GET /healthz HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\nconnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return String::new();
    }
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    String::from_utf8_lossy(&raw).into_owned()
}

fn copy_binary(from: &str, to: &Path) {
    std::fs::copy(from, to).unwrap_or_else(|error| panic!("cannot install {from}: {error}"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o755));
    }
}

// ---------------------------------------------------------------------------

#[test]
fn task_6120_deployed_chats_authorization_holds_against_hostile_clients() {
    let build_tag = std::env::var("TASK6120_BUILD_TAG").unwrap_or_else(|_| "production".to_owned());
    let starve_role = std::env::var("TASK6120_STARVE_ROLE").unwrap_or_default();
    let starve_op = std::env::var("TASK6120_STARVE_OP").unwrap_or_default();
    let starve_id = std::env::var("TASK6120_STARVE_ID").unwrap_or_default();
    let starve_authorized = std::env::var("TASK6120_STARVE_AUTHORIZED").is_ok();
    let starve_hostile = std::env::var("TASK6120_STARVE_HOSTILE").is_ok();
    let starve_observe = std::env::var("TASK6120_STARVE_OBSERVE").is_ok();
    let starve_bytes = std::env::var("TASK6120_STARVE_BYTES").is_ok();

    let mut failures: Vec<String> = Vec::new();

    let run_dir = std::env::temp_dir().join(format!(
        "task6120-{build_tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&run_dir);
    std::fs::create_dir_all(&run_dir).expect("run dir");

    // ---- deploy: the shipping binaries are installed, then run from the
    // ---- install prefix. Nothing in this process links the service's
    // ---- authorization code.
    let deploy_bin = run_dir.join("deploy/bin");
    let client_bin_dir = run_dir.join("install/bin");
    std::fs::create_dir_all(&deploy_bin).expect("deploy dir");
    std::fs::create_dir_all(&client_bin_dir).expect("client dir");
    let service_path = deploy_bin.join("osl-chats-service");
    let client_path = client_bin_dir.join("osl-chats-client");
    copy_binary(env!("CARGO_BIN_EXE_osl-chats-service"), &service_path);
    copy_binary(env!("CARGO_BIN_EXE_osl-chats-client"), &client_path);

    // ---- install five independently keyed clients ----
    let mut public_keys: BTreeMap<&str, String> = BTreeMap::new();
    let mut install_dirs: BTreeMap<&str, PathBuf> = BTreeMap::new();
    for (label, person) in ROLES {
        let install = run_dir.join(format!("install/{label}"));
        std::fs::create_dir_all(&install).expect("install dir");
        let output = Command::new(&client_path)
            .args([
                "--generate",
                "--install-dir",
                install.to_str().unwrap(),
                "--label",
                label,
                "--person",
                person,
            ])
            .output()
            .expect("client install");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let key = text
            .split_whitespace()
            .find_map(|token| token.strip_prefix("public="))
            .unwrap_or_default()
            .to_owned();
        if key.is_empty() {
            failures.push(format!("install produced no key for role {label}"));
        }
        println!("TASK6120 INSTALL label={label} person={person} public={key}");
        public_keys.insert(label, key);
        install_dirs.insert(label, install);
    }
    let distinct_keys: BTreeSet<&String> = public_keys.values().collect();
    if distinct_keys.len() != ROLES.len() {
        failures.push(format!(
            "installed clients are not independently keyed: {} distinct keys for {} roles",
            distinct_keys.len(),
            ROLES.len()
        ));
    }

    // ---- freeze the fixture and seed the deployed service from it ----
    let identity = |label: &str, person: &str, on_roster: bool, excluded: bool, owner: bool, rights: &[&str]| {
        serde_json::json!({
            "label": label,
            "personName": person,
            "publicKeyB64": public_keys.get(label).cloned().unwrap_or_default(),
            "onRoster": on_roster,
            "excluded": excluded,
            "owner": owner,
            "rights": rights,
        })
    };
    let fixture = serde_json::json!({
        "enclaveId": ENCLAVE,
        "ownerName": OWNER_NAME,
        "joinedAt": "2026-08-13T00:00:00Z",
        "identities": [
            identity("owner", OWNER_NAME, true, false, true,
                &["read", "send", "invite", "make-channels", "remove-messages", "remove-people", "change-server"]),
            identity("member", MEMBER_NAME, true, false, false, &["read", "send"]),
            identity("limited", LIMITED_NAME, true, false, false, &["read", "send"]),
            identity("excluded", EXCLUDED_NAME, false, true, false, &["read", "send"]),
            identity("never", NEVER_NAME, false, false, false, &[] as &[&str]),
        ],
        "openChannelId": OPEN_CHANNEL,
        "restrictedChannelId": RESTRICTED_CHANNEL,
        "restrictedPersonNames": [OWNER_NAME, LIMITED_NAME],
        "openThreadId": OPEN_THREAD,
        "childThreadId": CHILD_THREAD,
        "messages": [
            {"messageId": MSG_OPEN, "channelId": OPEN_CHANNEL, "ciphertextHex": hex::encode(ciphertext_of(MSG_OPEN))},
            {"messageId": MSG_RESTRICTED, "channelId": RESTRICTED_CHANNEL, "ciphertextHex": hex::encode(ciphertext_of(MSG_RESTRICTED))},
            {"messageId": MSG_OPEN_THREAD, "channelId": OPEN_CHANNEL, "threadId": OPEN_THREAD, "ciphertextHex": hex::encode(ciphertext_of(MSG_OPEN_THREAD))},
            {"messageId": MSG_CHILD, "channelId": RESTRICTED_CHANNEL, "threadId": CHILD_THREAD, "ciphertextHex": hex::encode(ciphertext_of(MSG_CHILD))},
        ],
    });
    let fixture_path = run_dir.join("deploy/fixture.json");
    std::fs::write(
        &fixture_path,
        serde_json::to_vec_pretty(&fixture).expect("fixture"),
    )
    .expect("write fixture");
    println!(
        "TASK6120 FROZEN enclave={ENCLAVE} open={OPEN_CHANNEL} restricted={RESTRICTED_CHANNEL} open_thread={OPEN_THREAD} child_thread={CHILD_THREAD} messages={MSG_OPEN}|{MSG_OPEN_THREAD}|{MSG_RESTRICTED}|{MSG_CHILD}"
    );

    let data_dir = run_dir.join("deploy/data");
    let ready_file = run_dir.join("deploy/port");
    let mut service = Command::new(&service_path)
        .args([
            "--data-dir",
            data_dir.to_str().unwrap(),
            "--fixture",
            fixture_path.to_str().unwrap(),
            "--ready-file",
            ready_file.to_str().unwrap(),
            "--build-tag",
            &build_tag,
        ])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("deploy service");

    let port = match wait_for_port(&ready_file) {
        Some(port) => port,
        None => {
            let _ = service.kill();
            println!("TASK6120 FAIL deployed service never bound a port");
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        }
    };
    let health = healthz(port);
    if !health.contains("200") || !health.contains(&format!("\"tag\":\"{build_tag}\"")) {
        failures.push(format!(
            "deployed service health probe did not answer for build {build_tag}: {}",
            health.replace('\n', " ").replace('\r', "")
        ));
    }
    println!("TASK6120 DEPLOYED port={port} tag={build_tag} install={}", service_path.display());

    // ---- drive every request through an installed client process ----
    let mut all = plans();
    if !starve_role.is_empty() {
        all.retain(|plan| plan.label != starve_role);
    }
    if !starve_op.is_empty() {
        all.retain(|plan| plan.op != starve_op);
    }
    if !starve_id.is_empty() {
        all.retain(|plan| {
            !plan.must_contain.iter().any(|token| token == &starve_id)
                && !plan
                    .exact
                    .as_ref()
                    .is_some_and(|(_, ids)| ids.iter().any(|id| id == &starve_id))
        });
    }
    if starve_authorized {
        all.retain(|plan| plan.hostile);
    }
    if starve_hostile {
        all.retain(|plan| !plan.hostile);
    }

    struct Observed {
        plan: Plan,
        raw: Vec<u8>,
        status: String,
    }
    let replies_dir = run_dir.join("replies");
    std::fs::create_dir_all(&replies_dir).expect("replies dir");
    let mut observed: Vec<Observed> = Vec::new();

    for (index, plan) in all.iter().enumerate() {
        let body_path = replies_dir.join(format!("req-{index:03}.json"));
        std::fs::write(
            &body_path,
            serde_json::to_vec(&plan.body).expect("request body"),
        )
        .expect("write body");
        let out_path = replies_dir.join(format!("rsp-{index:03}.bin"));
        let install = install_dirs.get(plan.label).expect("install dir").clone();
        let nonce = format!("n{index:04}");
        let base = format!("127.0.0.1:{port}");

        let mut command = Command::new(&client_path);
        command.args([
            "--install-dir",
            install.to_str().unwrap(),
            "--base",
            &base,
            "--op",
            plan.op,
            "--nonce",
            &nonce,
            "--body",
            body_path.to_str().unwrap(),
        ]);
        if !starve_bytes {
            command.args(["--out", out_path.to_str().unwrap()]);
        }
        if let Some(forged) = plan.forge_key_of {
            command.args(["--forge-public", public_keys.get(forged).unwrap()]);
        }
        if plan.tamper {
            command.arg("--tamper");
        }
        let output = command.output().expect("installed client run");
        if !output.status.success() {
            failures.push(format!(
                "installed client {} could not reach the deployed service for {}: {}",
                plan.label,
                plan.op,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let status = stdout
            .split_whitespace()
            .find_map(|token| token.strip_prefix("status="))
            .unwrap_or_default()
            .to_owned();
        let raw = std::fs::read(&out_path).unwrap_or_default();
        observed.push(Observed {
            plan: plan.clone(),
            raw,
            status,
        });
    }

    // ---- kill the deployment, then read its own record off disk ----
    let _ = service.kill();
    let _ = service.wait();

    let audit_lines: Vec<serde_json::Value> = if starve_observe {
        Vec::new()
    } else {
        std::fs::read_to_string(data_dir.join("audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    };
    let final_rights: BTreeMap<String, Vec<String>> = if starve_observe {
        BTreeMap::new()
    } else {
        std::fs::read_to_string(data_dir.join("rights.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    };
    if audit_lines.is_empty() {
        failures.push(
            "independent service-side observation is missing: the deployed service's audit journal was not read"
                .to_owned(),
        );
    }
    if final_rights.is_empty() {
        failures.push(
            "independent service-side observation is missing: the deployed service's rights snapshot was not read"
                .to_owned(),
        );
    }

    // ---- per-request judgement over the bytes the client actually got ----
    let mut authorized = 0usize;
    let mut hostile = 0usize;
    let mut hostile_member_names = 0usize;
    let mut hostile_channel_thread_ids = 0usize;
    let mut hostile_message_ids = 0usize;
    let mut hostile_ciphertext_bytes = 0usize;
    let mut hostile_response_bytes = 0usize;
    let mut scoped_leaks = 0usize;
    let mut seen_roles: BTreeSet<&str> = BTreeSet::new();
    let mut authorized_ops: BTreeSet<&str> = BTreeSet::new();
    let mut hostile_ops: BTreeSet<&str> = BTreeSet::new();
    let mut identifiers_returned: BTreeSet<String> = BTreeSet::new();

    for (index, item) in observed.iter().enumerate() {
        let plan = &item.plan;
        seen_roles.insert(plan.label);
        if item.raw.is_empty() {
            failures.push(format!(
                "request #{index} ({} {}) captured no response bytes from the deployed service",
                plan.label, plan.op
            ));
            continue;
        }
        let body = body_of(&item.raw);
        let parsed: serde_json::Value = serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);

        let mut leaks = 0usize;
        if plan.hostile {
            hostile += 1;
            hostile_ops.insert(plan.op);
            hostile_response_bytes += item.raw.len();
            for name in FROZEN_MEMBER_NAMES {
                let hits = count_occurrences(&item.raw, name.as_bytes());
                hostile_member_names += hits;
                leaks += hits;
            }
            for id in FROZEN_CHANNEL_THREAD_IDS {
                let hits = count_occurrences(&item.raw, id.as_bytes());
                hostile_channel_thread_ids += hits;
                leaks += hits;
            }
            for id in FROZEN_MESSAGE_IDS {
                let hits = count_occurrences(&item.raw, id.as_bytes());
                hostile_message_ids += hits;
                leaks += hits;
            }
            for id in FROZEN_MESSAGE_IDS {
                let bytes = ciphertext_of(id);
                let hits = count_occurrences(&item.raw, &bytes)
                    + count_occurrences(&item.raw, ciphertext_b64(id).as_bytes())
                    + count_occurrences(&item.raw, hex::encode(&bytes).as_bytes());
                hostile_ciphertext_bytes += hits * bytes.len();
                leaks += hits;
            }
            if leaks > 0 {
                let named: Vec<&str> = FROZEN_MEMBER_NAMES
                    .iter()
                    .chain(FROZEN_CHANNEL_THREAD_IDS.iter())
                    .chain(FROZEN_MESSAGE_IDS.iter())
                    .copied()
                    .filter(|token| count_occurrences(&item.raw, token.as_bytes()) > 0)
                    .collect();
                let ciphered: Vec<&str> = FROZEN_MESSAGE_IDS
                    .iter()
                    .copied()
                    .filter(|id| {
                        count_occurrences(&item.raw, &ciphertext_of(id)) > 0
                            || count_occurrences(&item.raw, ciphertext_b64(id).as_bytes()) > 0
                            || count_occurrences(&item.raw, hex::encode(ciphertext_of(id)).as_bytes()) > 0
                    })
                    .collect();
                failures.push(format!(
                    "forbidden {} by {} leaked: identifiers=[{}] ciphertext_of=[{}]",
                    plan.op,
                    plan.label,
                    named.join(", "),
                    ciphered.join(", ")
                ));
            }
            let code = parsed
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let ok = parsed.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(true);
            let expected = plan.expect_code.unwrap_or_default();
            if ok || code != expected {
                failures.push(format!(
                    "forbidden {} by {} was not refused: expected code {expected}, got ok={ok} code={code} status={}",
                    plan.op, plan.label, item.status
                ));
            }
        } else {
            authorized += 1;
            authorized_ops.insert(plan.op);
            let ok = parsed.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false);
            if !ok || item.status != "200" {
                failures.push(format!(
                    "authorized control {} by {} did not succeed: status={} body={}",
                    plan.op,
                    plan.label,
                    item.status,
                    String::from_utf8_lossy(body)
                ));
            }
            if let Some((field, expected)) = &plan.exact {
                let got = ids_from(&parsed, field);
                if &got != expected {
                    failures.push(format!(
                        "authorized control {} by {} returned {field}={got:?}, frozen value is {expected:?}",
                        plan.op, plan.label
                    ));
                } else {
                    identifiers_returned.extend(got);
                }
            }
            for token in &plan.must_contain {
                if count_occurrences(&item.raw, token.as_bytes()) == 0 {
                    failures.push(format!(
                        "authorized control {} by {} did not return frozen value {token}",
                        plan.op, plan.label
                    ));
                } else {
                    identifiers_returned.insert(token.clone());
                }
            }
            for token in &plan.must_not_contain {
                let hits = count_occurrences(&item.raw, token.as_bytes());
                if hits > 0 {
                    scoped_leaks += hits;
                    failures.push(format!(
                        "scoped answer to {} by {} leaked {token}",
                        plan.op, plan.label
                    ));
                }
            }
        }
        println!(
            "TASK6120 REQ #{index:03} label={} op={} kind={} status={} bytes={} leaks={}",
            plan.label,
            plan.op,
            if plan.hostile { "hostile" } else { "authorized" },
            item.status,
            item.raw.len(),
            leaks
        );
    }

    // ---- match the service's own journal against what the clients saw ----
    let mut audit_allowed = 0usize;
    let mut audit_denied = 0usize;
    let mut raised_rights = 0usize;
    let mut unchanged_hostile_rights = 0usize;
    if !audit_lines.is_empty() {
        if audit_lines.len() != observed.len() {
            failures.push(format!(
                "the deployed service journalled {} requests but {} were sent",
                audit_lines.len(),
                observed.len()
            ));
        }
        for (index, entry) in audit_lines.iter().enumerate() {
            let decision = entry
                .get("decision")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if decision == "allow" {
                audit_allowed += 1;
            } else {
                audit_denied += 1;
            }
            let Some(item) = observed.get(index) else {
                continue;
            };
            let plan = &item.plan;
            let label = entry
                .get("clientLabel")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let op = entry.get("op").and_then(serde_json::Value::as_str).unwrap_or_default();
            let code = entry.get("code").and_then(serde_json::Value::as_str).unwrap_or_default();
            let tag = entry
                .get("buildTag")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if tag != build_tag {
                failures.push(format!(
                    "journal entry {index} came from build {tag}, not the deployed build {build_tag}"
                ));
            }
            if label != plan.audit_label || op != plan.op {
                failures.push(format!(
                    "journal entry {index} is {label}/{op}, the client sent {}/{}",
                    plan.audit_label, plan.op
                ));
            }
            let want_allow = !plan.hostile;
            if want_allow != (decision == "allow") {
                failures.push(format!(
                    "the deployed service recorded {decision} for {}/{} but the request was {}",
                    plan.label,
                    plan.op,
                    if plan.hostile { "forbidden" } else { "authorized" }
                ));
            }
            if plan.hostile {
                if let Some(expected) = plan.expect_code {
                    if code != expected {
                        failures.push(format!(
                            "the deployed service recorded refusal {code} for {}/{}, expected {expected}",
                            plan.label, plan.op
                        ));
                    }
                }
                let before = ids_of(entry, "rightsBefore");
                let after = ids_of(entry, "rightsAfter");
                let subject = entry
                    .get("rightsSubject")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                if before == after {
                    unchanged_hostile_rights += 1;
                } else {
                    let gained: Vec<String> =
                        after.iter().filter(|right| !before.contains(right)).cloned().collect();
                    raised_rights += gained.len();
                    failures.push(format!(
                        "forbidden write by {} raised rights on {subject}: gained {:?}",
                        plan.label, gained
                    ));
                }
            }
        }
    }

    // ---- expected end state, read from the service's own snapshot ----
    let expected_rights: BTreeMap<String, Vec<String>> = BTreeMap::from([
        (
            OWNER_NAME.to_owned(),
            vec![
                "read", "send", "invite", "make-channels", "remove-messages", "remove-people",
                "change-server",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        ),
        (MEMBER_NAME.to_owned(), vec!["read".to_owned(), "send".to_owned()]),
        (
            LIMITED_NAME.to_owned(),
            vec!["read".to_owned(), "send".to_owned(), "invite".to_owned()],
        ),
        (EXCLUDED_NAME.to_owned(), Vec::new()),
        (NEVER_NAME.to_owned(), Vec::new()),
    ]);
    if !final_rights.is_empty() && !starve_authorized {
        for (person, expected) in &expected_rights {
            let mut want = expected.clone();
            want.sort();
            let mut got = final_rights.get(person).cloned().unwrap_or_default();
            got.sort();
            if want != got {
                failures.push(format!(
                    "the deployed service's rights for {person} are {got:?}, expected {want:?}"
                ));
                for right in &got {
                    if !want.contains(right) {
                        raised_rights += 1;
                    }
                }
            }
        }
    }

    // ---- starvation: nothing may be missing from the run ----
    for (label, _) in ROLES {
        if !seen_roles.contains(label) {
            failures.push(format!("no request was sent from the {label} role"));
        }
    }
    for op in OPS {
        if !authorized_ops.contains(op) {
            failures.push(format!("no authorized control exercised the {op} endpoint"));
        }
        if !hostile_ops.contains(op) {
            failures.push(format!("no hostile request exercised the {op} endpoint"));
        }
    }
    for identifier in FROZEN_IDENTIFIERS {
        if !identifiers_returned.contains(identifier) {
            failures.push(format!(
                "no authorized control returned the frozen identifier {identifier}"
            ));
        }
    }
    if authorized == 0 {
        failures.push("no authorized control ran".to_owned());
    }
    if hostile == 0 {
        failures.push("no hostile request ran".to_owned());
    }
    if hostile_response_bytes == 0 {
        failures.push(
            "no hostile response bytes were observed, so the zero counts prove nothing".to_owned(),
        );
    }

    println!(
        "TASK6120 SUMMARY clients={} distinct_keys={} authorized_controls={authorized} forbidden_requests={hostile} \
hostile_member_names={hostile_member_names} hostile_channel_thread_ids={hostile_channel_thread_ids} \
hostile_message_ids={hostile_message_ids} hostile_ciphertext_bytes={hostile_ciphertext_bytes} \
hostile_response_bytes={hostile_response_bytes} scoped_leaks={scoped_leaks} \
audit_allowed={audit_allowed} audit_denied={audit_denied} raised_rights={raised_rights} \
unchanged_hostile_rights={unchanged_hostile_rights} roles={} endpoints={} frozen_identifiers={} run_dir={}",
        ROLES.len(),
        distinct_keys.len(),
        seen_roles.len(),
        authorized_ops.len(),
        identifiers_returned
            .iter()
            .filter(|id| FROZEN_IDENTIFIERS.contains(&id.as_str()))
            .count(),
        run_dir.display()
    );

    if !failures.is_empty() {
        for failure in &failures {
            println!("TASK6120 FAIL {failure}");
        }
        println!("TASK6120 RESULT fail failures={}", failures.len());
        let _ = std::io::stdout().flush();
        std::process::exit(1);
    }
    println!(
        "TASK6120 RESULT pass build={build_tag} authorized={authorized} forbidden={hostile} \
hostile_member_names=0 hostile_channel_thread_ids=0 hostile_ciphertext_bytes=0 raised_rights=0"
    );
    let _ = std::io::stdout().flush();
}

fn ids_of(entry: &serde_json::Value, field: &str) -> Vec<String> {
    entry
        .get(field)
        .and_then(serde_json::Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
