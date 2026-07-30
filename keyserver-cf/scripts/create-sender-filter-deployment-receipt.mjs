#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import {
  createSenderFilterDeploymentReceipt,
  SENDER_FILTER_LIVE_NULL_FIXTURE,
  SENDER_FILTER_SOURCE_FILES,
} from "./sender-filter-deployment-contract.mjs";

function usage() {
  throw new Error(
    "usage: node scripts/create-sender-filter-deployment-receipt.mjs " +
      "--expected-commit <full-current-git-commit>",
  );
}

export function parseSenderFilterReceiptArgs(argv) {
  if (
    argv.length !== 2 ||
    argv[0] !== "--expected-commit" ||
    !/^[0-9a-f]{40}$/.test(argv[1] ?? "")
  ) {
    usage();
  }
  return { expectedCommit: argv[1] };
}

function git(repoRoot, args, encoding = "utf8") {
  return execFileSync("git", ["-C", repoRoot, ...args], {
    encoding,
    maxBuffer: 16 * 1024 * 1024,
  });
}

export function loadSenderFilterReceiptInputs(
  repoRoot,
  expectedCommit,
  gitRun = git,
) {
  const resolved = gitRun(
    repoRoot,
    ["rev-parse", "--verify", `${expectedCommit}^{commit}`],
  ).trim();
  if (resolved !== expectedCommit) {
    throw new Error("expected worker commit did not resolve exactly");
  }
  const head = gitRun(repoRoot, ["rev-parse", "HEAD"]).trim();
  if (head !== expectedCommit) {
    throw new Error("expected worker commit is stale relative to HEAD");
  }
  const repositoryTree = gitRun(
    repoRoot,
    ["rev-parse", `${expectedCommit}^{tree}`],
  ).trim();
  const keyserverTree = gitRun(
    repoRoot,
    ["rev-parse", `${expectedCommit}:keyserver-cf`],
  ).trim();
  const fileValues = {};
  for (const sourcePath of Object.keys(SENDER_FILTER_SOURCE_FILES)) {
    fileValues[sourcePath] = gitRun(
      repoRoot,
      ["show", `${expectedCommit}:${sourcePath}`],
      null,
    );
  }
  const evidenceBytes = gitRun(
    repoRoot,
    ["show", `${expectedCommit}:${SENDER_FILTER_LIVE_NULL_FIXTURE}`],
    null,
  );
  let liveEvidence;
  try {
    liveEvidence = JSON.parse(evidenceBytes.toString("utf8"));
  } catch {
    throw new Error("committed live-null evidence is not JSON");
  }
  return {
    anchor: {
      commit: expectedCommit,
      repository_tree: repositoryTree,
      keyserver_tree: keyserverTree,
    },
    fileValues,
    liveEvidence,
  };
}

export function runSenderFilterReceiptCli(
  argv,
  dependencies = {},
) {
  const { expectedCommit } = parseSenderFilterReceiptArgs(argv);
  const repoRoot =
    dependencies.repoRoot ??
    path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
  const loadInputs =
    dependencies.loadInputs ??
    ((commit) => loadSenderFilterReceiptInputs(repoRoot, commit));
  const write = dependencies.write ?? ((text) => process.stdout.write(text));
  const now = dependencies.now ?? (() => Date.now());
  const inputs = loadInputs(expectedCommit);
  const receipt = createSenderFilterDeploymentReceipt({
    ...inputs,
    nowMs: now(),
  });
  write(`${JSON.stringify(receipt, null, 2)}\n`);
  return receipt;
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  try {
    runSenderFilterReceiptCli(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  }
}
