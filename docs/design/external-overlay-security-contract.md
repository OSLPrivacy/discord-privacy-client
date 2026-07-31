# External overlay security contract

OSL's intended Windows composer is a separate OSL-owned window positioned over
the verified message composer in an isolated hosted service child or a
separately verified native app. It does not
inject into a page, read credentials, call service APIs, or send without the
configured user action. A small OSL ownership mark remains visible so the field
cannot become an invisible password-capture surface.

The generic external overlay is **not enabled for any service**.
`external_overlay.rs` is a compiled but uncalled source prototype: it has no
production consumer, Tauri handler, registered command, or UI adapter. The
shipping desktop's Discord surface is a separate native overlay and does not use
this module. Within the prototype, the composer guard refuses changed context,
movement, resize, minimize, focus loss, password fields, and uncertain geometry.
Those are prototype state-machine properties, not shipping behavior. A future
service may enable the design only after a dedicated adapter can prove the exact
service, account, conversation, recipients, native window generation,
non-password field, and pixel bounds.

The same rules apply to the planned reading overlay. Each plaintext replacement
must bind to an exact encrypted carrier-message identity and verified pixel
bounds. Clicks pass through outside the OSL-rendered plaintext. Native reactions,
edits, deletes, or other unsupported interactions must not be imitated.

Within the prototype, `VisiblePlaintextCache` is configured for 250 messages /
4 MiB / 30 minutes on Free and 1,000 messages / 16 MiB / 2 hours on Pro. Its
entries use `Zeroizing<String>`; explicit eviction, expiry, `lock()`, and drop
clear them. No production lifecycle constructs or calls this cache. Context loss
and app switching do not call its `lock()`, and the shipping native Discord
renderer uses separate session-scoped JavaScript memory. These are not shipping
zeroization or retention guarantees. The `EncryptedLocalOverlayCacheRecord`
type is only a **format contract** today:
authenticated, identity-keyed SSD persistence is not wired. Until it is, there
is no SSD fallback. Future persistence must remain encrypted and authenticated
locally; plaintext at rest and server-side message caches are forbidden. Its
default hard budget is deliberately bounded: 50 MiB / 30 days on Free and 512 MiB /
90 days on Pro, with oldest-first eviction and a user-visible disable/lower-limit
control. It is an on-demand local cache, never OSL-hosted history.
