#!/usr/bin/env node

/**
 * D2 stale-reservation promotion/proof helper.
 *
 * Default invocation is inert and prints the future promotion contract.
 * The explicit execution mode is intentionally narrow: it deploys an exact
 * committed `cipher-store-cf` archive only after local gates and a read-only
 * schema/count preflight, then waits for one naturally scheduled five-minute
 * cycle. It never applies a migration, invokes a scheduler, deletes a D1 row,
 * or calls an R2 API.
 *
 * Raw Wrangler/D1/tail output stays in memory and is discarded. The only
 * retained/stdout artifact is the sanitized aggregate proof returned below.
 */

import { execFile, spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export const BASELINE_COMMIT = "e8fbd3ffa857d5bfd5f173b556a88a6a3bfee88f";
export const WORKER_NAME = "oslprivacy-cipher-store";
export const DATABASE_NAME = "osl-cipher-store-prod";
export const NATURAL_CRON = "*/5 * * * *";
export const CYCLE_MARKER = "[attachment-sweep-cycle] complete";
export const CONFIRMATION = "D2_PROMOTE_EXACT_ARCHIVE_AND_WAIT_NATURAL_CRON";
export const VERSION_TAG_PREFIX = "osl-d2";
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const SHA_RE = /^[0-9a-f]{40}$/;
const MAX_CAPTURE_BYTES = 2 * 1024 * 1024;

/**
 * One fixed aggregate-only statement. The helper does not accept caller SQL.
 *
 * `stale_total` deliberately includes both unmarked and already-expired rows.
 * If R2 abort fails, e8fbd3f changes an unmarked row into retryable metadata;
 * counting only `expires_at >= unixepoch()` would falsely report reclamation.
 */
export const AGGREGATE_SQL = `
WITH stale AS (
  SELECT expires_at
    FROM attachment_objects AS candidate
   WHERE candidate.state = 'uploading'
     AND candidate.upload_id IS NOT NULL
     AND candidate.content_expires_at IS NULL
     AND candidate.created_at < unixepoch() - 900
     AND NOT EXISTS (
       SELECT 1 FROM attachment_parts AS part
        WHERE part.attachment_id = candidate.id
     )
)
SELECT
  (SELECT COUNT(*) FROM d1_migrations
    WHERE name = '0006_session_budget_and_atomic_rate_counters.sql')
    AS migration_0006_count,
  (SELECT COUNT(*) FROM d1_migrations
    WHERE name = '0007_link_grant_consumption.sql')
    AS migration_0007_count,
  (SELECT COUNT(*) FROM pragma_table_info('attachment_objects')
    WHERE name = 'content_expires_at') AS content_expiry_column_count,
  (SELECT COUNT(*) FROM sqlite_master
    WHERE type = 'table' AND name = 'attachment_parts')
    AS attachment_parts_table_count,
  COUNT(*) AS stale_total,
  COALESCE(SUM(CASE WHEN expires_at >= unixepoch() THEN 1 ELSE 0 END), 0)
    AS stale_unmarked,
  COALESCE(SUM(CASE WHEN expires_at < unixepoch() THEN 1 ELSE 0 END), 0)
    AS stale_retryable
FROM stale`.trim();

const AGGREGATE_KEYS = [
  "migration_0006_count",
  "migration_0007_count",
  "content_expiry_column_count",
  "attachment_parts_table_count",
  "stale_total",
  "stale_unmarked",
  "stale_retryable",
] as const;

export interface AggregateSnapshot {
  migration_0006_count: number;
  migration_0007_count: number;
  content_expiry_column_count: number;
  attachment_parts_table_count: number;
  stale_total: number;
  stale_unmarked: number;
  stale_retryable: number;
}

export interface SourceFacts {
  commit_sha: string;
  cipher_store_tree_sha: string;
}

export interface CycleWitness {
  scheduled_time_ms: number;
  event_time_ms: number;
  cron: string;
  outcome: "ok";
  marker: typeof CYCLE_MARKER;
}

export interface PromotionProof {
  schema_version: 1;
  verdict: "proved";
  source: SourceFacts;
  worker: {
    version_id: string;
    source_tag: string;
    traffic_percentage: 100;
  };
  migration: {
    migration_0006_count: 1;
    migration_0007_count: 1;
    content_expiry_column_count: 1;
    attachment_parts_table_count: 1;
  };
  cycle: {
    cron: typeof NATURAL_CRON;
    scheduled_time_ms: number;
    event_time_ms: number;
  };
  stale_reservations: {
    before: Pick<AggregateSnapshot, "stale_total" | "stale_unmarked" | "stale_retryable">;
    after: Pick<AggregateSnapshot, "stale_total" | "stale_unmarked" | "stale_retryable">;
    reclaimed: number;
  };
}

interface CommandRunner {
  run(file: string, args: readonly string[], cwd: string): Promise<string>;
}

const realRunner: CommandRunner = {
  async run(file, args, cwd) {
    const result = await execFileAsync(file, [...args], {
      cwd,
      encoding: "utf8",
      maxBuffer: MAX_CAPTURE_BYTES,
      env: {
        ...process.env,
        WRANGLER_WRITE_LOGS: "false",
      },
    });
    return result.stdout;
  },
};

function exactKeys(value: Record<string, unknown>, expected: readonly string[], label: string): void {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw new Error(`${label} has unexpected fields`);
  }
}

