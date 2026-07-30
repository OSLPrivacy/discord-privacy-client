import {
  mkdtempSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";

type Bindable = string | number | bigint | null | Uint8Array;

export interface D2DurableChallengeFixture {
  challenge_id: string;
  sequence: number;
  issued_at_ms: number;
  expires_at_ms: number;
  expected_evidence_sha256: string;
  expected_transcript_root_sha256: string;
  expected_authority_snapshot_sha256: string;
}

export interface D2DurableChallengeRow extends D2DurableChallengeFixture {
  consumed_at_ms: number | null;
  consumed_evidence_sha256: string | null;
  consumed_transcript_root_sha256: string | null;
  consumed_authority_snapshot_sha256: string | null;
}

export interface FileBackedD2AuthorityD1 {
  d1: D1Database;
  raw: DatabaseSync;
  close(): void;
}

export interface D2DurableAuthorityFixture {
  directory: string;
  databasePath: string;
  connection: FileBackedD2AuthorityD1;
  openConnection(): FileBackedD2AuthorityD1;
  cleanup(): void;
}

export const D2_DURABLE_AUTHORITY_SCHEMA = `
CREATE TABLE d2_admission_authority_state (
  singleton_id INTEGER PRIMARY KEY NOT NULL CHECK (singleton_id = 1),
  authority_snapshot_sha256 TEXT NOT NULL
) STRICT;

CREATE TABLE d2_admission_challenges (
  challenge_id TEXT PRIMARY KEY NOT NULL,
  sequence INTEGER NOT NULL UNIQUE CHECK (sequence > 0),
  issued_at_ms INTEGER NOT NULL CHECK (issued_at_ms > 0),
  expires_at_ms INTEGER NOT NULL CHECK (expires_at_ms > issued_at_ms),
  expected_evidence_sha256 TEXT NOT NULL,
  expected_transcript_root_sha256 TEXT NOT NULL,
  expected_authority_snapshot_sha256 TEXT NOT NULL,
  consumed_at_ms INTEGER,
  consumed_evidence_sha256 TEXT,
  consumed_transcript_root_sha256 TEXT,
  consumed_authority_snapshot_sha256 TEXT,
  CHECK (
    (
      consumed_at_ms IS NULL
      AND consumed_evidence_sha256 IS NULL
      AND consumed_transcript_root_sha256 IS NULL
      AND consumed_authority_snapshot_sha256 IS NULL
    )
    OR
    (
      consumed_at_ms IS NOT NULL
      AND consumed_evidence_sha256 IS NOT NULL
      AND consumed_transcript_root_sha256 IS NOT NULL
      AND consumed_authority_snapshot_sha256 IS NOT NULL
    )
  )
) STRICT;
`;

function toBindable(value: unknown): Bindable {
  if (value === null || value === undefined) return null;
  if (
    typeof value === "string"
    || typeof value === "number"
    || typeof value === "bigint"
  ) {
    return value;
  }
  if (typeof value === "boolean") return value ? 1 : 0;
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  throw new TypeError(
    `unsupported D1 bind value: ${Object.prototype.toString.call(value)}`,
  );
}

function openAdapter(databasePath: string): FileBackedD2AuthorityD1 {
  const raw = new DatabaseSync(databasePath);
  raw.exec("PRAGMA busy_timeout = 5000");
  raw.exec("PRAGMA journal_mode = WAL");

  const bound = (sql: string, values: unknown[]) => {
    const params = values.map(toBindable);
    return {
      async run() {
        await Promise.resolve();
        const result = raw.prepare(sql).run(...params);
        return {
          success: true,
          meta: {
            changes: Number(result.changes),
            changed_db: Number(result.changes) > 0,
            rows_written: Number(result.changes),
          },
        };
      },
      async first<T>(): Promise<T | null> {
        await Promise.resolve();
        return (raw.prepare(sql).get(...params) ?? null) as T | null;
      },
      async all<T>() {
        await Promise.resolve();
        return {
          success: true,
          results: raw.prepare(sql).all(...params) as T[],
          meta: {},
        };
      },
    };
  };

  const prepare = (sql: string) => ({
    bind: (...values: unknown[]) => bound(sql, values),
    ...bound(sql, []),
  });

  const d1 = {
    prepare,
    async batch(statements: Array<{ run(): Promise<unknown> }>) {
      const results = [];
      for (const statement of statements) results.push(await statement.run());
      return results;
    },
    async exec(sql: string) {
      await Promise.resolve();
      raw.exec(sql);
      return { count: 0, duration: 0 };
    },
  } as unknown as D1Database;

  let closed = false;
  return {
    d1,
    raw,
    close() {
      if (closed) return;
      closed = true;
      raw.close();
    },
  };
}

export function createD2DurableAuthorityFixture(): D2DurableAuthorityFixture {
  const directory = mkdtempSync(join(tmpdir(), "osl-d2-durable-authority-"));
  const databasePath = join(directory, "authority.sqlite");
  const connections = new Set<FileBackedD2AuthorityD1>();

  const openConnection = () => {
    const connection = openAdapter(databasePath);
    connections.add(connection);
    return connection;
  };

  const connection = openConnection();
  connection.raw.exec(D2_DURABLE_AUTHORITY_SCHEMA);
  let cleaned = false;

  return {
    directory,
    databasePath,
    connection,
    openConnection,
    cleanup() {
      if (cleaned) return;
      cleaned = true;
      for (const item of connections) item.close();
      rmSync(directory, { recursive: true, force: true });
    },
  };
}

export function insertD2DurableChallenge(
  raw: DatabaseSync,
  challenge: D2DurableChallengeFixture,
): void {
  raw.exec("BEGIN IMMEDIATE");
  try {
    raw.prepare(`
      INSERT INTO d2_admission_authority_state (
        singleton_id,
        authority_snapshot_sha256
      ) VALUES (1, ?)
      ON CONFLICT(singleton_id) DO UPDATE SET
        authority_snapshot_sha256 = excluded.authority_snapshot_sha256
    `).run(challenge.expected_authority_snapshot_sha256);
    raw.prepare(`
      INSERT INTO d2_admission_challenges (
        challenge_id,
        sequence,
        issued_at_ms,
        expires_at_ms,
        expected_evidence_sha256,
        expected_transcript_root_sha256,
        expected_authority_snapshot_sha256
      ) VALUES (?, ?, ?, ?, ?, ?, ?)
    `).run(
      challenge.challenge_id,
      challenge.sequence,
      challenge.issued_at_ms,
      challenge.expires_at_ms,
      challenge.expected_evidence_sha256,
      challenge.expected_transcript_root_sha256,
      challenge.expected_authority_snapshot_sha256,
    );
    raw.exec("COMMIT");
  } catch (error) {
    raw.exec("ROLLBACK");
    throw error;
  }
}

export function readD2DurableChallengeRow(
  raw: DatabaseSync,
  challengeId: string,
): D2DurableChallengeRow | null {
  return (raw.prepare(`
    SELECT
      challenge_id,
      sequence,
      issued_at_ms,
      expires_at_ms,
      expected_evidence_sha256,
      expected_transcript_root_sha256,
      expected_authority_snapshot_sha256,
      consumed_at_ms,
      consumed_evidence_sha256,
      consumed_transcript_root_sha256,
      consumed_authority_snapshot_sha256
    FROM d2_admission_challenges
    WHERE challenge_id = ?
  `).get(challengeId) ?? null) as D2DurableChallengeRow | null;
}
