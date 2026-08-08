//! Direct operator command for task 0078.
//!
//! Examples:
//! `cargo run -p stego --example task_0078_layers -- --nine`
//! `cargo run -p stego --example task_0078_layers -- --sweep`
//! `cargo run -p stego --example task_0078_layers -- --pointer high --capitalisation low --vocabulary high --spelling off`

use std::collections::HashMap;
use std::process::ExitCode;

use stego::{
    decode_layered_cover, encode_layered_cover, CoverLayerSettings, LayerStrength,
    LayeredCoverInput, SHRUNK_TOKEN_ID_BYTES, TOKEN_ID_BYTES,
};

const KEY: &[u8] = b"task-0078-paired-shared-key";
const SEED: [u8; TOKEN_ID_BYTES] = *b"task0078-seed-000001";
const HANDLE: [u8; SHRUNK_TOKEN_ID_BYTES] = *b"msg0078a";

fn message() -> String {
    let mut message = String::new();
    while message.chars().count() < 200 {
        message.push_str(
            "private task 0078 message: the original stays in the paired store while every cover layer can be reviewed at a gentler setting. ",
        );
    }
    message.chars().take(200).collect()
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.as_slice() == ["--nine"] {
        // Nine deliberately spaced settings make the acceptance evidence
        // compact while covering off, low, and high for every layer.
        let settings = [
            CoverLayerSettings::new(
                LayerStrength::Off,
                LayerStrength::Off,
                LayerStrength::Off,
                LayerStrength::Off,
            ),
            CoverLayerSettings::new(
                LayerStrength::Off,
                LayerStrength::Low,
                LayerStrength::Off,
                LayerStrength::Off,
            ),
            CoverLayerSettings::new(
                LayerStrength::Off,
                LayerStrength::High,
                LayerStrength::Off,
                LayerStrength::Off,
            ),
            CoverLayerSettings::new(
                LayerStrength::Off,
                LayerStrength::Off,
                LayerStrength::Low,
                LayerStrength::Low,
            ),
            CoverLayerSettings::new(
                LayerStrength::Off,
                LayerStrength::Off,
                LayerStrength::High,
                LayerStrength::Off,
            ),
            CoverLayerSettings::new(
                LayerStrength::Off,
                LayerStrength::Off,
                LayerStrength::High,
                LayerStrength::High,
            ),
            CoverLayerSettings::new(
                LayerStrength::Low,
                LayerStrength::Low,
                LayerStrength::High,
                LayerStrength::High,
            ),
            CoverLayerSettings::new(
                LayerStrength::Low,
                LayerStrength::High,
                LayerStrength::High,
                LayerStrength::High,
            ),
            CoverLayerSettings::new(
                LayerStrength::High,
                LayerStrength::High,
                LayerStrength::High,
                LayerStrength::High,
            ),
        ];
        let mut counts = std::collections::HashSet::new();
        for settings in settings {
            counts.insert(report(settings)?);
        }
        let mut distinct: Vec<usize> = counts.into_iter().collect();
        distinct.sort_unstable();
        if distinct.len() != 9 {
            return Err(format!(
                "expected nine distinct task-0078 word counts, got {distinct:?}"
            ));
        }
        println!("TASK0078 settings_checked=9 distinct_word_counts={distinct:?}");
        return Ok(());
    }
    if arguments.as_slice() == ["--sweep"] {
        let mut counts = std::collections::HashSet::new();
        for pointer in LayerStrength::ALL {
            for capitalisation in LayerStrength::ALL {
                for vocabulary in LayerStrength::ALL {
                    for spelling in LayerStrength::ALL {
                        counts.insert(report(CoverLayerSettings::new(
                            pointer,
                            capitalisation,
                            vocabulary,
                            spelling,
                        ))?);
                    }
                }
            }
        }
        let mut distinct: Vec<usize> = counts.into_iter().collect();
        distinct.sort_unstable();
        println!("TASK0078 settings_checked=81 distinct_word_counts={distinct:?}");
        return Ok(());
    }

    let settings = parse_settings(&arguments)?;
    report(settings)?;
    Ok(())
}

fn parse_settings(arguments: &[String]) -> Result<CoverLayerSettings, String> {
    let mut settings = CoverLayerSettings::default();
    let mut cursor = 0usize;
    while cursor < arguments.len() {
        let flag = arguments.get(cursor).ok_or("missing setting flag")?;
        let value = arguments
            .get(cursor + 1)
            .ok_or_else(|| format!("missing value after {flag}"))?;
        let strength = LayerStrength::parse(value)?;
        match flag.as_str() {
            "--pointer" => settings.pointer = strength,
            "--capitalisation" => settings.capitalisation = strength,
            "--vocabulary" => settings.vocabulary = strength,
            "--spelling" => settings.spelling = strength,
            "--help" => return Err("use --nine, --sweep, or set --pointer/--capitalisation/--vocabulary/--spelling to off, low, or high".into()),
            other => return Err(format!("unknown option {other}")),
        }
        cursor += 2;
    }
    Ok(settings)
}

fn report(settings: CoverLayerSettings) -> Result<usize, String> {
    let original = message();
    let input = match settings.pointer {
        LayerStrength::High => LayeredCoverInput::SharedHandle(HANDLE),
        LayerStrength::Off | LayerStrength::Low => LayeredCoverInput::Seed(SEED),
    };
    let cover = encode_layered_cover(KEY, input, settings).map_err(|error| error.to_string())?;
    let decoded = decode_layered_cover(KEY, settings, &cover).ok_or("cover did not read back")?;
    let mut seeds = HashMap::new();
    seeds.insert(SEED, original.clone());
    let mut handles = HashMap::new();
    handles.insert(HANDLE, original.clone());
    let recovered = match decoded {
        LayeredCoverInput::Seed(seed) => seeds.get(&seed),
        LayeredCoverInput::SharedHandle(handle) => handles.get(&handle),
    }
    .ok_or("decoded pointer was not in the paired store")?;
    let words = cover.split_ascii_whitespace().count();
    println!(
        "TASK0078 pointer={} capitalisation={} vocabulary={} spelling={} word_count={} recovered_exact={} original={}",
        settings.pointer.label(), settings.capitalisation.label(), settings.vocabulary.label(), settings.spelling.label(), words, recovered == &original, original
    );
    Ok(words)
}
