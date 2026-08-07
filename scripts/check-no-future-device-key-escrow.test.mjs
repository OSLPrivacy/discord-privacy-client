import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import {
  formatReport,
  scanFutureDeviceKeyEscrow,
  SURFACES,
} from "./check-no-future-device-key-escrow.mjs";

test("task 4806 scanner names the required future-device-reachable surfaces", () => {
  assert.ok(SURFACES.length >= 4);
  assert.deepEqual(
    SURFACES.slice(0, 4).map((surface) => surface.name),
    ["key server tables", "blob store", "local sealed store", "sync payload shapes"],
  );
});

test("task 4806 scanner reports a forbidden sync payload field by file and name", () => {
  const root = path.join(
    tmpdir(),
    `osl-task-4806-${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}`,
  );
  const syncDir = path.join(root, "crates/ipc/src");
  mkdirSync(syncDir, { recursive: true });
  writeFileSync(
    path.join(syncDir, "ordinary_sync.rs"),
    [
      "use serde::{Deserialize, Serialize};",
      "#[derive(Serialize, Deserialize)]",
      "pub struct SyncPayload {",
      "    pub escrowed_message_key: String,",
      "}",
      "",
    ].join("\n"),
  );

  const result = scanFutureDeviceKeyEscrow(root);
  assert.equal(result.hits.length, 1);
  assert.deepEqual(result.hits[0], {
    surface: "sync payload shapes",
    file: "crates/ipc/src/ordinary_sync.rs",
    line: 4,
    field: "escrowed_message_key",
  });
  const report = formatReport(result);
  assert.match(report, /hits: 1/u);
  assert.match(report, /field escrowed_message_key/u);
  assert.match(report, /crates\/ipc\/src\/ordinary_sync\.rs:4/u);
});
