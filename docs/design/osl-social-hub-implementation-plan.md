# OSL Privacy social integration implementation plan

Status: planning only
Date: 2026-07-16
Scope: future work after the current Discord reliability and security work is complete

## Executive decision

OSL should become a local-first social privacy hub with one OSL identity and separately authorized platform connections. The first three consumer connectors for the OSL 1.0 release candidate should be Telegram, Instagram and Signal, but they should not pretend to have equal technical support:

OSL ships as a packaged standalone Rust/Tauri desktop application. It is not a localhost website and does not ask the user to open the app in a normal browser. The application bundles its local UI assets and may open service sites in isolated embedded WebViews with separate profiles per account. Rust owns account brokering, encryption, policy, action verification and local persistence; untrusted service content never becomes the privileged app shell.

Embedded service profiles intentionally persist their own local cookies and
cache so users remain signed in. Those files are ordinary WebView2 profile
data, not OSL-encrypted message state. Remote service views receive no Tauri
capability. Plaintext drafts, decrypted overlays and Scrub excerpts may appear
in OSL-owned local process/DOM memory, but must never enter OSL networking,
analytics or logs; screen capture and a compromised local device remain outside
the confidentiality boundary.

- Telegram can be production-grade through the official TDLib client library.
- Instagram should launch as a guarded desktop beta using a self-healing UI adapter for personal accounts and official APIs wherever Meta actually permits them.
- Signal should launch only after an upstream, licensing, trademark and linked-device security review. It is an official-client integration problem, not a normal OAuth connector.

The best additional revenue connectors are Slack and Microsoft Teams because they have official APIs and business buyers. WhatsApp Business and Messenger/Instagram professional accounts are useful commercial lanes, but must not be presented as full access to personal inboxes. LinkedIn personal-message automation, TikTok DMs and Snapchat should remain out of the initial roadmap because platform access and account-safety risks are too high.

The new warning feature should be an optional, private, on-device pre-send risk nudge. It must not declare that a message is illegal, claim that law enforcement is monitoring the user, report the user, or automatically delete anything.

### Privacy boundary

The hub does not automatically make another platform end-to-end encrypted. OSL's ciphertext overlay can protect content only when every required participant is using a compatible OSL client and the connector supports safe content transformation. Otherwise OSL provides local hygiene, warning, account isolation and cleanup while the platform's own transport and encryption properties remain unchanged. The UI must show `OSL encrypted`, `platform-native encryption` or `not end-to-end encrypted` per conversation and must never collapse those into one generic lock icon.

## Product promises

OSL may promise:

- one OSL account and recovery identity across supported connectors;
- separate, explicit authorization for every connected platform account;
- plaintext analysis and search on the user's device by default;
- no sale, advertising use or model training on message content;
- capability-aware behavior that accurately says what each platform permits;
- destructive actions that are previewable, resumable, rate-limited and verifiable;
- connector drift detection, safe rollback and signed repairs;
- failure closed when OSL cannot prove a send, encryption or deletion action is safe.

OSL must not promise:

- that Instagram or another private web client will never change or break;
- that an unsent message was never seen, copied, forwarded, screenshotted or retained;
- that deletion from the user's view deletes every platform backup or recipient copy;
- that a risk warning is a legal determination;
- that one OSL login bypasses platform login, consent or multi-factor authentication;
- that every platform exposes the same history, deletion or encryption capabilities.

The honest resilience promise is: **detect drift quickly, disable unsafe actions, preserve the last known-good adapter, and recover through signed updates without silently sending plaintext or deleting the wrong content.**

## Usability is a release requirement

Most users should never need to understand connector tiers, key epochs, selector packs, OAuth scopes or deletion receipts. OSL should offer a three-minute default path:

1. Sign in to OSL.
2. Connect one or more accounts in their real platform login flows.
3. Choose `Basic`, `Balanced` or `Maximum`; recommend `Balanced`.
4. Choose `Manual`, `Clipboard` or `Double Enter` Send behavior; recommend Manual and allow a per-service/account override.
5. Show one plain-language summary of what OSL will and will not do.
6. Finish and let the overlays stay quiet until useful.

The first-run Send choices are:

- **Manual:** OSL prepares; the user places and sends.
- **Clipboard:** OSL encrypts and copies; the user pastes and sends.
- **Double Enter:** a first distinct user Enter encrypts and atomically places the capsule; a second distinct user Enter sends after fresh target verification.

No default mode silently sends. Settings preserves a clear inheritance chain from global to service to account, with `Reset to inherited` and `Undo last change`. Conversation-scoped temporary choices expire with the conversation. `Experimental Single Enter / one-click` is a separate Advanced, per-service/account opt-in: after one explicit Enter or click it may encrypt, atomically place and submit, but only after a realistic warning and all target checks. It remains visibly experimental and never permits unattended send or anti-detection behavior.

Profiles:

- `Basic`: metadata cleanup, tracker blocking and account-health reminders; no automatic deletion.
- `Balanced`: Basic plus local warnings, attachment sanitization, monthly cleanup review and native privacy features when the user explicitly enables them.
- `Maximum`: Balanced plus require-OSL-encryption policies, stricter public-post warnings and optional protected-network gating.

Principles:

- Safe defaults, with no automatic destructive action enabled during onboarding.
- Progressive disclosure: ordinary users see outcomes; advanced users can inspect scopes, receipts and connector details.
- One setting vocabulary across platforms, with capability-specific exceptions explained inline.
- Ask once for a recurring policy, then keep it visible, pausable and easy to revoke.
- Never ask a user to configure every friend or conversation. Verified OSL contacts inherit the global policy automatically unless overridden.
- Avoid notification fatigue. Combine low-priority findings into a digest and interrupt only before a risky send or destructive action.
- Use verbs such as `warn`, `remove from my account`, `unsend for everyone` and `OSL content expires`, not protocol jargon.
- Every warning offers one recommended action and one clear escape.
- The overlay occupies one status chip until expanded; the full hub holds complexity.
- Accessibility, localization, keyboard operation and readable contrast are launch gates.

Usability acceptance gates:

- A new user connects one account and understands their protection state in under three minutes without documentation.
- A usability test participant can explain the difference between local removal, platform unsend and OSL expiry after seeing the UI once.
- Basic and Balanced setup require no per-contact configuration.
- Advanced settings never block or clutter the default flow.
- A failed connector presents one actionable sentence, not a stack trace or generic `something went wrong`.
- Destructive previews show the exact scope and count without requiring technical knowledge.

## 1. Risk Warning

### 1.1 User experience

Add a Settings toggle named `Message risk warning`, with three levels:

- Off
- Balanced, the default if the user enables the feature
- Strict

Advanced settings expose category toggles and per-account, per-conversation and per-contact overrides. The first supported categories are:

- hate, dehumanization or identity-based harassment;
- threats, intimidation or encouragement of violence;
- discussion that appears to describe, plan or admit illegal activity;
- sexual content involving minors or ambiguous age;
- doxxing, secrets and personally identifying information;
- financial, medical, legal or workplace-sensitive information;
- high-confidence harassment or coercion;
- operational details whose disclosure creates a personal safety risk.

