use std::{collections::BTreeMap, env, fs, path::Path, process};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use task_3096_build_proof::{verify_signed_build_proof, SignedBuildProof};

const PROOF_FLAG: &str = "--proof-file";
const KEY_FLAG: &str = "--trusted-public-key-file";

fn main() {
    match run(env::args().skip(1)) {
        Ok(()) => println!("signature valid"),
        Err(error) => {
            eprintln!("osl-check-build-proof-signature: {error}");
            process::exit(if error == "bad-signature" { 1 } else { 2 });
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let values = parse_flags(args)?;
    let proof_path = values
        .get(PROOF_FLAG)
        .ok_or_else(|| format!("missing required proof file: {PROOF_FLAG}"))?;
    let public_key_path = values
        .get(KEY_FLAG)
        .ok_or_else(|| format!("missing required trusted public key file: {KEY_FLAG}"))?;

    let proof_json = fs::read(Path::new(proof_path))
        .map_err(|error| format!("read proof file {proof_path}: {error}"))?;
    let signed: SignedBuildProof = serde_json::from_slice(&proof_json)
        .map_err(|error| format!("read signed proof JSON: {error}"))?;
    let trusted_public_key = read_base64_key(public_key_path)?;
    verify_signed_build_proof(&signed, trusted_public_key)
}

fn parse_flags(args: impl IntoIterator<Item = String>) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        let (flag, inline_value) = match argument.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_owned())),
            None => (argument.as_str(), None),
        };
        if flag != PROOF_FLAG && flag != KEY_FLAG {
            return Err(format!("unknown argument: {argument}"));
        }
        if values.contains_key(flag) {
            return Err(format!("argument supplied more than once: {flag}"));
        }
        let value = inline_value
            .or_else(|| args.next())
            .ok_or_else(|| format!("missing value after {flag}"))?;
        values.insert(flag.to_owned(), value);
    }
    Ok(values)
}

fn read_base64_key(path: &str) -> Result<[u8; 32], String> {
    let encoded = fs::read_to_string(Path::new(path))
        .map_err(|error| format!("read trusted public key file {path}: {error}"))?;
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|_| "trusted public key file must contain base64".to_owned())?;
    bytes
        .try_into()
        .map_err(|_| "trusted public key must decode to exactly 32 bytes".to_owned())
}
