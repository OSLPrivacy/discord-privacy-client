#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import {
  ARTIFACT_DEFINITIONS,
  READINESS_FILES,
  READINESS_TOOL_PATHS,
  readAndVerifyReadinessArtifact,
  sha256,
  validateReadinessArchiveIndex,
  validateReadinessSelection,
} from "./readiness-artifact-contract.mjs";
import {
  buildReadinessArtifacts,
  resolveCleanBuildSource,
} from "./build-readiness-artifacts.mjs";

export const ADMISSION_FORMAT = "osl.keyserver.readiness-admission.v1";
export const READINESS_WORKER = "oslprivacy-keyserver";
export const READINESS_DATABASE = "osl-keyserver-prod";
export const READINESS_DATABASE_ID = "1de837cd-3bf6-4d33-be82-12d358523600";
export const READINESS_ENVIRONMENT = "production";
export const READINESS_CAPTURE_MAX_MS = 120_000;
export const READINESS_DATABASE_CLOCK_SKEW_MS = 10_000;

export const READINESS_SCHEMA_QUERY = `SELECT
  'osl-readiness-schema-v1' AS query_id,
  unixepoch() AS database_unix_time,
  EXISTS(
    SELECT 1 FROM sqlite_master
     WHERE type = 'table' AND name = 'worker_schema_capabilities'
  ) AS capability_table_exists`;

export const READINESS_MARKERS_QUERY = `SELECT
  'osl-readiness-markers-v1' AS query_id,
  unixepoch() AS database_unix_time,
  MAX(CASE
        WHEN capability = 'control_inbox_sender_disposition'
        THEN version
      END) AS control_inbox_sender_disposition,
  MAX(CASE
        WHEN capability = 'control_inbox_sender_reconciliation_started'
        THEN version
      END) AS control_inbox_sender_reconciliation_started
FROM worker_schema_capabilities
WHERE capability IN (
  'control_inbox_sender_disposition',
  'control_inbox_sender_reconciliation_started'
)`;

const EXPECTED_FLAGS = Object.freeze([
  "--expected-commit",
  "--archive-dir",
  "--artifact",
  "--expected-active-version",
]);

function usage() {
  throw new Error(
    "usage: node scripts/admit-readiness-archive.mjs " +
      "--expected-commit <full-git-commit> --archive-dir <archive-directory> " +
      "--artifact <A|B> --expected-active-version <worker-version-uuid>",
  );
}

export function parseAdmissionArgs(argv) {
  if (argv.length !== EXPECTED_FLAGS.length * 2) usage();
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!EXPECTED_FLAGS.includes(flag) || value === undefined ||
        Object.hasOwn(values, flag)) {
      usage();
    }
    values[flag] = value;
  }
  const expectedCommit = values["--expected-commit"];
  const artifact = values["--artifact"];
  const expectedActiveVersion = values["--expected-active-version"];
  if (!/^[0-9a-f]{40}$/.test(expectedCommit)) {
    throw new Error("expected commit must be a full 40-character object id");
  }
  if (!ARTIFACT_DEFINITIONS[artifact]) {
    throw new Error("artifact must be exactly A or B");
  }
  if (
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
      expectedActiveVersion,
    )
  ) {
    throw new Error("expected active Worker version must be a UUID");
  }
  return {
    expectedCommit,
    archiveDir: path.resolve(values["--archive-dir"]),
    artifact,
    expectedActiveVersion,
  };
}

function run(file, args, options = {}) {
  try {
    return execFileSync(file, args, {
      cwd: options.cwd,
      encoding: Object.hasOwn(options, "encoding") ? options.encoding : "utf8",
      env: {
        ...process.env,
        WRANGLER_SEND_METRICS: "false",
      },
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 16 * 1024 * 1024,
    });
  } catch (error) {
    const detail =
      error && typeof error === "object" && "stderr" in error
        ? String(error.stderr).trim().split("\n").at(-1)
        : "";
    throw new Error(
      `trusted command failed: ${path.basename(file)} ${args[0] ?? ""}` +
        (detail ? ` (${detail})` : ""),
    );
  }
}

function git(repoRoot, args, options = {}) {
  return run("git", ["-C", repoRoot, ...args], options);
}