The warning appears before network transmission and highlights the relevant span when possible. It offers:

- Edit message
- Send anyway
- Do not warn for this kind of message
- Turn warnings off for this conversation

Recommended general copy:

> This draft may create legal, safety, employment or reputational risk. Messages can be saved, forwarded or disclosed. Review it before sending.

Recommended illegal-activity copy:

> This draft appears to discuss or describe potentially illegal activity. OSL cannot determine legality or provide legal advice. Consider removing unnecessary identifying or operational details before sending.

Do not use definitive copy such as “law enforcement can use this against you,” “this is illegal,” or “you are under surveillance.” The feature should inform decisions, not frighten users with an unsupported legal conclusion.

### 1.2 Classification pipeline

Run the entire default pipeline locally:

1. Normalize text without changing the draft.
2. Run deterministic detectors for secrets, account numbers, addresses, phone numbers and other high-confidence PII.
3. Run a compact contextual classifier for risk categories and quoted/reported speech.
4. Apply conversation context, user threshold and local allowlists.
5. Produce category, confidence, highlighted span and a short non-judgmental explanation.
6. Discard transient inference inputs after the result is rendered.

Rules and classifier must be context-aware. A protected identity word, news quotation, anti-racist discussion, song lyric or victim report must not trigger merely because it contains a keyword. Precision is more important than maximum recall for the default setting.

Cloud classification may be offered only as a separate opt-in later. It cannot be enabled by default, cannot be required for basic warnings, and must disclose exactly what leaves the device and how long it is retained.

Inbound message classification is outside the first release. It creates a different consent, retention and moderation surface and should receive a separate threat model before it is considered.

### 1.3 Safety invariants

- The warning engine never sends, blocks, reports or deletes content by itself.
- The engine never reports users or risk scores to OSL, a platform, law enforcement, an employer or another participant.
- Plaintext and highlighted spans never enter analytics, crash reports or debug logs.
- Local feedback is encrypted and stays local unless a user explicitly exports a diagnostic sample.
- The engine does not advise a user how to evade investigation or destroy evidence.
- The result is framed as uncertainty and potential risk, not guilt or legality.
- Users can always inspect why the warning appeared and override it.
- Category and threshold changes are versioned so behavior is reproducible.

### 1.4 Warning acceptance gates

- Zero plaintext network requests in an offline packet-capture test.
- Zero plaintext, draft hashes or classifier spans in logs and crash reports.
- P95 warning latency below 100 ms for deterministic checks and below 300 ms for the local contextual model on minimum supported hardware.
- Protected-class terms alone produce no warning in the test corpus.
- Quotation, condemnation, reporting and reclamation cases have dedicated regression fixtures.
- Default false-positive rate below 5% on a consented, independently reviewed validation set.
- Keyboard-only and screen-reader flows can inspect, edit, dismiss and send.
- Closing or crashing the warning dialog cannot accidentally send the draft.
- A stale classifier or failed model load degrades to deterministic PII/secret checks and clearly reports reduced coverage.

### 1.5 Public Post Guard

Extend the outgoing warning to public and semi-public surfaces such as X posts, Reddit comments, Instagram captions, public Discord channels and LinkedIn posts. The guard considers both content and audience context:

- whether the audience is public, follower-only, group-limited or private;
- whether search engines and third-party archives can index the surface;
- sensitive identity, affiliation, location, employer or health disclosures;
- threats, illegal-activity descriptions or operational details;
- doxxing of the user or another person;
- embedded image metadata and visible background details;
- whether the post names an employer, school, home area or current location.

Recommended copy:

> This post is public and may be searchable, archived, quoted or used for profiling. It includes details that may create legal, employment, safety or reputational risk. Review before posting.

OSL must not say that a user is on, or will be placed on, a government, employer or platform watchlist. It has no reliable way to know that. It may say that public content can be collected or used for profiling, and it may identify the exact detail that caused the warning.

Public Post Guard remains local, advisory and overridable. It never reports the draft, silently changes it, downranks a political view or treats a protected identity or lawful political affiliation as inherently suspicious.

## 2. One OSL login without one dangerous credential store

“Single login” should mean one OSL identity, subscription, device list and recovery flow. It must not mean that OSL's servers hold a master password, reusable platform passwords, browser cookies or a decryptable copy of every platform token.

### 2.1 Account broker

Build a local Account Broker with these properties:

- OSL sign-in uses a passkey or device-bound key plus an encrypted recovery mechanism.
- Each platform connection is a separate grant with its own account ID, scopes, expiry and revocation state.
- OAuth tokens live in the OS credential vault and are released only to that connector process.
- QR-linked and local web sessions remain in per-connector encrypted profiles.
- Platform passwords are never requested or stored by OSL where OAuth, QR linking or an existing user session is available.
- Server sync stores only end-to-end encrypted connection metadata needed for device continuity. It does not store usable bearer tokens by default.
- Disconnecting one platform cryptographically erases its local grant without affecting the OSL identity or other connectors.

### 2.2 Connector isolation

Run every connector in a separate process or sandbox with:

- a platform-specific network allowlist;
- a private storage namespace;
- the minimum IPC capability set;
- no access to another connector's token, cookies, plaintext cache or WebView;
- explicit capability negotiation with the hub;
- a kill switch and per-version rollback.

For minimal-touch UI companions, the remote service performs all of its own networking inside an isolated account profile. OSL must not use webhooks, private APIs, platform-token capture, fetch/XHR interception, CSP stripping, runtime injection or background history scraping. The trusted OSL process operates separately through visible accessibility/UI Automation state and an OSL-owned overlay, activated only by an explicit user action or a newly visible OSL capsule event. Product copy must disclose that the host platform sees the ciphertext carrier and may detect accessibility automation.

The current Discord remote-origin IPC boundary must be tightened before the social hub reuses it. A platform WebView must never be able to invoke history export, account switching, key management, burn operations or another connector's commands directly.

### 2.3 One GUI and one settings model

OSL should expose one desktop shell rather than a separate product per platform. Its primary navigation should be:

- Home: connected accounts, connector health and urgent privacy actions;
- Conversations: an optional local unified inbox with clear platform provenance;
- People: verified OSL contacts and their connected platform identities;
- Privacy: warning, retention, view-once and cleanup policies;
- Encryption: OSL key state, trusted devices, contacts and conversation coverage;
- Accounts: authorize, reauthorize, isolate and disconnect connectors;
- Receipts: verified send, deletion, expiry and recovery outcomes;
- Settings: one inherited policy tree with platform-specific overrides.

Settings use three levels: global OSL default, platform/account override and conversation override. The most specific explicit setting wins. The UI always shows where a setting was inherited from and offers `reset to inherited` instead of silently copying values between levels.

The hub should be paired with a thin, least-privilege overlay inside each supported app. The overlay contains only contextual controls:

