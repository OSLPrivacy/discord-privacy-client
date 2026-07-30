#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import {
  copyFile,
  mkdir,
  mkdtemp,
  readFile,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import {
  ARTIFACT_DEFINITIONS,
  READINESS_ARCHIVE_FORMAT,
  READINESS_MANIFEST_FORMAT,
  READINESS_TOOL_PATHS,
  readinessArchiveId,
  sha256,
  validateReadinessArchiveIndex,
  validateReadinessMetafile,
  verifyReadinessBundle,
} from "./readiness-artifact-contract.mjs";

const BUILDER_PATH = READINESS_TOOL_PATHS.builder;

function usage() {
  throw new Error(
    "usage: node scripts/build-readiness-artifacts.mjs --commit <full-git-commit> --out-dir <new-output-directory>",
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
  if (!parsed.commit || !parsed["out-dir"] || Object.keys(parsed).length !== 2) {
    usage();
  }
  return { commit: parsed.commit, outDir: path.resolve(parsed["out-dir"]) };
}

function run(file, args, options = {}) {
  return execFileSync(file, args, {
    cwd: options.cwd,
    encoding: Object.hasOwn(options, "encoding") ? options.encoding : "utf8",
    env: {
      ...process.env,
      WRANGLER_SEND_METRICS: "false",
    },
    stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
  });
}

function git(repoRoot, args, options = {}) {
  return run("git", ["-C", repoRoot, ...args], options);
}

function inside(parent, candidate) {
  const relative = path.relative(parent, candidate);
  return relative === "" ||
    (!relative.startsWith(`..${path.sep}`) && relative !== ".." &&
      !path.isAbsolute(relative));
}

async function exactTool(repoRoot, commit, name, expectedPath) {
  const diskPath = path.join(repoRoot, expectedPath);
  const diskBytes = await readFile(diskPath);
  const objectBytes = git(
    repoRoot,
    ["show", `${commit}:${expectedPath}`],
    { encoding: null },
  );
  if (!diskBytes.equals(objectBytes)) {
    throw new Error(`${name} source differs from expected commit`);
  }
  return {
    path: expectedPath,
    sha256: sha256(objectBytes),
    bytes: objectBytes.byteLength,
  };
}

export async function resolveCleanBuildSource(
  repoRoot,
  requestedCommit,
  outDir,
) {
  if (!/^[0-9a-f]{40}$/.test(requestedCommit)) {
    throw new Error("expected commit must be a full 40-character object id");
  }
  const commit = git(repoRoot, [
    "rev-parse",
    "--verify",
    `${requestedCommit}^{commit}`,
  ]).trim();
  if (commit !== requestedCommit) {
    throw new Error("expected commit does not resolve to itself");
  }
  const head = git(repoRoot, ["rev-parse", "HEAD"]).trim();
  if (head !== commit) {
    throw new Error("expected commit is not the checked-out HEAD");
  }
  const dirty = git(repoRoot, [
    "status",
    "--porcelain=v1",
    "--untracked-files=all",
  ]).trim();
  if (dirty !== "") {
    throw new Error("readiness build requires a completely clean checkout");
  }
  if (inside(repoRoot, outDir)) {
    throw new Error("readiness output directory must be outside the repository");
  }
  const repositoryTree = git(repoRoot, [
    "rev-parse",
    `${commit}^{tree}`,
  ]).trim();
  const keyserverTree = git(repoRoot, [
    "rev-parse",
    `${commit}:keyserver-cf`,
  ]).trim();
  const toolchain = {
    builder: await exactTool(
      repoRoot,
      commit,
      "builder",
      READINESS_TOOL_PATHS.builder,
    ),
    verifier: await exactTool(
      repoRoot,
      commit,
      "verifier",
      READINESS_TOOL_PATHS.verifier,
    ),
    contract: await exactTool(
      repoRoot,
      commit,
      "contract",
      READINESS_TOOL_PATHS.contract,
    ),
    clean_checkout_required: true,
  };
  return { commit, repositoryTree, keyserverTree, toolchain };
}

export async function buildReadinessArtifacts({
  repoRoot,
  requestedCommit,
  outDir,
}) {
  const {
    commit,
    repositoryTree,
    keyserverTree,
    toolchain,
  } = await resolveCleanBuildSource(repoRoot, requestedCommit, outDir);

  await mkdir(outDir, { recursive: false });
  const staging = await mkdtemp(path.join(tmpdir(), "osl-keyserver-readiness-"));
  const archivePath = path.join(staging, "source.tar");
  git(repoRoot, [
    "archive",
    "--format=tar",
    `--output=${archivePath}`,
    commit,
    "--",
    "keyserver-cf",
  ]);
  const archiveBytes = await readFile(archivePath);
  const source = {
    commit,
    repository_tree: repositoryTree,
    keyserver_tree: keyserverTree,
    archive_file: "source.tar",
    archive_sha256: sha256(archiveBytes),
    archive_bytes: archiveBytes.byteLength,
  };
  const archiveId = readinessArchiveId(source, toolchain);
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
  const artifactRecords = [];
  for (const definition of Object.values(ARTIFACT_DEFINITIONS)) {
    const relativeBuildDir =
      `.readiness-build/artifact-${definition.artifact.toLowerCase()}`;
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

    const bundleBytes = await readFile(
      path.join(sourceDir, relativeBuildDir, "index.js"),
    );
    const metafileBytes = await readFile(
      path.join(sourceDir, relativeMetafile),
    );
    validateReadinessMetafile(
      definition.artifact,
      JSON.parse(metafileBytes.toString("utf8")),
    );
    const manifest = {
      format: READINESS_MANIFEST_FORMAT,
      archive_id: archiveId,
      artifact: definition.artifact,
      role: definition.role,
      source,
      build: {
        entrypoint: "src/index.ts",
        aliases: definition.aliases,
        command: args,
        wrangler_version: wranglerVersion,
        bundle_file: definition.bundle_file,
        bundle_sha256: sha256(bundleBytes),
        bundle_bytes: bundleBytes.byteLength,
        metafile_file: definition.metafile_file,
        metafile_sha256: sha256(metafileBytes),
        metafile_bytes: metafileBytes.byteLength,
      },
      policy: {
        requires_0031: definition.requires_0031,
        forbidden_after_reconciliation:
          definition.forbidden_after_reconciliation,
      },
    };
    verifyReadinessBundle(manifest, bundleBytes);
    const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
    await writeFile(
      path.join(outDir, definition.bundle_file),
      bundleBytes,
      { flag: "wx" },
    );
    await writeFile(
      path.join(outDir, definition.metafile_file),
      metafileBytes,
      { flag: "wx" },
    );
    await writeFile(
      path.join(outDir, definition.manifest_file),
      manifestBytes,
      { flag: "wx" },
    );
    artifactRecords.push({
      artifact: definition.artifact,
      role: definition.role,
      manifest_file: definition.manifest_file,
      manifest_sha256: sha256(manifestBytes),
      manifest_bytes: manifestBytes.byteLength,
      bundle_file: definition.bundle_file,
      bundle_sha256: manifest.build.bundle_sha256,
      bundle_bytes: manifest.build.bundle_bytes,
      metafile_file: definition.metafile_file,
      metafile_sha256: manifest.build.metafile_sha256,
      metafile_bytes: manifest.build.metafile_bytes,
      policy: manifest.policy,
    });
  }

  await copyFile(archivePath, path.join(outDir, "source.tar"));
  const index = {
    format: READINESS_ARCHIVE_FORMAT,
    archive_id: archiveId,
    source,
    toolchain,
    artifacts: artifactRecords,
  };
  validateReadinessArchiveIndex(index);
  await writeFile(
    path.join(outDir, "readiness-archive.json"),
    `${JSON.stringify(index, null, 2)}\n`,
    { flag: "wx" },
  );
  return index;
}

async function main() {
  const { commit, outDir } = parseArgs(process.argv.slice(2));
  const repoRoot = git(process.cwd(), ["rev-parse", "--show-toplevel"]).trim();
  const index = await buildReadinessArtifacts({
    repoRoot,
    requestedCommit: commit,
    outDir,
  });
  process.stdout.write(`${JSON.stringify(index, null, 2)}\n`);
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
