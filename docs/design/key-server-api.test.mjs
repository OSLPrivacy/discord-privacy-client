import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const documentPath = new URL("key-server-api.md", import.meta.url);

test("selector resilience documents the T3-B5 loader handoff", async () => {
  const document = await readFile(documentPath, "utf8");
  const handoff = document.match(
    /### T3-B5 integration handoff\n([\s\S]*?)(?=\n## |\n### |$)/,
  )?.[1];

  assert.ok(handoff, "selector resilience must define the T3-B5 handoff");
  assert.match(handoff, /ManifestSource-backed provider/);
  assert.match(handoff, /load_signed_adapter_profile_or_compiled_in/);
  assert.match(handoff, /signature must not change/);
  assert.match(handoff, /Only a validated `ManifestState::Loaded`/);
  assert.match(handoff, /launch and each hourly refresh/);
});
