# Transparency-log policy

**Status:** FROZEN 2026-08-01. **Owner:** T17. **Decision:** X11 in
`09-DECISIONS.md` overrides this document if they conflict.

```json transparency-log-policy
{
  "format": 1,
  "published_hash_list": {
    "delivery": "bundle the signed build-hashes.json list and signature into the client at build time",
    "runtime_verdict": "compare the locally computed executable SHA-256 against the bundled signed list",
    "network_refresh": "forbidden"
  },
  "transparency_log": {
    "provider": "Sigstore/Rekor via actions/attest-build-provenance",
    "purpose": "public, independent audit of release provenance",
    "runtime_queries": "forbidden",
    "availability": "Rekor has a 99.5% SLO, not an SLA; search endpoints are excluded"
  },
  "privacy": {
    "risk": "a live lookup reveals which build a user runs to a permanently public, bulk-queryable log observer",
    "rule": "never perform a Rekor, Sigstore, or transparency-log lookup from a client runtime path"
  },
  "enforcement": {
    "gate": "node scripts/check_no_runtime_transparency_queries.mjs",
    "runtime_roots": ["apps/osl-hub/src", "apps/osl-hub-ui/src"]
  }
}
```

The release workflow publishes provenance after building the installer. That record is for
auditors, release tooling, and people verifying an artifact outside the running client. It is
not a client dependency: log unavailability must neither change a local hash verdict nor reject
a peer during a conversation or handshake.

The signed published-hash list is the sole runtime authority. A stale or absent bundled list is
an honest local `Unknown` result; the client must not replace it with a network lookup.

Run the gate before accepting any change to a client runtime path:

```sh
node scripts/check_no_runtime_transparency_queries.mjs
```
