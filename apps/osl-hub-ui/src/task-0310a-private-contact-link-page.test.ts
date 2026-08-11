import { describe, expect, it } from "vitest";
import type { HubPrivateContactLink } from "./adapters";
import { PrivateContactLinkPageController } from "./onboarding-identity";

function issued(index: number): HubPrivateContactLink {
  const token = String(index).padStart(43, String(index));
  return {
    linkValue: `OSLCL2.${token}`,
    revocationSecret: String(index + 4).repeat(43),
    issuedAtUnixSeconds: 1_786_444_600 + index,
    expiresAtUnixSeconds: 1_786_444_601 + index,
    usesAllowed: 1,
    usesRecorded: 0,
  };
}

describe("TASK 0310a rendered private-link issuer page", () => {
  it("creates three distinct links, shows exact service expiry, and physically invokes revoke", async () => {
    const queue = [issued(1), issued(2), issued(3)];
    const revoked: string[] = [];
    const page = new PrivateContactLinkPageController({
      async create() { return queue.shift() ?? null; },
      async revoke(link) { revoked.push(link.linkValue); return true; },
    });
    expect(await page.create()).toBe(true);
    expect(await page.create()).toBe(true);
    expect(await page.create()).toBe(true);

    const before = page.render();
    expect(new Set(page.links.map((link) => link.linkValue)).size).toBe(3);
    expect(before.match(/data-private-contact-link>/gu)).toHaveLength(3);
    expect(before.match(/data-private-link-expiry/gu)).toHaveLength(3);
    expect(before.match(/Revoke now/gu)).toHaveLength(3);
    for (const link of page.links) {
      const exact = new Date(link.expiresAtUnixSeconds * 1000).toISOString();
      expect(before).toContain(`datetime="${exact}"`);
      expect(before).toContain(`data-expires-at-unix-seconds="${link.expiresAtUnixSeconds}"`);
    }
    expect(before).toContain('data-searchable-identifier="none"');
    expect(before).not.toContain("publicName");

    const third = page.links[2]!;
    expect(await page.revoke(third.linkValue)).toBe(true);
    expect(revoked).toEqual([third.linkValue]);
    const after = page.render();
    expect(after).toContain('data-private-link-state="revoked"');
    expect(after).toContain("Revoked");
    console.log(
      `TASK0310A_RENDERED links=${page.links.length} distinct=${new Set(page.links.map((link) => link.linkValue)).size} `
      + `exact_expiries=3 revoke_controls=3 physical_revoke_calls=${revoked.length} searchable_identifier=none`,
    );
  });
});
