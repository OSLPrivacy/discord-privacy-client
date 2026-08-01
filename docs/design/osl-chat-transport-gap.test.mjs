import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const gapRecord = new URL("./osl-chat-transport-gap.md", import.meta.url);

test("OSL Chat transport gap records every T1 blocking divergence", async () => {
  const record = await readFile(gapRecord, "utf8");

  for (const requirement of [
    /\/v1\/control-inbox/,
    /\/v1\/wrapped-keys/,
    /native_overlay_relay_scope_id/,
    /Violates P3/,
    /Violates D3/,
    /T1 must not close its transport contract claiming OSL Chat coverage/,
  ]) {
    assert.match(record, requirement);
  }
});
