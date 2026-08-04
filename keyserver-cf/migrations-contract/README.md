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