async function exactDirectoryFiles(directory) {
  const names = (await readdir(directory)).sort();
  const expected = [...READINESS_FILES].sort();
  if (JSON.stringify(names) !== JSON.stringify(expected)) {
    throw new Error("readiness archive file set is not exact");
  }
  for (const name of names) {
    const info = await lstat(path.join(directory, name));
    if (!info.isFile() || info.isSymbolicLink()) {
      throw new Error(`readiness archive member is not a regular file: ${name}`);
    }
  }
}

function requireSameSource(actual, expected, label) {
  for (const field of [
    "commit",
    "repository_tree",
    "keyserver_tree",
    "archive_file",
    "archive_sha256",
    "archive_bytes",
  ]) {
    if (actual[field] !== expected[field]) {
      throw new Error(`${label} source ${field} does not match archive index`);
    }
  }
}

function requireFileDigest(bytes, digest, count, label) {
  if (bytes.byteLength !== count) {
    throw new Error(`${label} byte count does not match archive index`);
  }
  if (sha256(bytes) !== digest) {
    throw new Error(`${label} SHA-256 does not match archive index`);
  }
}

export async function verifyArchiveDirectory(directory, expectedAnchor) {
  await exactDirectoryFiles(directory);
  const indexBytes = await readFile(
    path.join(directory, "readiness-archive.json"),
  );
  const index = validateReadinessArchiveIndex(
    JSON.parse(indexBytes.toString("utf8")),
  );
  if (
    index.source.commit !== expectedAnchor.commit ||
    index.source.repository_tree !== expectedAnchor.repositoryTree ||
    index.source.keyserver_tree !== expectedAnchor.keyserverTree
  ) {
    throw new Error("archive source does not match the expected Git objects");
  }
  for (const name of ["builder", "verifier", "contract"]) {
    const expected = expectedAnchor.toolchain[name];
    const actual = index.toolchain[name];
    if (
      actual.path !== expected.path ||
      actual.sha256 !== expected.sha256 ||
      actual.bytes !== expected.bytes
    ) {
      throw new Error(`${name} source is not pinned to the expected commit`);
    }
  }
  const sourceTar = await readFile(path.join(directory, "source.tar"));
  requireFileDigest(
    sourceTar,
    index.source.archive_sha256,
    index.source.archive_bytes,
    "source archive",
  );

  const manifests = {};
  for (const entry of index.artifacts) {
    const manifestBytes = await readFile(
      path.join(directory, entry.manifest_file),
    );
    const bundleBytes = await readFile(
      path.join(directory, entry.bundle_file),
    );
    const metafileBytes = await readFile(
      path.join(directory, entry.metafile_file),
    );
    requireFileDigest(
      manifestBytes,
      entry.manifest_sha256,
      entry.manifest_bytes,
      `${entry.artifact} manifest`,
    );
    requireFileDigest(
      bundleBytes,
      entry.bundle_sha256,
      entry.bundle_bytes,
      `${entry.artifact} bundle`,
    );
    requireFileDigest(
      metafileBytes,
      entry.metafile_sha256,
      entry.metafile_bytes,
      `${entry.artifact} metafile`,
    );
    const manifest = await readAndVerifyReadinessArtifact(
      path.join(directory, entry.manifest_file),
      path.join(directory, entry.bundle_file),
      path.join(directory, entry.metafile_file),
      index.archive_id,
    );
    requireSameSource(manifest.source, index.source, entry.artifact);
    if (
      manifest.role !== entry.role ||
      manifest.build.bundle_sha256 !== entry.bundle_sha256 ||
      manifest.build.bundle_bytes !== entry.bundle_bytes ||
      manifest.build.metafile_sha256 !== entry.metafile_sha256 ||
      manifest.build.metafile_bytes !== entry.metafile_bytes ||
      manifest.build.bundle_file !== entry.bundle_file ||
      manifest.build.metafile_file !== entry.metafile_file
    ) {
      throw new Error(`${entry.artifact} manifest is not cross-linked`);
    }
    run(process.execPath, ["--check", path.join(directory, entry.bundle_file)]);
    manifests[entry.artifact] = manifest;
  }
  return { index, manifests };
}

