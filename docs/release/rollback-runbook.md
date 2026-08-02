# Recovering from a bad OSL Privacy release

This is a recovery procedure, not a rollback mechanism. The updater accepts
only a greater version, so an installed build cannot be pushed backwards.
Rolling `hub-latest` back protects only people who have not updated.

## Triage

1. Stop promotion immediately and preserve the failing installer, its SHA-256,
   the release tag, workflow run URLs, and reports. Do not replace assets under
   the existing tag.
2. Remove the bad version from `hub-latest/latest.json` using the authorised
   promotion/recovery workflow. This prevents further eligible clients from
   being offered the bad release; it does not alter installed clients.
3. Classify whether the app reaches update checking. A build that fails before
   the updater starts cannot self-recover.

## Remediation

For an update-capable failure, make and test a fix, assign a **higher** SemVer
version, and release it through the normal signed candidate and VM gate. Never
retag, downgrade, or overwrite the broken version.

For a startup failure, publish the same higher-version repair for future
installs, then communicate an out-of-band manual recovery path: download the
new installer from the official release page, verify its published checksum,
run it, and reopen OSL. The user must be told about that page outside OSL;
the broken application cannot discover the repair itself.

## What cannot be undone

- A downloaded installer cannot be recalled.
- An installation cannot be remotely downgraded.
- A startup-bricked installation cannot receive an automatic repair.
- Moving `hub-latest` cannot repair an installation that already updated.

Record these limits and the number of manual user actions in the incident
report. Exercise this procedure against a deliberately broken test-feed build
before relying on it in production.
