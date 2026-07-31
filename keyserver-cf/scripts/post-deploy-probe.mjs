#!/usr/bin/env node
/// Post-deploy probe for the keyserver's 2026-07-26 audit fixes.
///
/// A deploy without a live probe is an assumption. This proves the fixes are
/// running in production, not merely that a deploy command succeeded.
///
/// Run AFTER `wrangler deploy`, per keyserver-cf/DEPLOY.md §11b.
///
///   node scripts/post-deploy-probe.mjs --host https://keyserver.oslprivacy.com \
///        [--user <known-opaque-osl-id>] [--inbox-probe --yes]
///
/// # Why this imports the Worker's own source
///
/// The control-inbox probe has to produce a signature the live Worker will
/// accept. Re-implementing `canonicalControlInboxPostBytes` here would create a
/// second copy that can drift from the one the Worker verifies against, and a
/// drifted probe fails with 401 and looks like a deploy problem. It imports the
/// real module instead, which Node can load directly because `canonical.ts` has
/// no imports of its own and Node 23+ strips types natively.

import { canonicalControlInboxPostBytes } from "../src/lib/canonical.ts";
import {
  controlInboxDispositionHealthError,
} from "./post-deploy-health-capability.mjs";

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const value = (name) => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};

const HOST = (value("--host") ?? "").replace(/\/$/, "");
const KNOWN_USER = value("--user");
const RUN_INBOX = flag("--inbox-probe");
const CONFIRMED = flag("--yes");

if (!HOST) {
  console.error("usage: node scripts/post-deploy-probe.mjs --host https://<keyserver> [--user <id>] [--inbox-probe --yes]");
  process.exit(2);
}

let failures = 0;
const pass = (name, detail = "") => console.log(`PASS  ${name}${detail ? ` — ${detail}` : ""}`);
const fail = (name, detail) => {
  failures++;
  console.log(`FAIL  ${name} — ${detail}`);
};
const skip = (name, why) => console.log(`SKIP  ${name} — ${why}`);

const b64 = (bytes) => Buffer.from(bytes).toString("base64");

// ---------------------------------------------------------------------------
// Probe 0 — exact 0031 Worker/schema capability. DECISIVE, read-only.
//
// A legacy Worker returns only {ok:true}; accepting that would make rollback
// silent even though its drain ignores delivery_status. The exact capability
// is therefore a release gate, not informational telemetry.
// ---------------------------------------------------------------------------
async function probeControlInboxDispositionCapability() {
  const name = "control-inbox sender disposition schema is active";
  const response = await fetch(`${HOST}/v1/healthz`, {
    headers: { "cache-control": "no-cache" },
  });
  const body = await response.json().catch(() => null);
  const healthError = controlInboxDispositionHealthError(response.status, body);
  if (healthError) {
    fail(name, healthError);
    return;
  }
  pass(name);
}

// ---------------------------------------------------------------------------
// Probe 1 — public lookup publishes no lifecycle timing. DECISIVE.
//
// Old response carried `last_rotated_at` and a full-precision `registered_at`.
// Costs nothing, writes nothing, and needs only an identifier that already
// exists.
// ---------------------------------------------------------------------------
async function probePubkeys() {
  const name = "pubkeys publishes no lifecycle timing";
  if (!KNOWN_USER) {
    skip(name, "pass --user <a-known-opaque-osl-id> to run this");
    return;
  }
  const response = await fetch(`${HOST}/v1/pubkeys/${encodeURIComponent(KNOWN_USER)}`);
  if (response.status !== 200) {
    fail(name, `expected 200 for a known identity, got ${response.status}`);
    return;
  }
  const body = await response.json();
  if ("last_rotated_at" in body) {
    fail(name, "last_rotated_at is still published — the old Worker is live");
    return;
  }
  if (!/^\d{4}-\d{2}-\d{2}T00:00:00Z$/.test(String(body.registered_at))) {
    fail(name, `registered_at is not date-granular: ${JSON.stringify(body.registered_at)}`);
    return;
  }
  // The key bundle must be intact — minimisation must not have cost anything.
  for (const field of ["ik_x25519_pub", "ik_ed25519_pub", "ik_mlkem768_pub", "registration_sig"]) {
    if (typeof body[field] !== "string") {
      fail(name, `key bundle is incomplete: ${field} missing`);
      return;
    }
  }
  pass(name, `registered_at=${body.registered_at}, no last_rotated_at, bundle intact`);
}

