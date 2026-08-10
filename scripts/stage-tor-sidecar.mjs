#!/usr/bin/env node
// Build the OSL-owned Tor sidecar for the same target as the Tauri package,
// then give Tauri the target-suffixed external-bin filename it requires.
import { spawn } from "node:child_process";
import { copyFile, mkdir, stat } from "node:fs/promises";
import { join, resolve } from "node:path";

const BINARIES = ["osl-tor-sidecar", "osl-bridge-transport"];

function parseArgs(argv) {
  const options = { release: false };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--target") options.target = argv[++index];
    else if (flag === "--release") options.release = true;
    else if (flag === "--help") return { help: true };
    else throw new Error(`unknown argument: ${flag}`);
  }
  if (!options.help && !options.target) throw new Error("--target TARGET is required");
  return options;
}

function run(command, args, options) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, { ...options, stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) => code === 0 ? resolveRun() : reject(new Error(`${command} exited ${code}`)));
  });
}

export async function stageTorSidecar(options, root = process.cwd()) {
  const profile = options.release ? "release" : "debug";
  const targetDir = process.env.CARGO_TARGET_DIR
    ? resolve(process.env.CARGO_TARGET_DIR)
    : join(root, "apps", "osl-tor-sidecar", "target");
  const suffix = options.target.includes("windows") ? ".exe" : "";
  await run("cargo", ["build", "--locked", "--manifest-path", "apps/osl-tor-sidecar/Cargo.toml", "--bins", "--target", options.target, ...(options.release ? ["--release"] : [])], { cwd: root });
  await mkdir(join(root, "apps", "osl-hub", "binaries"), { recursive: true });
  const staged = [];
  let bytes = 0;
  for (const binary of BINARIES) {
    const source = join(targetDir, options.target, profile, `${binary}${suffix}`);
    const destination = join(root, "apps", "osl-hub", "binaries", `${binary}-${options.target}${suffix}`);
    await copyFile(source, destination);
    const binaryBytes = (await stat(destination)).size;
    if (binaryBytes === 0) throw new Error(`staged sidecar binary is empty: ${destination}`);
    staged.push(destination);
    bytes += binaryBytes;
    console.log(`tor_package_binary=${binary}:${binaryBytes}`);
  }
  console.log(`tor_sidecar_staged=${staged[0]}`);
  console.log(`tor_sidecar_bytes=${bytes}`);
  return { staged: staged[0], bytes, binaries: staged };
}

export async function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  if (options.help) {
    console.log("Usage: CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/i node scripts/stage-tor-sidecar.mjs --target TARGET [--release]");
    return;
  }
  await stageTorSidecar(options);
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(`sidecar staging failed: ${error.message}`);
    process.exitCode = 1;
  });
}
