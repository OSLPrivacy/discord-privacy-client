# Accessibility bench evidence

`BenchEvidence` is the shared JSON record emitted by accessibility bench
collectors and consumed by independent gates. It is intentionally small,
nonsecret, and fail-closed: absence of consent, target binding, or observation
authority is a refusal, never permission.

The record describes only what the bench measured. It must not contain raw
message text, account identifiers, credentials, selectors copied from a user
profile, window handles, or unbounded logs. Any implementation that maps this
record into a typed language must hand-write `Debug` and `Display` equivalents
or omit them, so future fields cannot accidentally expose private values.

## BenchEvidence v1

The canonical wire form is UTF-8 JSON with exact keys. Unknown keys are refused.
All timestamps are Unix milliseconds. All hashes are lowercase SHA-256 hex.

```json
{
  "schema": "osl-a11y-bench-evidence-v1",
  "benchId": "native-visible-row-accessibility",
  "runId": "018f2c18-59e3-7cc6-98cf-6f3132439850",
  "createdAtUnixMs": 1772110800000,
  "subject": {
    "platform": "windows",
    "appBuildSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "executableSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    "surface": "conversation-view"
  },
  "consent": {
    "granted": true,
    "source": "local-manual-qa-run",
    "observedAtUnixMs": 1772110799000
  },
  "binding": {
    "method": "launched-subject-process",
    "subjectBindingSha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    "observedAtUnixMs": 1772110799500
  },
  "authority": {
    "collector": "windows-uia-read-only",
    "verifier": "independent-host-gate",
    "observedAtUnixMs": 1772110800100
  },
  "measurements": [
    {
      "name": "visible-row-count",
      "status": "pass",
      "observedAtUnixMs": 1772110800200,
      "facts": {
        "rowsObserved": 3,
        "ownRows": 1,
        "peerRows": 1,
        "unknownRows": 1
      }
    }
  ],
  "artifacts": [
    {
      "kind": "structured-json",
      "relativePath": "a11ybench/facts.json",
      "sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
      "byteLength": 2048,
      "containsUserContent": false
    }
  ],
  "privacy": {
    "noRawText": true,
    "noAccountIdentifiers": true,
    "noSecrets": true,
    "boundedArtifacts": true
  },
  "verdict": "pass"
}
```

## Field contract

| Field | Rule |
|---|---|
| `schema` | Must be exactly `osl-a11y-bench-evidence-v1`. |
| `benchId` | Stable, nonsecret bench name. It is a label, not a target selector. |
| `runId` | Canonical UUID for one bench run. It binds every artifact in the run. |
| `createdAtUnixMs` | Must be within the run window known to the verifier. |
| `subject.platform` | Must be a supported platform label. v1 accepts `windows`. |
| `subject.appBuildSha256` | Hash of the source or build identity under test. |
| `subject.executableSha256` | Hash of the exact executable observed by the collector. |
| `subject.surface` | Bounded product surface label, such as `conversation-view`. |
| `consent.granted` | Must be `true`. Missing, false, or ambiguous consent is `refused`. |
| `consent.source` | Must name a local, explicit run approval source. |
| `binding.method` | Must name the binding mechanism used before any measurement. |
| `binding.subjectBindingSha256` | Hash of the nonsecret process and surface binding facts. |
| `authority.collector` | Must name the read-only collector authority. |
| `authority.verifier` | Must name the independent verifier authority. |
| `measurements` | Non-empty array. Each entry must have a fixed `name`, `status`, timestamp, and bounded numeric or boolean facts. |
| `artifacts` | May be empty. Paths must be relative, inside the bundle root, hash-bound, size-bound, and marked `containsUserContent: false`. |
| `privacy` | Every flag must be `true`; a missing flag is refusal. |
| `verdict` | One of `pass`, `fail`, or `refused`. `pass` requires every measurement to pass. |

## Refusal rules

A verifier must return `refused` when any of these facts is absent or malformed:

1. explicit local consent;
2. subject binding captured before measurement;
3. read-only collector authority;
4. independent verifier authority;
5. exact executable hash;
6. at least one measurement;
7. privacy flags proving the evidence is nonsecret and bounded.

`fail` is reserved for a valid bench record that proves an accessibility
requirement was not met. `refused` means the record is not trustworthy enough to
decide the requirement.

## Same-file contract tests

These examples are the minimum unit tests for any `BenchEvidence` parser or
validator. They live with the record definition so every implementation tests
the same refusal boundary.

| Test | Mutation | Expected verdict |
|---|---|---|
| accepts complete positive record | Use the example above with verifier-known run bounds that include every timestamp. | `pass` |
| refuses missing consent | Remove `consent`. | `refused` |
| refuses denied consent | Set `consent.granted` to `false`. | `refused` |
| refuses missing binding | Remove `binding`. | `refused` |
| refuses late binding | Set `binding.observedAtUnixMs` after the first measurement timestamp. | `refused` |
| refuses missing authority | Remove `authority.collector` or `authority.verifier`. | `refused` |
| refuses unbounded artifact | Set an artifact `relativePath` to an absolute path or set `byteLength` above the verifier limit. | `refused` |
| refuses user content | Set `containsUserContent` to `true` or any `privacy` flag to `false`. | `refused` |
| refuses unknown fields | Add any top-level key not listed in v1. | `refused` |
| fails measured violation | Keep the record well-formed but set one measurement `status` to `fail`. | `fail` |

Recommended status reducer:

```text
if schema or exact keys are invalid: refused
if consent, binding, authority, subject, privacy, or artifact bounds are invalid: refused
if measurements is empty or any measurement is malformed: refused
if any measurement status is fail: fail
if every measurement status is pass: pass
otherwise: refused
```

The reducer is deliberately monotonic. Adding missing data later creates a new
record; it never upgrades an already emitted refusal in place.
