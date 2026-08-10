use std::{collections::BTreeMap, env, process};

use task_3096_build_proof::{make_build_proof, BuildProofInput};

const REQUIRED_FLAGS: [(&str, &str); 5] = [
    ("--build-fingerprint", "build fingerprint"),
    ("--device-id", "device ID"),
    ("--person-id", "person ID"),
    ("--made-at-unix-seconds", "made-at time"),
    ("--stops-counting-at-unix-seconds", "stops-counting-at time"),
];

fn main() {
    match run(env::args().skip(1)) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("osl-build-proof: {error}");
            process::exit(2);
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<String, String> {
    let mut values = BTreeMap::new();
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        let (flag, inline_value) = match argument.split_once('=') {
            Some((flag, value)) => (flag, Some(value.to_owned())),
            None => (argument.as_str(), None),
        };
        if !REQUIRED_FLAGS.iter().any(|(required, _)| *required == flag) {
            return Err(format!("unknown argument: {argument}"));
        }
        if values.contains_key(flag) {
            return Err(format!("argument supplied more than once: {flag}"));
        }
        let value = match inline_value {
            Some(value) => value,
            None => args
                .next()
                .ok_or_else(|| format!("missing value after {flag}"))?,
        };
        values.insert(flag.to_owned(), value);
    }

    for (flag, label) in REQUIRED_FLAGS {
        if !values.contains_key(flag) {
            return Err(format!("missing required {label}: {flag}"));
        }
    }

    let take = |values: &mut BTreeMap<String, String>, flag: &str| {
        values
            .remove(flag)
            .expect("required flags were checked before construction")
    };
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
    serde_json::to_string_pretty(&proof).map_err(|error| format!("encode proof: {error}"))
}

fn parse_unix_seconds(label: &str, value: String) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("{label} must be an unsigned integer number of Unix seconds"))
}
