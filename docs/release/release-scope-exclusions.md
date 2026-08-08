# Owner-ruled release exclusions

These are deliberate release boundaries, not failed release gates and not
permission to build the deferred work quietly. The `release-exclusions` block
is the one source used by both the published release notes and the Home tile
labels. Keep Liam's recorded words and the ruling date together here.

```json release-exclusions
{
  "owner": "Liam",
  "items": [
    {
      "id": "code-signing",
      "name": "Code signing",
      "ruling_date": "2026-07-31",
      "owner_words": "ship unsigned",
      "release_note": "Code signing is not in this release; the Windows installer ships unsigned."
    },
    {
      "id": "osl-notes",
      "name": "OSL Notes",
      "ruling_date": "2026-08-06",
      "owner_words": "OSL Notes was already out.",
      "release_note": "OSL Notes is not in this release and is not being built for it.",
      "tile_label": "Not started"
    },
    {
      "id": "osl-mail",
      "name": "OSL Mail",
      "ruling_date": "2026-08-06",
      "owner_words": "osl mail is on hold until the other stuff.",
      "release_note": "OSL Mail is not in this release and is not being built for it.",
      "tile_label": "Not started"
    }
  ]
}
```

For OSL Notes, 6 August is the dated reaffirmation recorded alongside the Mail
ruling: Notes "was already out" before that ruling. It is not presented as the
unknown earlier decision date.
