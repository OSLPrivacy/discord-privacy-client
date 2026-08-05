//! W3-8 / D-272 — every cross-document reference a contract makes must resolve.
//!
//! `spaces.md` deferred its envelope and capability rules to a document that did
//! not exist, and the gate on that deferral checked that the LINK TEXT was
//! present rather than that the target was. It passed over a void for as long as
//! the void lasted.
//!
//! `t21-space-contract.test.mjs` now resolves `spaces.md`'s references. This
//! file applies the same check to the whole directory, because the defect was
//! never specific to one contract: any contract may defer, and a deferral is
//! only worth the document it reaches.

import assert from "node:assert/strict";
import test from "node:test";
import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

import {
  describeUnresolved,
  extractReferences,
  hasSection,
  unresolvedReferences,
} from "./contract-references.mjs";

const directory = fileURLToPath(new URL("./", import.meta.url));
const contracts = readdirSync(directory)
  .filter((name) => name.endsWith(".md"))
  .sort();

test("the contract directory is non-empty, so this gate cannot pass vacuously", () => {
  assert.ok(contracts.length > 0, "no contracts found; this gate would check nothing");
});

for (const contract of contracts) {
  test(`${contract}: every typed reference resolves to a real file and a real section`, () => {
    const unresolved = unresolvedReferences(join(directory, contract));
    assert.deepEqual(unresolved, [], unresolved.length ? describeUnresolved(unresolved) : undefined);
  });
}

// The checks above are only meaningful if the extractor actually recognises the
// forms contracts use. These pin that, so narrowing the extractor to match
// nothing shows up here instead of silently emptying every assertion above.
test("the extractor recognises both reference forms, with and without a section", () => {
  const found = extractReferences(
    [
      "See [`transport.md`](transport.md) §6b for the envelope rules.",
      "See [the lifecycle contract](lifecycle.md#boundary).",
      // A section binds only when it immediately follows the reference, which
      // is the form the contracts use (`storage.md` §2b). Prose that puts words
      // in between is not claiming that section belongs to that file.
      "The `ratchet.md` §4 covers recovery.",
      "The `entitlement.md` document, §9, is prose and binds no section.",
      "Nothing typed here: transport.md mentioned as bare prose.",
      "External [spec](https://example.invalid/spec.md) is not a repo reference.",
    ].join("\n"),
  );

  assert.deepEqual(
    found.map(({ form, target, anchor, section }) => ({ form, target, anchor, section })),
    [
      { form: "markdown-link", target: "transport.md", anchor: null, section: "§6b" },
      { form: "markdown-link", target: "lifecycle.md", anchor: "boundary", section: null },
      { form: "backticked-file", target: "ratchet.md", anchor: null, section: "§4" },
      { form: "backticked-file", target: "entitlement.md", anchor: null, section: null },
    ],
  );
});

test("a section token must match a heading exactly, not a neighbouring one", () => {
  const document = ["# Doc", "## 1b. Group sends", "### 6b. Delivery tag"].join("\n");

  assert.ok(hasSection(document, "§6b"), "§6b must match the `6b.` heading");
  assert.ok(hasSection(document, "§1b"), "§1b must match the `1b.` heading");
  // The boundary is the whole point: without it `§1` is satisfied by `1b.` and
  // a citation to a section that does not exist is waved through by its
  // neighbour -- the near-miss this gate exists to catch.
  assert.ok(!hasSection(document, "§1"), "§1 must NOT be satisfied by the `1b.` heading");
  assert.ok(!hasSection(document, "§6"), "§6 must NOT be satisfied by the `6b.` heading");
  assert.ok(!hasSection(document, "§7"), "§7 does not exist and must not match");
});
