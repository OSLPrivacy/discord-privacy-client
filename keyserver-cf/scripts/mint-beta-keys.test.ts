import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "vitest";
import {
  parseDeliveryPayload,
  writeCompTextFile,
} from "./mint-beta-keys.ts";

const BATCH_ID = "comp_0123456789abcdef0123456789abcdef";
const EXPIRES_AT = 1_800_000_000;
const CODES = [
  "OSL-2222-3333-4444-5555",
  "OSL-6666-7777-8888-9999",
];

function encoded(extra: Record<string, unknown> = {}): Uint8Array {
  return new TextEncoder().encode(JSON.stringify({
    version: 1,
    batch_id: BATCH_ID,
    issuer: "production",
    expires_at: EXPIRES_AT,
    activation_codes: CODES,
    ...extra,
  }));
}

test("strictly validates, zeroes decrypted bytes, and writes mode-0600 text", () => {
  const decryptedBytes = encoded();
  const payload = parseDeliveryPayload(decryptedBytes, {
    batchId: BATCH_ID,
    quantity: 2,
    expiresAt: EXPIRES_AT,
  });
  assert.ok(decryptedBytes.every((byte) => byte === 0));

  const directory = mkdtempSync(join(tmpdir(), "osl-comp-test-"));
  const output = join(directory, `${BATCH_ID}.txt`);
  writeCompTextFile(output, payload);
  if (process.platform !== "win32") {
    assert.equal(statSync(output).mode & 0o777, 0o600);
  }
  const text = readFileSync(output, "utf8");
  assert.match(text, new RegExp(`^OSL comp batch: ${BATCH_ID}\\n`));
  for (const code of CODES) assert.equal(text.split("\n").filter((line) => line === code).length, 1);
  assert.deepEqual(payload.activation_codes, ["", ""]);
});

test("rejects unexpected fields and still zeroes decrypted bytes", () => {
  const decryptedBytes = encoded({ plaintext_backup: CODES[0] });
  assert.throws(
    () => parseDeliveryPayload(decryptedBytes, {
      batchId: BATCH_ID,
      quantity: 2,
      expiresAt: EXPIRES_AT,
    }),
    /fields are unexpected/,
  );
  assert.ok(decryptedBytes.every((byte) => byte === 0));
});

test("refuses non-text output paths", () => {
  const payload = parseDeliveryPayload(encoded(), {
    batchId: BATCH_ID,
    quantity: 2,
    expiresAt: EXPIRES_AT,
  });
  assert.throws(() => writeCompTextFile("codes.json", payload), /must end in \.txt/);
});
