use std::env;
use std::path::Path;
use std::process::ExitCode;
use task_5131_messenger_contract::{check, Attacks, CONTRACT_JSON, INVENTORY_IDS};

fn main() -> ExitCode {
    let repo_root = env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let attacks = Attacks {
        empty_contract: env::var_os("TASK5131_EMPTY_CONTRACT").is_some(),
        starve_contract: env::var("TASK5131_STARVE_CONTRACT").ok(),
        starve_inventory: env::var("TASK5131_STARVE_INVENTORY").ok(),
        promote: env::var("TASK5131_PROMOTE").ok(),
    };
    match check(Path::new(&repo_root), CONTRACT_JSON, &attacks) {
        Ok(summary) => {
            println!(
                "TASK5131_CONTRACT_OK composer_keys={} geometry_keys={} type_keys={} style_keys={} status=test-development-contract-only shipping_eligible=false",
                summary.composer_keys,
                summary.geometry_keys,
                summary.type_keys,
                summary.style_keys
            );
            for id in INVENTORY_IDS {
                println!("TASK5131_INVENTORY {id}={}", summary.inventory_counts[id]);
            }
            println!(
                "TASK5131_SHIPPING live_providers=0 imports=0 painters=0 actions=0 release_claims=0 installer_files=0"
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}
