#!/usr/bin/env node
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const keyserverRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const migration = readFileSync(
  join(keyserverRoot, "migrations", "0044_one_use_invite_links.sql"),
  "utf8",
);

const db = new DatabaseSync(":memory:");
try {
  db.exec(migration);

  const createdAt = 1_800_000_000;
  const expiresAt = createdAt + 3600;
  db.prepare(
    `INSERT INTO one_use_invite_links
      (invite_id, creator, intended_use, created_at, expires_at)
     VALUES (?, ?, ?, ?, ?)`,
  ).run(
    Buffer.from("02160216021602160216021602160216", "hex"),
    Buffer.alloc(32, 0xaa),
    "space_admission",
    createdAt,
    expiresAt,
  );

  const row = db.prepare(
    `SELECT lower(hex(invite_id)) AS invite_id,
            lower(hex(creator)) AS creator,
            intended_use,
            use_limit,
            expires_at,
            consumed_at IS NULL AS unused
       FROM one_use_invite_links
      WHERE consumed_at IS NULL
      ORDER BY expires_at
      LIMIT 1`,
  ).get();

  if (!row || row.unused !== 1 || row.expires_at !== expiresAt) {
    throw new Error("unused invite record with expiry was not found");
  }

  console.log(
    `unused invite invite_id=${row.invite_id} creator=${row.creator} ` +
    `intended_use=${row.intended_use} use_limit=${row.use_limit} ` +
    `unused=${row.unused} expiry=${row.expires_at}`,
  );
} finally {
  db.close();
}