export async function compareArchiveToTrustedRebuild(
  candidateDir,
  trustedDir,
  expectedAnchor,
) {
  const candidate = await verifyArchiveDirectory(candidateDir, expectedAnchor);
  await verifyArchiveDirectory(trustedDir, expectedAnchor);
  for (const name of READINESS_FILES) {
    const candidateBytes = await readFile(path.join(candidateDir, name));
    const trustedBytes = await readFile(path.join(trustedDir, name));
    if (!candidateBytes.equals(trustedBytes)) {
      throw new Error(
        `candidate archive differs from trusted rebuild: ${name}`,
      );
    }
  }
  return candidate;
}

function resultRows(value) {
  const containers = Array.isArray(value) ? value : [value];
  const rows = [];
  for (const container of containers) {
    if (!container || typeof container !== "object") {
      throw new Error("D1 JSON result is malformed");
    }
    if (container.success === false) {
      throw new Error("D1 read-only query did not succeed");
    }
    if (Array.isArray(container.results)) rows.push(...container.results);
    if (container.result && Array.isArray(container.result.results)) {
      rows.push(...container.result.results);
    }
  }
  return rows;
}

function oneRow(value, queryId) {
  const rows = resultRows(value);
  if (rows.length !== 1 || rows[0]?.query_id !== queryId) {
    throw new Error(`D1 ${queryId} result is not exactly one bound row`);
  }
  return rows[0];
}

function exactMarker(value, label) {
  if (value === null || value === 1) return value;
  throw new Error(`${label} must be exactly 1 or null`);
}

export function validateCapturedEvidence(
  evidence,
  expectedActiveVersion,
  nowMs = Date.now(),
) {
  if (!evidence || typeof evidence !== "object" || Array.isArray(evidence)) {
    throw new Error("captured evidence is malformed");
  }
  const expectedFields = [
    "database",
    "database_id",
    "environment",
    "schema_query_sha256",
    "markers_query_sha256",
    "schema_output_sha256",
    "markers_output_sha256",
    "deployment_status_before_sha256",
    "deployment_status_after_sha256",
    "captured_started_at",
    "captured_finished_at",
    "database_unix_time",
    "deployment_id",
    "deployment_created_on",
    "active_worker_version",
    "active_worker_percentage",
    "capability_table_exists",
    "control_inbox_sender_disposition",
    "control_inbox_sender_reconciliation_started",
  ].sort();
  if (
    JSON.stringify(Object.keys(evidence).sort()) !==
    JSON.stringify(expectedFields)
  ) {
    throw new Error("captured evidence fields are not exact");
  }
  if (
    evidence.database !== READINESS_DATABASE ||
    evidence.database_id !== READINESS_DATABASE_ID ||
    evidence.environment !== READINESS_ENVIRONMENT
  ) {
    throw new Error("captured database/environment is not exact");
  }
  if (
    evidence.schema_query_sha256 !== sha256(Buffer.from(READINESS_SCHEMA_QUERY)) ||
    evidence.markers_query_sha256 !==
      sha256(Buffer.from(READINESS_MARKERS_QUERY))
  ) {
    throw new Error("read-only query provenance is not exact");
  }
  if (
    !/^[0-9a-f]{64}$/.test(evidence.schema_output_sha256) ||
    !(
      evidence.markers_output_sha256 === null ||
      /^[0-9a-f]{64}$/.test(evidence.markers_output_sha256)
    )
  ) {
    throw new Error("read-only output provenance is malformed");
  }
  if (
    !/^[0-9a-f]{64}$/.test(evidence.deployment_status_before_sha256) ||
    !/^[0-9a-f]{64}$/.test(evidence.deployment_status_after_sha256)
  ) {
    throw new Error("deployment status provenance is malformed");
  }
  const started = Date.parse(evidence.captured_started_at);
  const finished = Date.parse(evidence.captured_finished_at);
  if (
    !Number.isFinite(started) ||
    !Number.isFinite(finished) ||
    finished < started ||
    finished - started > READINESS_CAPTURE_MAX_MS ||
    nowMs - finished < 0 ||
    nowMs - finished > READINESS_CAPTURE_MAX_MS
  ) {
    throw new Error("read-only evidence capture is stale or incoherent");
  }
  if (
    !Number.isSafeInteger(evidence.database_unix_time) ||
    evidence.database_unix_time * 1000 <
      started - READINESS_DATABASE_CLOCK_SKEW_MS ||
    evidence.database_unix_time * 1000 >
      finished + READINESS_DATABASE_CLOCK_SKEW_MS
  ) {
    throw new Error("D1 timestamp is stale or outside the capture");
  }
  if (
    evidence.active_worker_version !== expectedActiveVersion ||
    evidence.active_worker_percentage !== 100
  ) {
    throw new Error("active Worker version changed or is not at 100%");
  }
  if (
    typeof evidence.deployment_id !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
      evidence.deployment_id,
    ) ||
    !Number.isFinite(Date.parse(evidence.deployment_created_on)) ||
    Date.parse(evidence.deployment_created_on) >
      finished + READINESS_DATABASE_CLOCK_SKEW_MS
  ) {
    throw new Error("active deployment provenance is malformed");
  }
  if (
    evidence.capability_table_exists !== 0 &&
    evidence.capability_table_exists !== 1
  ) {
    throw new Error("capability table evidence must be exactly 0 or 1");
  }
  const markers = {
    control_inbox_sender_disposition: exactMarker(
      evidence.control_inbox_sender_disposition,
      "disposition marker",
    ),
    control_inbox_sender_reconciliation_started: exactMarker(
      evidence.control_inbox_sender_reconciliation_started,
      "reconciliation marker",
    ),
  };
  if (
    evidence.capability_table_exists === 0 &&
    (markers.control_inbox_sender_disposition !== null ||
      markers.control_inbox_sender_reconciliation_started !== null)
  ) {
    throw new Error("markers exist while capability table is absent");
  }
  if (
    evidence.capability_table_exists === 0 &&
    evidence.markers_output_sha256 !== null
  ) {
    throw new Error("marker output exists while capability table is absent");
  }
  if (
    evidence.capability_table_exists === 1 &&
    evidence.markers_output_sha256 === null
  ) {
    throw new Error("marker output is absent while capability table exists");
  }
  return markers;
}

