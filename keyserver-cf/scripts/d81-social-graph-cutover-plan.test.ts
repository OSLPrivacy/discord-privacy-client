import { readFile } from "node:fs/promises";
import path from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = path.resolve(import.meta.dirname, "../..");
const planPath = path.join(ROOT, "keyserver-cf/MIGRATION-0038-D81-SOCIAL-GRAPH.md");

async function source(relative: string): Promise<string> {
  return await readFile(path.join(ROOT, relative), "utf8");
}

describe("D81 F2 — social-graph migration is a coordinated cutover", () => {
  it("records that v1 identities are signed, so a server-only column swap is forbidden", async () => {
    const [plan, canonical, endpoint] = await Promise.all([
      source("keyserver-cf/MIGRATION-0038-D81-SOCIAL-GRAPH.md"),
      source("keyserver-cf/src/lib/canonical.ts"),
      source("keyserver-cf/src/endpoints/control-inbox.ts"),
    ]);

    expect(canonical).toContain("lpString(args.sender_id)");
    expect(canonical).toContain("lpString(args.recipient_id)");
    expect(canonical).toContain("lpString(args.user_id)");
    expect(endpoint).toContain("canonicalControlInboxPostBytes");
    expect(endpoint).toContain("canonicalControlInboxGetBytes");
    expect(plan).toContain("do not apply a D1 migration yet");
    expect(plan).toContain("every deployed v1 client fail signature verification");
    expect(plan).toContain("refuse v1 traffic");
  });

  it("requires client-secret per-pair routing, not a renamed identity or server-keyed hash", async () => {
    const plan = await source("keyserver-cf/MIGRATION-0038-D81-SOCIAL-GRAPH.md");

    expect(plan).toContain("client-generated, per-pair 256-bit `pair_route`");
    expect(plan).toContain("must never be derived from a user id, username, or a server\nsecret");
    expect(plan).toContain("sender attribution is taken from the sealed control bundle");
    expect(plan).toContain("identity is request-transient and is never written\nto D1");
  });

  it("makes the full-D1 named-graph query impossible after cutover, across every known envelope surface", async () => {
    const plan = await source("keyserver-cf/MIGRATION-0038-D81-SOCIAL-GRAPH.md");

    for (const table of [
      "control_inbox",
      "control_inbox_requests",
      "wrapped_keys",
      "wrapped_key_post_receipts",
      "consuming_get_receipts",
      "mail_sender_consents",
    ]) {
      expect(plan).toContain(table);
    }
    expect(plan).toContain("no foreign key, index, trigger, view, or retained\ncolumn that joins `pair_route`");
    expect(plan).toContain("the audit query must fail");
    expect(plan).toMatch(/The migration must not copy legacy\s+rows/);
  });
});
