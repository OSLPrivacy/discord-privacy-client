import { execFileSync } from "node:child_process";
import {
  lstat,
  mkdir,
  open,
  readFile,
  rename,
  rmdir,
} from "node:fs/promises";
import { homedir } from "node:os";
import path from "node:path";
import {
  canonicalJson,
  sha256,
} from "./readiness-artifact-contract.mjs";

const MAX_RECEIPT_BYTES = 1024 * 1024;

function git(repoRoot, args, encoding = "utf8") {
  return execFileSync("git", ["-C", repoRoot, ...args], {
    encoding,
    maxBuffer: 16 * 1024 * 1024,
  });
}

export function loadCommittedMigrationClosure(
  repoRoot,
  expectedCommit,
  gitRun = git,
) {
  const output = gitRun(repoRoot, [
    "ls-tree",
    "-r",
    "--name-only",
    expectedCommit,
    "--",
    "keyserver-cf/migrations",
  ]);
  const paths = output.trim().split("\n").filter(Boolean).sort();
  if (paths.length === 0) {
    throw new Error("committed migration closure is empty");
  }
  const expectedPrefix = "keyserver-cf/migrations/";
  return paths.map((sourcePath) => {
    if (
      !sourcePath.startsWith(expectedPrefix) ||
      !/^\d{4}_[a-z0-9_]+\.sql$/.test(sourcePath.slice(expectedPrefix.length))
    ) {
      throw new Error(`committed migration path is invalid: ${sourcePath}`);
    }
    const bytes = gitRun(
      repoRoot,
      ["show", `${expectedCommit}:${sourcePath}`],
      null,
    );
    if (!Buffer.isBuffer(bytes) || bytes.byteLength === 0) {
      throw new Error(`committed migration is empty: ${sourcePath}`);
    }
    return {
      name: sourcePath.slice(expectedPrefix.length),
      sha256: sha256(bytes),
    };
  });
}

export async function readDeploymentEvidenceReceipt(receiptPath) {
  if (!path.isAbsolute(receiptPath)) {
    throw new Error("deployment evidence receipt path must be absolute");
  }
  const info = await lstat(receiptPath);
  if (!info.isFile() || info.isSymbolicLink()) {
    throw new Error("deployment evidence receipt is not a regular file");
  }
  if (info.size <= 0 || info.size > MAX_RECEIPT_BYTES) {
    throw new Error("deployment evidence receipt is empty or oversized");
  }
  const bytes = await readFile(receiptPath);
  try {
    return JSON.parse(bytes.toString("utf8"));
  } catch {
    throw new Error("deployment evidence receipt is not JSON");
  }
}

function exactHead(value) {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    JSON.stringify(Object.keys(value).sort()) !==
      JSON.stringify([
        "producer_identity",
        "producer_key_id",
        "producer_run_id",
        "receipt_sha256",
        "sequence",
      ])
  ) {
    throw new Error("deployment evidence replay ledger head is invalid");
  }
  return value;
}

async function secureLedgerDirectory(directory) {
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const info = await lstat(directory);
  if (
    !info.isDirectory() ||
    info.isSymbolicLink() ||
    (info.mode & 0o077) !== 0
  ) {
    throw new Error("deployment evidence replay ledger is not private");
  }
}

export async function consumeDeploymentEvidenceOnce(
  verified,
  {
    ledgerDirectory = path.join(
      homedir(),
      ".local",
      "state",
      "osl-keyserver",
      "deployment-evidence-ledger-v1",
    ),
  } = {},
) {
  const keyFile = sha256(Buffer.from(verified.producer_key_id));
  const headPath = path.join(ledgerDirectory, `${keyFile}.json`);
  const lockPath = path.join(ledgerDirectory, `${keyFile}.lock`);
  await secureLedgerDirectory(ledgerDirectory);
  try {
    await mkdir(lockPath, { mode: 0o700 });
  } catch (error) {
    if (error && typeof error === "object" && error.code === "EEXIST") {
      throw new Error("deployment evidence replay ledger is busy");
    }
    throw error;
  }

  try {
    let head = null;
    try {
      const headInfo = await lstat(headPath);
      if (
        !headInfo.isFile() ||
        headInfo.isSymbolicLink() ||
        (headInfo.mode & 0o077) !== 0
      ) {
        throw new Error(
          "deployment evidence replay ledger head is not a private regular file",
        );
      }
      head = exactHead(JSON.parse(await readFile(headPath, "utf8")));
    } catch (error) {
      if (!(error && typeof error === "object" && error.code === "ENOENT")) {
        throw error;
      }
    }
    const payload = verified.payload;
    if (head === null) {
      if (
        payload.producer_sequence !== 1 ||
        payload.previous_receipt_sha256 !== "0".repeat(64)
      ) {
        throw new Error("first producer receipt is not a genesis receipt");
      }
    } else if (
      payload.producer_sequence !== head.sequence + 1 ||
      payload.previous_receipt_sha256 !== head.receipt_sha256
    ) {
      throw new Error("deployment evidence receipt is replayed or out of chain");
    }

    const next = {
      producer_identity: verified.producer_identity,
      producer_key_id: verified.producer_key_id,
      producer_run_id: payload.producer_run_id,
      receipt_sha256: verified.receipt_sha256,
      sequence: payload.producer_sequence,
    };
    const temporaryPath = path.join(
      ledgerDirectory,
      `${keyFile}.${payload.producer_run_id}.tmp`,
    );
    const handle = await open(temporaryPath, "wx", 0o600);
    try {
      await handle.writeFile(`${canonicalJson(next)}\n`);
      await handle.sync();
    } finally {
      await handle.close();
    }
    await rename(temporaryPath, headPath);
    return next;
  } finally {
    await rmdir(lockPath);
  }
}
