# OSL security and privacy boundary

OSL is not considered secure merely because part of it is difficult to inspect.
The client cryptography, protocol formats, key lifecycle, local encrypted store,
scope permissions, burn logic, capability manifests, update verification, and
the tests that exercise those boundaries must remain publicly reviewable.

## What must remain public

- message and attachment encryption, ratchets, sender keys, safety numbers;
- local key sealing, password derivation, encrypted state and cleanup code;
- friend and per-scope approval rules;
- ciphertext-store and key-directory request/response formats, schemas,
  retention rules, and authentication checks;
- Tauri capabilities, CSP, service-host allowlists and updater verification;
- reproducible security, dependency, secret and release audits.

Service-specific reliability adapters and hosted deployment operations may be
maintained separately, but they must not hold decryption keys or become a
confidentiality boundary. A closed component may route opaque ciphertext; it
must not be required to trust a claim about encryption, deletion, scanning, or
recipient selection.

## Data handling

- OSL application servers must never receive message plaintext, attachment
  plaintext, recovery phrases, passwords, browser cookies, or service tokens.
- The cipher store accepts opaque E2E ciphertext and capability material only.
  Its request observability is disabled and rows have bounded server TTLs.
- The key server necessarily stores public keys, opaque protocol records and
  commerce records. Request observability is disabled. Operational logs must
  use fixed event names or aggregate counts, never URLs, identifiers, payloads,
  provider error bodies, email addresses, payment addresses, or tokens.
- Platform login state belongs to the embedded first-party browser profile. It
  is not copied to OSL servers and remote pages receive no Tauri capability.
  Browser-profile files are still ordinary WebView data protected by the OS
  account/full-disk encryption, not by OSL's message-store encryption.

OSL does render plaintext locally when the user types a draft, opens a
decrypted message, or reviews a local Scrub finding. Those previews exist in
OSL-owned process/DOM memory and may be visible to the screen, accessibility
tools, local malware, crash dumps, or a camera. They are never evidence that
the service page or an OSL server received the plaintext. Capture resistance is
best-effort defense in depth, not a promise that plaintext cannot be copied.

These points are explicit release limitations: a persistent integrated
browser cannot honestly promise that every cache or cookie byte is encrypted by
OSL. Production documentation must require OS full-disk encryption and must not
describe browser profiles or locally rendered previews as OSL-encrypted at rest.

## Local sensitive-content scanning

The current deterministic Scrub scanner can classify bounded text that the user
explicitly imports or supplies to the trusted local UI. It has no filesystem,
network, process-execution, runtime-IPC, persistence, or model-download path.
Automatic hosted-service page scanning and deletion are not implemented. Any
future scanner or connector expansion must:

1. run in a local process with no network capability;
2. accept only the text or attachment the user explicitly asks it to inspect;
3. keep plaintext in memory only and zero bounded buffers where practical;
4. emit a category and local explanation, never the original content;
5. have reproducible tests proving no HTTP, telemetry, log, crash-report, model
   download, clipboard-history or server-storage path receives the content;
6. remain advisory. It must not silently block, report, upload, or modify text.

Models and rules must be packaged locally with signed hashes. Cloud inference is
not a fallback for this feature.

## Friends and scope approval

Importing a signed friend code does not grant decryption. The user must verify
the safety number and then approve each DM, group, channel, or whole-space scope.
Whole-space approval is an explicit broad action, not a default or a side effect
of adding a friend. Key changes revoke verified state until rechecked.

## Honest one-party cleanup

One user can safely do three independent things:

1. destroy their local OSL keys, ledgers, caches and browser profile;
2. delete OSL ciphertext blobs when they possess the delete capability, while
   reporting each remote failure separately;
3. request deletion of messages they authored through the service's ordinary
   user-visible controls and show manual directions when automation is unsafe.

OSL must not claim that one party can erase another device, provider backups,
screenshots, exports, notifications, quoted text, or already-read plaintext.
Platform deletion remains a best-effort provider action; local key destruction
must still complete when the provider is offline.

## Reporting

Do not open a public issue containing an exploit, identity, log, ciphertext,
recovery phrase, token, or provider credential. Use the private security contact
listed on the project website. Never send test-account passwords or 2FA material
through an issue or chat; enter them directly into the first-party login page.
