import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { assertNoRuntimeTransparencyQueries } from "./check_no_runtime_transparency_queries.mjs";

async function fixtureRuntime(source) {
  const root = await mkdtemp(path.join(os.tmpdir(), "osl-transparency-gate-"));
  const hubSource = path.join(root, "apps/osl-hub/src");
  const uiSource = path.join(root, "apps/osl-hub-ui/src");
  await Promise.all([mkdir(hubSource, { recursive: true }), mkdir(uiSource, { recursive: true })]);
  await writeFile(path.join(hubSource, "runtime.rs"), source, "utf8");
  return root;
}

test("runtime transparency gate accepts ordinary client code", async () => {
  const root = await fixtureRuntime("pub fn start() {}\n");
  try {
    await assertNoRuntimeTransparencyQueries(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("runtime transparency gate refuses a Rekor lookup", async () => {
  const root = await fixtureRuntime('const LOG = "https://rekor.sigstore.dev/api/v1/log";\n');
  try {
    await assert.rejects(
      assertNoRuntimeTransparencyQueries(root),
      /runtime transparency-log reference is forbidden: apps\/osl-hub\/src\/runtime\.rs/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
