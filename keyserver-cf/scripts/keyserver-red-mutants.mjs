#!/usr/bin/env node
/**
 * Mutation proof for the keyserver red-test lane (fix/keyserver-red-tests).
 *
 * Each mutant reintroduces exactly one defect that this lane removed, runs the
 * suite that is supposed to catch it, and requires a NON-ZERO exit code.  A
 * mutant that survives means the gate is decoration.
 *
 * Run:  node scripts/keyserver-red-mutants.mjs
 */
import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const BT = String.fromCharCode(96);
const lines = (...rows) => rows.join("\n");

const MUTANTS = [
  {
    id: "M1-archive-eviction-order",
    why: "drop the ORDER BY from Archive.entries(), restoring id-lexicographic eviction receipts",
    edits: [{
      file: "src/archive/archive.ts",
      from: lines(
        "      " + BT + "SELECT id, object_key, received_at, expires_at, byte_length FROM archive_entries",
        "        ORDER BY received_at ASC, id ASC" + BT + ",",
      ),
      to: '      "SELECT id, object_key, received_at, expires_at, byte_length FROM archive_entries",',
    }],
    suites: ["test/integration/pool-separation.test.ts"],
  },
  {
    id: "M2-mail-fixture-does-not-release-the-retired-handle",
    why: "stop releasing the tombstone in claimUsername, so a handle the identity lane retired can never be re-held by the fixture",
    edits: [{
      file: "test/integration/mail.test.ts",
      from: '  await env.DB.prepare("DELETE FROM username_tombstones WHERE username = ? OR skeleton = ?")\n    .bind(username, username).run();\n',
      to: "",
    }],
    suites: ["test/integration/mail.test.ts"],
  },
  {
    id: "M3-mail-address-reissued-after-tombstone",
    why: "re-issue a permanently tombstoned OSL Mail address: drop the explicit reservation guard AND let the insert overwrite the tombstone",
    edits: [
      {
        file: "src/endpoints/mail.ts",
        from: '  if (reserved) return conflict("mail address is permanently reserved");',
        to: '  if (reserved && false) return conflict("mail address is permanently reserved");',
      },
      {
        file: "src/endpoints/mail.ts",
        from: "        " + BT + "INSERT INTO mail_address_epochs(address, username, user_id, address_epoch, state, created_at)",
        to: "        " + BT + "INSERT OR REPLACE INTO mail_address_epochs(address, username, user_id, address_epoch, state, created_at)",
      },
    ],
    suites: ["test/integration/mail.test.ts"],
  },
  {
    id: "M4-mail-consent-gate-removed",
    why: "deliver OSL-to-OSL mail without recipient consent",
    edits: [{
      file: "src/endpoints/mail.ts",
      from: '    if (consent?.allowed !== 1) return forbidden("recipient has not allowed this sender");',
      to: '    if (false) return forbidden("recipient has not allowed this sender");',
    }],
    suites: ["test/integration/mail.test.ts"],
  },
  {
    id: "M5-mail-burn-does-not-tombstone",
    why: "let a burned mailbox leave its address live instead of tombstoning it",
    edits: [{
      file: "src/endpoints/mail.ts",
      from: '    "UPDATE mail_address_epochs SET state = \'tombstoned\', tombstoned_at = ? WHERE user_id = ? AND state = \'active\'",',
      to: '    "UPDATE mail_address_epochs SET tombstoned_at = ? WHERE user_id = ? AND state = \'active\'",',
    }],
    suites: ["test/integration/mail.test.ts"],
  },
  {
    id: "M6-external-inbound-plaintext-not-scrubbed",
    why: "stop zeroing the external MIME plaintext after envelope encryption, so inbound cleartext survives in the buffer",
    edits: [{
      file: "src/mail/inbound.ts",
      from: "    offset += chunk.byteLength;\n    chunk.fill(0);",
      to: "    offset += chunk.byteLength;",
    }],
    suites: ["test/integration/mail.test.ts"],
  },
  {
    id: "M7-telegram-readvertises-inert-controls",
    why: "put the inert coordination controls back into /osl help",
    edits: [{
      file: "src/lib/telegram.ts",
      from: lines(
        '    "/osl downloads: download requests",',
        '  ].join("\\n"));',
      ),
      to: lines(
        '    "/osl downloads: download requests",',
        '    "/osl on|off|quiet|bind|unbind: coordination controls when owner binding is active",',
        '  ].join("\\n"));',
      ),
    }],
    suites: ["test/integration/telegram-route.test.ts"],
  },
  {
    id: "M8-stripe-relinks-license-to-payment-intent",
    why: "put the Stripe PaymentIntent id back into licenses.subscription_id",
    edits: [{
      file: "src/lib/stripe-checkout-claims.ts",
      from: "  const entitlementId = oneTimeEntitlementId(claim.license_hash);",
      to: "  const entitlementId = input.paymentIntentId;",
    }],
    suites: ["test/unit/stripe-checkout.test.ts", "test/integration/stripe-webhook.test.ts"],
  },
  {
    id: "M10-deployment-receipt-digest-is-degenerate",
    why: "emit an all-zero receipt_sha256, the placeholder the digest gate exists to reject",
    edits: [{
      file: "scripts/deployment-evidence-receipt-contract.mjs",
      from: "    receipt_sha256: sha256(Buffer.from(canonicalJson(envelope))),",
      to: '    receipt_sha256: "0".repeat(64),',
    }],
    suites: ["scripts/deployment-evidence-receipt.test.ts"],
    config: "vitest.node.config.ts",
  },
  {
    id: "M9-stripe-issues-active-entitlement",
    why: "issue a paid checkout as an ACTIVE entitlement instead of an unredeemed PENDING code",
    edits: [{
      file: "src/lib/stripe-checkout-claims.ts",
      from: '  const initialStatus = priorTerminal ? priorObservation.status : "PENDING";',
      to: '  const initialStatus = priorTerminal ? priorObservation.status : "ACTIVE";',
    }],
    suites: ["test/unit/stripe-checkout.test.ts"],
  },
];

let failures = 0;
for (const mutant of MUTANTS) {
  const touched = [...new Set(mutant.edits.map((edit) => edit.file))];
  const originals = new Map(touched.map((file) => [file, readFileSync(file, "utf8")]));
  const missing = mutant.edits.find((edit) => !originals.get(edit.file).includes(edit.from));
  if (missing) {
    console.log(`SURVIVED(unanchored) ${mutant.id}: pattern not found in ${missing.file}`);
    failures += 1;
    continue;
  }
  const mutated = new Map(originals);
  for (const edit of mutant.edits) {
    mutated.set(edit.file, mutated.get(edit.file).replace(edit.from, edit.to));
  }
  for (const [file, text] of mutated) writeFileSync(file, text);
  const args = mutant.config
    ? ["vitest", "run", "--config", mutant.config, ...mutant.suites]
    : ["vitest", "run", ...mutant.suites];
  const run = spawnSync("npx", args, {
    stdio: ["ignore", "pipe", "pipe"],
    encoding: "utf8",
  });
  for (const [file, text] of originals) writeFileSync(file, text);
  const code = run.status;
  const killed = code !== 0;
  if (!killed) failures += 1;
  console.log(`${killed ? "KILLED  " : "SURVIVED"} ${mutant.id}  exit=${code}  (${mutant.why})`);
}

console.log(failures === 0
  ? `\nall ${MUTANTS.length} mutants killed`
  : `\n${failures} of ${MUTANTS.length} mutants SURVIVED`);
process.exit(failures === 0 ? 0 : 1);
