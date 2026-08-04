# The `0038` pair — expand/contract step order

**D-175.** The two `0038` migrations were a coupled change with no safe
single-step ordering: applying them broke the Worker running in production, and
deploying the consuming Worker first broke that Worker. Both directions were
measured. This document is the replacement: a sequence in which **you can stop
after any step and production still works**, stated per step, with the evidence
for each claim.

Nothing in here has been run against production. Every number below comes from
the live schema dumped read-only, rebuilt locally, and driven with SQL lifted out
of the two Worker bundles.

---

## What changed in the files

| File | Was | Is |
|---|---|---|
| `migrations/0038_username_identity_hardening.sql` | columns + index + tombstones + 2 triggers, one step | **expand only**: nullable columns, backfill, index, tombstone table, retire trigger, retired-name rejection gated to the skeleton writer |
| `migrations/0038_account_ownership_binding_account_unique.sql` | column + index + replacement insert guard | **expand only**: column + index |
| `migrations-contract/0100_username_identity_contract.sql` | — | **new**: guards, then mandatory skeleton/display, mandatory tombstone skeleton, unconditional retired-name ABORT |
| `migrations-contract/0101_account_ownership_binding_contract.sql` | — | **new**: guard, then the replacement insert guard, verbatim |
| `wrangler.contract.toml` | — | **new**: selects `migrations-contract/` for `d1 migrations apply` |

**No constraint was dropped.** Four moved, each named in both halves:

| Constraint | Deferred from | Carried by |
|---|---|---|
| `username_skeleton` / `display_username` mandatory | `0038_username_identity_hardening.sql` | `0100` insert + update guards |
| `username_tombstones.skeleton` mandatory | `0038_username_identity_hardening.sql` | `0100` insert guard |
| retired-name rejection for a non-skeleton writer | `0038_username_identity_hardening.sql` | `0100` unconditional ABORT (in the expand window it is a silent no-op, so the claim is still refused) |
| `service_account_sha256` mandatory | `0038_account_ownership_binding_account_unique.sql` | `0101` replacement insert guard, byte-identical |

Everything else is live from the expand step: the skeleton unique index, the
tombstone table, retirement on all three release paths, retired-name rejection
for the consuming Worker, and the account/owner unique index.

---

## Why the split needs two directories

`wrangler d1 migrations apply` has **no subset selection** — it applies every
unapplied file in the configured `migrations_dir` or none. A contract file next
to its expand half would be dragged into the same batch, which is the coupled
change again. A second config with a different `migrations_dir` is the supported
way to hold it back. Verified live, read-only, 2026-08-04:

```
$ npx wrangler d1 migrations list osl-keyserver-prod --remote
  0038_account_ownership_binding_account_unique.sql
  0038_username_identity_hardening.sql
  0039_device_roster.sql
  0040_license_redemption.sql
  0041_space_event_queue_reserved.sql

$ npx wrangler d1 migrations list osl-keyserver-prod --remote --config wrangler.contract.toml
  0100_username_identity_contract.sql
  0101_account_ownership_binding_contract.sql
```

Both configs point at the same database and the same `d1_migrations` table.

**And the cost of that mechanism, stated up front:** the preflight gate on
`fix/migrate-preflight-gate` discovers migrations with a hardcoded
`path.join(packageRoot, "migrations")`, so **`migrations-contract/` is outside
it**. The gate is not this lane's to edit (D-173), so the gap is recorded in
`migrations-contract/README.md` with the one-line shape of the fix — take a list
of directories, and bind receipts to directory *and* filename. **The contract
step must not be run until the gate covers this directory.**

---

## The steps

Run the step-order test before and after any change to these files:

```sh
node scripts/expand-contract-step-order.mjs \
  --schema <live-schema.json> --deployed <deployed index.js> --candidate <dry-run index.js>
node scripts/expand-contract-mutants.mjs   --schema ... --deployed ... --candidate ...
```

### Step 0 · baseline

Production runs keyserver version `08df28ba` (2026-07-31T08:52:37Z) on the
schema through `0037`.

**Stop here and production works:** it is production.
**Measured:** the deployed bundle's 215 SQL literals all prepare, and its
username-claim, rename, retire, lookup, license-read and license-insert paths all
execute. The candidate bundle does **not**: 13 statements cannot prepare. That
is the deploy-first outage, reproduced as this harness's control.

### Step 1 · EXPAND — apply `migrations/`

```sh
cd keyserver-cf && npx wrangler d1 migrations apply osl-keyserver-prod --remote
```

Applies the five pending files: both `0038`s (now expand-only), `0039`, `0040`,
`0041`. Additive only. No unique index that can collide on a row the deployed
Worker writes, and no trigger that can abort one.

