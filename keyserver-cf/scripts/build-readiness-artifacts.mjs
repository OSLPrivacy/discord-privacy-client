#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, readFile, copyFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import {
  BRIDGE_ALIASES,
  READINESS_MANIFEST_FORMAT,
  sha256,
  verifyReadinessBundle,
} from "./readiness-artifact-contract.mjs";

function usage() {
  throw new Error(
    "usage: node scripts/build-readiness-artifacts.mjs --commit <git-commit> --out-dir <empty-output-directory>",
  );
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || value === undefined) usage();
    parsed[key.slice(2)] = value;
  }
  if (!parsed.commit || !parsed["out-dir"]) usage();
  return { commit: parsed.commit, outDir: path.resolve(parsed["out-dir"]) };
}

function run(file, args, options = {}) {
  return execFileSync(file, args, {
    cwd: options.cwd,
    encoding: options.encoding ?? "utf8",
    env: {
      ...process.env,
      WRANGLER_SEND_METRICS: "false",
    },
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  });
}

function git(repoRoot, args) {
  return run("git", ["-C", repoRoot, ...args]).trim();
}

async function main() {
  const { commit: requestedCommit, outDir } = parseArgs(process.argv.slice(2));
  const repoRoot = git(process.cwd(), ["rev-parse", "--show-toplevel"]);
  const commit = git(repoRoot, ["rev-parse", `${requestedCommit}^{commit}`]);
  const repositoryTree = git(repoRoot, ["rev-parse", `${commit}^{tree}`]);
  const keyserverTree = git(repoRoot, ["rev-parse", `${commit}:keyserver-cf`]);

  await mkdir(outDir, { recursive: false });
  const staging = await mkdtemp(path.join(tmpdir(), "osl-keyserver-readiness-"));
  const archivePath = path.join(staging, "source.tar");
  run("git", [
    "-C",
    repoRoot,
    "archive",
    "--format=tar",
    `--output=${archivePath}`,
    commit,
    "--",
    "keyserver-cf",
  ]);
  const archiveBytes = await readFile(archivePath);
  const archiveSha256 = sha256(archiveBytes);
  const extracted = path.join(staging, "archive");
  await mkdir(extracted);
  run("tar", ["-xf", archivePath, "-C", extracted]);
  const sourceDir = path.join(extracted, "keyserver-cf");
  run("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund"], {
    cwd: sourceDir,
  });
  const wrangler = path.join(sourceDir, "node_modules", ".bin", "wrangler");
  const wranglerVersion = run(wrangler, ["--version"], { cwd: sourceDir })
    .trim()
    .replace(/^.*?(\d+\.\d+\.\d+).*$/s, "$1");

  const commonArgs = [
    "deploy",
    "src/index.ts",
    "--config",
    "wrangler.toml",
    "--dry-run",
    "--minify",
  ];
  const definitions = [
    {
      artifact: "A",
      role: "pre-0031-bridge",
      bundleFile: "artifact-a.bridge.mjs",
      aliases: BRIDGE_ALIASES,
      policy: {
        requires_0031: false,
        forbidden_after_reconciliation: true,
      },
    },
    {
      artifact: "B",
      role: "0031-aware-final",
      bundleFile: "artifact-b.final.mjs",
      aliases: {},
      policy: {
        requires_0031: true,
        forbidden_after_reconciliation: false,
      },
    },
  ];

  const manifests = [];
  for (const definition of definitions) {
    const bundlePath = path.join(outDir, definition.bundleFile);
    const relativeBuildDir = `.readiness-build/artifact-${definition.artifact.toLowerCase()}`;
    const relativeMetafile = `${relativeBuildDir}/meta.json`;
    const args = [
      ...commonArgs,
      "--outdir",
      relativeBuildDir,
      "--metafile",
      relativeMetafile,
    ];
    for (const [from, to] of Object.entries(definition.aliases)) {
      args.push("--alias", `${from}:${to}`);
    }
    run(wrangler, args, { cwd: sourceDir });
    await copyFile(
      path.join(sourceDir, relativeBuildDir, "index.js"),
      bundlePath,
    );
    await copyFile(
      path.join(sourceDir, relativeMetafile),
      path.join(
        outDir,
        definition.bundleFile.replace(/\.mjs$/, ".meta.json"),
      ),
    );
    const bundleBytes = await readFile(bundlePath);
    const manifest = {
      format: READINESS_MANIFEST_FORMAT,
      artifact: definition.artifact,
      role: definition.role,
      source: {
        commit,
        repository_tree: repositoryTree,
        keyserver_tree: keyserverTree,
        archive_sha256: archiveSha256,
      },
      build: {
        entrypoint: "src/index.ts",
        aliases: definition.aliases,
        command: args,
        wrangler_version: wranglerVersion,
        bundle_file: definition.bundleFile,
        bundle_sha256: sha256(bundleBytes),
        bundle_bytes: bundleBytes.byteLength,
      },
      policy: definition.policy,
    };
    verifyReadinessBundle(manifest, bundleBytes);
    const manifestFile = definition.bundleFile.replace(
      /\.mjs$/,
      ".manifest.json",
    );
    await writeFile(
      path.join(outDir, manifestFile),
      `${JSON.stringify(manifest, null, 2)}\n`,
      { flag: "wx" },
    );
    manifests.push({ manifest: manifestFile, ...manifest });
  }

  await copyFile(archivePath, path.join(outDir, "source.tar"));
  const index = {
    format: "osl.keyserver.readiness-archive.v1",
    source: {
      commit,
      repository_tree: repositoryTree,
      keyserver_tree: keyserverTree,
      archive_sha256: archiveSha256,
      archive_file: "source.tar",
    },
    artifacts: manifests.map(({ manifest, artifact, role, build, policy }) => ({
      manifest,
      artifact,
      role,
      bundle_file: build.bundle_file,
      bundle_sha256: build.bundle_sha256,
      policy,
    })),
  };
  await writeFile(
    path.join(outDir, "readiness-archive.json"),
    `${JSON.stringify(index, null, 2)}\n`,
    { flag: "wx" },
  );
  process.stdout.write(`${JSON.stringify(index, null, 2)}\n`);
}

main().catch((error) => {
  process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
  process.exitCode = 1;
});
