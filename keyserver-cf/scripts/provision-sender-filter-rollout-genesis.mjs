import { createHash, randomBytes } from "node:crypto";
import {
  chmod,
  link,
  lstat,
  mkdir,
  open,
  readdir,
  unlink,
} from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";

export const GENESIS_MANIFEST_FORMAT =
  "osl.sender-filter.rollout-genesis-secret.v1";
export const GENESIS_DATABASE = "osl-keyserver-prod";

function parseArguments(argv) {
  let outputPath = null;
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
  return { outputPath };
}

export function buildGenesisProvisioning(nonce, provisionedAtMs) {
  if (
    !Buffer.isBuffer(nonce) ||
    nonce.length !== 32 ||
    !Number.isSafeInteger(provisionedAtMs) ||
    provisionedAtMs <= 0
  ) {
    throw new Error("sender-filter genesis inputs are invalid");
  }
  const nonceSha256 = createHash("sha256").update(nonce).digest("hex");
  const nonceBase64Url = nonce.toString("base64url");
  const sql =
    "INSERT INTO sender_filter_rollout_genesis " +
    "(nonce_sha256, provisioned_at_ms, consumed_at_ms) " +
    `VALUES ('${nonceSha256}', ${provisionedAtMs}, NULL);`;
  return {
    manifest: {
      format: GENESIS_MANIFEST_FORMAT,
      genesis_nonce: nonceBase64Url,
      genesis_nonce_sha256: nonceSha256,
      provisioned_at_ms: provisionedAtMs,
      database: GENESIS_DATABASE,
    },
    sql,
  };
}

export function runWranglerGenesisProvision(sql, spawn = spawnSync) {
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
    parsed.length !== 1 ||
    parsed[0]?.success !== true
  ) {
    throw new Error("D1 did not confirm sender-filter genesis provisioning");
  }
  return parsed;
}

async function syncDirectory(directory) {
  const handle = await open(directory, "r");
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function requireProtectedDirectory(directory) {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const metadata = await lstat(directory);
  if (
    !metadata.isDirectory() ||
    metadata.isSymbolicLink() ||
    (metadata.mode & 0o077) !== 0
  ) {
    throw new Error(
      "recovery-manifest directory must be a private non-symlink directory",
    );
  }
}

async function refuseExistingRecoveryState(outputPath) {
  const directory = path.dirname(outputPath);
  const base = path.basename(outputPath);
  const entries = await readdir(directory);
  if (
    entries.includes(base) ||
    entries.some((entry) => entry.startsWith(`${base}.pending-`))
  ) {
    throw new Error(
      "sender-filter genesis recovery state already exists; recover it instead of generating a new nonce",
    );
  }
}

export async function stageProtectedRecoveryManifest(
  outputPath,
  manifest,
  processId = process.pid,
) {
  if (!path.isAbsolute(outputPath)) {
    throw new Error("recovery manifest path must be absolute");
  }
  const directory = path.dirname(outputPath);
  await requireProtectedDirectory(directory);
  await refuseExistingRecoveryState(outputPath);
  const temporaryPath = `${outputPath}.pending-${processId}`;
  const handle = await open(temporaryPath, "wx", 0o600);
  try {
    await handle.writeFile(`${JSON.stringify(manifest)}\n`, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
  // The file sync does not durably commit its directory entry. Commit the
  // pending name before starting the ambiguous remote mutation.
  await syncDirectory(directory);
  return temporaryPath;
}

export async function commitProtectedRecoveryManifest(
  temporaryPath,
  outputPath,
) {
  const directory = path.dirname(outputPath);
  if (
    !path.isAbsolute(outputPath) ||
    path.dirname(temporaryPath) !== directory ||
    !path.basename(temporaryPath).startsWith(
      `${path.basename(outputPath)}.pending-`,
    )
  ) {
    throw new Error("recovery manifest paths do not match");
  }
  await link(temporaryPath, outputPath);
  await chmod(outputPath, 0o600);
  // Commit the final name before removing the pending name. At every crash
  // boundary at least one private name still refers to the nonce inode.
  await syncDirectory(directory);
  await unlink(temporaryPath);
  await syncDirectory(directory);
}

export async function provisionSenderFilterGenesis(
  outputPath,
  manifest,
  sql,
  provision = runWranglerGenesisProvision,
) {
  const temporaryPath = await stageProtectedRecoveryManifest(
    outputPath,
    manifest,
  );
  try {
    provision(sql);
    await commitProtectedRecoveryManifest(temporaryPath, outputPath);
  } catch (error) {
    // Any process/transport/parse failure can be ambiguous after the command
    // started. Preserve the protected pending nonce so a possibly committed D1
    // row is never made permanently unconsumable.
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}; ` +
      `protected recovery manifest retained at ${temporaryPath}`,
    );
  }
  return outputPath;
}

async function main() {
  const { outputPath } = parseArguments(process.argv.slice(2));
  const { manifest, sql } = buildGenesisProvisioning(
    randomBytes(32),
    Date.now(),
  );
  await provisionSenderFilterGenesis(
    outputPath,
    manifest,
    sql,
  );
  process.stdout.write(
    "Sender-filter genesis provisioned; protected nonce manifest written.\n",
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
