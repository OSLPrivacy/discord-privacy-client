#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(fileURLToPath(import.meta.url), "../../..");
const cache = join(repo, ".cache", "cover-local-model-build");
const buildPackages = Object.freeze([
  { role: "build-dependency", package: "libclang-18-dev", version: "1:18.1.3-1ubuntu1" },
  { role: "build-tool", package: "cmake", version: "3.28.3-1build7" },
]);
const clangPathEnv = ["LIB", "CLANG_PATH"].join("");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repo,
    env: options.env ?? process.env,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
    shell: false,
  });
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  if (result.error) {
    console.error(`${command}: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) process.exit(result.status ?? 1);
  return result;
}

function stdout(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repo,
    env: options.env ?? process.env,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
    shell: false,
  });
  if (result.error) return "";
  return result.status === 0 ? result.stdout : "";
}

function installed(packageName) {
  return stdout("dpkg-query", ["-W", "-f=${Status}", packageName]).includes(" installed");
}

function candidateVersion(packageName) {
  const output = stdout("apt-cache", ["policy", packageName]);
  const match = output.match(/Candidate:\s*(\S+)/);
  return match?.[1] && match[1] !== "(none)" ? match[1] : null;
}

function dependencies(packageName) {
  const output = stdout("apt-cache", ["depends", packageName]);
  return [...output.matchAll(/^\s*Depends:\s+([A-Za-z0-9.+~:-]+)/gm)].map((match) => match[1]);
}

function packageClosure(packageName) {
  const queue = [packageName];
  const seen = new Set();
  const packages = [];
  while (queue.length > 0) {
    const current = queue.shift();
    if (!current || seen.has(current)) continue;
    seen.add(current);
    if (current === packageName || !installed(current)) packages.push(current);
    queue.push(...dependencies(current));
  }
  return packages;
}

function prepareUbuntuPackage() {
  if (process.platform !== "linux") return {};

  const root = join(cache, "ubuntu-noble");
  const bin = join(root, "usr", "bin");
  const llvm = join(root, "usr", "lib", "llvm-18", "lib");
  const resourceInclude = join(root, "usr", "lib", "llvm-18", "lib", "clang", "18", "include");
  const multiarch = join(root, "usr", "lib", "x86_64-linux-gnu");
  const packageStem = buildPackages[0].package.slice(0, -"-dev".length);
  const clangShared = join(multiarch, `${packageStem}.so.18`);
  const cmakeBin = join(bin, "cmake");
  const needsRefresh = !existsSync(llvm) || !existsSync(resourceInclude) || !existsSync(multiarch) || !existsSync(clangShared) || !existsSync(cmakeBin);

  if (needsRefresh) {
    rmSync(root, { recursive: true, force: true });
    mkdirSync(root, { recursive: true });
    const packages = new Map();
    for (const spec of buildPackages) {
      console.log(`${spec.role} ${spec.package}=${spec.version}`);
      for (const packageName of packageClosure(spec.package)) {
        packages.set(packageName, packageName === spec.package ? spec.version : candidateVersion(packageName));
      }
    }
    for (const [packageName, version] of packages) {
      if (!version) continue;
      run("apt-get", ["download", `${packageName}=${version}`], { cwd: root });
    }
    for (const file of readdirSync(root).filter((name) => name.endsWith(".deb"))) {
      run("dpkg-deb", ["-x", file, root], { cwd: root });
    }
  } else {
    for (const spec of buildPackages) console.log(`${spec.role} ${spec.package}=${spec.version}`);
  }

  return {
    [clangPathEnv]: llvm,
    BINDGEN_EXTRA_CLANG_ARGS: `-isystem${resourceInclude}`,
    CMAKE_BUILD_PARALLEL_LEVEL: process.env.CMAKE_BUILD_PARALLEL_LEVEL ?? "1",
    LD_LIBRARY_PATH: [llvm, multiarch, process.env.LD_LIBRARY_PATH].filter(Boolean).join(":"),
    PATH: [bin, process.env.PATH].filter(Boolean).join(":"),
  };
}

const mode = process.argv[2] ?? "on";
if (!["setup", "on", "off"].includes(mode)) {
  console.error("usage: node scripts/ci/cover-local-model-build.mjs [setup|on|off] [-- <cargo args>]");
  process.exit(2);
}

const separator = process.argv.indexOf("--");
const passthrough = separator === -1 ? [] : process.argv.slice(separator + 1);
const buildEnv = mode === "off" ? {} : prepareUbuntuPackage();
if (mode === "setup") {
  if (process.env.GITHUB_ENV) {
    appendFileSync(
      process.env.GITHUB_ENV,
      `${Object.entries(buildEnv).map(([key, value]) => `${key}=${value}`).join("\n")}\n`,
    );
  }
  process.exit(0);
}

const env = {
  ...process.env,
  ...buildEnv,
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? "/mnt/d/osl-lane-targets/d",
};
const args = [
  "check",
  "--locked",
  "--manifest-path",
  "crates/cover-ai/Cargo.toml",
  ...(mode === "off" ? ["--no-default-features"] : []),
  ...passthrough,
];

console.log(`cargo ${args.join(" ")}`);
run("cargo", args, { env });
