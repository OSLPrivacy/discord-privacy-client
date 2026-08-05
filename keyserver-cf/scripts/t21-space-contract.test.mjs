import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describeUnresolved, unresolvedReferences } from "../../03-CONTRACTS/contract-references.mjs";

const contractUrl = new URL("../../03-CONTRACTS/spaces.md", import.meta.url);
const contractPath = fileURLToPath(contractUrl);
const contract = readFileSync(contractUrl, "utf8");

// --- the original assertions, unchanged ---------------------------------
assert.match(contract, /\[`transport\.md`\]\(transport\.md\) §6b/);
assert.match(contract, /rotating delivery tag/);
assert.doesNotMatch(contract, /recipient_user_id|osl_user_id/);
assert.match(contract, /0041.*0043/);

// --- W3-8 / D-272: check the REFERENT, not the reference ----------------
//
// The four assertions above check that `spaces.md` SAYS the right things.
// Every one of them passed while ``[`transport.md`](transport.md) §6b`` --
// asserted verbatim on the first line -- pointed at a document that did not
// exist, so the envelope and capability rules this lane was told it inherited
// were never written and no gate noticed. A link is not a rule.
//
// The check below is deliberately general rather than a second hard-coded
// string: it resolves EVERY typed reference the contract makes, so the next
// deferral into a void fails here too instead of being silent. Both halves
// matter -- a reference must reach a real file AND a real section, because
// `§6b` in a document with no `§6b` is the same void wearing a valid filename.
const unresolved = unresolvedReferences(contractPath);
assert.deepEqual(unresolved, [], unresolved.length ? describeUnresolved(unresolved) : undefined);

// The reference above is the one this gate exists for; assert that it is
// actually among the references the resolver checked. Without this, narrowing
// the extractor to match nothing would make the gate vacuously green -- the
// same failure mode in a new place.
const { extractReferences } = await import("../../03-CONTRACTS/contract-references.mjs");
const references = extractReferences(contract);
assert.ok(
  references.some((reference) => reference.target === "transport.md" && reference.section === "§6b"),
  "the resolver must see `transport.md` §6b as a reference; if it does not, this gate is checking nothing",
);
