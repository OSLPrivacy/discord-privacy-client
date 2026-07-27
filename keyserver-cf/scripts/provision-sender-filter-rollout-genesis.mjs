import { createHash, randomBytes } from "node:crypto";
import {
  lstat,
  open,
  readFile,
} from "node:fs/promises";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import {
  CANONICAL_ROLLOUT_DATABASE,
  validateCanonicalRolloutProvisioningReceipt,
} from "./canonical-rollout-admission-contract.mjs";

export const GENESIS_MANIFEST_FORMAT =
  "osl.sender-filter.rollout-genesis-secret.v1";
export const GENESIS_DATABASE = "osl-keyserver-prod";
export const CANONICAL_GENESIS_RECOVERY_PATH =
  "/var/lib/oslprivacy/keyserver/canonical-rollout-genesis.json";
export const GENESIS_DERIVED_RECEIPT_FORMAT =
  "osl.sender-filter.rollout-genesis-derived-receipt.v1";

export function parseGenesisProvisioningArguments(argv) {
  let outputPath = null;
  let admissionPath = null;
  let expectedCommit = null;
  let expectedTree = null;
  let expectedKeyserverTree = null;
  let apply = false;
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--apply") {
      apply = true;
      continue;
    }
    if (argument === "--output") {
      outputPath = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (argument === "--admission") {
      admissionPath = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (argument === "--expected-commit") {
      expectedCommit = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (argument === "--expected-tree") {
      expectedTree = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    if (argument === "--expected-keyserver-tree") {
      expectedKeyserverTree = argv[index + 1] ?? null;
      index += 1;
      continue;
    }
    throw new Error(`unknown argument: ${argument}`);
  }
  if (!apply) {
    throw new Error(
      "refusing sender-filter genesis without explicit --apply",
    );
  }
  if (!outputPath || !path.isAbsolute(outputPath)) {
    throw new Error("--output must be an absolute protected-file path");
  }
  if (!admissionPath || !path.isAbsolute(admissionPath)) {
    throw new Error("--admission must be an absolute receipt path");
  }
  for (const [value, label] of [
    [expectedCommit, "--expected-commit"],
    [expectedTree, "--expected-tree"],
    [expectedKeyserverTree, "--expected-keyserver-tree"],
  ]) {
    if (!/^[0-9a-f]{40}$/.test(value ?? "")) {
      throw new Error(`${label} must be a full lowercase Git object`);
    }
  }
  return {
    admissionPath,
    expectedCommit,
    expectedKeyserverTree,
    expectedTree,
    outputPath,
  };
}

export async function readCanonicalRolloutProvisioningReceipt(
  receiptPath,
  expected,
) {
  if (!path.isAbsolute(receiptPath)) {
    throw new Error("canonical rollout admission path must be absolute");
  }
  const metadata = await lstat(receiptPath);
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.size <= 0 ||
    metadata.size > 1024 * 1024
  ) {
    throw new Error("canonical rollout admission is not a bounded regular file");
  }
  let receipt;
  try {
    receipt = JSON.parse(await readFile(receiptPath, "utf8"));
  } catch {
    throw new Error("canonical rollout admission is not JSON");
  }
  return validateCanonicalRolloutProvisioningReceipt(receipt, expected);
}

export function assertCurrentCanonicalRolloutSource(
  {
    expectedCommit,
    expectedRepositoryTree,
    expectedKeyserverTree,
  },
  gitRun = (args) => execFileSync(
    "git",
    ["-C", path.resolve(import.meta.dirname, "../.."), ...args],
    { encoding: "utf8" },
  ),
) {
  const actual = {
    commit: gitRun(["rev-parse", "HEAD"]).trim(),
    repository_tree: gitRun(["rev-parse", "HEAD^{tree}"]).trim(),
    keyserver_tree: gitRun(["rev-parse", "HEAD:keyserver-cf"]).trim(),
  };
  if (
    actual.commit !== expectedCommit ||
    actual.repository_tree !== expectedRepositoryTree ||
    actual.keyserver_tree !== expectedKeyserverTree
  ) {
    throw new Error("provisioning checkout does not match the admitted commit/tree");
  }
  return actual;
}

export function buildGenesisProvisioning(
  nonce,
  provisionedAtMs,
  admissionReceipt,
) {
  if (
    !Buffer.isBuffer(nonce) ||
    nonce.length !== 32 ||
    !Number.isSafeInteger(provisionedAtMs) ||
    provisionedAtMs <= 0 ||
    !admissionReceipt ||
    typeof admissionReceipt !== "object"
  ) {
    throw new Error("sender-filter genesis inputs are invalid");
  }
  const receiptSha256 = admissionReceipt.payload_sha256;
  const source = admissionReceipt.source;
  if (
    typeof receiptSha256 !== "string" ||
    !/^[0-9a-f]{64}$/.test(receiptSha256) ||
    !source ||
    typeof source !== "object" ||
    !/^[0-9a-f]{40}$/.test(source.commit ?? "") ||
    !/^[0-9a-f]{40}$/.test(source.repository_tree ?? "") ||
    !/^[0-9a-f]{40}$/.test(source.keyserver_tree ?? "")
  ) {
    throw new Error("sender-filter provisioning admission is invalid");
  }
  const nonceSha256 = createHash("sha256").update(nonce).digest("hex");
  const nonceBase64Url = nonce.toString("base64url");
  const sql =
    "INSERT INTO sender_filter_rollout_genesis " +
    "(singleton, nonce_sha256, admission_receipt_sha256, worker_commit, " +
    "repository_tree, keyserver_tree, provisioned_at_ms, consumed_at_ms) " +
    `VALUES (1, '${nonceSha256}', '${receiptSha256}', '${source.commit}', ` +
    `'${source.repository_tree}', '${source.keyserver_tree}', ` +
    `${provisionedAtMs}, NULL) ` +
    "RETURNING singleton, nonce_sha256, admission_receipt_sha256, " +
    "provisioned_at_ms, consumed_at_ms;";
  return {
    manifest: {
      format: GENESIS_MANIFEST_FORMAT,
      genesis_nonce: nonceBase64Url,
      genesis_nonce_sha256: nonceSha256,
      provisioned_at_ms: provisionedAtMs,
      database: GENESIS_DATABASE,
      database_binding: CANONICAL_ROLLOUT_DATABASE.binding,
      database_id: CANONICAL_ROLLOUT_DATABASE.database_id,
      provisioning_admission_sha256: receiptSha256,
      worker_commit: source.commit,
      repository_tree: source.repository_tree,
      keyserver_tree: source.keyserver_tree,
    },
    sql,
    expectedReadback: {
      singleton: 1,
      nonce_sha256: nonceSha256,
      admission_receipt_sha256: receiptSha256,
      provisioned_at_ms: provisionedAtMs,
      consumed_at_ms: null,
    },
  };
}

export function runWranglerGenesisProvision(
  sql,
  expectedReadback,
  spawn = spawnSync,
) {
  const result = spawn(
    process.execPath,
    [
      "./node_modules/wrangler/bin/wrangler.js",
      "d1",
      "execute",
      GENESIS_DATABASE,
      "--remote",
      "--config",
      "wrangler.toml",
      "--command",
      sql,
      "--json",
    ],
    {
      cwd: path.resolve(import.meta.dirname, ".."),
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  if (result.status !== 0) {
    throw new Error(
      `D1 genesis provisioning failed: ${String(result.stderr).trim()}`,
    );
  }
  let parsed;
  try {
    parsed = JSON.parse(String(result.stdout));
  } catch {
    throw new Error("Wrangler did not return JSON provisioning evidence");
  }
  if (
    !Array.isArray(parsed) ||
    parsed.length === 0 ||
    parsed.some((entry) => entry?.success !== true)
  ) {
    throw new Error("D1 did not confirm sender-filter genesis provisioning");
  }
  const readbacks = parsed.flatMap((entry) =>
    Array.isArray(entry.results) ? entry.results : []);
  if (
    readbacks.length !== 1 ||
    JSON.stringify(readbacks[0]) !== JSON.stringify(expectedReadback)
  ) {
    throw new Error("D1 genesis post-provision readback is empty or mismatched");
  }
  return readbacks[0];
}

async function syncDirectory(directory) {
  const handle = await open(directory, "r");
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function requireProtectedDirectory(
  directory,
  expectedUid = process.getuid?.(),
) {
  const metadata = await lstat(directory);
  if (
    !metadata.isDirectory() ||
    metadata.isSymbolicLink() ||
    (metadata.mode & 0o777) !== 0o700 ||
    !Number.isInteger(expectedUid) ||
    metadata.uid !== expectedUid
  ) {
    throw new Error(
      "canonical recovery directory must be owner-only, owner-matched, and non-symlink",
    );
  }
}

async function pathExists(targetPath) {
  try {
    await lstat(targetPath);
    return true;
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    throw error;
  }
}

function validateRecoveryManifest(manifest) {
  const expectedKeys = [
    "database",
    "database_binding",
    "database_id",
    "format",
    "genesis_nonce",
    "genesis_nonce_sha256",
    "keyserver_tree",
    "provisioned_at_ms",
    "provisioning_admission_sha256",
    "repository_tree",
    "worker_commit",
  ].sort();
  if (
    !manifest ||
    typeof manifest !== "object" ||
    Array.isArray(manifest) ||
    JSON.stringify(Object.keys(manifest).sort()) !== JSON.stringify(expectedKeys) ||
    manifest.format !== GENESIS_MANIFEST_FORMAT ||
    manifest.database !== GENESIS_DATABASE ||
    manifest.database_binding !== CANONICAL_ROLLOUT_DATABASE.binding ||
    manifest.database_id !== CANONICAL_ROLLOUT_DATABASE.database_id ||
    typeof manifest.genesis_nonce !== "string" ||
    !/^[A-Za-z0-9_-]{43}$/.test(manifest.genesis_nonce) ||
    typeof manifest.genesis_nonce_sha256 !== "string" ||
    !/^[0-9a-f]{64}$/.test(manifest.genesis_nonce_sha256) ||
    createHash("sha256")
      .update(Buffer.from(manifest.genesis_nonce, "base64url"))
      .digest("hex") !== manifest.genesis_nonce_sha256
  ) {
    throw new Error("canonical recovery manifest is invalid");
  }
  return manifest;
}

export async function refuseExistingCanonicalRecoveryState(
  recoveryPath = CANONICAL_GENESIS_RECOVERY_PATH,
  {
    expectedPath = CANONICAL_GENESIS_RECOVERY_PATH,
    expectedUid = process.getuid?.(),
  } = {},
) {
  if (recoveryPath !== expectedPath || !path.isAbsolute(recoveryPath)) {
    throw new Error("canonical recovery path is not exact");
  }
  await requireProtectedDirectory(path.dirname(recoveryPath), expectedUid);
  if (await pathExists(recoveryPath)) {
    throw new Error(
      "canonical sender-filter recovery state already exists; recover its exact nonce",
    );
  }
}

export async function reserveCanonicalRecoveryManifest(
  recoveryPath,
  manifest,
  {
    expectedPath = CANONICAL_GENESIS_RECOVERY_PATH,
    expectedUid = process.getuid?.(),
  } = {},
) {
  if (recoveryPath !== expectedPath || !path.isAbsolute(recoveryPath)) {
    throw new Error("canonical recovery path is not exact");
  }
  validateRecoveryManifest(manifest);
  const directory = path.dirname(recoveryPath);
  await requireProtectedDirectory(directory, expectedUid);
  const handle = await open(recoveryPath, "wx", 0o600);
  try {
    await handle.writeFile(`${JSON.stringify(manifest)}\n`, "utf8");
    await handle.sync();
    const metadata = await handle.stat();
    if (
      !metadata.isFile() ||
      (metadata.mode & 0o777) !== 0o600 ||
      metadata.uid !== expectedUid ||
      metadata.nlink !== 1
    ) {
      throw new Error(
        "canonical recovery reservation is not an owner-only single-link file",
      );
    }
  } finally {
    await handle.close();
  }
  // The one canonical name is created exclusively and synced before remote
  // mutation. It is never renamed, unlinked, or replaced by this command.
  await syncDirectory(directory);
  return recoveryPath;
}

export async function loadCanonicalRecoveryManifest(
  recoveryPath = CANONICAL_GENESIS_RECOVERY_PATH,
  {
    expectedPath = CANONICAL_GENESIS_RECOVERY_PATH,
    expectedUid = process.getuid?.(),
  } = {},
) {
  if (recoveryPath !== expectedPath || !path.isAbsolute(recoveryPath)) {
    throw new Error("canonical recovery path is not exact");
  }
  await requireProtectedDirectory(path.dirname(recoveryPath), expectedUid);
  const metadata = await lstat(recoveryPath);
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    (metadata.mode & 0o777) !== 0o600 ||
    metadata.uid !== expectedUid ||
    metadata.nlink !== 1 ||
    metadata.size <= 0 ||
    metadata.size > 64 * 1024
  ) {
    throw new Error(
      "canonical recovery state is not an owner-only single-link regular file",
    );
  }
  let manifest;
  try {
    manifest = JSON.parse(await readFile(recoveryPath, "utf8"));
  } catch {
    throw new Error("canonical recovery state is not JSON");
  }
  return validateRecoveryManifest(manifest);
}

export async function provisionSenderFilterGenesis(
  recoveryPath,
  manifest,
  sql,
  provision = runWranglerGenesisProvision,
  recoveryOptions = {},
) {
  await reserveCanonicalRecoveryManifest(
    recoveryPath,
    manifest,
    recoveryOptions,
  );
  try {
    provision(sql);
  } catch (error) {
    // Any process/transport/parse failure may be ambiguous. The one canonical
    // reservation remains intact, so recovery reuses this exact nonce and no
    // alternate report path can start another remote mutation.
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}; ` +
      `protected recovery manifest retained at ${recoveryPath}`,
    );
  }
  return recoveryPath;
}

export async function writeDerivedProvisioningReceipt(
  outputPath,
  manifest,
  readback,
) {
  if (!path.isAbsolute(outputPath)) {
    throw new Error("derived provisioning receipt path must be absolute");
  }
  const receipt = {
    format: GENESIS_DERIVED_RECEIPT_FORMAT,
    database: manifest.database,
    database_binding: manifest.database_binding,
    database_id: manifest.database_id,
    worker_commit: manifest.worker_commit,
    repository_tree: manifest.repository_tree,
    keyserver_tree: manifest.keyserver_tree,
    provisioning_admission_sha256:
      manifest.provisioning_admission_sha256,
    genesis_nonce_sha256: manifest.genesis_nonce_sha256,
    provisioned_at_ms: manifest.provisioned_at_ms,
    post_provision_readback: readback,
  };
  const handle = await open(outputPath, "wx", 0o600);
  try {
    await handle.writeFile(`${JSON.stringify(receipt)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
  return receipt;
}

async function main() {
  const options = parseGenesisProvisioningArguments(process.argv.slice(2));
  assertCurrentCanonicalRolloutSource({
    expectedCommit: options.expectedCommit,
    expectedRepositoryTree: options.expectedTree,
    expectedKeyserverTree: options.expectedKeyserverTree,
  });
  const receipt = await readCanonicalRolloutProvisioningReceipt(
    options.admissionPath,
    {
      expectedCommit: options.expectedCommit,
      expectedRepositoryTree: options.expectedTree,
      expectedKeyserverTree: options.expectedKeyserverTree,
    },
  );
  await refuseExistingCanonicalRecoveryState();
  const { manifest, sql, expectedReadback } = buildGenesisProvisioning(
    randomBytes(32),
    Date.now(),
    receipt,
  );
  let readback;
  await provisionSenderFilterGenesis(
    CANONICAL_GENESIS_RECOVERY_PATH,
    manifest,
    sql,
    (statement) => {
      readback = runWranglerGenesisProvision(statement, expectedReadback);
      return readback;
    },
  );
  await writeDerivedProvisioningReceipt(
    options.outputPath,
    manifest,
    readback,
  );
  process.stdout.write(
    "Sender-filter genesis provisioned; canonical recovery state retained and derived receipt written.\n",
  );
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    process.stderr.write(
      `${error instanceof Error ? error.message : String(error)}\n`,
    );
    process.exitCode = 1;
  });
}
