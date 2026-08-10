//! Test-only model of the pre-schema-1 build check used by TASK 3219.
//!
//! This deliberately weak old build accepts a claimed build after comparing
//! only its public app name and version. It then emits the schema-0 proof that
//! a current peer must classify as an old-build uncertainty, never as clean.

use std::{collections::BTreeMap, env, fs, path::Path, process};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use task_3096_build_proof::{
    make_build_proof, sign_build_proof, BuildProofInput, SignedBuildProof,
};

const EXPECTED_APP_NAME: &str = "OSL Privacy";
const EXPECTED_APP_VERSION: &str = "0.9.0";
const OLD_SCHEMA_VERSION: u8 = 0;
const FLAGS: [&str; 9] = [
    "--app-name",
    "--app-version",
    "--actual-build-fingerprint",
    "--build-fingerprint",
    "--device-id",
    "--person-id",
    "--made-at-unix-seconds",
    "--stops-counting-at-unix-seconds",
    "--signing-key-file",
];

fn main() {
    match run(env::args().skip(1)) {
        Ok(proof) => println!("{proof}"),
        Err(error) => {
            eprintln!("task-3219-old-weak-build: {error}");
            process::exit(2);
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<String, String> {
    let mut values = parse_flags(args)?;
    for flag in FLAGS {
        if !values.contains_key(flag) {
            return Err(format!("missing required value: {flag}"));
        }
    }

    // This is the intentional weakness under attack: matching two public
    // labels is the old build's entire acceptance decision. In particular it
    // does not authenticate the claimed fingerprint against executable bytes.
    let app_name = take(&mut values, "--app-name");
    let app_version = take(&mut values, "--app-version");
    let _actual_build_fingerprint = take(&mut values, "--actual-build-fingerprint");
    if app_name != EXPECTED_APP_NAME || app_version != EXPECTED_APP_VERSION {
        return Err("old name/version check refused the claimed build".to_owned());
    }

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
    let signing_seed = read_base64_key(&take(&mut values, "--signing-key-file"))?;
    let mut signed: SignedBuildProof = sign_build_proof(proof, signing_seed)?;
    signed.schema_version = OLD_SCHEMA_VERSION;
    serde_json::to_string_pretty(&signed).map_err(|error| format!("encode old proof: {error}"))
}

fn parse_flags(args: impl IntoIterator<Item = String>) -> Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        let (flag, inline_value) = match argument.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_owned())),
            None => (argument.as_str(), None),
        };
        if !FLAGS.contains(&flag) {
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

fn read_base64_key(path: &str) -> Result<[u8; 32], String> {
    let encoded = fs::read_to_string(Path::new(path))
        .map_err(|error| format!("read signing key file {path}: {error}"))?;
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|_| "signing key file must contain base64".to_owned())?;
    bytes
        .try_into()
        .map_err(|_| "signing key must decode to exactly 32 bytes".to_owned())
}
