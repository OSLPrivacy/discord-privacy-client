import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function readRelative(relativePath: string): string {
  return readFileSync(new URL(relativePath, import.meta.url), "utf8");
}

const checklist = readRelative("../../../docs/security/isolation-boundary-credential-handling-checklist.md");

function compact(value: string): string {
  return value.replace(/\s+/gu, " ");
}

function section(name: string): string {
  const start = checklist.indexOf(`## ${name}`);
  expect(start, `${name} section should exist`).toBeGreaterThanOrEqual(0);
  const next = checklist.indexOf("\n## ", start + 1);
  return next < 0 ? checklist.slice(start) : checklist.slice(start, next);
}

describe("f75 security-review checklist acceptance artifact", () => {
  it("requires the central review evidence for isolation and credential handling", () => {
    const requiredEvidence = section("Required Evidence");
    for (const gate of [
      "Explicit user consent",
      "Account binding",
      "Operation authority",
      "Isolation boundary",
      "Credential handling",
      "Plaintext minimization",
      "Deletion semantics",
      "Challenge handling",
      "Auditability",
    ]) {
      expect(requiredEvidence).toContain(gate);
    }

    expect(requiredEvidence).toMatch(/Missing, expired, revoked, implied, inherited, or ambiguous consent refuses/u);
    expect(requiredEvidence).toMatch(/Missing, stale, cross-account, guessed, copied, or UI-only account binding refuses/u);
    expect(requiredEvidence).toMatch(/Missing, synthetic, test-only, comments-only, or broader-than-requested authority refuses/u);
    expect(requiredEvidence).toMatch(/Secret-bearing diagnostics, account handles, credentials, or stable private identifiers refuse/u);
  });

  it("makes absence of consent, binding, authority, and reviewer approval fail closed", () => {
    const boundary = section("Boundary Requirements");
    expect(compact(boundary)).toContain(
      "Absence of consent, binding, authority, permission, entitlement, reviewer approval, or verified capability means refusal or Unavailable, never permission.",
    );
    expect(boundary).toContain("Advice cannot widen permissions");
    expect(boundary).toContain("Challenge, cancellation, timeout, account mismatch, target mismatch");
    expect(boundary).toContain("Browser import has exactly two user choices");
  });

  it("keeps secrets, account identifiers, handles, and credentials out of diagnostics", () => {
    const credentialHandling = section("Credential Handling Requirements");
    const compactCredentialHandling = compact(credentialHandling);
    for (const sensitive of [
      "credential",
      "account identifier",
      "user handle",
      "access token",
      "refresh token",
      "cookie",
      "session secret",
      "recovery phrase",
      "private key",
      "bearer token",
      "native window handle",
    ]) {
      expect(compactCredentialHandling).toContain(sensitive);
    }
    expect(credentialHandling).toContain("Debug or Display output");
    expect(credentialHandling).toContain("redacted digests");
    expect(credentialHandling).toContain("maximum byte count");
  });

  it("preserves the product-language and Burn-copy bans from the design specs", () => {
    const userLanguage = section("User-Facing Language Requirements");
    for (const bannedTerm of [
      "keyserver",
      "ratchet",
      "receipt",
      "browser profile",
      "provider adapter",
    ]) {
      expect(userLanguage).toMatch(new RegExp(`- ${bannedTerm}`, "u"));
    }

    expect(userLanguage).toContain("Local OSL copy");
    expect(userLanguage).toContain("OSL server copy");
    expect(userLanguage).toContain("Other person's app");
    expect(userLanguage).toContain("Connected service message");
    expect(userLanguage).toContain("Already opened copies");

    expect(userLanguage).toContain("cryptographic burn");
    expect(userLanguage).toContain("destroys keys, not messages");
    expect(userLanguage).toContain("permanent ciphertext");
    expect(userLanguage).toContain("disappears forever");
    expect(userLanguage).toContain("permanently undecryptable");
    expect(userLanguage).toContain("gone for good");
    expect(userLanguage).toContain("Do not fabricate ban-risk percentages");
  });

  it("requires RN delivery preconditions while leaving runtime activation closed", () => {
    const rnSource = readRelative("../../../crates/ipc/src/wire_rn.rs");
    const stateSource = readRelative("../../../crates/ipc/src/state.rs");
    const clientSource = readRelative("../../../crates/keystore/src/client.rs");

    expect(rnSource).toMatch(/pub const RN_WIRE_IN_ENABLED: bool = true;/u);
    expect(stateSource).toContain("rn_wire_in_enabled: AtomicBool::new(false)");
    expect(stateSource).toContain("pub fn set_rn_wire_in_enabled");
    expect(clientSource).toContain("CLIENT_RN_CAPABILITY_FLOOR: u32 = rn_capabilities_for_wire_in(true)");
    expect(rnSource).toContain('pub const RN_SESSION_DIR: &str = "rn_sessions"');
    expect(rnSource).toContain(".create_new(true)");
  });

  it("records release-blocking checklist items for a human security review", () => {
    const acceptance = section("Acceptance Checklist");
    const requiredChecks = acceptance.match(/^- \[ \]/gmu) ?? [];
    expect(requiredChecks).toHaveLength(10);
    expect(acceptance).toContain("reviewed commit and file list");
    expect(acceptance).toContain("Every absence case above refuses instead of permitting");
    expect(acceptance).toContain("Plaintext paths are explicit, size bounded, and redacted");
    expect(acceptance).toContain("Any unchecked item is a release blocker");
  });
});