// ---------------------------------------------------------------------------
// Probe 2 — snowflake refusal (migration 0029) is still live. DECISIVE, free.
//
// Included because probe 3 depends on 0029's `identity_lookup_enabled` column
// existing; if this regresses, probe 3's setup SQL is wrong too.
// ---------------------------------------------------------------------------
async function probeSnowflakeRefusal() {
  const name = "Discord snowflakes are refused as identities";
  const response = await fetch(`${HOST}/v1/pubkeys/900000000000000001`);
  if (response.status !== 400) {
    fail(name, `expected 400, got ${response.status}`);
    return;
  }
  pass(name);
}

// ---------------------------------------------------------------------------
// Probe 3 — a full inbox refuses rather than evicting a stranger's row.
// DECISIVE, but it WRITES PRODUCTION STATE. Opt-in behind --inbox-probe --yes.
//
// The endpoint requires a registered sender and a registered recipient, so
// there is no way to exercise it without production identity rows existing.
// Two compromises keep that contained:
//
//   * The probe does not call /v1/register. The operator inserts two clearly
//     namespaced rows with the printed SQL, so setup is auditable and exactly
//     reversible, and the real registration path is left alone.
//   * The victim row is filler, not a third identity. It only has to be a row
//     from a *different* sender, and `sender_id` is free text.
// ---------------------------------------------------------------------------
async function probeControlInbox() {
  const name = "full control inbox refuses instead of evicting a stranger";
  if (!RUN_INBOX) {
    skip(name, "pass --inbox-probe --yes to run it (writes production rows; see the SQL it prints)");
    return;
  }

  const stamp = new Date().toISOString().slice(0, 10);
  const nonce = Buffer.from(crypto.getRandomValues(new Uint8Array(6))).toString("hex");
  const recipientId = `probe-inbox-recipient-${stamp}-${nonce}`;
  const attackerId = `probe-inbox-attacker-${stamp}-${nonce}`;
  const victimSender = `probe-inbox-victim-${stamp}-${nonce}`;

  const keyPair = await crypto.subtle.generateKey({ name: "Ed25519" }, true, ["sign", "verify"]);
  const attackerPubB64 = b64(new Uint8Array(await crypto.subtle.exportKey("raw", keyPair.publicKey)));
  const stubX = b64(new Uint8Array(32).fill(0x11));
  const stubMlkem = b64(new Uint8Array(1184).fill(0x22));
  const stubSig = b64(new Uint8Array(64).fill(0x44));
  const nowIso = new Date().toISOString();
  const nowSec = Math.floor(Date.now() / 1000);
  const expiresAt = nowSec + 7 * 24 * 60 * 60;

  // `identity_lookup_enabled = 1` is required: migration 0029 quarantines every
  // identity by default, and `getUserForVerify` filters on it.
  const setupSql = [
    `INSERT INTO users (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub, ik_x25519_signature, registered_at, identity_lookup_enabled) VALUES ('${recipientId}','${stubX}','${attackerPubB64}','${stubMlkem}','${stubSig}','${nowIso}',1);`,
    `INSERT INTO users (user_id, ik_x25519_pub, ik_ed25519_pub, ik_mlkem768_pub, ik_x25519_signature, registered_at, identity_lookup_enabled) VALUES ('${attackerId}','${stubX}','${attackerPubB64}','${stubMlkem}','${stubSig}','${nowIso}',1);`,
    // One victim row, oldest in the lane, from an unrelated sender.
    `INSERT INTO control_inbox (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at, kind, collapse_key) VALUES (randomblob(16),'${recipientId}','${victimSender}','probe-victim-scope',x'01',${expiresAt},${nowSec},'',NULL);`,
    // 511 filler rows across 16 synthetic senders, all newer than the victim,
    // so the lane sits at exactly the 512 recipient-wide cap.
    `WITH RECURSIVE cnt(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM cnt WHERE x < 511) INSERT INTO control_inbox (id, recipient_id, sender_id, scope_id, bundle, expires_at, created_at, kind, collapse_key) SELECT randomblob(16),'${recipientId}','probe-filler-${nonce}-'||(x%16),'probe-filler-scope-'||x,x'01',${expiresAt},${nowSec + 10},'',NULL FROM cnt;`,
  ].join("\n");

  const cleanupSql = [
    `DELETE FROM control_inbox WHERE recipient_id = '${recipientId}';`,
    `DELETE FROM control_inbox_requests WHERE sender_id = '${attackerId}';`,
    `DELETE FROM users WHERE user_id IN ('${recipientId}','${attackerId}');`,
  ].join("\n");

  console.log("\n--- run this BEFORE continuing -------------------------------");
  console.log(`npx wrangler d1 execute osl-keyserver-prod --remote --command "${setupSql.replace(/\n/g, " ")}"`);
  console.log("--- and this AFTERWARDS, whatever the result ------------------");
  console.log(`npx wrangler d1 execute osl-keyserver-prod --remote --command "${cleanupSql.replace(/\n/g, " ")}"`);
  console.log("---------------------------------------------------------------\n");

  if (!process.env.OSL_PROBE_SETUP_DONE) {
    skip(name, "set OSL_PROBE_SETUP_DONE=1 and re-run once the setup SQL above has been applied");
    return;
  }

  const bundle = new TextEncoder().encode(`probe-bundle-${nonce}`);
  const bundleHash = new Uint8Array(await crypto.subtle.digest("SHA-256", bundle));
  const scopeId = `probe-attacker-scope-${nonce}`;
  const timestampMs = Date.now();
  const signature = new Uint8Array(await crypto.subtle.sign(
    { name: "Ed25519" },
    keyPair.privateKey,
    canonicalControlInboxPostBytes({
      sender_id: attackerId,
      recipient_id: recipientId,
      scope_id: scopeId,
      timestamp_ms: timestampMs,
      bundle_sha256: bundleHash,
    }),
  ));

  const response = await fetch(`${HOST}/v1/control-inbox`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      sender_id: attackerId,
      recipient_id: recipientId,
      scope_id: scopeId,
      timestamp_ms: timestampMs,
      bundle_b64: b64(bundle),
      signature_b64: b64(signature),
    }),
  });
  const body = await response.json().catch(() => ({}));

  if (response.status === 401) {
    fail(name, "signature rejected — the probe and the Worker disagree on canonical bytes, not a fix regression");
    return;
  }
  if (response.status === 201) {
    fail(name, "post was ACCEPTED at a full inbox: the old evicting Worker is live, and a stranger's row was just destroyed");
    return;
  }
  if (response.status !== 429 || body.error !== "recipient_inbox_full") {
    fail(name, `expected 429 recipient_inbox_full, got ${response.status} ${JSON.stringify(body)}`);
    return;
  }

  console.log(`      verify the victim row survived, then run the cleanup SQL:`);
  console.log(`      npx wrangler d1 execute osl-keyserver-prod --remote --command "SELECT COUNT(*) FROM control_inbox WHERE sender_id = '${victimSender}'"  # expect 1`);
  pass(name, `429 recipient_inbox_full, scope=${body.scope}`);
}

console.log(`keyserver post-deploy probe → ${HOST}\n`);
if (RUN_INBOX && !CONFIRMED) {
  console.log("--inbox-probe writes production identity and control-inbox rows.");
  console.log("Re-run with --yes once you have read the SQL it will ask you to apply.");
  process.exit(0);
}

// A transport error is a probe failure, not a crash: an operator reading this
// output after a deploy needs a verdict line, not a stack trace.
const run = async (name, probe) => {
  try {
    await probe();
  } catch (error) {
    fail(name, `probe threw: ${error instanceof Error ? error.message : String(error)}`);
  }
};

await run("control-inbox sender disposition schema is active", probeControlInboxDispositionCapability);
await run("pubkeys publishes no lifecycle timing", probePubkeys);
await run("Discord snowflakes are refused as identities", probeSnowflakeRefusal);
await run("full control inbox refuses instead of evicting a stranger", probeControlInbox);

console.log(
  "\nNOTE: the reserved-headroom rule (a sender holding <4 rows may use the full 512)"
  + "\nis not probed live — proving it needs a second full-lane setup, and the refusal"
  + "\npath above is the half that carries the security property.",
);
console.log(failures === 0 ? "\nALL PROBES PASSED" : `\n${failures} PROBE(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
