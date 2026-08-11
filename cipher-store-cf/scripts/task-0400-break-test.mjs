#!/usr/bin/env node
import { appendFileSync, cpSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(process.cwd());
const probe = mkdtempSync(join(tmpdir(), "osl-task-0400-"));
try {
  for (const item of ["src", "test", "message-permission-authority-map.json", "wrangler.toml"]) {
    cpSync(join(root, item), join(probe, item), { recursive: true });
  }
  appendFileSync(join(probe, "src/index.ts"), `
// TASK 0400 negative control: reachable legacy route directly deletes storage.
async function legacyDirectDelete(env: Env, path: string): Promise<void> {
  if (path === "/v1/legacy-delete") await env.PAYLOADS.delete("legacy-direct-key");
}
`);
  const result = spawnSync(process.execPath, [join(root, "scripts/check-message-permission-map.mjs")], {
    cwd: root,
    env: { ...process.env, OSL_CIPHER_STORE_ROOT: probe },
    encoding: "utf8",
  });
  const output = `${result.stdout}${result.stderr}`;
  if (result.status === 0 || !output.includes("/v1/legacy-delete") || !output.includes("src/index.ts")) {
    throw new Error(`legacy-route negative control unexpectedly passed:\n${output}`);
  }
  process.stdout.write(`TASK 0400 BREAK PASS: exit ${result.status}; ${output.trim()}\n`);
} finally {
  rmSync(probe, { recursive: true, force: true });
}
