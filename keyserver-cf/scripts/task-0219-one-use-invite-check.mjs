#!/usr/bin/env node
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const keyserverRoot = dirname(dirname(fileURLToPath(import.meta.url)));

function migration(name) {
  return readFileSync(join(keyserverRoot, "migrations", name), "utf8");
}

function sha256Hex(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function inviteId(text) {
  const bytes = Buffer.from(text, "utf8");
  if (bytes.length !== 16) throw new Error(`${text} is not a 16-byte invite id`);
  return bytes;
}

function countPending(db, creator) {
  return db
    .prepare("SELECT COUNT(*) AS count FROM space_event_queue WHERE recipient_tag = ?")
    .get(creator).count;
}

function pendingFingerprint(db, creator) {
  const row = db
    .prepare("SELECT ciphertext FROM space_event_queue WHERE recipient_tag = ?")
    .get(creator);
  if (!row) throw new Error("pending request not found");
  return sha256Hex(row.ciphertext);
}

function seedInvite(db, id, creator, createdAt, expiresAt) {
  db.prepare(
    `INSERT INTO one_use_invite_links
      (invite_id, creator, intended_use, created_at, expires_at)
     VALUES (?, ?, 'space_admission', ?, ?)`,
  ).run(id, creator, createdAt, expiresAt);
}

function redeem(db, invite, requestCiphertext, now) {
  const row = db
    .prepare(
      `SELECT creator, intended_use, expires_at, consumed_at
         FROM one_use_invite_links
        WHERE invite_id = ?`,
    )
    .get(invite);
  if (!row || row.intended_use !== "space_admission") {
    return { ok: false, error: "invalid Space invite redemption" };
  }
  if (row.consumed_at !== null) return { ok: false, error: "invite already used" };
  if (row.expires_at <= now) return { ok: false, error: "invite expired" };

  db.exec("BEGIN IMMEDIATE");
  try {
    const update = db.prepare(
      `UPDATE one_use_invite_links
          SET consumed_at = ?
        WHERE invite_id = ?
          AND consumed_at IS NULL
          AND expires_at > ?`,
    ).run(now, invite, now);
    if (update.changes !== 1) {
      db.exec("ROLLBACK");
      const changed = db
        .prepare("SELECT expires_at, consumed_at FROM one_use_invite_links WHERE invite_id = ?")
        .get(invite);
      if (changed?.consumed_at !== null) return { ok: false, error: "invite already used" };
      if (changed && changed.expires_at <= now) return { ok: false, error: "invite expired" };
      return { ok: false, error: "invalid Space invite redemption" };
    }
    const eventId = Buffer.from("TASK0219EVENTID1", "utf8");
    const insert = db.prepare(
      `INSERT INTO space_event_queue
         (id, recipient_tag, ciphertext, expires_at, created_at, lease_until)
       SELECT ?, creator, ?, expires_at, ?, 0
         FROM one_use_invite_links
        WHERE invite_id = ? AND consumed_at = ?`,
    ).run(eventId, requestCiphertext, now, invite, now);
    if (insert.changes !== 1) throw new Error("could not queue Space invite redemption");
    db.exec("COMMIT");
    return { ok: true, request_fingerprint: sha256Hex(requestCiphertext) };
  } catch (error) {
    db.exec("ROLLBACK");
    throw error;
  }
}

const db = new DatabaseSync(":memory:");
try {
  db.exec(migration("0041_space_event_queue_reserved.sql"));
  db.exec(migration("0043_space_event_expiry_hardening.sql"));
  db.exec(migration("0044_one_use_invite_links.sql"));

  const label = "PLUM-0219";
  const requestName = "REQ-0219";
  const liveInvite = inviteId("PLUM-0219-live01");
  const expiredCopyInvite = inviteId("PLUM-0219-copy01");
  const creator = Buffer.alloc(32, 0x21);
  const requestCiphertext = Buffer.from(requestName, "utf8");
  const now = 1_800_000_000;
  const createdAt = now - 60;
  const expiresAt = now + 3600;

  seedInvite(db, liveInvite, creator, createdAt, expiresAt);
  const readable = db
    .prepare(
      "SELECT lower(hex(invite_id)) AS invite_id, consumed_at FROM one_use_invite_links WHERE invite_id = ?",
    )
    .get(liveInvite);
  if (!readable || readable.consumed_at !== null) throw new Error("PLUM-0219 is not readable");

  const before = countPending(db, creator);
  const first = redeem(db, liveInvite, requestCiphertext, now);
  if (!first.ok) throw new Error(`first redemption failed: ${first.error}`);
  const afterFirst = countPending(db, creator);
  const fingerprintBefore = pendingFingerprint(db, creator);

  const used = redeem(db, liveInvite, requestCiphertext, now);
  if (used.error !== "invite already used") throw new Error(`used refusal was ${used.error}`);

  seedInvite(db, expiredCopyInvite, creator, createdAt, expiresAt);
  db.prepare("UPDATE one_use_invite_links SET expires_at = ? WHERE invite_id = ?")
    .run(now - 1, expiredCopyInvite);
  const expired = redeem(db, expiredCopyInvite, requestCiphertext, now);
  if (expired.error !== "invite expired") throw new Error(`expiry refusal was ${expired.error}`);

  const afterRefusals = countPending(db, creator);
  const fingerprintAfter = pendingFingerprint(db, creator);
  if (before !== 0 || afterFirst !== 1 || afterRefusals !== 1) {
    throw new Error(`unexpected pending counts: ${before}/${afterFirst}/${afterRefusals}`);
  }
  if (first.request_fingerprint !== fingerprintBefore || fingerprintBefore !== fingerprintAfter) {
    throw new Error("REQ-0219 fingerprint changed");
  }

  console.log(`TASK0219_INVITE=${label}`);
  console.log(`TASK0219_INVITE_READABLE=${readable !== null}`);
  console.log(`TASK0219_PENDING_COUNT_BEFORE=${before}`);
  console.log(`TASK0219_FIRST_USE_REQUEST=${requestName}`);
  console.log(`TASK0219_PENDING_COUNT_AFTER_FIRST_USE=${afterFirst}`);
  console.log(`TASK0219_CHANGED_USED_FIELD_REFUSAL=${used.error}`);
  console.log("TASK0219_EXPIRED_COPY_CHANGED_ONLY=expires_at");
  console.log(`TASK0219_CHANGED_EXPIRY_FIELD_REFUSAL=${expired.error}`);
  console.log(`TASK0219_PENDING_COUNT_AFTER_REFUSALS=${afterRefusals}`);
  console.log(`TASK0219_REQ_0219_FINGERPRINT_BEFORE=${fingerprintBefore}`);
  console.log(`TASK0219_REQ_0219_FINGERPRINT_AFTER=${fingerprintAfter}`);
} finally {
  db.close();
}
