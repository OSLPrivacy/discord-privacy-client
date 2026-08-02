import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseEnclaveAudience } from "./osl-collab";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("Enclaves destination content", () => {
  const enclaves = functionSource("enclavesDestinationContent", "publicEnclavesUnavailableMarkup");
  const inbox = functionSource("inboxDestinationContent", "activityDestinationContent");

  it("renders OSL Enclaves as private audience feeds in the Inbox", () => {
    expect(source).toContain('import { parseEnclaveAudience, type EnclaveAudience } from "./osl-collab"');
    expect(source).toContain("let privateEnclaveAudiences: EnclaveAudience[]");
    expect(source).toContain(".map((record) => parseEnclaveAudience(record))");
    expect(enclaves).toContain('data-inbox-osl-surface="enclaves"');
    expect(enclaves).toContain('data-enclave-feeds="private-audiences"');
    expect(enclaves).toContain("Private audience feeds");
    expect(enclaves).toContain("Posts and comments are encrypted for the selected audience.");
    expect(enclaves).toContain("Audience membership is shown before posting.");
    expect(enclaves).toContain('data-enclave-feed-order="chronological"');
    expect(enclaves).toContain("no ranking or behavioral advertising");
    expect(inbox).toContain('if (id === "enclaves") return enclavesDestinationContent()');
  });

  it("refuses Enclave posting when consent, audience selection, or account authority is absent", () => {
    const approved = parseEnclaveAudience({ audienceId: "a".repeat(32), name: "Close friends", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Maya", verified: true }], consentGranted: true, boundToCurrentEnclave: true, postingAuthorized: true });
    const noConsent = parseEnclaveAudience({ audienceId: "b".repeat(32), name: "Family", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "2".repeat(32), name: "Ari", verified: true }], consentGranted: false, boundToCurrentEnclave: true, postingAuthorized: true });
    const noAudience = parseEnclaveAudience({ audienceId: "c".repeat(32), name: "Book club", memberCount: 2, membershipVisibility: "count-only", visibleMembers: [], consentGranted: true, boundToCurrentEnclave: false, postingAuthorized: true });
    const noAuthority = parseEnclaveAudience({ audienceId: "d".repeat(32), name: "Neighborhood", memberCount: 3, membershipVisibility: "hidden", visibleMembers: [], consentGranted: true, boundToCurrentEnclave: true, postingAuthorized: false });

    expect(approved?.canPost).toBe(true);
    expect(noConsent).toMatchObject({ canPost: false, refusal: "consent" });
    expect(noAudience).toMatchObject({ canPost: false, refusal: "binding" });
    expect(noAuthority).toMatchObject({ canPost: false, refusal: "authority" });
    expect(enclaves).toContain('data-enclave-posting="${audience.canPost ? "ready" : "refused"}"');
    expect(enclaves).toContain('aria-disabled="${audience.canPost ? "false" : "true"}"');
    expect(source).toContain('if (audience.refusal === "consent") return { label: "Refused"');
    expect(source).toContain('if (audience.refusal === "binding") return { label: "Refused"');
    expect(source).toContain('return { label: "Refused", detail: "This account is not allowed to post to that audience." }');
  });

  it("keeps available Enclaves product-facing and avoids implementation-facing copy", () => {
    expect(enclaves).toContain('data-enclave-state="${enclaveSurface.state}"');
    expect(enclaves).toContain("OSL Enclaves");
    expect(enclaves).not.toContain("OSL Spaces");
    expect(enclaves).toContain("Private audience feeds");
    expect(enclaves).toContain("Posts and comments are encrypted for the selected audience.");
    const visibleCopy = enclaves.replace(/\$\{[^}]+\}/g, "");
    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    expect(visibleCopy).not.toMatch(/global feed|public .*end-to-end encrypted|screenshots impossible|forwarding impossible/i);
    expect(visibleCopy).not.toMatch(/auto.?retry|retry automatically|silently send/i);
  });
});
