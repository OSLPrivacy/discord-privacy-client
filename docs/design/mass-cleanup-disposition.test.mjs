import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const dispositionUrl = new URL("./mass-cleanup-disposition.md", import.meta.url);

function frontMatter(markdown) {
  const match = /^---\n([\s\S]*?)\n---\n/.exec(markdown);
  assert.ok(match, "the disposition must start with YAML front matter");

  return Object.fromEntries(
    match[1].split("\n").map((line) => {
      const separator = line.indexOf(": ");
      assert.notEqual(separator, -1, `invalid front-matter line: ${line}`);
      return [line.slice(0, separator), line.slice(separator + 2)];
    }),
  );
}

test("Mass Cleanup retires its duplicate executor only after ScrubAdapter parity", async () => {
  const record = frontMatter(await readFile(dispositionUrl, "utf8"));

  assert.deepEqual(record, {
    id: "MASS-CLEANUP-VERDICT",
    status: "accepted",
    decision: "retire-duplicate-seam",
    authority: "scrub-adapter-engine",
    transition: "retain-manifest-and-entitlement-until-engine-parity",
    safety: "remain-fail-closed-until-parity",
  });
});
