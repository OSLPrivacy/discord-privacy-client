import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import test from "node:test";

test("TASK 1543 Terms shows the service-rule and ban-risk agreement before any scanning words, each phrase exactly 1 time", () => {
  const output = execFileSync("node", ["scripts/check-terms-page-words.mjs"], { encoding: "utf8" });

  assert.match(output, /ban-risk heading count=1/);
  assert.match(output, /ban-risk agreement count=1/);
  assert.match(output, /ban-risk separate agreement count=1/);
  assert.match(output, /scanning and deletion heading count=1/);
  assert.match(output, /account scanning terms count=1/);
  assert.match(output, /account deletion terms count=1/);
  assert.match(output, /shared pricing facts count=1/);
  assert.match(output, /shared purchase terms 1 count=1/);
  assert.match(output, /shared purchase terms 2 count=1/);
  assert.match(output, /shared purchase terms 3 count=1/);
  assert.match(output, /ban-risk agreement ends at char \d+; first scanning word "\w+" at char \d+/);
  assert.match(
    output,
    /service-rule and ban-risk agreement precedes every scanning word, and all 10 phrases appear exactly 1 time\./,
  );
});
