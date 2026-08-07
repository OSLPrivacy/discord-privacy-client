use std::process::ExitCode;

use ipc::mutual_discovery::{
    fixed_discovery_deck, mutual_discovery_label, one_sided_discovery_label,
    publish_discovery_card, scan_mutual_discovery_cards, ONE_SIDED_DISCOVERY_LABEL,
};

const ALICE: &str = "alice@osl.test";
const BOB: &str = "bob@osl.test";

fn main() -> ExitCode {
    let one_sided = std::env::args().any(|arg| arg == "--one-sided");
    let result = if one_sided {
        run_one_sided_check()
    } else {
        run_mutual_check()
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            println!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run_mutual_check() -> Result<(), String> {
    let label_ab = mutual_discovery_label(ALICE, BOB)?;
    let label_ba = mutual_discovery_label(BOB, ALICE)?;
    if label_ab != label_ba || !label_ab.contains(ALICE) || !label_ab.contains(BOB) {
        return Err("mutual discovery label check failed".to_owned());
    }
    let deck = fixed_discovery_deck(vec![
        publish_discovery_card(ALICE, BOB)?,
        publish_discovery_card(BOB, ALICE)?,
    ])?;
    let alice = scan_mutual_discovery_cards(ALICE, BOB, &deck)?;
    let bob = scan_mutual_discovery_cards(BOB, ALICE, &deck)?;
    if alice.status() != "matched"
        || bob.status() != "matched"
        || alice.matching_cards.len() != 1
        || bob.matching_cards.len() != 1
    {
        return Err("mutual discovery card check failed".to_owned());
    }
    println!("TASK4754 named_check=mutual label={label_ab}");
    Ok(())
}

fn run_one_sided_check() -> Result<(), String> {
    let pair_label = mutual_discovery_label(ALICE, BOB)?;
    let alice_only = one_sided_discovery_label(ALICE)?;
    let bob_only = one_sided_discovery_label(BOB)?;
    if alice_only != pair_label || bob_only != pair_label {
        return Err(ONE_SIDED_DISCOVERY_LABEL.to_owned());
    }
    Ok(())
}
