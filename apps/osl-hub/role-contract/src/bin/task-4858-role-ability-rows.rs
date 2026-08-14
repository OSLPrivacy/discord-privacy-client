//! TASK 4858: write the resolved ability rows the enclave role screen draws.
//!
//! The screen never re-decides anything. This binary runs the one TASK 4860
//! resolver over the whole catalogue for the fixture role and writes the
//! answers — including the plain reason on every denied row — to the JSON the
//! renderer and the UI check both read.

#[allow(dead_code)]
#[path = "../../../src/osl_enclave_roles.rs"]
mod osl_enclave_roles;

#[allow(dead_code)]
#[path = "../../../src/osl_enclave_role_ability.rs"]
mod osl_enclave_role_ability;

#[allow(dead_code)]
#[path = "../../fixtures/task_4858_role_ability_fixture.rs"]
mod fixture;

use std::path::PathBuf;

use fixture::fixture_ability_view;
use osl_enclave_role_ability::ENCLAVE_ROLE_ABILITY_REASONS;

fn main() {
    let view = fixture_ability_view();

    let missing = view.denied_rows_without_reason();
    if !missing.is_empty() {
        eprintln!(
            "TASK4858_ROWS_FAIL denied permission has no reason: {}",
            missing.join(",")
        );
        std::process::exit(1);
    }

    let out_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_out_path);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).expect("the ability row directory can be created");
    }
    std::fs::write(&out_path, view.to_json()).expect("the ability rows can be written");

    println!(
        "TASK4858_ROWS role={} channel=#{} rows={} allowed={} denied={} path={}",
        view.role_name,
        view.channel_name,
        view.row_count(),
        view.allowed_count(),
        view.denied_count(),
        out_path.display()
    );
    let counts = view.denied_reason_counts();
    for reason in ENCLAVE_ROLE_ABILITY_REASONS {
        println!(
            "TASK4858_ROWS_REASON reason=\"{reason}\" denied_rows={}",
            counts.get(reason).copied().unwrap_or_default()
        );
    }
}

fn default_out_path() -> PathBuf {
    // role-contract/src/bin/ -> role-contract/ -> osl-hub/ -> apps/
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../osl-hub-ui/src/fixtures/task-4858-role-ability-rows.json")
}
