/**
 * T14-D3 — the first-party OSL Chat surface may reveal protected content only
 * through T2's verified view-once viewer.
 *
 * This is deliberately a wiring proof, not an assertion that a Tauri command
 * returned `true`: the Windows gate itself must establish all four conditions,
 * read the affinity back, and the OSL Chat command must apply that boundary
 * before it asks the broker for plaintext. The actual Win32 calls remain
 * platform-gated in Rust, so non-Windows builds refuse the open path.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const hubMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const gate = readFileSync(new URL("../../../crates/runtime/src/screenshot_gate.rs", import.meta.url), "utf8");
const viewOnceOpen = readFileSync(new URL("../../osl-hub/src/view_once_open.rs", import.meta.url), "utf8");
const watcher = readFileSync(new URL("../../osl-hub/src/view_once_watch.rs", import.meta.url), "utf8");

function rustFunction(source: string, name: string, nextName: string): string {
  const start = source.indexOf(`fn ${name}`);
  const end = source.indexOf(`fn ${nextName}`, start + 1);
  expect(start, `${name} must exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} must follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("OSL Chat view-once capture protection", () => {
  it("requires every Windows capture-protection precondition and exact affinity readback", () => {
    const verify = rustFunction(gate, "verify_capture_protection", "format_win32_error");

    // These are independent checks: accepting the SetWindowDisplayAffinity
    // return value is insufficient because Windows can silently downgrade it.
    for (const prerequisite of [
      "windows_build()?",
      "GetAncestor(hwnd, GA_ROOT) == hwnd",
      "dwm_is_composing()?",
      "WS_EX_LAYERED != 0",
    ]) expect(verify).toContain(prerequisite);
    expect(verify).toContain("validate_prerequisites(prerequisites)?");
    expect(verify).toContain("SetWindowDisplayAffinity");
    expect(verify).toContain("GetWindowDisplayAffinity");
    expect(verify).toContain("verify_affinity_readback(observed)?");
  });

  it("keeps OSL Chat plaintext behind the verified gate, then re-verifies while it is displayed", () => {
    const open = rustFunction(hubMain, "open_osl_chat_text", "list_osl_chat_history");
    expect(open.indexOf("screenshot::apply_to_window")).toBeLessThan(open.indexOf("broker::drain_osl_chat_text"));

    // The view-once lifecycle has no network leg and cannot unseal before the
    // exact protection verification. The watchdog closes the viewer if that
    // proof later changes.
    expect(viewOnceOpen.indexOf("effects.verify_protection()?"))
      .toBeLessThan(viewOnceOpen.indexOf("effects.unseal_local_payload()?"));
    expect(watcher).toContain("protection_is_current");
    expect(watcher).toContain("close_viewer");
  });
});
