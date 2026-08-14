import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const page = readFileSync(new URL("./download.html", import.meta.url), "utf8");

test("TASK 1604: unsigned-publisher note contains all 4 required strings", () => {
  // Check for "unsigned" (case-insensitive)
  assert.match(page, /unsigned/i, "Note must contain 'unsigned'");

  // Check for "unknown publisher" (case-insensitive)
  assert.match(page, /unknown\s+publisher/i, "Note must contain 'unknown publisher'");

  // Check for "warning" (case-insensitive)
  assert.match(page, /warning/i, "Note must contain 'warning'");

  // Check for "checksum" (case-insensitive)
  assert.match(page, /checksum/i, "Note must contain 'checksum'");
});

test("TASK 1604 sabotage: fixture without checksum string fails the check", () => {
  const fixtureWithoutChecksum = page.replace(/checksum/gi, "");

  // Should still have unsigned
  assert.match(fixtureWithoutChecksum, /unsigned/i);

  // Should still have unknown publisher
  assert.match(fixtureWithoutChecksum, /unknown\s+publisher/i);

  // Should still have warning
  assert.match(fixtureWithoutChecksum, /warning/i);

  // Should NOT have checksum - this check should fail
  assert.throws(
    () => assert.match(fixtureWithoutChecksum, /checksum/i),
    "Fixture without checksum should fail the check"
  );
});
