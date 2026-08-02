import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const contract = readFileSync(new URL("../../03-CONTRACTS/spaces.md", import.meta.url), "utf8");
assert.match(contract, /\[`transport\.md`\]\(transport\.md\) §6b/);
assert.match(contract, /rotating delivery tag/);
assert.doesNotMatch(contract, /recipient_user_id|osl_user_id/);
assert.match(contract, /0041.*0043/);
