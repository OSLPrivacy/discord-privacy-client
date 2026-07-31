//! Unit a9 (checklist A1): does the recovery-phrase display path always
//! apply screen-capture protection *before* recovery-secret pixels can
//! exist -- confirmed, not merely requested?
//!
//! The gate under test (`RecoveryCaptureGate`) and every place that can
//! paint or copy the recovery phrase (`recoveryContent()`, the
//! `#copy-recovery-kit` click handler) live entirely in the `osl-hub-ui`
//! TypeScript frontend (`apps/osl-hub-ui/src/main.ts`,
//! `apps/osl-hub-ui/src/ui-behavior.ts`). There is no Rust entry point for
//! any of this and no JS runtime in this crate, so these tests cannot
//! execute the frontend. What they *can* do, and do, is parse the real,
//! live source on disk and assert the structural invariant a TOCTOU fix
//! depends on: the accepted capture-proof latch must appear, and still be
//! checked, before every place that reads the recovery phrase out of
//! memory. A future edit that removes or reorders that check breaks these
//! assertions -- see `the_gate_check_detects_removal_and_reordering` for
//! proof the check is not vacuous.
//!
//! A second, independent question is whether the "confirmation" the latch
//! is built on is a real OS-level readback or just a request whose return
//! value is trusted. `main_window_capture_protection_is_confirmed_by_os_readback_not_just_requested`
//! checks that against `crates/runtime/src/screenshot.rs` and is written to
//! fail today -- see its doc comment for why that failure is real, not a
//! test bug.

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // This crate lives at <repo>/apps/osl-hub, two levels below the root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("apps/ dir")
        .parent()
        .expect("repo root")
        .to_owned()
}

fn read_source(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Slices `source` from the first byte of `start_needle` up to (but not
/// including) the next occurrence of `end_needle` that begins at or after
/// it. Panics with a clear message if either anchor is missing, so a
/// rename or deletion of the guarded function fails loudly instead of the
/// test vacuously passing on an empty/wrong slice.
fn slice_between<'a>(source: &'a str, start_needle: &str, end_needle: &str) -> &'a str {
    let start = source.find(start_needle).unwrap_or_else(|| {
        panic!("start anchor not found (source moved or renamed): {start_needle:?}")
    });
    let after_start = start + start_needle.len();
    let end_offset = source[after_start..]
        .find(end_needle)
        .unwrap_or_else(|| panic!("end anchor not found after start anchor: {end_needle:?}"));
    &source[start..after_start + end_offset]
}

