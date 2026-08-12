#!/usr/bin/env node
// TASK 6841 — "run 6840" against an arbitrary tree root.
//
// TASK 6840's check is three things: the exhaustive user-facing scan, the
// Vitest naming regression, and the older-encrypted-attachment compatibility
// test. This runs them and exits 1 naming the surface that broke.
//
// Usage: node run-6840.mjs <treeRoot> [--stage=ui|wire|all]
//
//   ui    scan + Vitest  (the resource surfaces)
//   wire  cargo compatibility test (the encrypted wire type)
//   all   both
//
// Env: CARGO_TARGET_DIR must be set for the wire stage.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..");
const treeRoot = resolve(process.argv[2] ?? process.cwd());
const stageArg = (process.argv.find((a) => a.startsWith("--stage=")) ?? "--stage=all").slice(8);

const WIRE_SURFACE = "crates/ipc/src/attachment_wire.rs";
const WIRE_TEST = "task_6840_legacy_encrypted_text_attachment_still_opens";
const UI_SURFACES = [
  "apps/osl-hub-ui/overlay.html",
  "apps/osl-hub-ui/src/main.ts",
  "apps/osl-hub-ui/README.md",
];

const out = [];
const say = (line) => {
  out.push(line);
  console.log(line);
};

const run = (cmd, args, opts = {}) => {
  const r = spawnSync(cmd, args, { encoding: "utf8", ...opts });
  const text = `${r.stdout ?? ""}${r.stderr ?? ""}`;
  return { code: r.status === null ? 1 : r.status, text };
};

let failed = false;

// ---- stage: ui -------------------------------------------------------------
if (stageArg === "ui" || stageArg === "all") {
  const scan = run(process.execPath, [join(here, "scan-6840.mjs"), treeRoot]);
  process.stdout.write(scan.text);
  out.push(scan.text);
  if (scan.code !== 0) failed = true;

  const vitest = join(repoRoot, "apps", "osl-hub-ui", "node_modules", ".bin", "vitest");
  const uiRoot = join(treeRoot, "apps", "osl-hub-ui");
  if (!existsSync(vitest)) {
    say("TASK6841_RED surface=apps/osl-hub-ui class=missing slot=vitest detail=vitest binary absent");
    failed = true;
  } else {
    const v = run(vitest, ["run", "src/task-6840-text-attachment.test.ts"], { cwd: uiRoot });
    process.stdout.write(v.text);
    out.push(v.text);
    const passed = /Tests\s+(\d+) passed/u.exec(v.text);
    say(`TASK6841_VITEST exit=${v.code} tests_passed=${passed ? passed[1] : 0}`);
    if (v.code !== 0) {
      failed = true;
      // Vitest names the assertion, not the file it read. Attribute the
      // failure to whichever resource surface the diff points at.
      const named = UI_SURFACES.filter((s) => {
        const base = s.split("/").pop();
        return v.text.includes(base);
      });
      for (const s of named.length ? named : UI_SURFACES) {
        say(`TASK6841_RED surface=${s} class=vitest slot=task-6840-text-attachment detail=vitest exit ${v.code}`);
      }
    }
  }
}

// ---- stage: wire -----------------------------------------------------------
if (stageArg === "wire" || stageArg === "all") {
  const cargo = process.env.CARGO ?? join(process.env.HOME ?? "", ".cargo", "bin", "cargo");
  const c = run(cargo, [
    "test",
    "--manifest-path",
    join(treeRoot, "Cargo.toml"),
    "-p",
    "ipc",
    "--lib",
    WIRE_TEST,
    "--",
    "--test-threads=1",
    "--nocapture",
  ], { env: { ...process.env } });
  process.stdout.write(c.text);
  out.push(c.text);
  const result = /test result: (\w+)\. (\d+) passed; (\d+) failed/u.exec(c.text);
  say(
    `TASK6841_WIRE exit=${c.code} result=${result ? result[1] : "none"} ` +
      `passed=${result ? result[2] : 0} failed=${result ? result[3] : 0}`,
  );
  if (c.code !== 0 || !result || result[2] !== "1") {
    failed = true;
    say(
      `TASK6841_RED surface=${WIRE_SURFACE} class=compatibility slot=${WIRE_TEST} ` +
        `detail=older encrypted text attachment no longer opens (cargo exit ${c.code})`,
    );
  }
}

if (failed) {
  console.log("TASK6841_RUN6840 verdict=RED");
  process.exit(1);
}
console.log("TASK6841_RUN6840 verdict=GREEN");
process.exit(0);
