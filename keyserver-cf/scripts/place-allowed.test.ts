import { execFileSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error - plain-JS operator command, imported for its pure helpers.
import { placeAllowed, stableIdSha256 } from "./place-allowed.mjs";

const SCRIPT = join(process.cwd(), "scripts/place-allowed.mjs");

function createDb(path: string): InstanceType<typeof DatabaseSync> {
  const db = new DatabaseSync(path);
  db.exec(`
    CREATE TABLE worker_schema_capabilities (
      capability TEXT PRIMARY KEY,
      version INTEGER NOT NULL CHECK (version >= 1)
    ) WITHOUT ROWID;
    INSERT INTO worker_schema_capabilities (capability, version)
      VALUES ('account_ownership_binding_account_unique', 1);

    CREATE TABLE account_ownership_proof_bindings (
      binding_sha256 TEXT PRIMARY KEY,
      nonce_sha256 TEXT NOT NULL UNIQUE,
      owner_user_id TEXT NOT NULL,
      service TEXT NOT NULL,
      service_account_sha256 TEXT NOT NULL,
      proof_type TEXT NOT NULL,
      verified_at_unix_seconds INTEGER NOT NULL
    ) WITHOUT ROWID;
  `);
  return db;
}

function savePlaceRecord(
  db: InstanceType<typeof DatabaseSync>,
  args: { app: string; account: string; stableId: string },
): void {
  db.prepare(
    `INSERT INTO account_ownership_proof_bindings (
       binding_sha256, nonce_sha256, owner_user_id, service,
       service_account_sha256, proof_type, verified_at_unix_seconds
     ) VALUES (?, ?, ?, ?, ?, 'ed25519_identity_challenge_v1', 1900000040)`,
  ).run(
    "a".repeat(64),
    "b".repeat(64),
    args.account,
    args.app,
    stableIdSha256(args.stableId),
  );
}

function runPlaceAllowed(
  dbPath: string,
  args: { app: string; account: string; kind: string; stableId: string },
): { status: number; output: string } {
  try {
    return {
      status: 0,
      output: execFileSync("node", [
        SCRIPT,
        "--db", dbPath,
        "--app", args.app,
        "--account", args.account,
        "--kind", args.kind,
        "--stable-id", args.stableId,
      ], { encoding: "utf8", stdio: "pipe" }),
    };
  } catch (error) {
    const e = error as { status?: number; stdout?: string; stderr?: string };
    return {
      status: e.status ?? 1,
      output: `${e.stdout ?? ""}${e.stderr ?? ""}`,
    };
  }
}

describe("place-allowed command", () => {
  it("prints allowed for a saved account place and not allowed for a different stable ID", () => {
    const dir = mkdtempSync(join(tmpdir(), "osl-place-allowed-"));
    const dbPath = join(dir, "places.sqlite");
    try {
      const saved = {
        app: "discord",
        account: "osl-owner-0103",
        kind: "account",
        stableId: "900000000000001103",
      };
      const db = createDb(dbPath);
      savePlaceRecord(db, saved);

      expect(placeAllowed(db, saved)).toBe(true);
      expect(placeAllowed(db, { ...saved, stableId: "900000000000001104" })).toBe(false);
      db.close();

      const allowed = runPlaceAllowed(dbPath, saved);
      expect(allowed).toEqual({ status: 0, output: "allowed\n" });

      const denied = runPlaceAllowed(dbPath, {
        ...saved,
        stableId: "900000000000001104",
      });
      expect(denied).toEqual({ status: 0, output: "not allowed\n" });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("treats another place kind as not allowed even when app, account, and stable ID match", () => {
    const dir = mkdtempSync(join(tmpdir(), "osl-place-allowed-"));
    const dbPath = join(dir, "places.sqlite");
    try {
      const saved = {
        app: "discord",
        account: "osl-owner-0103",
        kind: "account",
        stableId: "900000000000001103",
      };
      const db = createDb(dbPath);
      savePlaceRecord(db, saved);
      db.close();

      const wrongKind = runPlaceAllowed(dbPath, {
        ...saved,
        kind: "conversation",
      });
      expect(wrongKind).toEqual({ status: 0, output: "not allowed\n" });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("refuses to answer against a database missing the 0101 contract marker", () => {
    const dir = mkdtempSync(join(tmpdir(), "osl-place-allowed-"));
    const dbPath = join(dir, "places.sqlite");
    try {
      const db = new DatabaseSync(dbPath);
      db.exec("CREATE TABLE worker_schema_capabilities (capability TEXT PRIMARY KEY, version INTEGER)");
      db.close();

      const result = runPlaceAllowed(dbPath, {
        app: "discord",
        account: "osl-owner-0103",
        kind: "account",
        stableId: "900000000000001103",
      });
      expect(result.status).toBe(2);
      expect(result.output).toContain("account ownership binding contract is not applied");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
