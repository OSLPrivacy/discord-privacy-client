# External overlay security contract

OSL's intended Windows composer is a separate OSL-owned window positioned over
the verified message composer in an isolated hosted service child or a
separately verified native app. It does not
inject into a page, read credentials, call service APIs, or send without the
configured user action. A small OSL ownership mark remains visible so the field
cannot become an invisible password-capture surface.

The external overlay is **not enabled for any service yet**. The current code is
a fail-closed state machine and a bundled local visual prototype. A service may
enable it only after a dedicated adapter can prove the exact service, account,
conversation, recipients, native window generation, non-password field, and
pixel bounds. Moving, resizing, minimizing, changing focus, changing account or
chat, or losing geometry certainty hides it immediately and requires explicit
recalibration where appropriate.

The same rules apply to the planned reading overlay. Each plaintext replacement
must bind to an exact encrypted carrier-message identity and verified pixel
bounds. Clicks pass through outside the OSL-rendered plaintext. Native reactions,
edits, deletes, or other unsupported interactions must not be imitated.

Only visible messages and a small adjacent scroll buffer may exist as plaintext
in RAM. Free is capped at 250 messages / 4 MiB / 30 minutes; Pro at 1,000
messages / 16 MiB / 2 hours. The cache is an LRU with hard message, byte, and expiry
limits; plaintext is zeroized on eviction, lock, context loss, app switch, and drop. The
`EncryptedLocalOverlayCacheRecord` type is only a **format contract** today:
authenticated, identity-keyed SSD persistence is not wired. Until it is, there
is no SSD fallback. Future persistence must remain encrypted and authenticated
locally; plaintext at rest and server-side message caches are forbidden. Its
default hard budget is deliberately bounded: 50 MiB / 30 days on Free and 512 MiB /
90 days on Pro, with oldest-first eviction and a user-visible disable/lower-limit
control. It is an on-demand local cache, never OSL-hosted history.