function objectValue(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value as Record<string, unknown>;
}

function countValue(value: unknown, label: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${label} must be a non-negative safe integer`);
  }
  return value;
}

export function assertAggregateSql(sql: string): void {
  const normalized = sql.trim();
  if (normalized !== AGGREGATE_SQL) throw new Error("aggregate SQL differs from the reviewed statement");
  if (!/^WITH\b[\s\S]+\bSELECT\b/i.test(normalized)) throw new Error("aggregate SQL is not SELECT-only");
  if (/;\s*\S/.test(normalized) || /\b(INSERT|UPDATE|DELETE|REPLACE|DROP|ALTER|CREATE|PRAGMA)\b/i.test(normalized)) {
    throw new Error("aggregate SQL contains a mutation or second statement");
  }
  if (/\bSELECT\s+(?:candidate|part)\.(?:id|object_key|upload_id)\b/i.test(normalized)) {
    throw new Error("aggregate SQL selects an identifier");
  }
}

export function parseAggregateOutput(raw: string): AggregateSnapshot {
  assertAggregateSql(AGGREGATE_SQL);
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw new Error("D1 output is not JSON");
  }
  if (!Array.isArray(parsed) || parsed.length !== 1) {
    throw new Error("D1 output must contain exactly one statement result");
  }
  const statement = objectValue(parsed[0], "D1 statement");
  exactKeys(statement, ["results", "success", "meta"], "D1 statement");
  if (statement.success !== true) throw new Error("D1 aggregate statement did not succeed");
  const results = statement.results;
  if (!Array.isArray(results) || results.length !== 1) {
    throw new Error("D1 statement must return exactly one aggregate row");
  }
  const meta = objectValue(statement.meta, "D1 meta");
  if (
    meta.changes !== 0 ||
    meta.changed_db !== false ||
    meta.rows_written !== 0
  ) {
    throw new Error("D1 aggregate query reports a write");
  }
  const row = objectValue(results[0], "D1 aggregate row");
  exactKeys(row, AGGREGATE_KEYS, "D1 aggregate row");
  const snapshot = Object.fromEntries(
    AGGREGATE_KEYS.map((key) => [key, countValue(row[key], key)]),
  ) as unknown as AggregateSnapshot;
  assertSnapshot(snapshot);
  return snapshot;
}

export function assertSnapshot(snapshot: AggregateSnapshot): void {
  if (
    snapshot.migration_0006_count !== 1 ||
    snapshot.migration_0007_count !== 1 ||
    snapshot.content_expiry_column_count !== 1 ||
    snapshot.attachment_parts_table_count !== 1
  ) {
    throw new Error("required migration 0006/0007 or attachment schema is missing/ambiguous");
  }
  if (snapshot.stale_total !== snapshot.stale_unmarked + snapshot.stale_retryable) {
    throw new Error("stale aggregate partitions are inconsistent");
  }
}

export function sourceTag(source: SourceFacts): string {
  if (!SHA_RE.test(source.commit_sha) || !SHA_RE.test(source.cipher_store_tree_sha)) {
    throw new Error("source hashes must be exact lowercase Git SHA-1 values");
  }
  return `${VERSION_TAG_PREFIX}-${source.commit_sha}-${source.cipher_store_tree_sha}`;
}

export function assertCleanCipherStatus(statusPorcelain: string): void {
  if (statusPorcelain.trim().length !== 0) {
    throw new Error("cipher-store source is dirty");
  }
}

export function parseDeploymentAndVersion(
  deploymentRaw: string,
  versionRaw: string,
  source: SourceFacts,
): { version_id: string; source_tag: string; traffic_percentage: 100 } {
  let deploymentValue: unknown;
  let versionValue: unknown;
  try {
    deploymentValue = JSON.parse(deploymentRaw);
    versionValue = JSON.parse(versionRaw);
  } catch {
    throw new Error("Worker version metadata is not JSON");
  }
  const deployment = objectValue(deploymentValue, "deployment");
  const traffic = deployment.versions;
  if (!Array.isArray(traffic) || traffic.length !== 1) {
    throw new Error("deployment is split or has no exact active version");
  }
  const active = objectValue(traffic[0], "deployment version");
  const versionId = active.version_id;
  if (typeof versionId !== "string" || !UUID_RE.test(versionId) || active.percentage !== 100) {
    throw new Error("deployment must bind one UUID at exactly 100 percent");
  }

  const version = objectValue(versionValue, "version");
  if (version.id !== versionId) throw new Error("version view does not match deployed UUID");
  const annotations = objectValue(version.annotations, "version annotations");
  const tag = annotations["workers/tag"];
  const expectedTag = sourceTag(source);
  if (tag !== expectedTag) throw new Error("Worker version has unknown or mismatched source binding");
  const resources = objectValue(version.resources, "version resources");
  const script = objectValue(resources.script, "version script");
  if (!Array.isArray(script.handlers) || !script.handlers.includes("scheduled")) {
    throw new Error("deployed version does not expose the scheduled handler");
  }
  return { version_id: versionId, source_tag: expectedTag, traffic_percentage: 100 };
}

function markerPresent(logs: unknown): boolean {
  if (!Array.isArray(logs)) return false;
  return logs.some((entry) => {
    const log = objectValue(entry, "tail log");
    return Array.isArray(log.message) && log.message.length === 1 && log.message[0] === CYCLE_MARKER;
  });
}

export function parseCycleWitness(value: unknown): CycleWitness {
  const trace = objectValue(value, "tail event");
  const event = objectValue(trace.event, "tail scheduled event");
  if (trace.outcome !== "ok" || event.cron !== NATURAL_CRON) {
    throw new Error("tail event is not a successful natural attachment cron");
  }
  if (
    typeof event.scheduledTime !== "number" ||
    !Number.isSafeInteger(event.scheduledTime) ||
    typeof trace.eventTimestamp !== "number" ||
    !Number.isSafeInteger(trace.eventTimestamp) ||
    !markerPresent(trace.logs)
  ) {
    throw new Error("tail event lacks the exact scheduled-cycle witness");
  }
  if ("request" in event) throw new Error("HTTP/manual scheduler event is not accepted");
  return {
    scheduled_time_ms: event.scheduledTime,
    event_time_ms: trace.eventTimestamp,
    cron: NATURAL_CRON,
    outcome: "ok",
    marker: CYCLE_MARKER,
  };
}

export function validateProof(input: {
  source: SourceFacts;
  worker: { version_id: string; source_tag: string; traffic_percentage: 100 };
  before: AggregateSnapshot;
  after: AggregateSnapshot;
  beforeCompletedMs: number;
  afterStartedMs: number;
  cycles: readonly CycleWitness[];
}): PromotionProof {
  const expectedTag = sourceTag(input.source);
  if (
    !UUID_RE.test(input.worker.version_id) ||
    input.worker.source_tag !== expectedTag ||
    input.worker.traffic_percentage !== 100
  ) {
    throw new Error("proof Worker is not bound to the exact source at 100 percent");
  }
  assertSnapshot(input.before);
  assertSnapshot(input.after);
  if (
    !Number.isSafeInteger(input.beforeCompletedMs) ||
    !Number.isSafeInteger(input.afterStartedMs) ||
    input.beforeCompletedMs >= input.afterStartedMs
  ) {
    throw new Error("measurement window is invalid");
  }
  if (input.before.stale_total <= 0) throw new Error("before state is vacuous");
  if (input.cycles.length !== 1) throw new Error("natural scheduled cycle is absent or ambiguous");
  const cycle = input.cycles[0]!;
  if (
    cycle.cron !== NATURAL_CRON ||
    cycle.outcome !== "ok" ||
    cycle.marker !== CYCLE_MARKER ||
    cycle.scheduled_time_ms <= input.beforeCompletedMs ||
    cycle.scheduled_time_ms >= input.afterStartedMs ||
    cycle.event_time_ms <= input.beforeCompletedMs ||
    cycle.event_time_ms >= input.afterStartedMs ||
    cycle.event_time_ms < cycle.scheduled_time_ms
  ) {
    throw new Error("scheduled cycle is outside the measured before/after interval");
  }
  if (input.after.stale_total >= input.before.stale_total) {
    throw new Error("stale total did not decrease; cleanup is unproved or retryable");
  }
  return {
    schema_version: 1,
    verdict: "proved",
    source: input.source,
    worker: input.worker,
    migration: {
      migration_0006_count: 1,
      migration_0007_count: 1,
      content_expiry_column_count: 1,
      attachment_parts_table_count: 1,
    },
    cycle: {
      cron: NATURAL_CRON,
      scheduled_time_ms: cycle.scheduled_time_ms,
      event_time_ms: cycle.event_time_ms,
    },
    stale_reservations: {
      before: {
        stale_total: input.before.stale_total,
        stale_unmarked: input.before.stale_unmarked,
        stale_retryable: input.before.stale_retryable,
      },
      after: {
        stale_total: input.after.stale_total,
        stale_unmarked: input.after.stale_unmarked,
        stale_retryable: input.after.stale_retryable,
      },
      reclaimed: input.before.stale_total - input.after.stale_total,
    },
  };
}

export function reviewedCommandPlan(input: {
  wranglerPath: string;
  archiveDir: string;
  source: SourceFacts;
  versionId?: string;
}): Array<{ purpose: string; file: string; args: string[] }> {
  const common = ["--config", join(input.archiveDir, "wrangler.toml")];
  const commands = [
    {
      purpose: "migration-and-count-preflight",
      file: input.wranglerPath,
      args: ["d1", "execute", DATABASE_NAME, "--remote", "--json", "--command", AGGREGATE_SQL, ...common],
    },
    {
      purpose: "exact-source-promotion",
      file: input.wranglerPath,
      args: [
        "deploy",
        "--strict",
        "--tag",
        sourceTag(input.source),
        "--message",
        `D2 exact source ${input.source.commit_sha}`,
        ...common,
      ],
    },
    {
      purpose: "deployment-readback",
      file: input.wranglerPath,
      args: ["deployments", "status", "--json", ...common],
    },
  ];
  if (input.versionId) {
    commands.push(
      {
        purpose: "version-readback",
        file: input.wranglerPath,
        args: ["versions", "view", input.versionId, "--json", ...common],
      },
      {
        purpose: "natural-cycle-tail",
        file: input.wranglerPath,
        args: [
          "tail",
          WORKER_NAME,
          "--format",
          "json",
          "--status",
          "ok",
          "--search",
          CYCLE_MARKER,
          "--version-id",
          input.versionId,
          ...common,
        ],
      },
    );
  }
  return commands;
}

export function assertReviewedCommandPlan(
  commands: ReadonlyArray<{ purpose: string; file: string; args: readonly string[] }>,
): void {
  const joined = commands.flatMap((command) => command.args).join(" ");
  if (
    /\b(?:migrations\s+apply|d1\s+delete|r2\b.*\b(?:delete|abort)|DELETE|ABORT|test-scheduled|__scheduled|cdn-cgi\/handler\/scheduled)\b/i.test(joined)
  ) {
    throw new Error("command plan contains a forbidden mutation or scheduler invocation");
  }
  const sqlArgs = commands.flatMap((command) =>
    command.args.flatMap((arg, index) => command.args[index - 1] === "--command" ? [arg] : [])
  );
  if (sqlArgs.length !== 1 || sqlArgs[0] !== AGGREGATE_SQL) {
    throw new Error("command plan contains caller-controlled or non-reviewed SQL");
  }
}

async function sourceFacts(repoRoot: string, runner: CommandRunner): Promise<SourceFacts> {
  const dirty = await runner.run(
    "git",
    ["status", "--porcelain=v1", "--untracked-files=all", "--", "cipher-store-cf"],
    repoRoot,
  );
  assertCleanCipherStatus(dirty);
  await runner.run("git", ["merge-base", "--is-ancestor", BASELINE_COMMIT, "HEAD"], repoRoot);
  const commit = (await runner.run("git", ["rev-parse", "HEAD"], repoRoot)).trim();
  const tree = (await runner.run("git", ["rev-parse", "HEAD:cipher-store-cf"], repoRoot)).trim();
  const facts = { commit_sha: commit, cipher_store_tree_sha: tree };
  sourceTag(facts);
  return facts;
}

function jsonObjectsFromBuffer(buffer: string): { values: unknown[]; remainder: string } {
  const values: unknown[] = [];
  let start = -1;
  let depth = 0;
  let inString = false;
  let escaped = false;
  let consumed = 0;
  for (let index = 0; index < buffer.length; index += 1) {
    const char = buffer[index]!;
    if (start < 0) {
      if (char === "{") {
        start = index;
        depth = 1;
      }
      continue;
    }
    if (inString) {
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === '"') inString = false;
      continue;
    }
    if (char === '"') inString = true;
    else if (char === "{") depth += 1;
    else if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        const candidate = buffer.slice(start, index + 1);
        try {
          values.push(JSON.parse(candidate));
        } catch {
          throw new Error("tail emitted malformed JSON");
        }
        consumed = index + 1;
        start = -1;
      }
    }
  }
  return { values, remainder: start >= 0 ? buffer.slice(start) : buffer.slice(consumed) };
}

async function observeNaturalCycle(
  wranglerPath: string,
  archiveDir: string,
  versionId: string,
): Promise<CycleWitness[]> {
  const command = reviewedCommandPlan({
    wranglerPath,
    archiveDir,
    source: { commit_sha: "0".repeat(40), cipher_store_tree_sha: "0".repeat(40) },
    versionId,
  }).find((item) => item.purpose === "natural-cycle-tail");
  if (!command) throw new Error("missing reviewed tail command");
  const child = spawn(command.file, command.args, {
    cwd: archiveDir,
    env: { ...process.env, WRANGLER_WRITE_LOGS: "false" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  return await new Promise<CycleWitness[]>((resolvePromise, rejectPromise) => {
    let buffer = "";
    let stderrBytes = 0;
    const witnesses: CycleWitness[] = [];
    const timer = setTimeout(() => {
      child.kill("SIGINT");
      rejectPromise(new Error("no unambiguous natural scheduled cycle observed"));
    }, 7 * 60 * 1000);
    child.stdout.on("data", (chunk: Buffer) => {
      buffer += chunk.toString("utf8");
      if (buffer.length > MAX_CAPTURE_BYTES) {
        clearTimeout(timer);
        child.kill("SIGINT");
        rejectPromise(new Error("tail output exceeded bound"));
        return;
      }
      try {
        const extracted = jsonObjectsFromBuffer(buffer);
        buffer = extracted.remainder;
        for (const value of extracted.values) witnesses.push(parseCycleWitness(value));
        if (witnesses.length > 0) {
          clearTimeout(timer);
          child.kill("SIGINT");
          resolvePromise(witnesses);
        }
      } catch (error) {
        clearTimeout(timer);
        child.kill("SIGINT");
        rejectPromise(error);
      }
    });
    child.stderr.on("data", (chunk: Buffer) => {
      // Drain stderr so Wrangler cannot block on a full pipe, but retain and
      // report none of it: authentication failures can contain operator or
      // account metadata.
      stderrBytes += chunk.byteLength;
      if (stderrBytes > MAX_CAPTURE_BYTES) {
        clearTimeout(timer);
        child.kill("SIGINT");
        rejectPromise(new Error("tail diagnostics exceeded bound"));
      }
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      rejectPromise(error);
    });
    child.on("exit", () => {
      if (witnesses.length === 0) {
        clearTimeout(timer);
        rejectPromise(new Error("tail ended without a natural scheduled cycle"));
      }
    });
  });
}

async function executePromotionAndProof(projectRoot: string, repoRoot: string): Promise<PromotionProof> {
  assertAggregateSql(AGGREGATE_SQL);
  const source = await sourceFacts(repoRoot, realRunner);
  await realRunner.run("npm", ["run", "typecheck"], projectRoot);
  await realRunner.run("npm", ["test"], projectRoot);

  const temporary = await mkdtemp(join(tmpdir(), "osl-d2-proof-"));
  const archive = join(temporary, "cipher-store.tar");
  const archiveDir = join(temporary, "source");
  try {
    await realRunner.run(
      "git",
      ["archive", "--format=tar", `--output=${archive}`, `${source.commit_sha}:cipher-store-cf`],
      repoRoot,
    );
    await realRunner.run("mkdir", ["-p", archiveDir], repoRoot);
    await realRunner.run("tar", ["-xf", archive, "-C", archiveDir], repoRoot);
    const wranglerPath = resolve(projectRoot, "node_modules/.bin/wrangler");
    const basePlan = reviewedCommandPlan({ wranglerPath, archiveDir, source });
    assertReviewedCommandPlan(basePlan);

    const preflight = basePlan.find((item) => item.purpose === "migration-and-count-preflight")!;
    // Schema/migration gate happens before promotion. Missing 0007 therefore
    // leaves the current Worker untouched.
    parseAggregateOutput(await realRunner.run(preflight.file, preflight.args, archiveDir));
    const promote = basePlan.find((item) => item.purpose === "exact-source-promotion")!;
    await realRunner.run(promote.file, promote.args, archiveDir);

    const deploymentCommand = basePlan.find((item) => item.purpose === "deployment-readback")!;
    const deploymentRaw = await realRunner.run(
      deploymentCommand.file,
      deploymentCommand.args,
      archiveDir,
    );
    const deployment = objectValue(JSON.parse(deploymentRaw), "deployment");
    const versions = deployment.versions;
    if (!Array.isArray(versions) || versions.length !== 1) {
      throw new Error("promotion did not produce one active version");
    }
    const versionId = objectValue(versions[0], "deployment version").version_id;
    if (typeof versionId !== "string" || !UUID_RE.test(versionId)) {
      throw new Error("promotion returned an invalid version UUID");
    }
    const fullPlan = reviewedCommandPlan({ wranglerPath, archiveDir, source, versionId });
    assertReviewedCommandPlan(fullPlan);
    const versionCommand = fullPlan.find((item) => item.purpose === "version-readback")!;
    const versionRaw = await realRunner.run(versionCommand.file, versionCommand.args, archiveDir);
    const worker = parseDeploymentAndVersion(deploymentRaw, versionRaw, source);

    // Tail starts first so a naturally arriving cron cannot be missed. A
    // before snapshot completing after that cycle is rejected by validateProof.
    const cyclePromise = observeNaturalCycle(wranglerPath, archiveDir, versionId);
    const beforeCommand = fullPlan.find((item) => item.purpose === "migration-and-count-preflight")!;
    const before = parseAggregateOutput(
      await realRunner.run(beforeCommand.file, beforeCommand.args, archiveDir),
    );
    const beforeCompletedMs = Date.now();
    if (before.stale_total <= 0) throw new Error("before state is vacuous");
    const cycles = await cyclePromise;
    const afterStartedMs = Date.now();
    const after = parseAggregateOutput(
      await realRunner.run(beforeCommand.file, beforeCommand.args, archiveDir),
    );

    // Refuse a deployment change during the proof window.
    const finalDeploymentRaw = await realRunner.run(
      deploymentCommand.file,
      deploymentCommand.args,
      archiveDir,
    );
    const finalVersionRaw = await realRunner.run(versionCommand.file, versionCommand.args, archiveDir);
    const finalWorker = parseDeploymentAndVersion(finalDeploymentRaw, finalVersionRaw, source);
    if (finalWorker.version_id !== worker.version_id) {
      throw new Error("Worker version changed during proof window");
    }
    return validateProof({
      source,
      worker,
      before,
      after,
      beforeCompletedMs,
      afterStartedMs,
      cycles,
    });
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

function printPlan(): void {
  console.log("D2 promotion/proof helper (inert plan)");
  console.log("No Cloudflare, D1, R2, deploy, migration, or scheduler command was run.");
  console.log("Execution requires: --execute --confirm " + CONFIRMATION);
  console.log("It refuses dirty cipher-store source, missing migration 0007, an untagged/split");
  console.log("Worker version, non-aggregate D1 output, and zero/ambiguous natural cron evidence.");
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  if (args.length === 0 || args.includes("--help")) {
    printPlan();
    return;
  }
  if (
    args.length !== 3 ||
    args[0] !== "--execute" ||
    args[1] !== "--confirm" ||
    args[2] !== CONFIRMATION
  ) {
    throw new Error("refusing: exact execution confirmation was not supplied");
  }
  const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const repoRoot = resolve(projectRoot, "..");
  const proof = await executePromotionAndProof(projectRoot, repoRoot);
  console.log(JSON.stringify(proof, null, 2));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : "promotion proof failed");
    process.exitCode = 1;
  });
}
