//! TASK 4817 restart readback: a separate process that is handed nothing but
//! the destination device's directory and the relay port.
//!
//! It re-derives the device identity from the sealed on-disk seed, re-fetches
//! the sealed self-messages from the live relay, opens and authenticates each
//! one, and prints what it recovered. Nothing is carried over from the process
//! that ran the sync — this is what "the receiver decrypts each exact source
//! value after restart" has to mean if it is to mean anything.

use ipc::ordinary_sync::FieldWiseState;
use ipc::sealed_sync::{at_rest, open_sync_message};
use keystore::identity_from_entropy;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

fn http_get(port: u16, path: &str) -> Result<String, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|err| err.to_string())?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|err| err.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| err.to_string())?;
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .ok_or_else(|| "relay response had no body".to_owned())?;
    Ok(body)
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    (0..hex.len() / 2)
        .map(|ix| u8::from_str_radix(&hex[ix * 2..ix * 2 + 2], 16).ok())
        .collect()
}

fn main() {
    let mut dir = PathBuf::new();
    let mut relay_port: u16 = 0;
    let mut channel = String::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dir" => dir = PathBuf::from(args.next().unwrap_or_default()),
            "--relay-port" => relay_port = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--channel" => channel = args.next().unwrap_or_default(),
            other => {
                eprintln!("task-4817-readback: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    let device_key_bytes = std::fs::read(dir.join("device.key")).expect("device key file");
    let mut device_key = [0u8; 32];
    device_key.copy_from_slice(&device_key_bytes);

    let seed_json = at_rest::read_record(&dir, &device_key, "identity").expect("sealed identity");
    let seed: serde_json::Value = serde_json::from_slice(&seed_json).expect("identity json");
    let entropy_hex = seed["entropy_hex"].as_str().expect("entropy_hex");
    let user_id = seed["user_id"].as_str().expect("user_id").to_owned();
    let sender_ik_hex = seed["paired_sender_ik_hex"]
        .as_str()
        .expect("paired_sender_ik_hex");

    let mut entropy = [0u8; 16];
    entropy.copy_from_slice(&hex_to_bytes(entropy_hex).expect("entropy bytes"));
    let identity = identity_from_entropy(entropy, user_id);
    let mlkem_sk = identity.mlkem_decapsulation_key();

    let mut sender_ik = [0u8; 32];
    sender_ik.copy_from_slice(&hex_to_bytes(sender_ik_hex).expect("sender ik bytes"));
    let sender_ik = crypto::x25519::PublicKey::from_bytes(sender_ik);

    // 1. The merged state this device persisted before the restart, re-opened
    //    from disk under the device key.
    let state_json = at_rest::read_record(&dir, &device_key, "sync-state").expect("sealed state");
    let state: FieldWiseState = serde_json::from_slice(&state_json).expect("state json");
    println!("RESTART_PERSISTED_FIELDS={}", state.field_count());
    let field_names: Vec<String> = serde_json::from_slice::<serde_json::Value>(&state_json)
        .ok()
        .and_then(|value| {
            value
                .get("fields")
                .and_then(|fields| fields.as_object())
                .map(|fields| fields.keys().cloned().collect())
        })
        .unwrap_or_default();
    for name in field_names {
        if let Some(value) = state.get(&name).and_then(|value| value.as_str()) {
            println!("RESTART_PERSISTED field={name} value={value}");
        }
    }

    // 2. Decrypt again, after the restart, straight from the live relay.
    let body = http_get(
        relay_port,
        &format!("/relay/v1/channels/{channel}/messages?after=0"),
    )
    .expect("relay fetch");
    let messages: serde_json::Value = serde_json::from_str(&body).expect("relay json");
    let mut decrypted = 0usize;
    for message in messages.as_array().cloned().unwrap_or_default() {
        let wire = match message["content"].as_str() {
            Some(wire) => wire,
            None => continue,
        };
        let opened = match open_sync_message(wire, &identity.x25519_secret, &mlkem_sk, &sender_ik) {
            Ok(opened) => opened,
            // Ordinary user messages on the same conversation are not sync
            // bodies. They open, but they carry no sync payload.
            Err(_) => continue,
        };
        let payload = opened.payload();
        if let Some(fields) = payload.body.get("fields").and_then(|f| f.as_object()) {
            for (field, value) in fields {
                if let Some(value) = value.as_str() {
                    println!(
                        "RESTART_DECRYPTED kind={} field={field} value={value}",
                        opened.kind()
                    );
                    decrypted += 1;
                }
            }
        }
    }
    println!("RESTART_DECRYPTED_COUNT={decrypted}");
}