- current encryption and connector-health badge;
- warning result before send;
- `view once` and `delete after` controls;
- the exact expiry guarantee available on this platform;
- the verified OSL contact behind the current platform account;
- a link back to the full OSL policy or receipt.

An overlay never receives account-export, account-switch, key-management or bulk-delete authority. It submits an immutable action intent to the local broker, which revalidates account, conversation, capability and policy before acting. This keeps the daily experience light without making every platform page a privileged control panel.

#### Send behavior and handoff state

All Send modes use one broker-owned state machine:

```text
Idle -> Drafting -> TargetVerified -> PayloadReady
PayloadReady -> PlacedAwaitingConfirmation -> TargetReverified -> SendAttempted
SendAttempted -> Sent | NotSent | OutcomeUnknown
any pre-send state -> Cancelled | Expired
```

The target snapshot binds connector, service, account epoch, window, conversation, recipients, native composer, protection mode and payload digest. Focus and the complete target are checked immediately before placement and immediately before Send. Account/conversation/recipient changes, ambiguous focus, connector drift, challenge screens, timeout, cancellation or restart invalidate the state. Recovery may restore an encrypted draft, but never an armed Send. There are no automatic retries after an unknown outcome.

For Double Enter, the first trusted user Enter is consumed, produces the protected payload and places it only after verification. The second must be a distinct trusted user Enter after key-up; key repeat, held keys, synthetic events and OSL-generated placement do not count. The armed interval is short, visible and bounded. Expiry or mismatch cancels rather than sends. `Cancel` and `Undo placement` are always available.

Atomic placement is the default. `Compatibility typing` is a separate per-service/account option for a verified composer that cannot accept atomic insertion. It deterministically enters the already-generated capsule character by character, with fixed behavior and immediate fail-closed cancellation on context drift. It must not randomize WPM, pauses, errors or pointer motion, mimic a human, claim to avoid detection or imply terms compliance. Neither placement method has permission to Send by itself except the explicitly opted-in Experimental Single Enter/one-click mode.

Before placement, the UI provides an expandable exact-payload preview containing the full carrier/capsule, capsule version, byte/character length and a draft-versus-platform formatting comparison. Connectors must test line breaks, Unicode/emoji, mentions, links, Markdown/rich-text, replies, quotes and attachment references. A meaning-changing conversion blocks Send until the user edits or explicitly accepts the shown conversion.

Inbound protected content is decrypted into an OSL-owned layer aligned with the native message while retaining a subtle, persistent OSL provenance lock, sender verification and the real platform/conversation context. Native-looking styling must not erase OSL provenance. If envelope authentication, sender binding, conversation binding or layout confidence fails, leave the native carrier visible and render no plaintext.

Semantic accessibility/browser anchors and account/conversation identity are the primary overlay-placement signals. A native transparent overlay may locally sample visible pixels, colors, typography and geometry for visual alignment, but pixel/theme sampling never establishes target identity or authorizes placement/Send.

Protected transport supports either (a) an inline ciphertext capsule with no OSL payload storage, or (b) an optional padded, sealed, short-TTL relay object referenced by an opaque token and deleted on successful fetch or expiry. The relay has no decryption key, but the UI and privacy inventory must disclose unavoidable transient access metadata such as time, network address and padded object size. Ordinary cover text is user-written or explicitly user-selected. A future Pro AI-assisted Cover Draft Relay may prepare a carrier draft only under the per-message approval and explicit-token contract below; autonomous cover traffic, fake engagement and unattended decoy activity remain prohibited.

#### AI-assisted Cover Draft Relay (future Pro)

This is a user-supervised drafting and local rendering feature. It must never become an autonomous conversation loop or a covert transport.

```text
Sender plaintext in OSL memory
  -> locally encrypt to verified recipient devices
  -> pad sealed blob to a coarse size bucket
  -> upload to user-controlled storage or blind short-TTL relay
  -> local model prepares editable cover draft
  -> show canonical plaintext + cover draft + exact token/payload preview
  -> user approves/edits and deliberately sends
  -> recipient visible-UIA connector notices explicit OSL token
  -> authenticate binding, fetch once, decrypt locally
  -> render canonical plaintext in provenance-marked OSL overlay
  -> optional local response draft
  -> recipient approves/edits and deliberately sends
```

The platform carrier contains an explicit authenticated opaque OSL token alongside the user-approved cover draft. Do not hide the token in word choice, punctuation, whitespace, Unicode variants, timing, images or other steganography. The token is an unguessable high-entropy single-use capability with a short TTL and cryptographic binding to protocol version, sender OSL identity, recipient device set, connector, platform account epoch and conversation context. Include nonce/replay state so a copied, replayed or cross-context token fails before fetch or plaintext rendering. A successful authorized fetch consumes the token; expiry, revocation or context mismatch makes it unusable.

The encrypted object may live on a user-controlled node or a blind relay. The relay stores only padded ciphertext plus the minimum capability state needed for delivery, deletes it on successful fetch or published expiry, and never receives plaintext, content-derived labels, encryption keys, platform credentials or model prompts. Its privacy inventory must disclose transient source/destination network metadata, timestamps, padded size class, delivery result and their exact retention. Padding reduces size leakage but does not make timing, participants or platform metadata invisible.

Inbound detection stays inside the minimal-touch connector boundary: observe only a newly visible explicit OSL token through accessibility/UI Automation, then pass an immutable token event to the local broker. Do not use webhooks, private APIs, page injection, fetch interception or hidden DOM/network scraping. The broker verifies the current service, account, conversation, visible sender and recipient bindings before contacting the relay. The decrypted overlay retains an OSL provenance mark and the canonical plaintext; the model-generated carrier is never represented as the canonical message.

Draft generation defaults to a local model and may use decrypted content only after the receiving user has authorized this feature for that conversation. An external model is a separate explicit opt-in with a precise disclosure of the text leaving the device. OSL-operated generation must not receive or retain plaintext. Every outward message requires its own visible preview, user approval or edit, and deliberate configured Send gesture. Approval is not reusable, and no inbound message may trigger automatic generation-and-send, impersonation or background activity.

Failure states are explicit and fail closed:

- Offline before upload: preserve only the locally encrypted draft and show `Not deliverable yet`.
- Relay unavailable: leave the native carrier visible and show `Protected message unavailable - try again`.
- Expired, consumed or revoked token: show `Protected message expired`; never create or infer replacement plaintext.
- Authentication, replay or context-binding failure: do not fetch or render; record a content-free local security receipt.
- Decryption or overlay-confidence failure: keep the native carrier visible and suppress response-draft generation.
- Unknown send outcome: do not retry automatically or advance the AI dialogue state.

Suggested records:

```text
CoverRelayObject
  object_id, padded_ciphertext, size_class, created_at, expires_at
  capability_digest, consumed_at, relay_schema_version

CoverTokenBinding
  protocol_version, capability_secret, sender_identity, recipient_device_set
  connector_id, account_epoch, conversation_binding, nonce, expires_at

CoverDraftState
  local_draft_id, canonical_message_digest, cover_text_encrypted_local
  model_kind, disclosure_choice, approval_state, target_snapshot
```

