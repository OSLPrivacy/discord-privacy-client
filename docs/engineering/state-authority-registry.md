# State-authority registry

This registry is a change-time contract. Before adding a persisted datum, flag,
or worker, add one record below and have its owner review it. A record has one
writer: other components request mutations through that authority. Readers and
every lifecycle reset are recorded so review can find stale copies and cleanup
obligations.

`scripts/check-state-authority.py` validates this document in CI. Its JSON fence
is deliberately machine-readable without a third-party parser. Do not replace a
required field with `TBD`, an issue link, or a generic statement: describe the
actual serialization, migration, downgrade, and lifecycle behavior. `reuse`
must explicitly say which existing ledger/contract was considered and why it is
not the authority; `conflicts` names competing sources of truth or says why none
exists. `mirrors` is required whenever a datum is represented on multiple
native/renderer/website surfaces, and every mirror needs a concrete drift test.

`paths` lists all files that implement the datum. On pull requests, CI compares
added lines for storage writes and worker creation against those paths; adding a
new one without a record fails the gate.

```json
{
  "records": []
}
```

## Record shape

```text
{
  "id": "example-sync-enabled",
  "datum": "Whether account sync is enabled",
  "kind": "flag",
  "authority": {"owner": "native settings service", "path": "apps/example/src/settings.rs", "reason": "only durable settings writer"},
  "readers": [{"actor": "renderer preferences", "path": "apps/example-ui/src/preferences.ts"}],
  "writers": [{"actor": "native settings service", "path": "apps/example/src/settings.rs"}],
  "resets": [{"actor": "account switch", "path": "apps/example/src/session.rs"}],
  "graphs": {"dependency": "docs/engineering/dependency-graph.md#settings", "interaction": "docs/engineering/interaction-graph.md#account-switch"},
  "lifecycle": {"serialization": "versioned settings JSON", "migration": "v1 missing key defaults false", "default": "false for new accounts", "downgrade": "older clients ignore the additive key", "startup": "native service loads before renderer boot", "restart": "reload from settings JSON", "crash": "atomic write preserves prior value", "account_switch": "discard account-scoped cache then reload", "teardown": "remove account settings on account deletion"},
  "reuse": "Reviewed existing account settings ledger; this is a field in that ledger, not a new store.",
  "conflicts": "No second truth: renderer cache is read-only and receives snapshots from native.",
  "paths": ["apps/example/src/settings.rs", "apps/example-ui/src/preferences.ts"],
  "mirrors": [{"surface": "native", "path": "apps/example/src/settings.rs", "drift_assertion": "settings integration test compares serialized value"}, {"surface": "renderer", "path": "apps/example-ui/src/preferences.ts", "drift_assertion": "preferences test compares IPC snapshot"}]
}
```
