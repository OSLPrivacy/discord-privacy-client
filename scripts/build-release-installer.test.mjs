import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import test from "node:test";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const script = new URL("./build-release-installer.mjs", import.meta.url).pathname;

async function writeFixture(root) {
  await mkdir(join(root, "apps/osl-hub"), { recursive: true });
  await mkdir(join(root, "apps/osl-hub-ui"), { recursive: true });
  await writeFile(join(root, "apps/osl-hub/tauri.conf.json"), `${JSON.stringify({ productName: "OSL Privacy", version: "0.0.0" }, null, 2)}\n`);
  await writeFile(join(root, "apps/osl-hub/Cargo.toml"), `[package]\nname = "osl-hub"\nversion = "0.0.0"\n`);
  await writeFile(join(root, "apps/osl-hub/Cargo.lock"), `version = 4\n\n[[package]]\nname = "osl-hub"\nversion = "0.0.0"\n`);
  await writeFile(join(root, "apps/osl-hub-ui/package.json"), `${JSON.stringify({ name: "osl-hub-ui", version: "0.0.0", scripts: { build: "vite build" } }, null, 2)}\n`);
  await writeFile(join(root, "apps/osl-hub-ui/package-lock.json"), `${JSON.stringify({ name: "osl-hub-ui", version: "0.0.0", lockfileVersion: 3, packages: { "": { name: "osl-hub-ui", version: "0.0.0" } } }, null, 2)}\n`);
}

async function writeFakeNpm(root) {
  const bin = join(root, "fake-bin");
  await mkdir(bin, { recursive: true });
  const fakeNpm = join(bin, "npm");
  await writeFile(fakeNpm, `#!/usr/bin/env node
const { mkdirSync, writeFileSync, appendFileSync } = require("node:fs");
const { join, resolve } = require("node:path");
const root = resolve(__dirname, "..");
appendFileSync(join(root, "fake-tools.jsonl"), JSON.stringify({ cwd: process.cwd(), args: process.argv.slice(2) }) + "\\n");
const args = process.argv.slice(2);
if (args[0] === "ci" && args[1] === "--prefix" && args[2] === "apps/osl-hub-ui") process.exit(0);
if (args[0] === "exec" && args[1] === "--" && args[2] === "vite" && args[3] === "build") {
  mkdirSync(join(root, "apps/osl-hub-ui/dist"), { recursive: true });
  writeFileSync(join(root, "apps/osl-hub-ui/dist/index.html"), "<!doctype html>");
  process.exit(0);
}
const expected = ["exec", "--yes", "--package", "@tauri-apps/cli@2.11.4", "--", "tauri", "build", "--target", "x86_64-pc-windows-gnu", "--features", "desktop", "--", "--locked"];
if (JSON.stringify(args) === JSON.stringify(expected)) {
  const bundle = join(process.env.CARGO_TARGET_DIR, "x86_64-pc-windows-gnu/release/bundle/nsis");
  mkdirSync(bundle, { recursive: true });
  writeFileSync(join(bundle, "setup-from-tauri.exe"), Buffer.from("fake nonzero installer"));
  process.exit(0);
}
console.error("unexpected npm invocation", JSON.stringify(args));
process.exit(99);
`);
  await chmod(fakeNpm, 0o755);
  return bin;
}

async function runRecipe(root, args) {
  const fakeBin = await writeFakeNpm(root);
  return execFileAsync(process.execPath, [script, "--root", root, ...args], {
    cwd: root,
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${fakeBin}${delimiter}${process.env.PATH}`,
      CARGO_TARGET_DIR: "/mnt/d/osl-lane-targets/h",
    },
  });
}

test("TASK 1600 recipe builds exactly one nonempty OSL-versioned installer", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "osl-installer-recipe-clean-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFixture(root);

  const { stdout } = await runRecipe(root, ["--version", "2.0.0"]);

  const outputDir = join(root, "release/installers");
  assert.deepEqual(await readdir(outputDir), ["OSL-2.0.0.exe"]);
  const size = (await stat(join(outputDir, "OSL-2.0.0.exe"))).size;
  assert.equal(size, 22);
  assert.match(stdout, /installer_count=1/);
  assert.match(stdout, /installer_name=OSL-2\.0\.0\.exe/);
  assert.match(stdout, /installer_size_bytes=22/);
  console.log(`TASK1600_SUCCESS installer_count=1 installer_name=OSL-2.0.0.exe installer_size_bytes=${size}`);

  assert.equal(JSON.parse(await readFile(join(root, "apps/osl-hub/tauri.conf.json"), "utf8")).version, "2.0.0");
  assert.match(await readFile(join(root, "apps/osl-hub/Cargo.toml"), "utf8"), /^version = "2\.0\.0"$/m);
  assert.match(await readFile(join(root, "apps/osl-hub/Cargo.lock"), "utf8"), /name = "osl-hub"\nversion = "2\.0\.0"/);
  assert.equal(JSON.parse(await readFile(join(root, "apps/osl-hub-ui/package.json"), "utf8")).version, "2.0.0");
  assert.equal(JSON.parse(await readFile(join(root, "apps/osl-hub-ui/package-lock.json"), "utf8")).packages[""].version, "2.0.0");

  const invocations = (await readFile(join(root, "fake-tools.jsonl"), "utf8"))
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line).args);
  assert.deepEqual(invocations, [
    ["ci", "--prefix", "apps/osl-hub-ui"],
    ["exec", "--", "vite", "build"],
    ["exec", "--yes", "--package", "@tauri-apps/cli@2.11.4", "--", "tauri", "build", "--target", "x86_64-pc-windows-gnu", "--features", "desktop", "--", "--locked"],
  ]);

  const repeated = await runRecipe(root, ["--version", "2.0.0"]);
  assert.match(repeated.stdout, /installer_count=1/);
});

test("TASK 1600 recipe fails nonzero and leaves no installer when version input is missing", async (t) => {
  const root = await mkdtemp(join(tmpdir(), "osl-installer-recipe-clean-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  await writeFixture(root);

  let exitCode;
  await assert.rejects(runRecipe(root, []), (error) => {
    assert.notEqual(error.code, 0);
    exitCode = error.code;
    assert.match(error.stderr, /version input missing/);
    return true;
  });

  assert.deepEqual(await readdir(root), ["apps", "fake-bin"]);
  console.log(`TASK1600_MISSING_VERSION exit_code=${exitCode} installer_count=0`);
});
