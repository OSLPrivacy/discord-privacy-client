#!/usr/bin/env node
import { spawn } from "node:child_process";
import { cp, mkdir, readdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";

const DEFAULT_OUTPUT_DIR = "release/installers";
const BUNDLE_DIR = "apps/osl-hub/target/release/bundle/nsis";
const VERSION_PATTERN = /^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$/;
const TAURI_CLI = "@tauri-apps/cli@2.11.4";
const TAURI_BUILD_ARGS = ["exec", "--yes", "--package", TAURI_CLI, "--", "tauri", "build", "--features", "desktop", "--", "--locked"];

function usage() {
  return [
    "Usage: node scripts/build-release-installer.mjs --version VERSION [--output-dir DIR]",
    "",
    "Exact release command:",
    "  CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/h node scripts/build-release-installer.mjs --version 2.0.0",
  ].join("\n");
}

function parseArgs(argv) {
  const options = { outputDir: DEFAULT_OUTPUT_DIR };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--help") return { help: true };
    if (flag === "--version") {
      options.version = argv[++index];
    } else if (flag === "--output-dir") {
      options.outputDir = argv[++index];
    } else if (flag === "--root") {
      options.root = argv[++index];
    } else {
      throw new Error(`unknown argument: ${flag}`);
    }
  }
  if (!options.version) throw new Error(`version input missing\n${usage()}`);
  if (!VERSION_PATTERN.test(options.version)) throw new Error(`invalid version input: ${options.version}`);
  if (!options.outputDir) throw new Error("--output-dir requires a value");
  return options;
}

function run(command, args, options) {
  return new Promise((resolveRun, reject) => {
    console.log(`run: ${[command, ...args].join(" ")}`);
    const child = spawn(command, args, { ...options, stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) resolveRun();
      else reject(new Error(`${command} exited ${code}`));
    });
  });
}

async function writeJsonVersion(path, version) {
  const document = JSON.parse(await readFile(path, "utf8"));
  document.version = version;
  await writeFile(path, `${JSON.stringify(document, null, 2)}\n`, "utf8");
}

async function writePackageLockVersion(path, version) {
  const document = JSON.parse(await readFile(path, "utf8"));
  if (Object.hasOwn(document, "version")) document.version = version;
  if (document.packages?.[""] && Object.hasOwn(document.packages[""], "version")) {
    document.packages[""].version = version;
  }
  await writeFile(path, `${JSON.stringify(document, null, 2)}\n`, "utf8");
}

async function writeCargoVersion(path, version) {
  const original = await readFile(path, "utf8");
  const next = original.replace(/(^\[package\][\s\S]*?^version\s*=\s*")[^"]+(")/m, `$1${version}$2`);
  if (next === original) throw new Error(`could not replace [package] version in ${path}`);
  await writeFile(path, next, "utf8");
}

async function writeCargoLockVersion(path, version) {
  const original = await readFile(path, "utf8");
  const next = original.replace(/(\[\[package\]\]\nname = "osl-hub"\nversion = ")[^"]+(")/, `$1${version}$2`);
  if (next === original) throw new Error(`could not replace osl-hub version in ${path}`);
  await writeFile(path, next, "utf8");
}

async function setReleaseVersion(root, version) {
  await writeJsonVersion(join(root, "apps/osl-hub/tauri.conf.json"), version);
  await writeCargoVersion(join(root, "apps/osl-hub/Cargo.toml"), version);
  await writeCargoLockVersion(join(root, "apps/osl-hub/Cargo.lock"), version);
  await writeJsonVersion(join(root, "apps/osl-hub-ui/package.json"), version);
  await writePackageLockVersion(join(root, "apps/osl-hub-ui/package-lock.json"), version);
}

async function listExeFiles(directory) {
  let entries;
  try {
    entries = await readdir(directory, { withFileTypes: true });
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".exe"))
    .map((entry) => join(directory, entry.name))
    .sort();
}

export async function buildReleaseInstaller(options) {
  const root = resolve(options.root ?? process.cwd());
  const outputDir = resolve(root, options.outputDir);
  const bundleDir = join(root, BUNDLE_DIR);
  const installerName = `OSL-${options.version}.exe`;
  const installerPath = join(outputDir, installerName);

  await setReleaseVersion(root, options.version);
  await rm(outputDir, { recursive: true, force: true });
  await rm(bundleDir, { recursive: true, force: true });

  await run("npm", ["ci", "--prefix", "apps/osl-hub-ui"], { cwd: root });
  await run("npm", ["run", "--prefix", "apps/osl-hub-ui", "build"], { cwd: root });
  await run("npm", TAURI_BUILD_ARGS, { cwd: join(root, "apps/osl-hub") });

  const bundles = await listExeFiles(bundleDir);
  if (bundles.length !== 1) throw new Error(`expected exactly one Tauri NSIS installer, got ${bundles.length}`);

  await mkdir(outputDir, { recursive: true });
  await cp(bundles[0], installerPath, { force: false });

  const installers = await listExeFiles(outputDir);
  if (installers.length !== 1 || installers[0] !== installerPath) {
    throw new Error(`expected exactly one release installer named ${installerName}, got ${installers.length}`);
  }
  const size = (await stat(installerPath)).size;
  if (size <= 0) throw new Error(`${installerName} is empty`);

  console.log(`installer_count=${installers.length}`);
  console.log(`installer_name=${installerName}`);
  console.log(`installer_size_bytes=${size}`);
  console.log(`installer_path=${installerPath}`);
  return { installerPath, installerName, installerCount: installers.length, size };
}

export async function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  if (options.help) {
    console.log(usage());
    return;
  }
  await buildReleaseInstaller(options);
}

if (import.meta.main) {
  try {
    await main();
  } catch (error) {
    console.error(`installer build recipe failed: ${error.message}`);
    process.exitCode = 1;
  }
}
