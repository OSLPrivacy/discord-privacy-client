use release_trust::{ReleaseConsumer, SequentialTrustClient};
use std::collections::BTreeSet;
use std::path::PathBuf;

const DEFAULT_NOW: u64 = 1_786_435_200; // 2026-08-11T00:00:00Z

fn committed_trust_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../release-trust")
}

fn requested_starvation() -> Result<Option<ReleaseConsumer>, String> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => Ok(None),
        [flag, name] if flag == "--starve" => ReleaseConsumer::ALL
            .into_iter()
            .find(|consumer| consumer.name() == name)
            .map(Some)
            .ok_or_else(|| format!("unknown consuming path: {name}")),
        _ => Err("usage: task-5170b [--starve <consumer-name>]".to_owned()),
    }
}

fn run() -> Result<(), String> {
    let starved = requested_starvation()?;
    let trust_dir = committed_trust_dir();
    let mut client =
        SequentialTrustClient::bootstrap(trust_dir.join("metadata/root.json"), DEFAULT_NOW)
            .map_err(|error| format!("could not bootstrap committed release root: {error}"))?;
    let mut invoked = BTreeSet::new();

    for consumer in ReleaseConsumer::ALL {
        if starved == Some(consumer) {
            continue;
        }

        // Keep these as concrete calls: this gate proves that every required
        // consuming path enters SequentialTrustClient rather than merely
        // exercising the generic refresh method or trusting an inventory.
        match consumer {
            ReleaseConsumer::WindowsUpdate => client.load_windows_update(&trust_dir, DEFAULT_NOW),
            ReleaseConsumer::UnmodifiedBuildProof => {
                client.load_unmodified_build_proof(&trust_dir, DEFAULT_NOW)
            }
            ReleaseConsumer::CarrierTable => client.load_carrier_table(&trust_dir, DEFAULT_NOW),
        }
        .map_err(|error| format!("{} trust evaluation failed: {error}", consumer.name()))?;
        invoked.insert(consumer);
    }

    for required in ReleaseConsumer::ALL {
        if !invoked.contains(&required) {
            return Err(format!(
                "TASK5170b missing consuming path: {}",
                required.name()
            ));
        }
    }

    println!(
        "TASK5170b consuming paths: {}",
        ReleaseConsumer::ALL
            .into_iter()
            .map(ReleaseConsumer::name)
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
