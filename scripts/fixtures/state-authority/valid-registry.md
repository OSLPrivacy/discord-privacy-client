# Fixture registry

```json
{
  "records": [{
    "id": "fixture-sync-enabled",
    "datum": "fixture persisted synchronization flag",
    "kind": "flag",
    "authority": {"owner": "native fixture service", "path": "native/settings.rs", "reason": "single durable owner"},
    "readers": [{"actor": "renderer", "path": "renderer/settings.ts"}, {"actor": "website", "path": "website/status.ts"}],
    "writers": [{"actor": "native fixture service", "path": "native/settings.rs"}],
    "resets": [{"actor": "startup", "path": "native/settings.rs"}, {"actor": "account switch", "path": "native/session.rs"}, {"actor": "teardown", "path": "native/cleanup.rs"}],
    "graphs": {"dependency": "docs/graphs.md#fixture", "interaction": "docs/interactions.md#fixture"},
    "lifecycle": {"serialization": "versioned JSON", "migration": "v1 adds false default", "default": "false", "downgrade": "unknown key ignored", "startup": "load native before IPC", "restart": "reload JSON", "crash": "atomic replacement", "account_switch": "clear account cache", "teardown": "delete fixture data"},
    "reuse": "Reviewed the settings ledger; this field reuses it rather than creating another store.",
    "conflicts": "No conflict: renderer and website are snapshot-only mirrors.",
    "paths": ["native/settings.rs", "native/worker.rs", "renderer/settings.ts", "website/status.ts"],
    "mirrors": [{"surface": "native", "path": "native/settings.rs", "drift_assertion": "native fixture test asserts serialized snapshot"}, {"surface": "renderer", "path": "renderer/settings.ts", "drift_assertion": "renderer fixture test compares IPC snapshot"}, {"surface": "website", "path": "website/status.ts", "drift_assertion": "website fixture test compares published snapshot"}]
  }, {
    "id": "fixture-cleanup-worker",
    "datum": "fixture account cleanup worker",
    "kind": "worker",
    "authority": {"owner": "native cleanup supervisor", "path": "native/worker.rs", "reason": "one supervised worker owns cleanup scheduling"},
    "readers": [{"actor": "native status API", "path": "native/status.rs"}],
    "writers": [{"actor": "native cleanup supervisor", "path": "native/worker.rs"}],
    "resets": [{"actor": "crash recovery", "path": "native/worker.rs"}],
    "graphs": {"dependency": "docs/graphs.md#cleanup-worker", "interaction": "docs/interactions.md#teardown"},
    "lifecycle": {"serialization": "worker checkpoint JSON", "migration": "checkpoint v1 is accepted", "default": "not running", "downgrade": "older client discards checkpoint", "startup": "supervisor restores one worker", "restart": "supervisor restarts from checkpoint", "crash": "checkpoint is replayed", "account_switch": "cancel then start account worker", "teardown": "join worker and delete checkpoint"},
    "reuse": "Reviewed the task scheduler; this worker is scheduled by it rather than creating a second queue.",
    "conflicts": "No competing worker is permitted; UI can only request status.",
    "paths": ["native/worker.rs"]
  }]
}
```
