use std::{collections::BTreeMap, env, fs, path::Path, process};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use task_3096_build_proof::{
    make_build_proof, sign_build_proof, BuildProofInput, SignedBuildProof,
};

const VALUE_FLAGS: [(&str, &str); 5] = [
    ("--build-fingerprint", "build fingerprint"),
    ("--device-id", "device ID"),
    ("--person-id", "person ID"),
    ("--made-at-unix-seconds", "made-at time"),
    ("--stops-counting-at-unix-seconds", "stops-counting-at time"),
];
const KEY_FLAG: &str = "--signing-key-file";

fn main() {
    match run(env::args().skip(1)) {
        Ok(proof) => println!("{proof}"),
        Err(error) => {
            eprintln!("osl-sign-build-proof: {error}");
            process::exit(2);
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<String, String> {
    let mut values = parse_flags(args)?;
    for (flag, label) in VALUE_FLAGS {
        require_flag(&values, flag, label)?;
    }
    require_flag(&values, KEY_FLAG, "signing key file")?;

    let proof = make_build_proof(BuildProofInput {
        build_fingerprint: take(&mut values, "--build-fingerprint"),
        device_id: take(&mut values, "--device-id"),
        person_id: take(&mut values, "--person-id"),
        made_at_unix_seconds: parse_unix_seconds(
            "made-at time",
            take(&mut values, "--made-at-unix-seconds"),
        )?,
        stops_counting_at_unix_seconds: parse_unix_seconds(
            "stops-counting-at time",
            take(&mut values, "--stops-counting-at-unix-seconds"),
        )?,
    })?;
    let signing_seed = read_base64_key(&take(&mut values, KEY_FLAG), "signing key")?;
    let signed: SignedBuildProof = sign_build_proof(proof, signing_seed)?;
    serde_json::to_string_pretty(&signed).map_err(|error| format!("encode signed proof: {error}"))
}

fn parse_flags(args: impl IntoIterator<Item = String>) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        let (flag, inline_value) = match argument.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_owned())),
            None => (argument.as_str(), None),
        };
        let known = flag == KEY_FLAG || VALUE_FLAGS.iter().any(|(required, _)| *required == flag);
        if !known {
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

fn require_flag(values: &BTreeMap<String, String>, flag: &str, label: &str) -> Result<(), String> {
    if values.contains_key(flag) {
        Ok(())
    } else {
        Err(format!("missing required {label}: {flag}"))
    }
}

fn take(values: &mut BTreeMap<String, String>, flag: &str) -> String {
    values
        .remove(flag)
        .expect("required flags were checked before construction")
}

fn parse_unix_seconds(label: &str, value: String) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{label} must be an unsigned integer number of Unix seconds"))
}

fn read_base64_key(path: &str, label: &str) -> Result<[u8; 32], String> {
    let encoded = fs::read_to_string(Path::new(path))
        .map_err(|error| format!("read {label} file {path}: {error}"))?;
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|_| format!("{label} file must contain base64"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{label} must decode to exactly 32 bytes"))
}
