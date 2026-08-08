#!/usr/bin/env node
/// Require an observed keyserver revision transition around a production deploy.
///
/// Usage:
///   node scripts/release-deploy-live-revision.mjs --host https://keyserver.example \
///     --record docs/evidence/keyserver-deploy.json -- npx wrangler deploy
///
/// The deploy command is deliberately supplied after `--`: this wrapper never
/// owns credentials or decides what production action is admitted.  It only
/// makes the release evidence fail closed unless the public health endpoint
/// reports a complete, changed live revision on both sides of that action.

import { spawnSync } from "node:child_process";
import { writeFile } from "node:fs/promises";

const RECORD_FORMAT = "osl.keyserver.release-deploy-live-revision.v1";
const REQUIRED_REPORT_FIELDS = ["revision", "build_time", "configuration_name"];

function usage() {
  return [
    "usage: node scripts/release-deploy-live-revision.mjs --host https://<keyserver> --record <path> -- <deploy-command> [args...]",
    "example: node scripts/release-deploy-live-revision.mjs --host https://keyserver.example --record docs/evidence/keyserver-deploy.json -- npx wrangler deploy",
  ].join("\n");
}

function parseArgs(args) {
  const separator = args.indexOf("--");
  if (separator < 0) throw new Error("deploy command is required after --");
  const options = args.slice(0, separator);
  const command = args.slice(separator + 1);
  let host = "";
  let recordPath = "";
  for (let index = 0; index < options.length; index += 1) {
    const option = options[index];
    if (option === "--host") host = options[++index] ?? "";
    else if (option === "--record") recordPath = options[++index] ?? "";
    else throw new Error(`unknown option: ${option}`);
  }
  if (!host) throw new Error("--host is required");
  if (!recordPath) throw new Error("--record is required");
  if (command.length === 0) throw new Error("deploy command is required after --");
  return { host: host.replace(/\/$/, ""), recordPath, command };
}

function requireLiveReport(body, stage) {
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new Error(`${stage} live report is not a JSON object`);
  }
  const report = {};
  for (const field of REQUIRED_REPORT_FIELDS) {
    const value = body[field];
    if (typeof value !== "string" || value.trim() === "") {
      throw new Error(`${field} is missing from ${stage} live report`);
    }
    report[field] = value.trim();
  }
  return report;
}

async function readLiveRevision(host, stage, { fetchImpl, now }) {
  const response = await fetchImpl(`${host}/v1/healthz`, {
    headers: { "cache-control": "no-cache" },
  });
  if (!response || !response.ok) {
    throw new Error(`${stage} live revision request failed with status ${response?.status ?? "unknown"}`);
  }
  let body;
  try {
    body = await response.json();
  } catch {
    throw new Error(`${stage} live report is not JSON`);
  }
  return {
    stage,
    read_at: new Date(now()).toISOString(),
    ...requireLiveReport(body, stage),
  };
}

function runDeploy(command, spawnSyncImpl) {
  const result = spawnSyncImpl(command[0], command.slice(1), { stdio: "inherit" });
  if (result.error) throw new Error(`server deploy could not start: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`server deploy failed with exit ${result.status ?? "unknown"}`);
}

export async function runReleaseDeployLiveRevisionCheck(options, dependencies = {}) {
  const fetchImpl = dependencies.fetchImpl ?? fetch;
  const now = dependencies.now ?? Date.now;
  const spawnSyncImpl = dependencies.spawnSyncImpl ?? spawnSync;
  const writeFileImpl = dependencies.writeFileImpl ?? writeFile;
  const before = await readLiveRevision(options.host, "before-deploy", { fetchImpl, now });
  runDeploy(options.command, spawnSyncImpl);
  const after = await readLiveRevision(options.host, "after-deploy", { fetchImpl, now });
  if (before.revision === after.revision) {
    throw new Error(`live revision did not change after deploy: ${before.revision}`);
  }
  const record = {
    format: RECORD_FORMAT,
    deploy_command: options.command,
    revision_reads: [before, after],
  };
  await writeFileImpl(options.recordPath, `${JSON.stringify(record, null, 2)}\n`, "utf8");
  return record;
}

export { parseArgs, RECORD_FORMAT };

async function main() {
  try {
    const options = parseArgs(process.argv.slice(2));
    const record = await runReleaseDeployLiveRevisionCheck(options);
    console.log(`PASS release deploy record=${options.recordPath} before=${record.revision_reads[0].revision} after=${record.revision_reads[1].revision}`);
  } catch (error) {
    console.error(`FAIL release deploy live revision check: ${error.message}`);
    console.error(usage());
    process.exitCode = 1;
  }
}

if (import.meta.url === new URL(process.argv[1], "file:").href) {
  await main();
}
