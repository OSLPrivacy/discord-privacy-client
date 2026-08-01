import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const evidencePath = new URL("./scrub-provider-routes-2026-07-31.md", import.meta.url);

test("T12-E2 records a dated primary-source route for every §4 provider", async () => {
  const evidence = await readFile(evidencePath, "utf8");
  const providers = [
    "IMAP",
    "Gmail",
    "Reddit",
    "X (Twitter)",
    "Instagram own media",
    "Instagram DMs and comments on others’ posts",
    "Discord",
    "Facebook / Instagram Activity UI",
  ];

  for (const provider of providers) {
    const heading = `## ${provider}`;
    const start = evidence.indexOf(heading);
    assert.notEqual(start, -1, `missing provider evidence: ${provider}`);
    const next = evidence.indexOf("\n## ", start + heading.length);
    const section = evidence.slice(start, next === -1 ? undefined : next);
    assert.match(section, /\*\*Verbatim clause:\*\*/);
    assert.match(section, /\*\*Primary source:\*\* \[[^\]]+\]\(https:\/\//);
    assert.match(section, /\*\*Fetched:\*\* 2026-07-31/);
  }

  assert.match(
    evidence,
    /Automating normal user accounts \(generally called\n?\s*\\?"self-bots\\?"\).*forbidden, and can result in\n?\s*an account termination if found\./s,
  );
});