function parseJsonOutput(output, label) {
  try {
    return JSON.parse(output);
  } catch {
    throw new Error(`${label} did not return JSON`);
  }
}

export function activeDeployment(status, expectedActiveVersion) {
  if (!status || typeof status !== "object" || Array.isArray(status)) {
    throw new Error("deployment status is malformed");
  }
  if (
    !Array.isArray(status.versions) ||
    status.versions.length === 0 ||
    status.versions.some(
      (version) =>
        !version ||
        typeof version !== "object" ||
        typeof version.version_id !== "string" ||
        !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
          version.version_id,
        ) ||
        !Number.isFinite(version.percentage) ||
        version.percentage < 0 ||
        version.percentage > 100,
    ) ||
    status.versions.reduce(
      (total, version) => total + version.percentage,
      0,
    ) !== 100
  ) {
    throw new Error("deployment traffic allocation is malformed");
  }
  const active = status.versions.filter(
    (version) => version.percentage === 100,
  );
  if (active.length !== 1) {
    throw new Error("production does not have exactly one 100% active version");
  }
  if (active[0].version_id !== expectedActiveVersion) {
    throw new Error("active Worker version differs from operator expectation");
  }
  if (
    typeof status.id !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
      status.id,
    ) ||
    !Number.isFinite(Date.parse(status.created_on))
  ) {
    throw new Error("active deployment provenance is malformed");
  }
  return {
    deployment_id: status.id,
    deployment_created_on: status.created_on,
    active_worker_version: active[0].version_id,
    active_worker_percentage: active[0].percentage,
  };
}

export function requireStableDeployment(before, after) {
  for (const field of [
    "deployment_id",
    "deployment_created_on",
    "active_worker_version",
    "active_worker_percentage",
  ]) {
    if (before[field] !== after[field]) {
      throw new Error("active Worker deployment changed during D1 capture");
    }
  }
  return before;
}

