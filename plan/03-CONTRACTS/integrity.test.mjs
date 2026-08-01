import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const contractPath = new URL("./integrity.md", import.meta.url);

async function readContract() {
  return readFile(contractPath, "utf8");
}

test("integrity contract fixes the signed manifest and v6 declaration boundary", async () => {
  const contract = await readContract();

  assert.match(contract, /build-hashes\.json\.sig/);
  assert.match(contract, /key id\n`3B6AE4739858E8D4`/);
  assert.match(contract, /u8 build_hash_sha256\[32\]/);
  assert.match(contract, /MSG_TYPE_BUILD_INTEGRITY = 0x0C/);
  assert.match(contract, /WIRE_VERSION_V6/);
  assert.match(contract, /No absent\nor malformed declaration may decode as `Verified`/);
});

test("integrity contract keeps peer integrity as disclosure rather than an enforcement gate", async () => {
  const contract = await readContract();

  assert.match(contract, /ReportedPublished/);
  assert.match(contract, /ReportedUnpublished/);
  assert.match(contract, /NotReported/);
  assert.match(contract, /never refuse, block, downgrade, or otherwise prevent messaging/);
  assert.match(contract, /never states or implies a\nproperty of the peer's machine/);
  assert.match(contract, /Integrity introduces no acknowledgement or receipt class/);
});
