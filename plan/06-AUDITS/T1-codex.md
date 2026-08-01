# T1 adversarial audit — not ready to certify

**Auditor:** Codex (independent pass)  
**Date:** 2026-08-01  
**Verdict:** **BLOCKED — no completed T1 merge exists to audit. Do not treat this as a passing
transport audit.**

## Gate evidence

`T1-83` requires **all of T1**. The track currently contains 50 `### T1-…` task headings.
On the integration `HEAD`, the only completed implementation task commits identifiable as T1 work
are:

| task | evidence on `HEAD` |
|---|---|
| T1-05 | `f9a98a72 claim transport crate for Tor` |
| T1-11 | `dbcdfab1 remove blob fetch token gate` |
| T1-15 | `e97f7d76 make blob route oracle-free` |
| T1-19 | `a22e274e add fail-closed OHTTP readiness stub` |
| T1-45 | `e1ae3911 decide legacy src-tauri command fate` |

In particular, the required transport contract/test work (T1-01/02/03), 160-bit capability
work (T1-10 and T1-30 through T1-37), persistent-frame delivery (T1-50 through T1-56), offline
send/burn work (T1-62/63), Tor routing (T1-70 through T1-75), and end-to-end/offline evidence
(T1-80 through T1-82) have not landed on this `HEAD`.

## Owner-decision audit criteria for the eventual rerun

The eventual T1 audit must reject any merge that violates the canonical decisions in
`09-DECISIONS.md`:

- D1/D2: pointer-only transport, using a 160-bit unlinkable capability; no inline fallback.
- D3: a fetch must not authenticate or reveal the fetcher; possession is authorization.
- D4/D5: server-enforced Padmé blob padding and a constant-rate persistent connection, not
  slow polling.
- D28-D30: local burn is immediate offline, remote effects are queued and shown pending; payloads
  are eagerly fetched, decrypted, and persisted locally; offline-inapplicable actions fail
  honestly.

## Limits

This is deliberately a gate verdict, not an implementation verdict. Auditing the partial merge
as if it were complete would create a false certification. No product test was added or run:
the task's declared test is `none — infra`, and this change is audit evidence only.

The shared repository has an existing empty Git object error when traversing all refs
(`objects/f4/fe776e…`); the `HEAD` history used above remains readable. Repairing repository
storage is outside T1-83's scope.
