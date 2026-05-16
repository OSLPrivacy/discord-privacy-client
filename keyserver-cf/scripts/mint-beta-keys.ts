/**
 * mint-beta-keys.ts — local batch minter for paid beta licenses.
 *
 * Mints N valid paid licenses WITHOUT Stripe, by REUSING the exact
 * code the `checkout.session.completed` webhook uses:
 *
 *   - `generateLicenseKey()`  (src/lib/license.ts)  — key format +
 *     SHA-256 hash + HMAC checksum. NOT reimplemented here.
 *   - `upsertSubscription()` / `insertLicense()`  (src/lib/
 *     subscriptions.ts) — the real INSERT statements. We drive them
 *     with a capturing D1 stub so the SQL that hits prod is byte-for-
 *     byte what the webhook would write (no hand-written rows, no
 *     hand-written hash). If those helpers change, this script
 *     follows automatically.
 *
 * The raw keys are printed to stdout ONCE. They are never written to
 * disk and never stored raw server-side (only SHA-256(key) lands in
 * D1) — by design. Capture them at run time; they are unrecoverable.
 *
 * ──────────────────────────────────────────────────────────────────
 * CHECKSUM / SECRET — READ THIS
 * ──────────────────────────────────────────────────────────────────
 * `POST /v1/license/validate` HARD-REJECTS a key whose 2-char
 * checksum doesn't verify under the deployed worker's
 * `LICENSE_HMAC_SECRET` (see src/endpoints/license.ts: `if
 * (!checksum_ok) return UNKNOWN`). The checksum is computed from that
 * secret. THEREFORE: this script must mint with the SAME secret the
 * production worker runs with, or every key it produces is DEAD.
 *
 *   - Export `LICENSE_HMAC_SECRET` to the exact value of the
 *     deployed wrangler secret before running.
 *   - If — and only if — the deployed worker has NO `LICENSE_HMAC_
 *     SECRET` set, the worker falls back to the literal
 *     "osl-license-default-v1"; pass that.
 *   - The script refuses to run if the env var is unset (it will
 *     NOT silently default — a wrong guess wastes a batch).
 *
 * ──────────────────────────────────────────────────────────────────
 * USAGE
 * ──────────────────────────────────────────────────────────────────
 *   cd keyserver-cf
 *   LICENSE_HMAC_SECRET='<same value as the deployed worker secret>' \
 *     npx tsx scripts/mint-beta-keys.ts \
 *       --label beta-wave-1 --count 20 --permanent
 *
 *   # exactly one of --permanent | --days <N> is required (operator
 *   # decision: permanent comps vs. time-limited)
 *
 * The script writes two files under scripts/out/ (git-ignored) and
 * prints the raw keys. INSPECT the mint SQL, then apply to prod D1:
 *
 *   npx wrangler d1 execute osl-keyserver-prod --remote \
 *     --file=scripts/out/mint-beta-<label>-<ts>.sql
 *
 * ──────────────────────────────────────────────────────────────────
 * REVOKING THESE LATER (cleanup before / at public launch)
 * ──────────────────────────────────────────────────────────────────
 * Every row this script creates is tagged so it can be found and
 * removed precisely:
 *
 *   subscription_id  = beta_grant_<label>_<NN>
 *   customer_id      = beta_grant_<label>
 *   customer_email   = beta+<label>-<NN>@oslprivacy.com
 *
 * A companion `revoke-beta-<label>-<ts>.sql` is generated alongside
 * the mint file. Two options (both in that file):
 *
 *   (A) SOFT REVOKE — keep the rows for audit, lock paid features
 *       immediately. This is the in-code equivalent of
 *       `revokeLicensesForSubscription(db, subId, 'manual')`:
 *
 *         UPDATE licenses
 *            SET revoked_at = unixepoch(), revoked_reason = 'manual'
 *          WHERE subscription_id LIKE 'beta_grant_<label>_%'
 *            AND revoked_at IS NULL;
 *         UPDATE subscriptions
 *            SET status = 'REVOKED', updated_at = unixepoch()
 *          WHERE subscription_id LIKE 'beta_grant_<label>_%';
 *
 *       (validate returns REVOKED on a revoked license OR a REVOKED
 *        subscription — either line alone is sufficient; do both.)
 *
 *   (B) HARD DELETE — wipe all beta grants entirely. Delete licenses
 *       first (FK child), then subscriptions:
 *
 *         DELETE FROM licenses     WHERE subscription_id LIKE 'beta_grant_%';
 *         DELETE FROM subscriptions WHERE subscription_id LIKE 'beta_grant_%';
 *
 * Apply whichever you want via:
 *   npx wrangler d1 execute osl-keyserver-prod --remote \
 *     --file=scripts/out/revoke-beta-<label>-<ts>.sql
 *
 * This script is LOCAL ONLY. It exposes NO HTTP endpoint.
 */

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { generateLicenseKey } from "../src/lib/license.js";
import { insertLicense, upsertSubscription } from "../src/lib/subscriptions.js";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(SCRIPT_DIR, "out");