`capability_secret` exists only in the authenticated platform token and authorized recipient client; the relay persists only a verifier/digest. `CoverDraftState` is local, encrypted at rest, isolated by OSL/platform account epoch and erased according to the user's draft policy.

#### Account-risk presentation

Every non-manual mode shows two independent fields:

- **Terms status:** `No known restriction`, `May conflict with terms`, `Explicitly restricted`, or `Unknown`.
- **Enforcement likelihood:** `Low`, `Medium`, `High`, or `Unknown`, with evidence source and review date when supported.

The app never fabricates a percentage. Missing, anecdotal or outdated evidence produces `Unknown`; a terms restriction alone does not prove an enforcement rate. Warnings name the specific behavior at issue and say that it may put the account at risk. Experimental Single Enter/one-click requires a separate acknowledgement per service/account, remains visibly marked, and cannot enable anti-detection, fingerprint spoofing, randomized typing, stealth or unattended automation.

### 2.4 OSL contacts and universal whitelist

Add an `OslContact` identity above platform participants:

```text
OslContact
  contact_id, display_name, root_identity_key, trust_state
  verified_devices[], platform_bindings[], policy_id

PlatformBinding
  connector_id, remote_account_id, proof_kind, proof, verified_at

ContactPolicy
  osl_encrypt_when_possible, allow_plaintext_fallback
  warning_level, retention_policy, attachment_policy
```

The People/Whitelist menu populates across connected chats by matching verified bindings. Linking must use one of:

- an OSL-to-OSL cryptographic contact exchange;
- a signed challenge sent through the claimed platform account;
- a QR code or safety-number comparison;
- explicit local user linking that remains marked `unverified` until proven.

OSL must never silently merge people based only on display name, username similarity, contact uploads, email address discovery or behavioral inference. A mistaken merge could encrypt to the wrong recipient, expose conversation history or apply a destructive policy to the wrong account.

The universal whitelist is an end-to-end encrypted policy set synced between the user's OSL devices. Platform connectors receive only the minimum derived decision needed for the current action. Blocking or removing a person on one platform does not automatically alter other platforms unless the user explicitly chooses an all-platform action.

### 2.5 Cross-platform OSL E2EE

When both participants have verified OSL identities, OSL may provide its own ciphertext overlay through any connector that can safely transport the required text or attachment envelope. The encryption session binds:

- both OSL root identities and active device keys;
- the connector and platform conversation identifier;
- the visible platform sender/recipient identities;
- the message's platform-independent OSL message identifier.

This prevents ciphertext copied from one platform or conversation from being accepted in another. Multi-device fanout encrypts to every active device and sender history device; revocation rotates future group state.

Before every send, the UI reports one of:

- `OSL E2EE verified`;
- `platform-native E2EE only`;
- `OSL encryption unavailable, plaintext to platform`;
- `encryption state unknown, send disabled`.

Plaintext fallback is a per-contact policy and is off by default for contacts explicitly marked `OSL encryption required`. Platform support and recipient participation remain prerequisites; the universal whitelist cannot manufacture E2EE when the other person is not running OSL.

For X, model two independent scopes:

- `X public`: posts, replies and quotes receive Public Post Guard, attachment sanitization, cleanup and scheduled deletion policies. Public content is never described as encrypted.
- `X DM`: one-to-one and group DM conversations receive normal OSL message warning, lifecycle and encryption-overlay capability checks. OSL E2EE is available only when the required participants are verified OSL contacts and the X transport can carry the versioned envelope safely.

An X username binding is not enough to trust an encryption key. The user must complete the same OSL signed-challenge or safety-number verification used on other platforms. Copying an OSL envelope between an X public post and an X DM, or between two X DM conversations, fails authentication because the platform surface and conversation are bound into associated data.

### 2.6 Encrypted posts on public platforms

Add an `Encrypted audience post` option for X, Reddit, Instagram captions/bios where workable, Discord announcements and open-protocol networks. The platform receives a clearly labeled OSL carrier, while the actual post is encrypted to one of:

- selected verified OSL contacts;
- an OSL contact group;
- anyone with an expiring access link or passphrase;
- a one-time browser capsule;
- a future creator/customer membership group with explicit key rotation.

For short-text platforms, prefer a compact HTTPS capsule link or stable OSL envelope identifier. Do not embed large ciphertext that may exceed character limits. Do not encode ciphertext directly into images that platforms recompress, and do not use steganography to evade moderation or platform controls.

The overlay renders:

```text
🔒 OSL encrypted audience post
From: verified OSL identity
Audience: Liam's Close Friends
Expires: 24 hours
```

Only after verifying the sender, platform surface, post identifier and audience grant does OSL render plaintext. The platform still sees who posted, when, the public carrier/link and engagement metadata. Recipients can still copy plaintext after opening it.

Revocation prevents future key retrieval but cannot erase a key or plaintext already obtained. Membership changes rotate future audience keys; they do not rewrite old access history. Replies use separate encrypted envelopes so a public reply cannot accidentally reveal the protected post.

This should launch after ordinary OSL DMs and encrypted capsules. Broad encrypted public broadcasting creates moderation, abuse and platform-policy questions, so the first version should be limited to user-selected verified contacts and explicit access-link recipients.

## 3. Platform-neutral connector contract

Introduce a connector SDK instead of copying Discord-specific logic.

### 3.1 Canonical records

Every ID is namespaced by connector and connected account. OSL must never assume that IDs are globally stable or comparable across platforms.

Core records:

```text
PlatformAccount
  connector_id, local_account_id, remote_account_id, display_name
  authorization_kind, authorization_state, granted_capabilities

ConversationRef
  connector_id, local_account_id, remote_conversation_id
  kind, title, participant_refs, parent_ref

MessageRef
  connector_id, local_account_id, remote_conversation_id, remote_message_id
  author_ref, sent_at, edited_at, content_parts, reply_ref

CapabilitySnapshot
  read_history, read_realtime, send_text, send_attachment
  edit_own, delete_local, delete_for_all, enumerate_own_content
  reactions, replies, groups, channels, encryption_overlay
  observed_at, source, confidence, expiry

ActionPlan
  plan_id, immutable target snapshot, requested action
  estimated_count, unsupported_count, warnings, created_at

ActionReceipt
  plan_id, action_id, exact remote target, attempt, result
  remote_ack, verification_result, timestamps, connector_version
```

Unknown platform fields should be retained in a bounded opaque extension map so additive API changes do not destroy data, but security decisions may use only validated typed fields.

### 3.2 Required connector interface

```text
authorize / reauthorize / disconnect
probe_capabilities
list_conversations / stream_events
list_messages / get_message
prepare_send / commit_send / verify_send
prepare_place / commit_place / undo_place / verify_target
plan_delete / commit_delete / verify_delete
health / diagnostics / version
```

