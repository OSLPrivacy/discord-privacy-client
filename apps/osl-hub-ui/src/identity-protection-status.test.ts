import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

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

function region(startNeedle: string, endNeedle: string): string {
  const start = source.indexOf(startNeedle);
  expect(start, `expected to find: ${startNeedle}`).toBeGreaterThan(-1);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  expect(end, `expected to find after start: ${endNeedle}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

type IdentityStorageProtection = "hardware" | "fallback" | "unknown";

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

  it("classifies the real hardware-backed sealer labels as hardware", () => {
    expect(classify("tpm-pcp")).toBe("hardware");
    expect(classify("keyring")).toBe("hardware");
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

  it("renders hardware-backed protection distinctly and as secure", () => {
    const html = markup("hardware");
    expect(html).toContain("Hardware-protected");
    expect(html).toMatch(/class="storage-protection-status secure"/);
    expect(html).not.toContain("insecure");
  });

  it("renders software fallback distinctly and as not secure", () => {
    const html = markup("fallback");
    expect(html).toContain("Software fallback storage");
    expect(html).toMatch(/class="storage-protection-status insecure"/);
    expect(html).not.toContain("Hardware-protected");
  });

  it("renders unknown protection as not secure — never defaults to implying safety", () => {
    const html = markup("unknown");
    expect(html).toContain("Storage protection unknown");
    expect(html).toMatch(/class="storage-protection-status insecure"/);
    expect(html).not.toContain("Hardware-protected");
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
    const burnedBranch = region('if (gate.outcome === "burned") {', "onboardingComplete = false;");
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
