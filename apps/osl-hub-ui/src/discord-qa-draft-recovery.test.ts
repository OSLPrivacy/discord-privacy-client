import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const adapter = readFileSync(
  new URL("../../osl-hub/src/native_discord_adapter.rs", import.meta.url),
  "utf8",
);
const nativeOverlay = readFileSync(
  new URL("../../osl-hub/src/native_discord_overlay.rs", import.meta.url),
  "utf8",
);
const nativeHost = readFileSync(
  new URL("../../osl-hub/src/native_window_host.rs", import.meta.url),
  "utf8",
);
const renderer = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");

describe("Discord QA draft recovery", () => {
  it("keeps the QA renderer dormant only for an explicit lock toggle", () => {
    const closeStart = main.indexOf("if !open {");
    const closeEnd = main.indexOf("\n    tauri::async_runtime::spawn_blocking", closeStart);
    expect(closeStart).toBeGreaterThan(-1);
    expect(closeEnd).toBeGreaterThan(closeStart);
    const close = main.slice(closeStart, closeEnd);

    expect(close).toContain(
      "native_discord_overlay::suspend_and_hide_for_qa_toggle(&app)?",
    );
    expect(close).toContain('#[cfg(feature = "discord-qa-shell")]');
    expect(close).toContain('#[cfg(not(feature = "discord-qa-shell"))]');
    expect(close).toContain("native_discord_overlay::clear_and_hide(&app);");
    expect(main).toContain(
      "native_discord_overlay::discard_changed_qa_toggle(&app);",
    );
  });

  it("keeps the protected draft in one hidden WebView without IPC or logging", () => {
    const start = nativeOverlay.indexOf(
      "pub(crate) fn suspend_and_hide_for_qa_toggle",
    );
    const end = nativeOverlay.indexOf(
      "\n#[cfg(feature = \"discord-qa-shell\")]\npub(crate) fn discard_changed_qa_toggle",
      start,
    );
    expect(start).toBeGreaterThan(-1);
    expect(end).toBeGreaterThan(start);
    const dormant = nativeOverlay.slice(start, end);

    expect(dormant).toContain("hide_window_for_qa_toggle(app)");
    expect(dormant).not.toContain(".close()");
    expect(dormant).not.toMatch(/\bplaintext\b|\bdraft\b|std::fs|write\(/u);
    expect(dormant).not.toContain("OVERLAY_CLOSED_EVENT");
    expect(nativeOverlay).toContain(
      'pub(crate) fn clear_and_hide(app: &tauri::AppHandle)',
    );
    expect(nativeOverlay).toContain(
      'app.emit_to("main", OVERLAY_CLOSED_EVENT, ())',
    );

    const ensureStart = nativeOverlay.indexOf("fn ensure_window(");
    const ensureEnd = nativeOverlay.indexOf("\npub(crate) fn show(", ensureStart);
    const ensure = nativeOverlay.slice(ensureStart, ensureEnd);
    // The session must reuse the retained WebView, never build a second one. The
    // reuse check now lives in the single construction gate, where it is repeated
    // under the build lock because Tauri registers a label only after the native
    // window exists.
    expect(ensure).toContain(
      "ensure_retained_protected_window(app, ProtectedSurface::Composer,",
    );
    const gateStart = nativeOverlay.indexOf("fn ensure_retained_protected_window(");
    const gateEnd = nativeOverlay.indexOf("\n/// Create the opaque capture shield", gateStart);
    expect(gateStart).toBeGreaterThan(-1);
    expect(gateEnd).toBeGreaterThan(gateStart);
    const gate = nativeOverlay.slice(gateStart, gateEnd);
    expect(gate).toContain("PROTECTED_WINDOW_BUILD_LOCK");
    expect(gate).toContain(
      "if let Some(window) = app.get_webview_window(surface.label())",
    );
    expect(gate.indexOf("PROTECTED_WINDOW_BUILD_LOCK")).toBeLessThan(
      gate.indexOf("app.get_webview_window(surface.label())"),
    );
    expect(gate.indexOf("app.get_webview_window(surface.label())")).toBeLessThan(
      gate.indexOf("build()?"),
    );

    const closeStart = main.indexOf("if !open {");
    const closeEnd = main.indexOf("\n    tauri::async_runtime::spawn_blocking", closeStart);
    const close = main.slice(closeStart, closeEnd);
    expect(close.indexOf("restore_suspended_native_draft")).toBeLessThan(
      close.indexOf("suspend_and_hide_for_qa_toggle"),
    );
  });

  it("does not intercept Backspace or Delete in the protected textarea", () => {
    const start = renderer.indexOf('draft.addEventListener("keydown"');
    const end = renderer.indexOf('draft.addEventListener("keyup"', start);
    expect(start).toBeGreaterThan(-1);
    expect(end).toBeGreaterThan(start);
    const keydown = renderer.slice(start, end);

    expect(keydown).not.toContain('"Backspace"');
    expect(keydown).not.toContain('"Delete"');
    expect(keydown).toContain(
      'if (plainTrustedEnter && mode !== "button") event.preventDefault();',
    );
  });

  it("records and clears every failed overlay open even when restoration fails", () => {
    const start = main.indexOf("opened.map_err(|error|");
    const end = main.indexOf("\n        })\n    })", start);
    expect(start).toBeGreaterThan(-1);
    expect(end).toBeGreaterThan(start);
    const failure = main.slice(start, end);

    expect(failure).toContain("let mut terminal_error = error;");
    expect(failure).toContain(
      '"OSL could not open protection or restore the saved Discord draft"',
    );
    expect(failure).toContain('record_overlay_open_stage(\n                "error"');
    expect(failure).toContain("native_discord_overlay::clear_and_hide(&app);");
    expect(failure.indexOf("clear_and_hide")).toBeGreaterThan(
      failure.indexOf("if restored.is_err()"),
    );
    expect(failure).not.toContain(
      'return "OSL could not open protection or restore the saved Discord draft"',
    );
  });

  it("keeps remount retries QA-only, bounded, and identity-empty exact", () => {
    const start = adapter.indexOf("fn locate_cleared_after_remount");
    const end = adapter.indexOf("\n    pub(super) fn suspend_and_calibrate", start);
    expect(start).toBeGreaterThan(-1);
    expect(end).toBeGreaterThan(start);
    const retry = adapter.slice(start, end);

    expect(adapter.slice(start - 80, start)).toContain(
      '#[cfg(feature = "discord-qa-shell")]',
    );
    expect(retry).toContain("Duration::from_millis(500)");
    expect(retry).toContain("post_mutation_binding_matches(expected, &located.binding)");
    expect(retry).toContain("composer_empty(&located.element)");
    expect(retry).not.toContain("set_native_draft");
  });

  it("rechecks exact Discord identity after overlay focus without realigning it", () => {
    const start = nativeOverlay.indexOf(
      "fn active_confirm_overlay_target_after_focus(",
    );
    const end = nativeOverlay.indexOf(
      "\n#[cfg(not(all(feature = \"discord-qa-shell\", target_os = \"windows\")))]",
      start,
    );
    expect(start).toBeGreaterThan(-1);
    expect(end).toBeGreaterThan(start);
    const qaConfirmation = nativeOverlay.slice(start, end);

    expect(nativeOverlay.slice(start - 100, start)).toContain(
      '#[cfg(all(feature = "discord-qa-shell", target_os = "windows"))]',
    );
    expect(qaConfirmation).toContain(
      "validate_current_discord_overlay_target_identity",
    );
    expect(nativeHost).toContain(
      "confirmed.generation != expected.generation",
    );
    expect(nativeHost).toContain(
      "confirmed.window != expected.window",
    );
    expect(nativeHost).toContain(
      "!process_is_trusted(confirmed.process_id)",
    );
    expect(qaConfirmation).not.toContain("discord_overlay_target(owner)");
    expect(nativeOverlay).toContain(
      "!exact_window_rect_matches(confirmed.window, target.rect)",
    );
    expect(nativeOverlay).toContain(
      'qa_overlay_window_stage("post_focus_host_confirmed")',
    );
    expect(nativeOverlay).toContain(
      'qa_overlay_window_stage("post_focus_context_confirmed")',
    );
  });

  it("allows only QA post-mutation runtime-id remounts with exact semantics and geometry", () => {
    const start = adapter.indexOf("fn qa_post_mutation_binding_equivalent");
    const end = adapter.indexOf("\n}\n\n/// One-message-only calibration", start);
    expect(start).toBeGreaterThan(-1);
    expect(end).toBeGreaterThan(start);
    const equivalent = adapter.slice(start, end);

    for (const exactField of [
      "generation",
      "scope_binding_hash",
      "conversation_binding_hash",
      "composer_name_hash",
      "automation_id_hash",
      "class_name_hash",
      "bounds",
      "display_bounds",
    ]) {
      expect(equivalent).toContain(`expected.${exactField} == actual.${exactField}`);
    }
    expect(equivalent).not.toContain("runtime_id");
    const gate = adapter.slice(start - 140, start);
    expect(gate).toContain("test");
    expect(gate).toContain('target_os = "windows"');
    expect(gate).toContain('feature = "discord-qa-shell"');
  });

  it("treats only the exact saved nonempty draft as already restored", () => {
    expect(adapter).toContain(
      "ExistingDraftRestore::AlreadyRestored => {",
    );
    expect(adapter).toContain("Some(before.binding)");
    expect(adapter).toContain("suspended.take();");
    // EXPECTED VALUE CHANGED: this used to pin the literal `current == saved`.
    // A raw string comparison made a draft that HAD been restored read as a
    // `Conflict` -- the renderer's own no-break-space substitution was enough --
    // which latched `calibration_stale_draft_not_restored` on every later lock
    // press and left the operator with no way to comply. The comparison is now
    // the same canonicalising one the "does the composer hold exactly what OSL
    // wrote" check uses, and it is covered directly by the Rust unit test
    // `a_restored_draft_is_recognised_through_the_renderers_substitutions`.
    //
    // The property this test exists for is unchanged and still pinned below:
    // ONLY a nonempty composer whose text matches the saved draft counts as
    // already restored. Empty writes the saved draft; anything else is a
    // conflict.
    expect(adapter).toContain(
      "else if exact_carrier_accessible_text(current, saved) {\n        ExistingDraftRestore::AlreadyRestored",
    );
    expect(adapter).toContain(
      "if canonical_accessible_text(current).is_empty() {\n        ExistingDraftRestore::WriteSaved",
    );
    expect(adapter).toContain("} else {\n        ExistingDraftRestore::Conflict");
  });

  it("permits semantic remount only after carrier insertion and adopts it before Enter", () => {
    const start = adapter.indexOf("pub(super) fn place(");
    const end = adapter.indexOf("\n    pub(super) fn snapshot(", start);
    expect(start).toBeGreaterThan(-1);
    expect(end).toBeGreaterThan(start);
    const place = adapter.slice(start, end);
    const finalCheck = place.indexOf("let Some(final_check)");

    expect(finalCheck).toBeGreaterThan(-1);
    expect(place.slice(0, finalCheck)).toContain(
      "binding_matches(&expected, &before.binding)",
    );
    expect(place.slice(0, finalCheck)).toContain(
      "binding_matches(&expected, &observed.binding)",
    );
    expect(place.slice(finalCheck)).toContain(
      "carrier_post_mutation_binding_matches(&expected, &final_check.binding)",
    );
    expect(place.slice(finalCheck)).toContain(
      "composer_holds_exact_text(&final_check.element, carrier)",
    );
    expect(place.slice(finalCheck)).toContain(
      "exact_composer_holds_keyboard_focus(target.window, &final_check.element, &expected)",
    );
    expect(place.slice(finalCheck)).toContain(
      "foreground_is_exact_discord(target.window)",
    );
    expect(place.slice(finalCheck).indexOf("*active_binding = Some(expected.clone());")).toBeLessThan(
      place.slice(finalCheck).indexOf("may_continue_input("),
    );
  });

  it("claims a native carrier send only after Discord consumes the exact draft", () => {
    const start = adapter.indexOf("pub(super) fn place(");
    const end = adapter.indexOf("\n    pub(super) fn snapshot(", start);
    expect(start).toBeGreaterThan(-1);
    expect(end).toBeGreaterThan(start);
    const place = adapter.slice(start, end);

    expect(adapter).toContain("fn confirm_carrier_consumed(");
    expect(adapter).toContain("composer_empty(&observed.element)");
    expect(adapter).toContain("KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP");
    expect(adapter).not.toContain("PostMessageW");
    expect(adapter).toContain("actual_target_process_id != target_process_id");
    expect(adapter).toContain("!process_is_trusted(focused_process_id)");
    expect(adapter).not.toContain("focused_process_id != target_process_id");
    expect(place.match(/confirm_carrier_consumed\(/gu)).toHaveLength(2);
    expect(place).toContain('#[cfg(not(feature = "discord-qa-shell"))]');
    expect(place).toContain(
      "if invoke_exact_send_action(target, process_is_trusted, &expected)",
    );
    // The accessibility-action fast path and the Enter fallback used to share
    // one `||` condition and one failure label. They are now two separate
    // `if` blocks with distinct labels (`place_refused_enter_not_injected`
    // vs. `place_refused_enter_unconfirmed`), because they are different
    // defects -- but both still gate `DiscordCarrierStatus::Sent` on
    // `confirm_carrier_consumed` proving the composer went empty, so a send
    // can still never be claimed until Discord has actually consumed the
    // exact draft.
    expect(place).toContain(
      "&& confirm_carrier_consumed(\n                target,\n                process_is_trusted,\n                &profile,\n                scope_binding,\n                &expected,\n            )",
    );
    expect(place).toContain(
      "if !confirm_carrier_consumed(\n            target,\n            process_is_trusted,\n            &profile,\n            scope_binding,\n            &expected,\n        ) {",
    );
    expect(place).toContain("if !finish_confirmed_carrier_send(");
    expect(adapter).toContain("let delays = [0, 15, 30, 60];");
    expect(place.indexOf("confirm_carrier_consumed(")).toBeLessThan(
      place.indexOf("DiscordCarrierStatus::Sent"),
    );
  });
});