All mutating methods are idempotent where the platform permits it. Every mutation takes an account epoch and immutable target reference so a late callback from one account cannot act after the user switches accounts.

### 3.3 Capability states

Each feature reports one of:

- Supported
- Supported with limitation
- Temporarily degraded
- Authorization required
- Unsupported by platform
- Disabled for safety

The UI must never render an unavailable platform action as if it were universally supported.

## 4. Destructive cleanup and retention

OSL may help a user review and delete content they control, but it must not turn a scan or risk score into automatic evidence destruction.

Every cleanup operation follows:

```text
Scan -> Preview -> Dry run -> Explicit confirmation
     -> Rate-limited execution -> Per-item receipt
     -> Remote verification -> Reconciliation report
```

Requirements:

- Default to the user's own sent content.
- Show exactly which accounts, conversations, date ranges and item types are selected.
- Separate “remove from my view” from “unsend/delete for everyone.”
- Explain platform-specific time limits and irreversible effects.
- Require reauthentication for large or cross-account operations.
- Use resumable jobs and stable idempotency keys.
- Stop on target ambiguity, capability drift, account epoch change or unexpected UI.
- Never interpret a missing DOM element as successful deletion.
- Verify through an independent refresh/read path where possible.
- Preserve a metadata-only local receipt that does not retain deleted plaintext.
- Offer export before deletion, but never preselect it.
- Do not automatically delete messages merely because the warning engine flagged them.

### 4.1 Three honest expiry guarantees

OSL must label expiry by what actually happens:

1. `Platform disappearing message`: OSL invokes a supported native timer or view-once feature.
2. `OSL content expires`: OSL destroys access to an OSL-encrypted payload or media key. This is strongest when every reader uses OSL.
3. `Remove from my account`: OSL schedules a later platform deletion or local removal. It cannot promise that recipients or platform backups lost their copies.

Never show a generic `self-destructing` badge that hides these differences.

### 4.2 Per-message timers

Add a `LifecyclePolicy`:

```text
LifecyclePolicy
  policy_id, scope, ownership_filter, content_types
  timer_anchor, ttl, action, executor
  late_job_behavior, legal_hold_behavior, version

timer_anchor = sent | read | first_osl_open
action = crypto_expire | delete_for_all | delete_for_self | local_only
executor = platform_native | local_device | user_owned_node | managed_agent
```

`sent` is the only portable timer anchor. `read` requires a trustworthy platform receipt; `first_osl_open` requires an OSL viewer. Unsupported anchors are unavailable rather than approximated.

The composer overlay offers simple presets such as one hour, one day, one week and custom. Its confirmation sentence names the exact behavior, for example:

> OSL will ask Telegram to delete this message for everyone 24 hours after it is sent.

or:

> OSL will remove this message from your Instagram account after 24 hours. Other people may retain copies.

Scheduled jobs contain the exact connector, account, conversation and message identifiers, account epoch, capability snapshot, due time, execution lease and idempotency key. A late job never targets a replacement message discovered by position or text.

### 4.3 View-once media

Use a fresh media key and lifecycle grant for every view-once item. The OSL viewer:

- fetches ciphertext without persisting plaintext;
- atomically redeems a one-time grant after an explicit open action;
- renders from bounded memory;
- disables normal cache, thumbnail, recent-file and backup paths;
- destroys the local key and asks the blob service to expire the ciphertext;
- records only a metadata receipt.

Link-preview and security scanners must not consume the grant. Browser-only encrypted capsules can let a recipient view without installing OSL by keeping the decryption key in the URL fragment and using an atomic one-time redemption, but they still reveal connection metadata and cannot prevent screenshots, cameras or a modified viewer.

### 4.4 Messages sent from another client

OSL can apply a scheduled policy to a message sent from another authorized client only when a connector can observe the account's event stream and obtain an immutable remote message ID. One local or remote worker holds the execution lease so multiple devices cannot race.

OSL cannot retroactively make already-delivered plaintext `view once`. It can only schedule a supported deletion or protect future OSL payloads.

### 4.5 Email retention and tracking protection

For personal email, OSL can safely provide:

- user-mailbox cleanup policies through official APIs or IMAP;
- tracking-pixel detection and blocking;
- tracking-link parameter stripping;
- sender-domain and link-risk explanations;
- attachment metadata and secret scanning before send;
- encrypted capsules for expiring confidential attachments.

Deleting email from the sender's mailbox does not recall the recipient's copy. Permanent Gmail deletion also requires a broad OAuth scope, so OSL should default to trash, preview affected messages and require reauthentication for permanent deletion.

If `pixel parsing` means email tracking pixels, prioritize it. Detect single-pixel, transparent, CSS-background and unique-URL trackers, block them by default, and optionally fetch ordinary remote images through a privacy proxy. If it means screenshot/OCR parsing, use local OCR to identify secrets and PII for user review. Fuzzy pixels or OCR may discover a candidate but may never authorize a send or deletion.

### 4.6 Company secrets

Enterprise retention should integrate with Slack, Teams, Google Workspace and Microsoft 365 governance rather than classifier-triggered shredding. Company policies may suggest labels, warn before send and schedule supported retention, but must:

- pause under legal hold or required records retention;
- separate administrators who author policy from those who approve destructive changes;
- retain content-free audit receipts;
- use tenant-controlled keys and storage where possible;
- never connect a secret classifier directly to automatic deletion;
- distinguish employee mailbox deletion from organization-wide retention.

### 4.7 Always-On Retention Agent

Use this name instead of `cloud delete`, because it accurately describes the trust tradeoff. Offer three modes:

1. On-device scheduler: safest and included; actions run while OSL is available.
2. User-owned runner: an always-on PC, NAS or customer cloud VM executes encrypted jobs.
3. Managed OSL agent: a later premium option restricted initially to official, narrowly scoped APIs such as Slack, Teams, Gmail and approved Meta business accounts.

A managed agent capable of deletion usually holds a credential capable of reading or acting on the account, so it is not zero-knowledge. It must show exact scopes, last action, pending jobs and a one-click revocation control. Do not place personal Instagram/Discord browser sessions, Telegram full-account authorization keys or Signal linked-device secrets on OSL infrastructure in the first release.

### 4.8 Current implementation prerequisite

The existing `scope TTL` controls cipher-store blob availability, not deletion of a platform message. It currently accepts any value from one hour through seven days while the blob service accepts only the fixed 24-hour, 72-hour and seven-day TTLs. This mismatch must be repaired before a user-facing timer ships.

Current scope blob indexing is also too broad for per-message expiry, and current attachment envelopes/cache behavior is not view-once safe. Lifecycle work therefore requires per-message blob capabilities, new versioned attachment grants and explicit no-cache coverage. Existing Discord `Burn` must continue to be described as OSL key/local/blob cleanup, not remote Discord unsend.

## 5. Self-healing connectors

### 5.1 Three connector tiers

Tier A, official API or client library:

- Telegram TDLib
- Slack APIs
- Microsoft Graph for Teams
- X API for posts and DMs, subject to pay-per-use budget controls
- open protocols such as ActivityPub and AT Protocol
- approved Meta business/professional APIs

