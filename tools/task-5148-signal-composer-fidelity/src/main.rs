use std::process::ExitCode;
use task_5148_signal_composer_fidelity::{
    known_good_contract_receipt, known_good_shipping_receipt, validate_fidelity,
    validate_mutant_inventory, validate_shipping_exclusion, REQUIRED_MUTANTS,
};

fn main() -> ExitCode {
    let scenario = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "preflight".into());
    let mut receipt = known_good_contract_receipt();

    let result = match scenario.as_str() {
        "contract-only" => validate_fidelity(&receipt),
        "shipping-contract-only" => validate_shipping_exclusion(&known_good_shipping_receipt()),
        "starved-live" => {
            receipt.live_candidate = false;
            validate_fidelity(&receipt)
        }
        "resolver-fixture" => {
            receipt.resolver = "task_1031_open_direct_message_fixture";
            validate_fidelity(&receipt)
        }
        "fixed-font" => {
            receipt.font_from_live_sample = false;
            validate_fidelity(&receipt)
        }
        "boundary-shift" => {
            receipt.boundary_displacement_px = 1;
            validate_fidelity(&receipt)
        }
        "missing-mutant" => validate_mutant_inventory(&REQUIRED_MUTANTS[..2]),
        "red-inventory" => validate_mutant_inventory(&REQUIRED_MUTANTS),
        "preflight" => Err(
            "TASK5148_BLOCKED missing gates 5103,5114,5115,5134,5135,5158; no real Signal candidate/reference captures"
                .into(),
        ),
        other => Err(format!("unknown scenario: {other}")),
    };

    match result {
        Ok(()) => {
            println!("TASK5148_{scenario}=PASS");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("TASK5148_{scenario}=FAIL: {error}");
            ExitCode::FAILURE
        }
    }
}