function configSections(config) {
  const sections = new Map([["", []]]);
  let current = "";
  for (const line of config.split(/\r?\n/)) {
    const header = line.match(/^\s*(\[\[?[^\]]+\]\]?)\s*(?:#.*)?$/);
    if (header) {
      current = header[1];
      if (!sections.has(current)) sections.set(current, []);
      sections.get(current).push([]);
    } else {
      const groups = sections.get(current);
      const group = Array.isArray(groups.at(-1)) ? groups.at(-1) : groups;
      group.push(line);
    }
  }
  return sections;
}

function exactConfigValue(lines, key, label) {
  const matches = lines
    .map((line) =>
      line.match(new RegExp(`^\\s*${key}\\s*=\\s*"([^"]*)"\\s*(?:#.*)?$`)),
    )
    .filter(Boolean);
  if (matches.length !== 1) {
    throw new Error(`expected commit does not pin ${label}`);
  }
  return matches[0][1];
}

export function validateProductionConfig(config) {
  const sections = configSections(config);
  const preamble = sections.get("");
  const vars = sections.get("[vars]");
  const databases = sections.get("[[d1_databases]]");
  if (
    !Array.isArray(preamble) ||
    !Array.isArray(vars) ||
    vars.length !== 1 ||
    !Array.isArray(databases) ||
    databases.length !== 1 ||
    exactConfigValue(preamble, "name", "production Worker name") !==
      READINESS_WORKER ||
    exactConfigValue(vars[0], "DEPLOYMENT_ENV", "production environment") !==
      READINESS_ENVIRONMENT ||
    exactConfigValue(databases[0], "binding", "production D1 binding") !== "DB" ||
    exactConfigValue(databases[0], "database_name", "production D1 name") !==
      READINESS_DATABASE ||
    exactConfigValue(databases[0], "database_id", "production D1 id") !==
      READINESS_DATABASE_ID
  ) {
    throw new Error("expected commit does not pin production Worker/D1 identity");
  }
  return true;
}

export async function captureReadOnlyEvidence({
  sourceTar,
  expectedActiveVersion,
  now = () => Date.now(),
}) {
  const staging = await mkdtemp(path.join(tmpdir(), "osl-readiness-evidence-"));
  await mkdir(path.join(staging, "source"));
  run("tar", ["-xf", sourceTar, "-C", path.join(staging, "source")]);
  const keyserverDir = path.join(staging, "source", "keyserver-cf");
  run("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund"], {
    cwd: keyserverDir,
  });
  const wrangler = path.join(
    keyserverDir,
    "node_modules",
    "wrangler",
    "bin",
    "wrangler.js",
  );
  const startedMs = now();
  const statusOutput = run(
    process.execPath,
    [wrangler, "deployments", "status", "--json", "--config", "wrangler.toml"],
    { cwd: keyserverDir },
  );
  const deploymentBefore = activeDeployment(
    parseJsonOutput(statusOutput, "deployment status"),
    expectedActiveVersion,
  );
  const d1Base = [
    wrangler,
    "d1",
    "execute",
    READINESS_DATABASE,
    "--remote",
    "--json",
    "--config",
    "wrangler.toml",
    "--command",
  ];
  const schemaRaw = run(
    process.execPath,
    [...d1Base, READINESS_SCHEMA_QUERY],
    { cwd: keyserverDir },
  );
  const schema = oneRow(
    parseJsonOutput(schemaRaw, "schema query"),
    "osl-readiness-schema-v1",
  );
  const tableExists = Number(schema.capability_table_exists);
  let disposition = null;
  let reconciliation = null;
  let databaseUnixTime = Number(schema.database_unix_time);
  let markersRawSha256 = null;
  if (tableExists === 1) {
    const markersRaw = run(
      process.execPath,
      [...d1Base, READINESS_MARKERS_QUERY],
      { cwd: keyserverDir },
    );
    const markers = oneRow(
      parseJsonOutput(markersRaw, "markers query"),
      "osl-readiness-markers-v1",
    );
    disposition = markers.control_inbox_sender_disposition;
    reconciliation = markers.control_inbox_sender_reconciliation_started;
    databaseUnixTime = Math.max(
      databaseUnixTime,
      Number(markers.database_unix_time),
    );
    markersRawSha256 = sha256(Buffer.from(markersRaw));
  } else if (tableExists !== 0) {
    throw new Error("schema query returned a non-boolean table result");
  }
  const statusAfterOutput = run(
    process.execPath,
    [wrangler, "deployments", "status", "--json", "--config", "wrangler.toml"],
    { cwd: keyserverDir },
  );
  const deploymentAfter = activeDeployment(
    parseJsonOutput(statusAfterOutput, "deployment status recheck"),
    expectedActiveVersion,
  );
  const deployment = requireStableDeployment(
    deploymentBefore,
    deploymentAfter,
  );
  const finishedMs = now();
  const evidence = {
    database: READINESS_DATABASE,
    database_id: READINESS_DATABASE_ID,
    environment: READINESS_ENVIRONMENT,
    schema_query_sha256: sha256(Buffer.from(READINESS_SCHEMA_QUERY)),
    markers_query_sha256: sha256(Buffer.from(READINESS_MARKERS_QUERY)),
    schema_output_sha256: sha256(Buffer.from(schemaRaw)),
    markers_output_sha256: markersRawSha256,
    deployment_status_before_sha256: sha256(Buffer.from(statusOutput)),
    deployment_status_after_sha256: sha256(Buffer.from(statusAfterOutput)),
    captured_started_at: new Date(startedMs).toISOString(),
    captured_finished_at: new Date(finishedMs).toISOString(),
    database_unix_time: databaseUnixTime,
    ...deployment,
    capability_table_exists: tableExists,
    control_inbox_sender_disposition: disposition,
    control_inbox_sender_reconciliation_started: reconciliation,
  };
  validateCapturedEvidence(evidence, expectedActiveVersion, finishedMs);
  return evidence;
}

