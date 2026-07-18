# OSL open-core and local-privacy boundary

Status: release architecture decision; not a claim that every item is complete.

OSL must be inspectable enough that users do not need to trust OSL Privacy to
protect message contents. Commercial defensibility must come from a reliable
hosted product and maintained service integration, not hidden cryptography.

## Public, reproducible security core

The following belongs in a public repository with deterministic test vectors,
threat models, reproducible builds, and independent-audit hooks:

- cryptographic primitives, key agreement, ratchets, sender-key design and
  attachment encryption;
- capsule and pointer wire formats, associated-data binding, padding and TTL
  semantics;
- local identity, key sealing, encrypted state, recovery and safety numbers;
- the trusted composer/decrypt overlay boundary and its capability manifest;
- local message-risk scanning rules and model interfaces;
- local history-cleanup plans, deletion receipts and honest limitations;
- friend, whitelist and per-scope authorization state machines;
- server data schemas proving that retained payloads are opaque and bounded;
- security tests, protocol test vectors, migration tools and release signature
  verification.

No private service may hold a second decryption implementation, escrow key,
plaintext logging path or secret algorithm required to validate OSL security.

## Private product layer

The following may live in a separate private repository because it is product
operations rather than the basis of confidentiality:

- hosted deployment topology, abuse controls, alert routing and operational
  dashboards;
- billing, fraud scoring, support tooling and internal release orchestration;
- frequently changing service layout signatures and the self-healing adapter
  training pipeline;
- QA account orchestration and reliability telemetry, after content and account
  identifiers have been removed or transformed locally;
- commercial ranking and scheduling policy for cleanup suggestions.

The public client must continue to work securely when these services are
unavailable. A private component may improve convenience or reliability; it
must not weaken encryption, grant itself content access or silently change a
security decision.

The repository is currently Apache-2.0. Code already published under that
license remains reproducible and redistributable. Before public release, move
private product code into a separate repository rather than attempting to hide
it inside an otherwise public tree or retroactively implying restrictions that
the existing license does not contain.

## Local-only cleanup assistant

The cleanup assistant is a one-party feature: a contact does not need OSL. A
trusted local adapter enumerates only the messages the signed-in user can
already view. Analysis runs on the user's device.

Each finding contains:

- service, account and conversation references;
- a local-only message locator and short preview;
- category and an understandable reason it was flagged;
- confidence, age and whether the user authored the message;
- available actions: jump to the message, show manual instructions, ignore, or
  perform a user-confirmed deletion through visible local automation.

The default scanner uses deterministic rules for credentials, financial data,
identity documents, precise locations, intimate media references, regulated or
illegal activity, threats and high-risk personal information. An optional
on-device model may add contextual suggestions. A cloud model cannot receive
message text, screenshots, embeddings, filenames, contacts or locators.

Scanning must be bounded, cancellable and resumable. Findings are encrypted at
rest with the active OSL identity, expire by default, and are never included in
analytics or crash reports. The UI always distinguishes:

- **reviewed locally** from **deleted on the service**;
- a successful deletion request from confirmed absence after refresh;
- OSL cryptographic burn from native-platform deletion;
- user-authored messages from other people's messages, which OSL normally
  cannot delete.

There is no automatic destructive default. Bulk deletion shows the exact
count, service and scope, supports dry-run review, and requires an explicit
local confirmation. If a service cannot offer reliable automation, OSL opens
or points to the message and provides current manual steps instead.

## Simple trust and whitelist flow

The normal path has three steps:

1. Add a friend by a short invite or scan code.
2. Compare a compact safety phrase when OSL reports a new or changed key.
3. Approve OSL's suggested scopes, such as this DM or this group.

OSL can suggest a scope from locally observed membership but cannot silently
grant it. A new member, changed key, account switch or ambiguous conversation
pauses encryption and asks once in the trusted local chrome. Advanced users can
open the full per-service/per-account/per-scope matrix; everyone else sees a
simple friend card with `Allowed here`, `Ask me`, or `Blocked`.

## Data-egress invariant

The client should enforce a narrow network allowlist by process and module:

- service webviews may contact only the selected service's allowlisted hosts;
- the cipher relay accepts only padded ciphertext plus a random capability;
- the key service receives only public-key and entitlement material defined in
  the public protocol;
- the local scanner has no network capability;
- crash and performance telemetry cannot contain service DOM, message text,
  screenshots, account credentials, contact lists, capsules or stable platform
  account identifiers.

CI should fail if the local scanner gains an HTTP dependency/capability, a
remote service document gains a native command capability, plaintext DTOs
derive `Debug`, or fixtures resembling credentials/private keys enter Git.

## Entitlement issuer boundary

A release client trusts exactly `https://keyserver.oslprivacy.com` for key and
entitlement protocol traffic. A local `keyserver.json` file cannot redirect a
release build, and the HTTP client does not follow redirects. Debug and test
builds may opt into numeric loopback HTTP(S) endpoints only; remote staging,
userinfo, query, and fragment overrides are rejected.

Production activation codes use the `OSL-` issuer marker. QA uses `OSLQ-` with
an independent server trust root. A release Hub rejects `OSLQ-` before making
a network request, and the production keyserver rejects it again before lookup.
Debug builds may accept `OSLQ-` only alongside the loopback keyserver seam.
