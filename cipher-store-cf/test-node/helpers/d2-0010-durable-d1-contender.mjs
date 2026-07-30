import { existsSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";

const [encoded] = process.argv.slice(2);
if (!encoded) {
  console.error("missing base64url contender input");
  process.exit(2);
}

const input = JSON.parse(Buffer.from(encoded, "base64url").toString("utf8"));
const deadline = Date.now() + 10_000;
while (!existsSync(input.barrier_path)) {
  if (Date.now() >= deadline) {
    console.error("contender barrier timed out");
    process.exit(3);
  }
  await new Promise((resolve) => setTimeout(resolve, 5));
}

const db = new DatabaseSync(input.database_path);
try {
  db.exec("PRAGMA busy_timeout = 5000");
  db.exec("PRAGMA journal_mode = WAL");
  const result = db.prepare(input.sql).run(
    input.consumed_at_ms,
    input.evidence_sha256,
    input.transcript_root_sha256,
    input.authority_snapshot_sha256,
    input.challenge_id,
    input.sequence,
    input.expires_at_ms,
    input.evidence_sha256,
    input.transcript_root_sha256,
    input.authority_snapshot_sha256,
    input.authority_snapshot_sha256,
    input.consumed_at_ms,
  );
  process.stdout.write(`${JSON.stringify(Number(result.changes) === 1)}\n`);
} finally {
  db.close();
}
