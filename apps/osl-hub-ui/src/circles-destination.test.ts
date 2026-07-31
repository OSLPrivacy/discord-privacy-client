import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseCircleAudience } from "./osl-collab";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("Circles destination content", () => {
  const circles = functionSource("circlesDestinationContent", "publicCirclesUnavailableMarkup");
  const inbox = functionSource("inboxDestinationContent", "activityDestinationContent");

  it("renders OSL Circles as private audience feeds in the Inbox", () => {
    expect(source).toContain('import { parseCircleAudience, type CircleAudience } from "./osl-collab"');
    expect(source).toContain("let privateCircleAudiences: CircleAudience[]");
    expect(source).toContain(".map((record) => parseCircleAudience(record))");
    expect(circles).toContain('data-inbox-osl-surface="circles"');
    expect(circles).toContain('data-circle-feeds="private-audiences"');
    expect(circles).toContain("Private audience feeds");
    expect(circles).toContain("Posts and comments are encrypted for the selected audience.");
    expect(circles).toContain("Audience membership is shown before posting.");
    expect(circles).toContain('data-circle-feed-order="chronological"');
    expect(circles).toContain("no ranking or behavioral advertising");
    expect(inbox).toContain('if (id === "circles") return circlesDestinationContent()');
  });

  it("refuses Circles posting when consent, audience selection, or account authority is absent", () => {
    const approved = parseCircleAudience({ audienceId: "a".repeat(32), name: "Close friends", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Maya", verified: true }], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: true });
    const noConsent = parseCircleAudience({ audienceId: "b".repeat(32), name: "Family", memberCount: 1, membershipVisibility: "visible", visibleMembers: [{ memberId: "2".repeat(32), name: "Ari", verified: true }], consentGranted: false, boundToCurrentCircle: true, postingAuthorized: true });
    const noAudience = parseCircleAudience({ audienceId: "c".repeat(32), name: "Book club", memberCount: 2, membershipVisibility: "count-only", visibleMembers: [], consentGranted: true, boundToCurrentCircle: false, postingAuthorized: true });
    const noAuthority = parseCircleAudience({ audienceId: "d".repeat(32), name: "Neighborhood", memberCount: 3, membershipVisibility: "hidden", visibleMembers: [], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: false });

    expect(approved?.canPost).toBe(true);
    expect(noConsent).toMatchObject({ canPost: false, refusal: "consent" });
    expect(noAudience).toMatchObject({ canPost: false, refusal: "binding" });
    expect(noAuthority).toMatchObject({ canPost: false, refusal: "authority" });
    expect(circles).toContain('data-circle-posting="${audience.canPost ? "ready" : "refused"}"');
    expect(circles).toContain('aria-disabled="${audience.canPost ? "false" : "true"}"');
    expect(source).toContain('if (audience.refusal === "consent") return { label: "Refused"');
    expect(source).toContain('if (audience.refusal === "binding") return { label: "Refused"');
    expect(source).toContain('return { label: "Refused", detail: "This account is not allowed to post to that audience." }');
  });

  it("keeps public Circles unavailable and avoids implementation-facing copy", () => {
    expect(circles).toContain("publicCirclesUnavailableMarkup()");
    expect(source).toContain('data-public-circles-network="unavailable"');
    expect(source).toContain("Public Circles network unavailable.");
    const visibleCopy = circles.replace(/\$\{[^}]+\}/g, "");
    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    expect(visibleCopy).not.toMatch(/global feed|available now|ready now|public .*end-to-end encrypted|screenshots impossible|forwarding impossible/i);
    expect(visibleCopy).not.toMatch(/auto.?retry|retry automatically|silently send/i);
  });
});