Tier B, reviewed open client or linked-device integration:

- Signal, subject to AGPL, trademark, upstream compatibility and security review

Tier C, local UI adapter where no suitable personal-account API exists:

- Instagram personal accounts
- current Discord WebView path until a better supported route exists

OSL should prefer Tier A, permit Tier B only under a formal review gate, and treat Tier C as a guarded compatibility layer rather than a secret API.

### 5.2 Instagram resilience design

Instagram's adapter should use multiple independent strategies:

- semantic accessibility roles, labels and stable text contracts;
- structural anchors and route state;
- bounded, schema-validated network observations only when lawful and necessary;
- platform version and capability probes;
- explicit post-action verification.

Avoid hashed CSS classes as the sole selector. Never let a machine-learning or fuzzy selector autonomously confirm a destructive action.

Ship signed adapter packs containing:

- connector version and compatible host versions;
- selector/strategy alternatives;
- required anchors and negative guards;
- non-destructive canary probes;
- feature-level enable/disable switches;
- expiry and minimum OSL version;
- rollback pointer.

Adapter packs are signed, schema-validated and recorded in a transparency log. The client retains at least two known-good versions. Bad signatures, unknown signing keys, expired packs, failed anchors or ambiguous targets preserve the last known-good pack or disable the affected feature.

### 5.3 Health state machine

```text
Healthy
  -> Suspect after probe or verification anomaly
  -> Degraded after repeated independent failures
  -> Disabled for unsafe send/delete/encryption paths
  -> Canary recovery with a signed adapter
  -> Healthy after staged verification
```

Controls:

- launch and hourly capability probes;
- mutation failure-rate and verification-mismatch budgets;
- non-destructive shadow canaries on sanitized test accounts;
- staged rollout by connector version and host build;
- automatic circuit breaker;
- one-click local rollback;
- remotely signed feature disable for emergencies;
- coarse opt-in health telemetry with no content, participant IDs or raw selectors from private conversations;
- sanitized fixture capture only after explicit user approval.

Self-healing means recovering known compatible patterns. Unknown UI states must fail closed and enter a repair queue.

### 5.4 Mullvad and protected-network integration

Add an optional, VPN-agnostic `Network Privacy` card, with Mullvad as the first supported local provider. OSL should integrate with the installed official Mullvad app through its documented CLI or local management interface. It must not request, display or store the user's Mullvad account number.

Simple states:

- `Protected`: Mullvad reports connected.
- `Connecting`: OSL waits without launching social connectors that require protection.
- `Not protected`: OSL warns or pauses according to the chosen OSL profile.
- `Unavailable`: Mullvad is not installed or its local service cannot be reached.

User-facing controls:

- Open Mullvad
- Connect
- Require protected network before opening OSL accounts
- Warn only, the Balanced default
- Pause OSL network activity if protection drops, an explicit Maximum option

OSL may read connection status and request a connect operation only after the user enables the integration. It does not silently change country, relay, DNS, split tunneling, multihop or lockdown mode. Mullvad's lockdown mode affects the whole device and therefore always requires the user to configure it in Mullvad or give a separate explicit confirmation.

Do not exclude OSL from the VPN through split tunneling. Changing VPN location can trigger platform login challenges, so OSL should preserve the user's Mullvad selection and explain authentication prompts rather than rotating relays automatically.

Define a small provider interface so IVPN, Proton VPN and generic WireGuard status can be added later. The feature remains useful without any VPN, and OSL does not imply that a VPN hides message content, account identity or platform metadata from the social platform.

Mullvad's official CLI exposes `status`, `connect`, auto-connect and lockdown controls, while the open-source app exposes a local daemon management interface. OSL should prefer the narrow CLI/status contract initially rather than embedding or forking Mullvad.

### 5.5 Android integration and Mobile Workspace

Offer Android in three progressively more privileged forms:

1. `OSL Companion`: an ordinary Android app paired end-to-end with the desktop hub. It provides share-sheet attachment sanitization, encrypted-capsule viewing, account alerts, local receipts and selected notification actions.
2. `Connect my phone`: a user-approved local pairing to a real Android device. Request individual Android permissions only when their related feature is enabled. Do not require ADB, root or a global Accessibility Service for the default experience.
3. `OSL Mobile Workspace`: an optional paid local Android virtual device for mobile-only apps or isolated accounts.

The Mobile Workspace appears as one card in the hub:

- Start private phone
- Installed apps
- Storage used
- Protected-network state
- Lock, snapshot and wipe
- Per-app OSL features and receipts

It runs on the user's computer through hardware virtualization and keeps a separate encrypted data image per workspace. OSL may install its companion and let the user install normal apps through a supported system image. It does not root the image, bypass app security, spoof device identity or automate apps without explicit per-feature consent.

The official Android Emulator supports persistent per-device user data, snapshots, ADB installation and hardware acceleration. Before distribution, OSL needs a licensing review for the emulator and Google/Play system images; it should not assume that development SDK components may be freely repackaged into a consumer subscription. A one-click bootstrap that downloads official components after the user accepts their licenses is preferable to silently redistributing them.

Security requirements:

- AVD data, snapshots, tokens and notifications are encrypted at rest with a device-bound OSL key.
- Locking OSL suspends or closes the workspace and removes decrypted shared files.
- Clipboard, camera, microphone, file sharing and host networking are off until explicitly enabled.
- ADB is bound locally, authenticated and never exposed to LAN or internet.
- Each workspace and social account has its own data image and account epoch.
- OSL cannot read another app's private data unless Android explicitly grants an allowed interface.
- Accessibility and custom-keyboard access receive separate high-risk consent screens and are not required for basic use.
- Emulator or app incompatibility is reported honestly; OSL does not bypass integrity or emulator-detection controls.

Do not offer a managed OSL-hosted personal Android phone initially. A cloud emulator would hold reusable social sessions and private notifications, create a concentrated breach target, cost meaningful RAM/CPU per active user and trigger platform cloud-IP/emulator controls. If remote Android is ever offered, start with a user-owned cloud VM or enterprise-managed test device and keep personal Signal, WhatsApp, Instagram and Discord sessions out of OSL infrastructure.

The Android Virtualization Framework is intended for secure isolated execution on supported ARM64 Android devices, not as the Windows desktop emulator. On Windows, use the supported Android Emulator/hypervisor path. Windows Subsystem for Android is not a suitable foundation because Microsoft ended support.

## 6. Platform release sequence

### Phase 0: contracts and threat model

Deliverables:

- platform capability matrix;
- canonical record and connector interfaces;
- local Account Broker design;
- per-platform terms, API, license and trademark review;
- warning taxonomy and evaluation set governance;
- destructive-action threat model;
- decision on what “OSL 1.0” guarantees.

Exit gate: security review agrees that a compromised platform page cannot access keys, other accounts, local history or destructive hub commands.

### Phase 1: shared hub foundation

Deliverables:

