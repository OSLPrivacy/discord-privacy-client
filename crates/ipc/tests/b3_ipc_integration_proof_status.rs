use ipc::wire_rn::RN_WIRE_IN_ENABLED;

const RATCHET_REPORT: &str = include_str!("../../../docs/reports/ratchet-lane-2026-07-26.md");
const CHECKLIST: &str = include_str!("../../../docs/design/osl-internal-build-checklist.md");

fn require_contains(label: &str, haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "{label} is missing required evidence text: {needle}"
    );
}

#[test]
fn b3_ipc_integration_proof_status() {
    assert!(
        !RN_WIRE_IN_ENABLED,
        "B3 proof documentation must not enable the RN production fuse"
    );

    require_contains(
        "ratchet report",
        RATCHET_REPORT,
        "## B3 IPC-integration proof status against checklist evidence row",
    );
    require_contains(
        "ratchet report",
        RATCHET_REPORT,
        "dependency closure `1d8bfa8` proves the later IPC integration boundary",
    );
    require_contains(
        "ratchet report",
        RATCHET_REPORT,
        "The historical archive's IPC integration gate is blocked",
    );
    require_contains(
        "ratchet report",
        RATCHET_REPORT,
        "remains `test-proven-only` on an `implemented-unwired` path",
    );
    require_contains(
        "ratchet report",
        RATCHET_REPORT,
        "no real traffic, two-identity proof, runtime proof, or B6 point implied",
    );
    require_contains(
        "ratchet report",
        RATCHET_REPORT,
        "the production fuse remains `RN_WIRE_IN_ENABLED = false`",
    );

    require_contains(
        "checklist",
        CHECKLIST,
        "exact ratchet and later dependency-closure archives prove sealed persistence",
    );
    require_contains(
        "checklist",
        CHECKLIST,
        "That historical archive's IPC integration gate is blocked by missing later IPC/keystore APIs",
    );
    require_contains(
        "checklist",
        CHECKLIST,
        "Exact dependency closure `1d8bfa8` separately passes 35",
    );
    require_contains(
        "checklist",
        CHECKLIST,
        "Status is `test-proven-only` on an `implemented-unwired` ratchet",
    );
}
