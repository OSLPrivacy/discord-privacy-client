# Service scope index and “This app” Burn

`service_scope_index v1` is an encrypted, authenticated write-ahead index keyed
by the active OSL identity, service, and local service-account profile. Before a
Hub operation may create or change protected scope state, the exact validated
scope, canonical channel identifiers, and local context-binding digest commit
to this index. App Burn holds the same transition barrier, freezes new writes,
and burns an immutable manifest with an idempotent per-scope journal.

Only service profiles created after clean index initialization can receive
`clean_post_index` coverage. Merely observing a write for an older profile
creates a `legacy_incomplete` entry; it does not upgrade coverage. One-way legacy
scope hashes cannot be safely reverse-mapped. A future trusted enumeration may
set `trusted_enumeration` only after its adapter proves complete coverage.

“This app” Burn is available only for complete coverage. It destroys indexed
OSL decryptability, scope mappings, local protected-ledger entries, caches, and
recorded encrypted blobs. Partial failures remain frozen and resume from the
journal. The service registry row, external browser/native profile, login
session, cookies, and native carrier history are never members of the manifest.
Unlink/logout is a separate user action.