- `crates/adapters` connector SDK;
- platform-neutral encrypted local store;
- OS-vault-backed grants and per-connector sandboxes;
- capability-aware hub UI;
- account epochs and cancellation of stale callbacks;
- action plan/receipt engine;
- connector health and signed update service;
- Manual, Clipboard and Double Enter handoff state machine with per-service/account inheritance, bounded timeout, undo and safe restart recovery;
- packaged Rust/Tauri desktop shell that starts without a localhost server and isolates every service/account WebView profile.

Exit gate: two mock connectors pass account isolation, crash recovery, rate limiting, rollback and ambiguous-target tests.

### Phase 2: warning engine in Discord first

Roll out in shadow mode to the two OSL test accounts, then Balanced opt-in mode. Measure local latency, false positives, user overrides and accessibility without collecting message content.

Exit gate: all Section 1.4 gates pass and no send path can bypass the final decision state accidentally.

### Phase 3: Telegram production connector

Use TDLib for authorization, local consistency, DMs, groups, channels, attachments and capability-aware deletion. Do not recreate Telegram networking in the UI adapter layer.

Exit gate:

- DM, group and channel coverage is explicit;
- self-only versus all-participant deletion matches TDLib capability checks;
- login, 2FA, relink, revocation and two-device scenarios pass;
- large-history pagination and rate limiting are resumable;
- send/delete receipts survive crash and restart.

### Phase 4: Instagram guarded beta

Start hygiene-first: local account inventory, optional outgoing-draft warnings, cleanup preview and user-confirmed deletion. Add OSL-composed message sending only after the read and deletion adapter has met its reliability gates. Support DMs and group chats first. Separate professional-account API capabilities from personal-account UI capabilities. Put every send, unsend and bulk cleanup behind runtime capability checks and independent verification.

Exit gate:

- current and two prior sanitized Instagram UI fixtures pass;
- selector-pack signature, expiry, rollback and kill-switch tests pass;
- unknown composer, wrong account, ambiguous chat and changed confirmation dialog make zero mutations;
- canary deployment remains below the defined mismatch/error budget;
- the product clearly labels beta and degraded capabilities.

### Phase 5: Signal gated integration

First complete an architecture decision record comparing:

- a separately distributed companion based on the official open-source Desktop client;
- an upstream-contribution path;
- a minimal linked-device connector with independently audited protocol handling;
- postponement if none meets the security and maintenance bar.

Do not ship a cloud bridge or unofficial credential collector. A third-party linked device can read private communications, so the trust surface must be obvious and auditable.

Exit gate:

- AGPL and trademark obligations are approved;
- device-link secrets never leave the local device;
- the connector is reproducibly built and independently audited;
- unlink, inactivity/relink, history transfer, backup and device-limit behavior are tested;
- upstream protocol changes have a maintained compatibility process;
- the connector cannot silently fall back to a non-private relay.

### Phase 6: unified OSL release candidate

The release candidate contains:

- one OSL identity with separately connected accounts;
- Discord, Telegram, Instagram beta and Signal only if its gate passed;
- a unified account/health dashboard;
- optional local index and search;
- per-platform warning and cleanup capability matrix;
- export, disconnect and cryptographic local erase;
- connector-specific troubleshooting and rollback.

OSL 1.0 should not be blocked indefinitely by pretending an unsafe Signal or Instagram connector is ready. If a gate fails, ship it as unavailable or beta with a public reason rather than weakening the gate.

## 7. Revenue-prioritized expansion

Rank opportunities by both willingness to pay and integration safety:

| Priority | Platform lane | Revenue case | Technical position |
|---|---|---|---|
| 1 | Instagram personal + professional | Large consumer/creator demand and strongest OSL acquisition story | Personal UI adapter is brittle; professional APIs where permitted |
| 2 | Slack | Teams pay for privacy hygiene, retention and local audit receipts | Strong official OAuth/API path |
| 3 | Microsoft Teams | Enterprise security and compliance budgets | Strong official Graph path with tenant approval |
| 4 | Telegram | Best consumer connector feasibility and broad international use | Official full-client TDLib path |
| 5 | X posts and DMs | Strong Public Post Guard, cleanup and private-conversation use cases | Official OAuth API exists; pay-per-use reads/writes require budgets and caching |
| 6 | WhatsApp Business / Meta business inboxes | Customer-support and small-business demand | Official business scope only; not equivalent to personal inbox access |
| 7 | Signal | Excellent brand fit and privacy credibility | Hard linked-device/open-client maintenance; likely lower direct revenue |
| 8 | Bluesky and Mastodon | Cheap protocol coverage and trust-building | Open APIs/protocols, smaller paid market |
| 9 | Reddit user content | Cleanup and account hygiene use case | Verify current OAuth and deletion rules before commitment |
| Hold | LinkedIn personal, TikTok DMs, Snapchat | Potentially valuable audiences | API access, account restriction and brittle automation risks |

Recommended packaging:

- Free: one connector, the complete local warning feature and cleanup preview.
- Pro: all consumer connectors, scheduled cleanup reminders, local encrypted search, exports, multi-device settings sync, verified cleanup receipts and the user-supervised AI-assisted Cover Draft Relay.
- Family: multiple independent OSL identities under shared billing, never shared plaintext.
- Teams/Enterprise: Slack/Teams, tenant-controlled storage, retention review, policy templates and auditable receipts.

Do not monetize by retaining, training on, selling or advertising against message data.

### 7.1 Solo-user products people can buy immediately

These do not require a friend, employer or recipient to install OSL. Rank them in this order:

1. **Verified history cleanup and scheduled retention.** Scan old posts, comments and owned messages, preview risk, delete through supported paths and prove what was verified gone, unsupported or still visible. Recent user discussion shows unusually strong pain around searchable years-old comments and the tediousness of bulk cleanup.
2. **Attachment Privacy Guard.** Strip EXIF/GPS, filenames, PDF/document authorship and revision metadata; use local OCR to find addresses, keys, IDs, cards and visible background details; provide one-tap blur/redaction.
3. **Email Privacy Overlay.** Block tracking pixels, sanitize tracking links, explain suspicious senders and offer local cleanup policies.
4. **Privacy Exposure Inventory.** One local dashboard for old content, public profile fields, connected apps, stale logins, third-party grants, weak account security and incomplete deletion jobs.
5. **Account Drift Watch.** Alert when a platform changes a privacy setting, enables a new data use, adds a login, changes connected-app access or invalidates an OSL safeguard. Guide the user to the platform's own control when no safe API exists.
6. **Scam and impersonation shield.** Local inbound link/attachment analysis, lookalike-account detection and a private warning. This needs a separate inbound-content consent and retention model from the outgoing risk warning.
7. **Encrypted one-time capsules.** Share view-once or expiring text/files through a browser viewer so recipients need no OSL account, with honest screenshot and metadata limitations.
8. **Always-On Retention Agent.** Local and user-owned runners first; managed official-API automation only after security review.
9. **Personal encrypted archive.** Normalize official platform exports into a locally searchable encrypted vault, detect duplicates and let users delete the platform copy without losing their own chosen history.
10. **Emergency account lockdown.** Guided session review, token/app revocation, local connector lock and recovery checklist across accounts. OSL should call official platform actions rather than pretending it can revoke what the platform does not expose.

