use std::{
    collections::BTreeMap,
    env, fs,
    path::Path,
    process,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use task_3096_build_proof::check_build_proof_file;

const PROOF_FLAG: &str = "--proof-file";
const KEY_FLAG: &str = "--trusted-public-key-file";
const FINGERPRINT_FLAG: &str = "--build-fingerprint";
const TIME_FLAG: &str = "--at-unix-seconds";

fn main() {
    match run(env::args().skip(1)) {
        Ok(answer) => println!("{answer}"),
        Err(error) => {
            eprintln!("osl-check-build-proof: {error}");
            process::exit(2);
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<String, String> {
    let values = parse_flags(args)?;
    let fingerprint = values
        .get(FINGERPRINT_FLAG)
        .ok_or_else(|| format!("missing required build fingerprint: {FINGERPRINT_FLAG}"))?;
    let checked_at = match values.get(TIME_FLAG) {
        Some(value) => value
            .parse::<u64>()
            .map_err(|_| format!("{TIME_FLAG} must be an unsigned integer"))?,
        None => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is before the Unix epoch".to_owned())?
            .as_secs(),
    };
    let proof_path = values.get(PROOF_FLAG).map(Path::new);
    let trusted_public_key = values
        .get(KEY_FLAG)
        .and_then(|path| read_base64_key(path).ok());

    Ok(check_build_proof_file(proof_path, trusted_public_key, fingerprint, checked_at).to_string())
}

fn parse_flags(args: impl IntoIterator<Item = String>) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        let (flag, inline_value) = match argument.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_owned())),
            None => (argument.as_str(), None),
        };
        if ![PROOF_FLAG, KEY_FLAG, FINGERPRINT_FLAG, TIME_FLAG].contains(&flag) {
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
    let encoded = fs::read_to_string(Path::new(path)).map_err(|error| error.to_string())?;
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|error| error.to_string())?;
    bytes
        .try_into()
        .map_err(|_| "trusted public key must decode to exactly 32 bytes".to_owned())
}
