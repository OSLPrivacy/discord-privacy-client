# `migrations-contract/` — the contracting half of expand/contract migrations

These files are **not** in `migrations/` on purpose.

`wrangler d1 migrations apply` has **no subset selection** — it applies every
unapplied file in the configured `migrations_dir`, or none. A contracting
migration sitting next to its expanding half would therefore be dragged into the
same batch, which is exactly the coupled change D-175 measured as having no safe
single-step ordering.

Keeping them in a second directory, selected by a second Wrangler config, is the
supported way to say "apply these later":

```sh
# expand batch — safe while the older Worker generation is still serving
npx wrangler d1 migrations apply osl-keyserver-prod --remote

# ... deploy the consuming Worker, settle, verify ...

# contract batch — only once no older generation is serving
npx wrangler d1 migrations apply osl-keyserver-prod --remote \
  --config wrangler.contract.toml
```

Both configs point at the same database and the same `d1_migrations` table, so
an applied contract migration is recorded exactly like any other and cannot be
applied twice.

## ⚠ THIS DIRECTORY IS CURRENTLY OUTSIDE THE PREFLIGHT GATE

`scripts/migration-preflight-gate.mjs` (branch `fix/migrate-preflight-gate`)
discovers migrations with a hardcoded `path.join(packageRoot, "migrations")` —
lines 130, 221 and 260 all name that one directory, and line 260 also reads the
committed bytes as `HEAD:keyserver-cf/migrations/<name>`. So a file in
`migrations-contract/` is invisible to it: the gate would neither admit nor
refuse it.

**That is a gap, not a feature, and it was not created to route around the
gate.** It is recorded here rather than fixed because the gate is D-173's, and
this lane's instruction was to make the migrations safe, not to touch the gate.
The fix is small and belongs to whoever owns the gate: take a *list* of migration
directories instead of a constant, and bind each receipt to the directory as well
as the filename, so `migrations/0100_x.sql` and `migrations-contract/0100_x.sql`
can never be confused.

**Until that lands, the contract step must not be run.** The step order in
`EXPAND-CONTRACT-0038.md` is already blocked earlier than this — its deploy step
sits behind D-173 and D-166 — so nothing is unblocked by leaving it open, but it
must not be forgotten when those clear.

## Rules for files in here

1. **Numbers are reserved from 0100 upward.** The ordinary sequence in
   `migrations/` must never reach this range; if it ever gets close, move this
   range, not the other one. Names are globally unique in `d1_migrations`.
2. **Guards first, before any DDL.** Every file must begin with statements that
   refuse if (a) its expanding half has not been applied, and (b) live data
   shows a pre-expand writer is still writing. A refusal must change nothing.
3. **Carry the spec, do not restate it.** Each file names the constraint it is
   carrying forward and the expanding file it came from, and each expanding file
   names the contract file that took the constraint. Neither half is complete
   alone, and both say so.
4. **Contracting is one-way for the Worker.** After a contract migration lands,
   rolling the Worker back below the consuming generation is an outage. The undo
   for each file is written out in `EXPAND-CONTRACT-0038.md`; there is no
   automatic reverse migration, because a reverse file in either directory would
   be picked up by the next `migrations apply`.