The strongest paid bundle is `cleanup + attachment guard + exposure inventory + always-on user-owned runner`. Keep the warning feature and basic tracker blocking free because they are safety controls and strong trust-building acquisition features.

## 8. Repository implementation map

Reusable foundations found in the current repository:

- `crates/selectors`: signature, staleness and last-known-good patterns. It is not currently a complete production self-healing system and needs a compatible runtime integration.
- `crates/store`: encrypted SQLite storage and deletion/checkpoint patterns. Generalize Discord-specific columns instead of copying the database per platform.
- `crates/ipc/src/scope.rs`: hierarchical scope pattern. Generalize to platform account and conversation scopes.
- `crates/runtime/src/clock.rs`: deterministic clock for retention, expiry and retry tests.
- `crates/runtime/src/rotation.rs`: useful state-machine structure for bounded retention/action scheduling, not reusable crypto semantics.

Planned additions:

```text
crates/adapters/
  connector.rs
  capabilities.rs
  records.rs
  actions.rs
  health.rs

crates/account-broker/
  grants.rs
  vault.rs
  epochs.rs
  recovery.rs

crates/risk-warning/
  taxonomy.rs
  deterministic.rs
  classifier.rs
  policy.rs
  explanation.rs

crates/cover-relay/
  capability.rs
  binding.rs
  padding.rs
  object_store.rs
  replay.rs

crates/cover-drafts/
  local_model.rs
  approval.rs
  disclosure.rs
  state.rs

crates/connectors/telegram/
crates/connectors/instagram/
crates/connectors/signal/

src-tauri/src/overlay/shared/
src-tauri/src/overlay/instagram_adapter.rs
```

Reimplement only accessibility and semantic-observation concepts from the current Discord work in the separate trusted overlay process. Do not ship runtime page injection, network interception, anti-detection, API-evasion or `toString`-spoofing behavior in new connectors.

## 9. Cross-platform acceptance matrix

Every connector release must test:

- fresh login, MFA/2FA, expired grant and revocation;
- two accounts on one device and one account on two devices;
- account switching during every asynchronous operation;
- DM, group chat and platform-specific larger scopes;
- text, reply, edit, reaction and supported attachment types;
- send interception, cancel, warning, send-anyway and retry;
- Manual, Clipboard, Double Enter and separately opted-in Experimental Single Enter/one-click behavior;
- first/second Enter distinction, key-repeat rejection, focus/account/conversation/recipient revalidation, timeout, cancel, undo and restart while armed;
- exact capsule preview, formatting-fidelity comparison, atomic placement and explicit deterministic Compatibility typing failure paths;
- terms status versus evidence-based enforcement likelihood, including `Unknown` with no fabricated percentages;
- inbound decrypted overlay provenance, sender/conversation binding and fail-closed layout confidence;
- semantic-anchor authorization versus non-authoritative local pixel/theme alignment;
- minimal-touch enforcement: no private API/token/network interception/injection/background-scrape path, and honest platform-visibility disclosure;
- inline-capsule and sealed-relay transport, TTL/delete-on-fetch, padding and metadata disclosure;
- future Cover Draft Relay: explicit authenticated token, local-model default, per-message preview/approval, recipient local decrypt overlay, single-use/context binding/replay rejection, and no autonomous send or hidden steganography;
- Cover Draft Relay offline, unavailable, expired, consumed, revoked, wrong-sender, wrong-account, wrong-conversation, copied-token, replay, decryption-failure and low-overlay-confidence states;
- local-only delete versus all-participant unsend where supported;
- large-history scan, pause, resume, crash and restart;
- platform rate limit, offline mode and partial response;
- stale UI/API schema, ambiguous target and missing confirmation;
- signed adapter update, bad signature, expiry, rollback and kill switch;
- zero cross-account token, plaintext, cache or callback leakage;
- screen-reader, keyboard, zoom and reduced-motion behavior;
- English plus a planned locale matrix before non-English warnings ship.

Required destructive-action assertions:

- An ambiguous target causes zero remote mutations.
- A changed confirmation dialog causes zero remote mutations.
- A successful local click without remote verification is reported as unverified, not successful.
- Retrying after a crash does not delete a different item.
- A connector cannot operate after its account epoch changes.
- Bulk deletion cannot start from a warning alert or hidden background job.

## 10. Operational rollout

Use four rings:

1. fixture and mock accounts;
2. OSL-owned test accounts such as Rose and OSL;
3. opt-in technical preview users;
4. general availability.

Promotion requires a fixed observation window, no unresolved wrong-target events, no plaintext leakage, an acceptable verification-mismatch rate and a working rollback. A single confirmed wrong-account or wrong-target mutation trips the global connector circuit breaker and requires incident review before re-enable.

Publish a connector status page that distinguishes platform outage, authorization expiry, detected UI drift, degraded read-only mode and OSL service failure.

## 11. Research basis

- Telegram describes TDLib as a full client library that handles networking, local storage and data consistency, and documents capability-aware message deletion: https://core.telegram.org/tdlib/getting-started and https://core.telegram.org/tdlib/docs/classtd_1_1td__api_1_1delete_messages.html
- Instagram's official unsend help says recipients may already have seen a message, which is why OSL must not imply that unsend erases observation or all copies: https://www.facebook.com/help/instagram/491370017690934
- Signal Desktop is an official linked-device client distributed under AGPLv3: https://github.com/signalapp/Signal-Desktop
- Slack's official Conversations API covers public/private channels, DMs and group DMs under explicit scopes: https://api.slack.com/apis/conversations-api
- LinkedIn prohibits scraping/non-official content access and automated activity outside permitted APIs, so personal-message automation is not an acceptable initial connector: https://www.linkedin.com/legal/l/api-terms-of-use
- Recent community discussion around third-party Signal linked devices emphasizes that users need verifiable software and a clear trust model. That concern supports the strict Signal gate rather than a quick cloud bridge: https://www.reddit.com/r/signal/comments/1upwi0l/signal_enabled_dumbphone/

## 12. First implementation tickets after approval

No tickets below are authorized by this planning document. They are the proposed first batch after explicit approval:

1. Write the connector capability matrix and threat model.
2. Define canonical records, account epochs and action receipts.
3. Refactor remote-origin IPC behind a least-privilege local broker.
4. Wire the signed selector engine end to end with last-known-good rollback.
5. Prototype the warning engine in offline shadow mode.
6. Build the TDLib connector against OSL-owned Telegram test accounts.
7. Build Instagram fixture/canary infrastructure before mutating Instagram features.
8. Complete the Signal architecture, license and trademark decision record.
9. Build the capability-aware hub shell and account-health UI.
10. Add Slack as the first post-consumer, high-value official connector.