// 2100-01-01T00:00:00Z — "permanent" beta comp. Far past any
// plausible public-launch cleanup date but still a real epoch the
// EXPIRED cron sweep / validate logic handles normally.
const PERMANENT_PERIOD_END = 4102444800;

interface Captured {
  sql: string;
  params: unknown[];
}

/** Minimal D1 stand-in. `upsertSubscription` / `insertLicense` only
 *  ever call `.prepare(sql).bind(...params).run()` and ignore the
 *  result, so we record the bound statement instead of executing it.
 *  Reusing the real helpers means we never hand-write the row SQL. */
function capturingDb(sink: Captured[]): import("@cloudflare/workers-types").D1Database {
  const stmt = (sql: string) => ({
    bind: (...params: unknown[]) => ({
      run: async () => {
        sink.push({ sql, params });
        return { success: true, meta: {} };
      },
    }),
  });
  return { prepare: (sql: string) => stmt(sql) } as unknown as
    import("@cloudflare/workers-types").D1Database;
}

function sqlLiteral(v: unknown): string {
  if (v === null || v === undefined) return "NULL";
  if (typeof v === "number") {
    if (!Number.isFinite(v)) throw new Error(`non-finite bind value: ${v}`);
    return String(v);
  }
  if (typeof v === "string") return `'${v.replace(/'/g, "''")}'`;
  throw new Error(`unsupported bind type: ${typeof v}`);
}

/** Inline the captured statement's bind params. Handles both the
 *  numbered (`?1..?8`, upsertSubscription) and positional (`?`,
 *  insertLicense) placeholder styles the helpers use. */
function inlineStatement(c: Captured): string {
  const sql = c.sql.trim();
  if (/\?\d/.test(sql)) {
    return (
      sql.replace(/\?(\d+)/g, (_m, n: string) => {
        const idx = Number(n) - 1;
        if (idx < 0 || idx >= c.params.length) {
          throw new Error(`placeholder ?${n} out of range`);
        }
        return sqlLiteral(c.params[idx]);
      }) + ";"
    );
  }
  let i = 0;
  return (
    sql.replace(/\?/g, () => {
      if (i >= c.params.length) throw new Error("positional ? out of range");
      return sqlLiteral(c.params[i++]);
    }) + ";"
  );
}