async function productionPrepare(options) {
  const repoRoot = git(process.cwd(), ["rev-parse", "--show-toplevel"]).trim();
  const staging = await mkdtemp(path.join(tmpdir(), "osl-readiness-admit-"));
  const trustedDir = path.join(staging, "trusted");
  const anchor = await resolveCleanBuildSource(
    repoRoot,
    options.expectedCommit,
    trustedDir,
  );
  const config = git(repoRoot, [
    "show",
    `${options.expectedCommit}:keyserver-cf/wrangler.toml`,
  ]);
  validateProductionConfig(config);
  await buildReadinessArtifacts({
    repoRoot,
    requestedCommit: options.expectedCommit,
    outDir: trustedDir,
  });
  return { anchor, trustedDir };
}

export async function runAdmissionCli(argv, dependencies = {}) {
  const options = parseAdmissionArgs(argv);
  const prepare = dependencies.prepare ?? productionPrepare;
  const capture = dependencies.capture ?? captureReadOnlyEvidence;
  const write = dependencies.write ?? ((text) => process.stdout.write(text));
  const now = dependencies.now ?? (() => Date.now());
  const { anchor, trustedDir } = await prepare(options);
  const verified = await compareArchiveToTrustedRebuild(
    options.archiveDir,
    trustedDir,
    anchor,
  );
  const evidence = await capture({
    sourceTar: path.join(trustedDir, "source.tar"),
    expectedActiveVersion: options.expectedActiveVersion,
    now,
  });
  const markers = validateCapturedEvidence(
    evidence,
    options.expectedActiveVersion,
    now(),
  );
  validateReadinessSelection(verified.manifests[options.artifact], markers);
  const result = {
    format: ADMISSION_FORMAT,
    admitted: true,
    expected_commit: options.expectedCommit,
    archive_id: verified.index.archive_id,
    artifact: options.artifact,
    database: evidence.database,
    database_id: evidence.database_id,
    environment: evidence.environment,
    active_worker_version: evidence.active_worker_version,
    deployment_id: evidence.deployment_id,
    captured_started_at: evidence.captured_started_at,
    captured_finished_at: evidence.captured_finished_at,
    database_unix_time: evidence.database_unix_time,
    schema_query_sha256: evidence.schema_query_sha256,
    markers_query_sha256: evidence.markers_query_sha256,
    deployment_status_before_sha256:
      evidence.deployment_status_before_sha256,
    deployment_status_after_sha256:
      evidence.deployment_status_after_sha256,
    markers,
  };
  write(`${JSON.stringify(result, null, 2)}\n`);
  return result;
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  runAdmissionCli(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
