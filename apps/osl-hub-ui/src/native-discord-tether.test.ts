import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const nativeSource = readFileSync(
  `${repoRoot}/apps/osl-hub/src/native_window_host.rs`,
  "utf8",
);
const mainSource = readFileSync(
  fileURLToPath(new URL("./main.ts", import.meta.url)),
  "utf8",
);
const behaviorSource = readFileSync(
  fileURLToPath(new URL("./ui-behavior.ts", import.meta.url)),
  "utf8",
);

function between(source: string, start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  expect(from, `${start} should exist`).toBeGreaterThanOrEqual(0);
  expect(to, `${end} should follow ${start}`).toBeGreaterThan(from);
  return source.slice(from, to);
}

describe("native Discord composite tether", () => {
  it("keeps only the exact adopted Discord identity bound to the OSL owner", () => {
    const identity = between(
      nativeSource,
      "unsafe fn borrowed_tether_target_identity_is_valid",
      "unsafe fn reconcile_borrowed_tether",
    );
    expect(identity).toContain("window_process_id(window) != Some(snapshot.process_id)");
    expect(identity).toContain("window_process_id(parent) == Some(std::process::id())");
    expect(identity).toContain("GetAncestor(parent, GA_ROOT) == parent");
    // The identity check now carries a short-lived, worker-state cache (see
    // BORROWED_TETHER_IDENTITY_MAX_AGE) so a healthy pass does not re-derive
    // the cross-process proof every 16 ms. The window/pid match above is
    // still unconditional on every pass, and the cache only ever serves a
    // verification that already ran against this exact HWND and pid, so the
    // identity property itself -- never trust a window whose pid, creation
    // time, session, or signed path drifted -- is unchanged.
    expect(identity).toContain("borrowed_tether_target_identity_is_valid(snapshot, &mut state)");
    expect(identity).toContain("borrowed_tether_parent_is_valid(snapshot)");
    expect(identity).toContain("borrowed_identity_fields_match(");

    const adoption = between(
      nativeSource,
      "unsafe fn adopt_existing_companion",
      "fn stop_owned_process",
    );
    expect(adoption).toContain("SetWindowLongPtrW(window, GWLP_HWNDPARENT, attached_owner)");
    expect(adoption).toContain("BorrowedWindowTether::create(tether_snapshot)");
    expect(adoption).toContain("if id == NativeAppId::Discord");
  });

  it("mirrors minimize and restores visibility, exact bounds, and z-order without activation", () => {
    const reconcile = between(
      nativeSource,
      "unsafe fn reconcile_borrowed_tether",
      "unsafe fn borrowed_window_tether_worker",
    );
    // The bare `if IsIconic(parent) != 0 { ... }` was restructured so the same
    // "is the host iconic" fact also arms the bounded restore-repaint retry
    // (the fix for Discord's compositor surface staying a flat colour after
    // restore). The mirrored-minimize behaviour it guards is unchanged: the
    // borrowed window is minimized with its host, and only if it was not
    // already minimized.
    expect(reconcile).toMatch(
      /let host_is_iconic = IsIconic\(parent\) != 0;[\s\S]*?if host_is_iconic \{[\s\S]*?if IsIconic\(window\) == 0 \{[\s\S]*?ShowWindow\(window, SW_MINIMIZE\);[\s\S]*?\}[\s\S]*?return BorrowedTetherOutcome::parent_minimized\(\);/,
    );
    expect(reconcile).toMatch(
      /if IsIconic\(window\) != 0 \{[\s\S]*?ShowWindow\(window, SW_RESTORE\);/,
    );
    expect(reconcile).toContain("let composite_is_active = foreground_root == parent || foreground_root == window;");
    expect(reconcile).toContain("borrowed_tether_requires_repair(");
    expect(reconcile).toContain("HWND_TOP");
    expect(reconcile).toContain("SWP_NOACTIVATE | SWP_SHOWWINDOW");
    expect(reconcile).toContain("rect_array(verified) != expected");

    expect(nativeSource).toContain(
      "const BORROWED_TETHER_ACTIVE_INTERVAL: Duration = Duration::from_millis(16);",
    );
    // The old binary 16 ms / 80 ms backoff was replaced by a four-tier
    // progressive one (active -> settling -> idle -> quiet), documented
    // inline against a measured live-run pass rate. The floor is still the
    // same 16 ms live-drag cadence; only the widened steady-state tiers
    // changed shape.
    expect(nativeSource).toContain(
      "const BORROWED_TETHER_SETTLING_INTERVAL: Duration = Duration::from_millis(48);",
    );
    expect(nativeSource).toContain(
      "const BORROWED_TETHER_IDLE_INTERVAL: Duration = Duration::from_millis(120);",
    );
    expect(nativeSource).toContain(
      "const BORROWED_TETHER_QUIET_INTERVAL: Duration = Duration::from_millis(320);",
    );
  });

  it("reconciles after parent move, resize, scale, restore, activation, and fullscreen settlement", () => {
    expect(mainSource).toContain(
      'import { bindWindowLifecycleRealignment } from "./window-lifecycle-bindings"',
    );
    expect(mainSource).toContain(
      "bindWindowLifecycleRealignment(\n    window,\n    desktopWindow,\n    document,\n    scheduleNativeHostRealignment,",
    );
    expect(mainSource).toContain("(handler) => desktopWindow.onFocusChanged(handler)");
    expect(behaviorSource).toContain(
      "return register(({ payload }) => dispatchMainWindowFocusChanged(payload, actions));",
    );
    expect(behaviorSource).toMatch(
      /if \(focused\) \{\s*actions\.scheduleNativeHostRealignment\(\);/,
    );

    const validate = between(
      mainSource,
      "async function validateNativeSurfacesPass",
      "async function validateNativeSurfaces",
    );
    expect(validate).toContain("resizeNativeAppWindow()");

    const fullscreen = between(
      mainSource,
      "async function toggleDesktopFullscreen",
      "async function focusActiveNativeCompanion",
    );
    expect(fullscreen.indexOf("await geometrySettled")).toBeLessThan(
      fullscreen.indexOf("resizeNativeAppWindow()"),
    );
    expect(fullscreen).toContain("await focusActiveNativeCompanion()");
  });

  it("uses a borderless non-activating owned shield that follows and samples Discord", () => {
    const worker = between(
      nativeSource,
      "unsafe fn borrowed_control_shield_worker",
      "unsafe fn position_borrowed_control_shield",
    );
    expect(worker).toContain("WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW");
    expect(worker).toContain("WS_POPUP | WS_VISIBLE");
    expect(worker).not.toContain("WS_BORDER");
    expect(worker).not.toContain("WS_CAPTION");
    expect(worker).toContain("target,");
    expect(worker).toContain("Duration::from_millis(16)");

    const position = between(
      nativeSource,
      "unsafe fn position_borrowed_control_shield",
      "unsafe fn paint_borrowed_control_shield",
    );
    // The DWM read moved out of this function into borrowed_control_shield_plan
    // when the shield stopped deriving its shape from a fixed reconstruction.
    // For Discord, DWM reports a zero-width caption cluster (custom HTML
    // titlebar), so the plan now prefers a real MSAA measurement and falls back
    // to the reconstruction only when that measurement is unavailable.
    expect(position).toContain("borrowed_control_shield_plan(target, probe.measured())");
    expect(position).toContain("HWND_TOP");
    expect(position).toContain("SWP_NOACTIVATE | SWP_SHOWWINDOW");
    expect(position).toContain("paint_borrowed_control_shield(");
    // Scheduling only: the measurement must never run on this thread, and an
    // already-correct shield must not be re-positioned or re-sampled every tick.
    expect(position).toContain("probe.poll(target, expected_process_id, (window_size, dpi))");
    expect(position).toMatch(/if !force[\s\S]*return \(true, false\);/u);

    const plan = between(
      nativeSource,
      "fn borrowed_control_shield_plan",
      "unsafe fn position_borrowed_control_shield",
    );
    expect(plan).toContain("DwmGetWindowAttribute(");
    expect(plan).toContain("DWMWA_CAPTION_BUTTON_BOUNDS");
    expect(plan).toContain("borrowed_control_shield_target(");

    const paint = between(
      nativeSource,
      "unsafe fn paint_borrowed_control_shield",
      "pub(super) enum TrustedWindowExecutable",
    );
    expect(paint).toContain("GetWindowDC(target)");
    expect(paint).toContain("GetPixel(target_dc");
    expect(paint).toContain("borrowed_control_shield_color(&samples)");
    expect(paint).toContain("FillRect(shield_dc");
  });
});
