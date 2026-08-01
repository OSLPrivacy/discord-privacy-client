import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

// Unit a11: identity storage-protection status must be rendered honestly —
// hardware-backed (TPM/keyring) shown as such, software fallback shown as
// such, and — critically — an UNKNOWN method (nothing learned this session)
// must render with the same not-secure weight as fallback, never as secure.
//
// These tests evaluate the real functions shipped in main.ts (extracted by
// source region, matching this codebase's established pattern for testing
// logic embedded in the non-importable UI entrypoint — see e.g.
// discord-qa-lock-refusal.test.ts) rather than a reimplementation of the
// logic, so a regression in the shipped code fails the test.

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

function visibleText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

function region(startNeedle: string, endNeedle: string): string {
  const start = source.indexOf(startNeedle);
  expect(start, `expected to find: ${startNeedle}`).toBeGreaterThan(-1);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  expect(end, `expected to find after start: ${endNeedle}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

type IdentityStorageProtection = "device" | "fallback" | "unknown";

function loadClassify(): (method: string | null) => IdentityStorageProtection {
  const body = region(
    "function classifyIdentityStorageProtection(method: string | null): IdentityStorageProtection {",
    "\nfunction identityStorageProtectionMarkup(",
  );
  const fnBody = body.slice(body.indexOf("{") + 1, body.lastIndexOf("}"));
  return new Function("method", fnBody) as (method: string | null) => IdentityStorageProtection;
}

function loadMarkup(): (protection: IdentityStorageProtection) => string {
  const body = region(
    "function identityStorageProtectionMarkup(protection: IdentityStorageProtection): string {",
    "\nfunction identitySettingsContent(",
  );
  const fnBody = body.slice(body.indexOf("{") + 1, body.lastIndexOf("}"));
  return new Function("protection", fnBody) as (protection: IdentityStorageProtection) => string;
}

describe("identity storage-protection status (unit a11)", () => {
  const classify = loadClassify();
  const markup = loadMarkup();

  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("(new) wire identity-protection status into the UI so it never conflate", async () => {
    const { __oslHubUiTest } = await loadUi();

    __oslHubUiTest.reset({ route: "settings", coreReady: true, storageMethod: "tpm-pcp" });
    const protectedHeader = __oslHubUiTest.renderRouteShell("settings");

    __oslHubUiTest.reset({ route: "settings", coreReady: true, storageMethod: "noop-insecure" });
    const fallbackHeader = __oslHubUiTest.renderRouteShell("settings");

    __oslHubUiTest.reset({ route: "settings", coreReady: false, storageMethod: "tpm-pcp" });
    const unfinishedHeader = __oslHubUiTest.renderRouteShell("settings");

    expect(protectedHeader).toContain('role="status" data-identity-protection="protected"');
    expect(protectedHeader).toContain('class="trust-state ready "');
    expect(visibleText(protectedHeader)).toMatch(/\bReady\b/iu);

    expect(fallbackHeader).toContain('role="status" data-identity-protection="not-secure"');
    expect(fallbackHeader).toContain('class="trust-state pending not-secure"');
    expect(visibleText(fallbackHeader)).toMatch(/\bNeeds attention\b/iu);
    expect(visibleText(fallbackHeader)).not.toMatch(/\bReady\b/iu);

    expect(unfinishedHeader).toContain('role="status" data-identity-protection="protected"');
    expect(unfinishedHeader).toContain('class="trust-state pending "');
    expect(visibleText(unfinishedHeader)).toMatch(/\bNeeds attention\b/iu);
    expect(visibleText(unfinishedHeader)).not.toMatch(/\bAccount protected\b|Device protection confirmed\b/iu);
  });

  it("classifies the two persistent platform-store sealer labels as device-held", () => {
    expect(classify("tpm-pcp")).toBe("device");
    expect(classify("keyring")).toBe("device");
  });

  it("classifies known software-fallback sealer labels as fallback", () => {
    expect(classify("noop-insecure")).toBe("fallback");
    expect(classify("memory-ephemeral")).toBe("fallback");
    expect(classify("memory-test")).toBe("fallback");
  });

  it("fails honest: an unrecognized non-null label is fallback, never hardware", () => {
    // A label OSL has never seen (e.g. a future sealer) must not be
    // silently trusted as secure just because it isn't a known bad one.
    expect(classify("some-future-sealer-label")).toBe("fallback");
    expect(classify("")).toBe("fallback");
  });

  it("fails honest: null (nothing learned this session) is unknown, not hardware", () => {
    expect(classify(null)).toBe("unknown");
  });

  it("renders platform-held protection distinctly and as secure", () => {
    const html = markup("device");
    expect(html).toContain("Protected by this device");
    expect(html).toMatch(/class="storage-protection-status secure"/);
    expect(html).not.toContain("insecure");
  });

  // The label the backend hands the UI ("keyring") does not say WHICH platform
  // store answered. On Linux this repo builds `keyring` with `linux-native`
  // (crates/keystore/Cargo.toml), i.e. the linux-keyutils kernel keyring: no
  // hardware root of trust and cleared by a reboot. The secure tier is shown
  // for that label, so its copy must not promise hardware.
  it("never claims hardware for the shared platform-store tier", () => {
    const html = markup("device");
    expect(html).not.toMatch(/hardware/iu);
    expect(html).toMatch(/TPM or operating-system credential store/u);
  });

  it("renders software fallback distinctly and as not secure", () => {
    const html = markup("fallback");
    expect(html).toContain("Software fallback storage");
    expect(html).toMatch(/class="storage-protection-status insecure"/);
    expect(html).not.toContain("Protected by this device");
  });

  it("renders unknown protection as not secure — never defaults to implying safety", () => {
    const html = markup("unknown");
    expect(html).toContain("Storage protection unknown");
    expect(html).toMatch(/class="storage-protection-status insecure"/);
    expect(html).not.toContain("Protected by this device");
    // Same non-secure visual tier as an explicit fallback: an alert role,
    // not a plain status role.
    expect(html).toMatch(/role="alert"/);
  });

  it("wires the status into the Account settings page, sourced from session state", () => {
    const content = region(
      "function identitySettingsContent(): string {",
      "\nfunction activationSettingsContent(",
    );
    expect(content).toContain("identityStorageProtectionMarkup(classifyIdentityStorageProtection(identityStorageMethod))");
  });

  it("never carries a stale status across an identity switch, burn, or decoy unlock", () => {
    // Switching identities: the switch result carries no storage method, so
    // status must come only from what THIS session learned about that exact
    // slot — never leftover from the previously active identity.
    const switchFn = region("async function switchIdentity(slotId: string): Promise<void> {", "\nasync function executeBurn(");
    expect(switchFn).toContain("identityStorageMethod = knownIdentityStorageMethods.get(slotId) ?? null;");

    // A duress/decoy unlock and a verified full-account burn both destroy or
    // hide the real identity; a stale "hardware-protected" reading must not
    // survive either.
    const decoyBranch = region('if (gate.outcome === "decoy") {', 'onboardingRoute = "decoy";');
    expect(decoyBranch).toContain("identityStorageMethod = null;");
    const burnedBranch = region("if (isVerifiedBurnGate(gate)) {", "onboardingComplete = false;");
    expect(burnedBranch).toContain("identityStorageMethod = null;");
  });

  it("does not overwrite the active identity's status when creating or recovering an inactive slot", () => {
    // identity_registry.rs creates additional slots inactive (slot_dto(&record,
    // false)) — the active identity does not change, so its status must not
    // be overwritten by a slot the user hasn't switched to yet.
    const createFn = region("async function createAdditionalIdentity(event: SubmitEvent): Promise<void> {", "\nasync function recoverAdditionalIdentity(");
    expect(createFn).toContain("knownIdentityStorageMethods.set(created.identity.slotId, created.storageMethod);");
    expect(createFn).not.toContain("identityStorageMethod = created.storageMethod");

    const recoverFn = region("async function recoverAdditionalIdentity(event: SubmitEvent): Promise<void> {", "\nasync function switchIdentity(");
    expect(recoverFn).toContain("knownIdentityStorageMethods.set(recovered.identity.slotId, recovered.storageMethod);");
    expect(recoverFn).not.toContain("identityStorageMethod = recovered.storageMethod");
  });
});