/// The real assertion: within `block`, `gate_needle` must be present, and
/// must appear before every one of `secret_needles`. Returns `Err` instead
/// of asserting directly so the same logic can be run against both the
/// real source (expected `Ok`) and synthetic mutated fixtures (expected
/// `Err`) -- proving this helper, and the tests built on it, can actually
/// fail.
fn gate_precedes_every_secret_use(
    block: &str,
    gate_needle: &str,
    secret_needles: &[&str],
) -> Result<(), String> {
    let gate_at = block
        .find(gate_needle)
        .ok_or_else(|| format!("capture-proof gate {gate_needle:?} is missing from the block"))?;
    for secret in secret_needles {
        match block.find(secret) {
            None => {
                return Err(format!(
                    "secret needle {secret:?} not found in the block (nothing left to protect -- update the test)"
                ))
            }
            Some(secret_at) if secret_at < gate_at => {
                return Err(format!(
                    "{secret:?} is used at byte {secret_at}, before the capture-proof gate {gate_needle:?} at byte {gate_at}"
                ));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

const MAIN_TS: &str = "apps/osl-hub-ui/src/main.ts";
const UI_BEHAVIOR_TS: &str = "apps/osl-hub-ui/src/ui-behavior.ts";
const SCREENSHOT_RS: &str = "crates/runtime/src/screenshot.rs";

const CAPTURE_GATE: &str = "recoveryCaptureGate.canRender()";
const IDENTITY_PHRASE: &str = "recoveryBundle.identityPhrase";
const PASSWORD_PHRASE: &str = "recoveryBundle.passwordPhrase";

/// `recoveryContent()` renders the onboarding "recovery" route. It must
/// refuse to interpolate `recoveryBundle.identityPhrase` /
/// `.passwordPhrase` into the returned HTML string -- the string that
/// becomes painted pixels once assigned to `innerHTML` -- unless
/// `recoveryCaptureGate.canRender()` (the accepted capture-proof latch) is
/// already true.
#[test]
fn recovery_phrase_render_is_gated_behind_the_capture_proof_latch() {
    let source = read_source(MAIN_TS);
    let block = slice_between(&source, "function recoveryContent(): string {", "\n}\n");
    gate_precedes_every_secret_use(block, CAPTURE_GATE, &[IDENTITY_PHRASE, PASSWORD_PHRASE])
        .expect("recoveryContent() must gate the recovery phrase behind the capture-proof latch");
}

/// The "Copy recovery kit" clipboard handler is a second path to the same
/// secret and must be gated the same way: clipboard contents are just as
/// exposed (clipboard history, sync, other apps polling the clipboard) as
/// painted pixels once the phrase leaves memory, so this handler must not
/// read `recoveryBundle.identityPhrase` / `.passwordPhrase` unless the
/// latch is accepted either.
#[test]
fn copy_recovery_kit_handler_is_gated_behind_the_capture_proof_latch() {
    let source = read_source(MAIN_TS);
    let block = slice_between(
        &source,
        r##"document.querySelector<HTMLButtonElement>("#copy-recovery-kit")?.addEventListener("click", async () => {"##,
        "\n  });\n",
    );
    gate_precedes_every_secret_use(block, CAPTURE_GATE, &[IDENTITY_PHRASE, PASSWORD_PHRASE])
        .expect(
        "copy-recovery-kit handler must gate the recovery phrase behind the capture-proof latch",
    );
}

/// Proves `gate_precedes_every_secret_use` is not vacuous: it must reject a
/// block where the gate has been deleted, and a block where the gate has
/// been reordered to after the secret use. This is the "if the confirmation
/// is removed or reordered, the test must fail" requirement made concrete
/// and self-verifying, independent of whether anyone actually breaks the
/// real files.
#[test]
fn the_gate_check_detects_removal_and_reordering() {
    let correctly_ordered = format!("if (!{CAPTURE_GATE}) return;\nconst x = {IDENTITY_PHRASE};");
    assert!(
        gate_precedes_every_secret_use(&correctly_ordered, CAPTURE_GATE, &[IDENTITY_PHRASE])
            .is_ok(),
        "a correctly ordered block must pass"
    );

    let gate_removed = format!("const x = {IDENTITY_PHRASE};");
    assert!(
        gate_precedes_every_secret_use(&gate_removed, CAPTURE_GATE, &[IDENTITY_PHRASE]).is_err(),
        "removing the gate must fail the check"
    );

    let gate_reordered = format!("const x = {IDENTITY_PHRASE};\nif (!{CAPTURE_GATE}) return;");
    assert!(
        gate_precedes_every_secret_use(&gate_reordered, CAPTURE_GATE, &[IDENTITY_PHRASE]).is_err(),
        "reordering the gate to after the secret use must fail the check"
    );
}

/// `RecoveryCaptureGate.accept` is what makes the latch race-safe: a
/// concurrent `invalidate()` (e.g. the window losing focus while
/// `proveRecoveryCaptureProtection()`'s `setScreenshotProtection` IPC call
/// is still in flight) must stamp a fresh generation so a late-arriving
/// `accept(checkpoint)` from the now-stale proof attempt cannot retroactively
/// satisfy `canRender()`. Both halves of that contract are checked directly
/// against the live class body.
#[test]
fn capture_proof_gate_rejects_a_stale_checkpoint_after_concurrent_invalidation() {
    let source = read_source(UI_BEHAVIOR_TS);
    let block = slice_between(&source, "export class RecoveryCaptureGate {", "\n}\n");
    assert!(
        block.contains("checkpoint !== this.generation"),
        "accept() must reject a checkpoint from a generation a concurrent invalidate() has since bumped past; class body was:\n{block}"
    );
    assert!(
        block.contains("this.provenGeneration === this.generation"),
        "canRender() must compare the proven generation against the *live* generation, not a cached boolean that concurrent invalidation can't clear; class body was:\n{block}"
    );
}

/// EXPECTED TO FAIL -- this documents a real gap, not a test bug.
///
/// `set_hub_screenshot_protection` (the Tauri command
/// `proveRecoveryCaptureProtection()` awaits) calls
/// `screenshot::apply_to_window`, which calls
/// `runtime::apply_to_hwnd_and_children`, which bottoms out in the Windows
/// `apply()` function sliced below. That function calls
/// `SetWindowDisplayAffinity` and reports success purely from that call's
/// own return value -- it never calls `GetWindowDisplayAffinity` to read
/// the affinity back and confirm the OS actually holds
/// `WDA_EXCLUDEFROMCAPTURE` for this HWND before the frontend's
/// `proveRecoveryCaptureProtection()` accepts the proof and lets
/// `recoveryContent()` paint. Per this crate's own doc comment on
/// `ScreenshotProtection`, Windows builds older than 10/2004 *silently
/// downgrade* `WDA_EXCLUDEFROMCAPTURE` to `WDA_MONITOR` -- a `Set` call
/// that still returns success -- so a plain "did Set error?" check cannot
/// distinguish the requested protection from a downgraded one.
///
/// This codebase already knows how to do better: `native_discord_overlay.rs`,
/// `native_image_viewer.rs`, and `native_window_host.rs` all call
/// `GetWindowDisplayAffinity` to confirm capture exclusion on other
/// protected surfaces before trusting it. The main window -- the one the
/// recovery phrase renders into -- does not get the same treatment. So the
/// whole `proveRecoveryCaptureProtection()` chain the recovery UI trusts is,
/// at its root, "requested and the API did not error", not "confirmed by
/// readback". A later unit should add the missing `GetWindowDisplayAffinity`
/// readback to `apply()` and flip this assertion to a regression guard.
#[test]
fn main_window_capture_protection_is_confirmed_by_os_readback_not_just_requested() {
    let source = read_source(SCREENSHOT_RS);
    let block = slice_between(
        &source,
        "pub(super) fn apply(hwnd_isize: isize, protection: ScreenshotProtection) -> Result<()> {",
        "\n    }\n",
    );
    assert!(
        block.contains("GetWindowDisplayAffinity"),
        "apply() sets display affinity but never reads it back with GetWindowDisplayAffinity \
         to confirm the OS actually holds WDA_EXCLUDEFROMCAPTURE before the caller reports \
         success. Other protected surfaces in this codebase (native_discord_overlay.rs, \
         native_image_viewer.rs, native_window_host.rs) do call GetWindowDisplayAffinity to \
         confirm; this path -- the one gating the recovery phrase -- does not. See unit a9's \
         report for the full chain. function body was:\n{block}"
    );
}
