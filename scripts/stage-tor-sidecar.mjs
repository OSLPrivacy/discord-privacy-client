#!/usr/bin/env node
// Build the OSL-owned Tor sidecar for the same target as the Tauri package,
// then give Tauri the target-suffixed external-bin filename it requires.
import { spawn } from "node:child_process";
import { copyFile, mkdir, stat } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SIDE_CAR = "osl-tor-sidecar";

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
  const source = join(targetDir, options.target, profile, `${SIDE_CAR}${suffix}`);
  const staged = join(root, "apps", "osl-hub", "binaries", `${SIDE_CAR}-${options.target}${suffix}`);
  await run("cargo", ["build", "--locked", "--manifest-path", "apps/osl-tor-sidecar/Cargo.toml", "--bin", SIDE_CAR, "--target", options.target, ...(options.release ? ["--release"] : [])], { cwd: root });
  await mkdir(join(root, "apps", "osl-hub", "binaries"), { recursive: true });
  await copyFile(source, staged);
  const bytes = (await stat(staged)).size;
  if (bytes === 0) throw new Error(`staged sidecar is empty: ${staged}`);
  console.log(`tor_sidecar_staged=${staged}`);
  console.log(`tor_sidecar_bytes=${bytes}`);
  return { staged, bytes };
}

export async function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  if (options.help) {
    console.log("Usage: CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/i node scripts/stage-tor-sidecar.mjs --target TARGET [--release]");
    return;
  }
  await stageTorSidecar(options);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`sidecar staging failed: ${error.message}`);
    process.exitCode = 1;
  });
}