function parseArgs(argv: string[]) {
  const a = {
    count: 20,
    label: "",
    days: 0,
    permanent: false,
    db: "osl-keyserver-prod",
  };
  for (let i = 0; i < argv.length; i++) {
    const k = argv[i];
    if (k === "--count") a.count = Number(argv[++i]);
    else if (k === "--label") a.label = String(argv[++i] ?? "");
    else if (k === "--days") a.days = Number(argv[++i]);
    else if (k === "--permanent") a.permanent = true;
    else if (k === "--db") a.db = String(argv[++i] ?? "");
    else throw new Error(`unknown arg: ${k}`);
  }
  return a;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));

  const hmacSecret = process.env.LICENSE_HMAC_SECRET;
  if (!hmacSecret) {
    console.error(
      "REFUSING: LICENSE_HMAC_SECRET is not set.\n" +
        "Keys minted with the wrong secret fail checksum validation on the\n" +
        "deployed worker and are permanently dead. Export the SAME value the\n" +
        "production worker uses (or 'osl-license-default-v1' iff the worker has\n" +
        "no LICENSE_HMAC_SECRET secret set), then re-run.",
    );
    process.exit(1);
  }
  if (!args.label || !/^[a-z0-9][a-z0-9-]*$/.test(args.label)) {
    console.error("REFUSING: --label is required, lowercase [a-z0-9-], e.g. --label beta-wave-1");
    process.exit(1);
  }
  if (!Number.isInteger(args.count) || args.count < 1 || args.count > 500) {
    console.error("REFUSING: --count must be an integer 1..500");
    process.exit(1);
  }
  const hasDays = Number.isInteger(args.days) && args.days > 0;
  if (args.permanent === hasDays) {
    console.error(
      "REFUSING: choose exactly one of --permanent OR --days <N>.\n" +
        "Operator decision: permanent comps vs. ~90-day time-limited beta.",
    );
    process.exit(1);
  }

  const nowSec = Math.floor(Date.now() / 1000);
  const periodEnd = args.permanent
    ? PERMANENT_PERIOD_END
    : nowSec + args.days * 86400;

  const mint: Captured[] = [];
  const db = capturingDb(mint);

  const distributed: { idx: string; key: string; subscriptionId: string }[] = [];

  for (let i = 1; i <= args.count; i++) {
    const nn = String(i).padStart(2, "0");
    const subscriptionId = `beta_grant_${args.label}_${nn}`;
    const customerId = `beta_grant_${args.label}`;
    const email = `beta+${args.label}-${nn}@oslprivacy.com`;

    // REUSE: identical key + hash + checksum the webhook produces.
    const license = await generateLicenseKey(hmacSecret);

    // REUSE: identical INSERT path the webhook runs. Subscription
    // first (licenses.subscription_id FK), then the license row.
    await upsertSubscription(db, {
      subscription_id: subscriptionId,
      customer_id: customerId,
      customer_email: email,
      status: "ACTIVE",
      current_period_end: periodEnd,
      cancel_at_period_end: 0,
    });
    await insertLicense(db, {
      license_hash: license.hash,
      subscription_id: subscriptionId,
    });

    distributed.push({ idx: nn, key: license.plaintext, subscriptionId });
  }

  const ts = new Date().toISOString().replace(/[:.]/g, "-");
  mkdirSync(OUT_DIR, { recursive: true });
  const mintPath = join(OUT_DIR, `mint-beta-${args.label}-${ts}.sql`);
  const revokePath = join(OUT_DIR, `revoke-beta-${args.label}-${ts}.sql`);

  const periodLabel = args.permanent
    ? `permanent (current_period_end=${PERMANENT_PERIOD_END}, 2100-01-01Z)`
    : `${args.days} days (current_period_end=${periodEnd})`;

  // NOTE: NO `BEGIN TRANSACTION`/`COMMIT`/`SAVEPOINT`/`ROLLBACK`.
  // Cloudflare D1's `wrangler d1 execute --file` manages atomicity
  // itself and rejects raw transaction-control SQL (the whole file
  // is refused). The file is plain semicolon-terminated statements;
  // D1 runs them as one atomic batch. Per-key order (subscription
  // row before its license row, for the FK) is preserved by the
  // capture order.
  const mintSql =
    `-- OSL beta licenses — label="${args.label}" count=${args.count}\n` +
    `-- generated ${new Date().toISOString()} — period: ${periodLabel}\n` +
    `-- Rows reuse upsertSubscription()/insertLicense() exactly.\n` +
    `-- Raw keys were printed to stdout ONCE and are NOT in this file.\n` +
    `-- D1 runs every statement in this file as one atomic batch — do\n` +
    `-- NOT add BEGIN/COMMIT/SAVEPOINT (wrangler d1 execute rejects them).\n` +
    `-- Apply: npx wrangler d1 execute ${args.db} --remote --file=${mintPath}\n` +
    mint.map(inlineStatement).join("\n") +
    `\n`;

  const like = `beta_grant_${args.label}_%`;
  const revokeSql =
    `-- REVOKE OSL beta licenses — label="${args.label}"\n` +
    `-- D1 runs this file as one atomic batch; no BEGIN/COMMIT allowed.\n` +
    `-- (A) SOFT REVOKE — keep rows, lock paid features now.\n` +
    `UPDATE licenses SET revoked_at = unixepoch(), revoked_reason = 'manual'\n` +
    ` WHERE subscription_id LIKE '${like}' AND revoked_at IS NULL;\n` +
    `UPDATE subscriptions SET status = 'REVOKED', updated_at = unixepoch()\n` +
    ` WHERE subscription_id LIKE '${like}';\n` +
    `\n-- (B) HARD DELETE — uncomment to wipe ALL beta grants entirely.\n` +
    `-- DELETE FROM licenses     WHERE subscription_id LIKE 'beta_grant_%';\n` +
    `-- DELETE FROM subscriptions WHERE subscription_id LIKE 'beta_grant_%';\n`;

  writeFileSync(mintPath, mintSql, { mode: 0o600 });
  writeFileSync(revokePath, revokeSql, { mode: 0o600 });

  // Raw keys — shown ONCE, never persisted raw.
  console.log(
    `\n=== ${args.count} beta licenses minted (label="${args.label}", ${periodLabel}) ===`,
  );
  console.log("Distribute these RAW keys. They are NOT stored raw anywhere:\n");
  for (const d of distributed) {
    console.log(`  ${d.idx}  ${d.key}   (${d.subscriptionId})`);
  }
  console.log(`\nMint SQL    : ${mintPath}`);
  console.log(`Revoke SQL  : ${revokePath}`);
  console.log(
    `\nNEXT: inspect the mint SQL, then apply to PROD D1:\n` +
      `  npx wrangler d1 execute ${args.db} --remote --file=${mintPath}\n`,
  );
}

main().catch((e) => {
  console.error("mint-beta-keys failed:", e);
  process.exit(1);
});