**Stop here and production works.** Measured, both generations:

| | deployed `08df28ba` | candidate |
|---|---|---|
| statements that cannot prepare | 0 | 0 |
| behaviour problems | 0 | 0 |

The deployed Worker keeps claiming, renaming, retiring and looking up usernames;
its `SELECT * FROM licenses` still yields the two fields its two callers read
(`license.revoked_at`, `license.subscription_id`) with `0040`'s four new columns
present and `null`. The candidate Worker becomes fully serveable at this step —
that is what makes step 3 safe.

*Verify:*
```sh
npx wrangler d1 execute osl-keyserver-prod --remote --json --command \
  "SELECT name FROM sqlite_master WHERE name IN
     ('idx_username_directory_skeleton','username_tombstones',
      'username_directory_retire_before_delete',
      'username_directory_reject_retired_before_insert',
      'username_directory_reject_retired_legacy_writer',
      'idx_account_ownership_proof_bindings_service_account',
      'device_roster','space_event_queue')"
# -> 8 rows.
npx wrangler d1 execute osl-keyserver-prod --remote --json --command \
  "SELECT capability, version FROM worker_schema_capabilities
    WHERE capability LIKE '%_expand'"
# -> username_identity_hardening_expand=1, account_ownership_binding_account_unique_expand=1
```

*Rollback:* none is needed — nothing the deployed Worker does changed. If one is
demanded anyway, the expand is reversible without data loss because
`username_directory` and `account_ownership_proof_bindings` are both **empty**
in production (measured 2026-08-04, `rows=0` each):
`DROP TRIGGER`/`DROP INDEX`/`DROP TABLE` for the objects listed above, and
`ALTER TABLE ... DROP COLUMN` for the three new columns. Do not do this while the
candidate Worker is deployed.

### Step 2 · BACKFILL — repeatable, no schema change

```sh
npx wrangler d1 execute osl-keyserver-prod --remote --command \
 "UPDATE username_directory
     SET username_skeleton = COALESCE(username_skeleton, username),
         display_username  = COALESCE(display_username, username)
   WHERE username_skeleton IS NULL OR display_username IS NULL"
```

This is the same statement the expand migration already ran; it is repeated as a
separate step because the deployed Worker keeps writing NULL-skeleton rows for as
long as it is serving. Run it, and run it again immediately before step 5.

**Stop here and production works:** it touches only rows the deployed Worker
wrote and sets them to the value the original migration's own backfill defined
(`skeleton = username`). Expected effect today: **0 rows** — the directory is
empty.

### Step 3 · DEPLOY the consuming Worker

```sh
cd keyserver-cf && npx wrangler deploy
```

**This step is gated elsewhere and is not this document's to authorize** — see
D-173 (the scheme-1 evidence cannot be produced and would be false) and D-166.
The schema does not change here.

**Stop here and production works, in both directions.** Measured: on the
post-expand schema the candidate serves with 0 unpreparable statements and 0
behaviour problems, **and the deployed Worker still does too** — so a rollback to
`08df28ba` at this point is safe. That is the property the expand phase bought.

*Verify:* `npx wrangler deployments list` shows the new version at 100%.

### Step 4 · SETTLE and verify

Wait at least 60 seconds after step 3, then:

```sh
npx wrangler d1 execute osl-keyserver-prod --remote --json --command \
  "SELECT COUNT(*) AS directory_rows_without_skeleton FROM username_directory
    WHERE username_skeleton IS NULL OR display_username IS NULL"
# -> 0.  Any other number means a pre-expand Worker has written since the expand.
npx wrangler d1 execute osl-keyserver-prod --remote --json --command \
  "SELECT COUNT(*) AS tombstones_without_skeleton FROM username_tombstones
    WHERE skeleton IS NULL"
# -> 0.
```

**Stop here and production works:** no schema change, both generations serve.

**What this step does not prove, stated plainly.** A zero count is evidence that
no pre-expand Worker has *written*, not proof that none is *live*. A pre-expand
Worker that is up but idle leaves no trace, and Cloudflare exposes no
version→commit mapping and no capability list to ask directly. The settle wait
and the 100% deployment reading are the other two pieces, and neither is a proof
either. This is the weakest link in the sequence and it is named here rather than
buried.

### Step 5 · CONTRACT — apply `migrations-contract/`

```sh
cd keyserver-cf && npx wrangler d1 migrations apply osl-keyserver-prod --remote \
  --config wrangler.contract.toml
```

Both files begin with guards, before any DDL, so a refusal changes nothing:

* `0100` refuses if the expand half is not applied (`no such column:
  username_skeleton`), if any directory row has a NULL skeleton or display name
  (`CHECK constraint failed: username_directory_rows_without_skeleton_must_be_zero`),
  or if any tombstone has a NULL skeleton
  (`CHECK constraint failed: username_tombstones_rows_without_skeleton_must_be_zero`).
* `0101` refuses if any binding row has no account commitment
  (`CHECK constraint failed: account_ownership_bindings_without_commitment_must_be_zero`).
  That refusal has no repair: `0037`'s immutability triggers make such a row
  neither updatable nor deletable. It cannot happen from the deployed Worker,
  which never names the table.

**Stop here and production works — with one stated exception.** Measured: the
candidate serves with 0 unpreparable statements and 0 behaviour problems. The
**deployed** generation no longer claims usernames, and that is the definition of
contracting rather than a defect: the whole point of the step is to forbid a
write shape only the old generation performs.

The damage of a rollback past this point is bounded on purpose. The abort text is
`username_directory.username_skeleton and display_username are required`, which
falls inside the deployed Worker's own catch
(`/username_directory\.username|UNIQUE|PRIMARY/i`, present in the downloaded
bundle), so a rolled-back Worker answers **409 "username is unavailable"** — a
wrong answer for a free name, but not a 500 and not a stack trace. Everything
else it does keeps working.

*Undo, if a rollback below the candidate is genuinely required.* There is no
reverse migration file, because a file in either directory would be swept up by
the next `migrations apply`. Run it as SQL:

```sh
npx wrangler d1 execute osl-keyserver-prod --remote --command "
DROP TRIGGER username_directory_skeleton_required_insert;
DROP TRIGGER username_directory_skeleton_required_update;
DROP TRIGGER username_tombstones_skeleton_required;
DROP TRIGGER username_directory_reject_retired_before_insert;
CREATE TRIGGER username_directory_reject_retired_before_insert
BEFORE INSERT ON username_directory
WHEN NEW.username_skeleton IS NOT NULL
 AND EXISTS (SELECT 1 FROM username_tombstones
              WHERE username = NEW.username OR skeleton = NEW.username_skeleton)
BEGIN SELECT RAISE(ABORT, 'username is retired'); END;
CREATE TRIGGER username_directory_reject_retired_legacy_writer
BEFORE INSERT ON username_directory
WHEN NEW.username_skeleton IS NULL
 AND EXISTS (SELECT 1 FROM username_tombstones WHERE username = NEW.username)
BEGIN SELECT RAISE(IGNORE); END;
DROP TRIGGER account_ownership_proof_bindings_insert_guard;
"
```
then re-create `0037`'s original insert guard from
`migrations/0037_account_ownership_proof_required.sql`. The `d1_migrations` rows
for `0100`/`0101` stay; delete them only if you intend to re-apply the contract.

*Verify success instead:*
```sh
npx wrangler d1 execute osl-keyserver-prod --remote --json --command \
  "SELECT capability, version FROM worker_schema_capabilities
    WHERE capability IN ('username_identity_hardening','account_ownership_binding_account_unique')"
# -> 2 rows, version 1 each.
```

---

## The summary table

| After step | Deployed `08df28ba` | Candidate | Stop here? |
|---|---|---|---|
| 0 baseline | SERVES | BROKEN (13 statements) | yes — it is production |
| 1 expand | SERVES | SERVES | **yes** |
| 2 backfill | SERVES | SERVES | **yes** |
| 3 deploy | SERVES | SERVES | **yes**, and rollback is safe |
| 4 settle | SERVES | SERVES | **yes** |
| 5 contract | claims refused (409) | SERVES | yes — but do not roll the Worker back |

---

## Two corrections this work produced

1. **`0038_account_ownership_binding_account_unique` never could break the
   deployed Worker.** The downloaded production bundle contains
   `account_ownership_proof_bindings` **0 times** and routes only
   `/v1/account-ownership/challenge`. D-166, D-167 and D-174 each recorded it as
   a proven break; all three came from a hand-written harness that reconstructed
   a deployed write path the running artifact does not have. It is split
   expand/contract here anyway, because the mandatory-column guard is a
   contracting change on principle and the split costs nothing.
2. **`0040_license_redemption` is inert for the deployed Worker, and now it is
   measured rather than suspected.** The deployed Worker does issue
   `SELECT * FROM licenses WHERE license_hash = ?` — that part of D-175's
   correction stands. Its only two callers (`handleLicenseValidate`,
   `handleBillingPortal` in the downloaded bundle) read `license.revoked_at` and
   `license.subscription_id` and nothing enumerates the row's keys, so the four
   nullable columns arrive as ignored `null`s. The step-order test executes that
   statement after `0040` and asserts both fields.
