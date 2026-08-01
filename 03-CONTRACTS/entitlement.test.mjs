import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";

const contractPath = new URL("./entitlement.md", import.meta.url);

async function contract() {
  return readFile(contractPath, "utf8");
}

test("entitlement contract preserves the explicit, non-extending redemption boundary", async () => {
  const text = await contract();

  assert.match(text, /UNREDEEMED\s+--POST \/v1\/license\/redeem--> ACTIVE/);
  assert.match(text, /Repeated redemption is idempotent/);
  assert.match(text, /must never extend or mint a second period/);
  assert.match(text, /Validate is read-only: it never\nstarts, extends, or otherwise changes the entitlement clock/);
});

test("entitlement contract keeps encryption and word-bank carrier outside Pro", async () => {
  const text = await contract();

  assert.match(text, /must never gate protected\ntext, encryption, decryption, receiving, local history, or the word-bank\ncarrier/);
  assert.match(text, /A Free user, or\n+a Pro user who declined or removed optional AI, retains the fully working\n+word-bank carrier and encryption path/);
});
