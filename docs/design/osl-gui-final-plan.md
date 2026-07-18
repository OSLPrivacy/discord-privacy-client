# OSL Privacy GUI final review plan

**Status:** Living product and interface specification. Implemented capabilities must remain narrower than this document wherever a connector or security proof is not yet complete.

## Product decision

OSL should become one simple, local-first privacy app with two kinds of protection:

1. **OSL protects accounts people already use.** The Windows-first launch lists Discord, Telegram, WhatsApp, Instagram, Snapchat, Signal, selected email providers, X and Facebook Messenger. Slack and LinkedIn Messaging remain later integrations. OSL launches the installed Windows client and attaches a separate transparent overlay/sidecar, with capabilities limited per platform.
2. **OSL offers a private place of its own.** OSL Chat, OSL Circles and, later, OSL Mail provide first-party encrypted communication without depending on another social platform.

The desktop OSL Privacy owns accounts, policies, keys, contacts, destructive actions and receipts. It ships as an installed, standalone Rust application, not a localhost website or browser-delivered HTML prototype. It does not embed remote social websites. Each service tile launches a fixed, reviewed native Windows application when one exists; otherwise it opens the service's fixed official URL in the separately installed Firefox browser. A separate OSL overlay then attaches only after the service, account, window, conversation and composer have been verified. Platform companions stay deliberately small: they show the current protection state, offer contextual actions and send user intent back to the app. They are separate OSL surfaces, not code injected into a platform or Firefox process. A companion never receives master credentials or unrestricted deletion authority.

The product should feel like a calm control center, not a hacker tool, enterprise admin console or wall of privacy switches.

## Approved scope

The eight approved OSL directions are:

1. One OSL Privacy with thin per-platform overlays.
2. Basic, Balanced and Maximum presets instead of mandatory manual configuration.
3. Separate protection for X public posts and X direct messages.
4. Encrypted-audience posts carried over public platforms.
5. Optional Mullvad integration for protected-network status and policy.
6. An Android Companion plus a paid, isolated Android Mobile Workspace.
7. Timed deletion, view-once media and retention cleanup with honest guarantees.
8. Privacy tools that help one customer even when none of their contacts use OSL.

Two first-party services are added to the roadmap:

9. **OSL Chat and Circles:** private DMs, group chats and audience-based social posts using OSL E2EE.
10. **OSL Mail:** an optional OSL email address and client, built after the messaging and connector foundation is reliable.

Every connected service also has an approved dual-mode contract:

- **Native:** open or continue in the installed platform client, or the fixed official site in external Firefox when no native Windows client exists, with the platform's own feature set and security guarantees.
- **OSL Protected:** use the OSL companion for the capabilities it can safely protect, with true OSL E2EE only when every participating endpoint is OSL-capable and verified.

Users do not give up calls, communities, rich media, bots, search, administration or any other native feature in order to use OSL. Full service functionality is preserved by companioning the native UI, not by reimplementing every platform inside the app. Each service offers a clear Native versus OSL Protected choice wherever a protected capability exists. If a recipient, endpoint or feature cannot use OSL protection, the UI explains why and offers Native or an explicit user-assisted fallback. It never silently changes mode and never describes an ordinary external platform message or email as OSL E2EE.

## Platform compatibility boundary

`Available at launch` means OSL can recognize the service, show an OSL-owned companion surface and provide the locally safe features the platform permits. It does not mean silent automation or identical capabilities everywhere.

OSL must not market non-API automation as automatically compliant with every platform. Some services expressly prohibit scripted website automation, automated user accounts, client modification, scraping or unauthorized third-party interaction. The launch design therefore has three capability levels:

| Level | What OSL may do |
|---|---|
| **Local protection** | Warn as the user types, sanitize user-selected content, encrypt locally and show privacy state |
| **User-assisted action** | Place protected text in the visible composer or guide the user to a message; the user performs the final send or delete action |
| **Authorized automation** | Perform repeated or unattended actions only when the platform, account type and current policy allow it |

OSL does not bypass rate limits, anti-bot systems, authentication controls or platform enforcement. When a capability cannot be offered safely, the UI says `Assist only` or `Unavailable` instead of attempting to stay undetected.

### Minimal-touch connector invariant

The remote service owns its network traffic inside an isolated account profile. OSL does not use webhooks, private APIs, platform-token capture, fetch/XHR interception, CSP stripping, runtime injection or background history scraping. A separate trusted OSL process uses only visible accessibility/UI Automation state and its own overlay, and only in response to an explicit user action or a newly visible OSL capsule event. The interface states honestly that the platform can see the ciphertext carrier and may be able to observe accessibility automation; the companion is not invisible to the service.

### No anti-detection or ban-bypass behavior

Self-healing repairs compatibility only. It must never evolve into platform-evasion behavior. OSL does not:

- spoof browser, device or canvas fingerprints;
- simulate human words-per-minute, mouse paths or keystroke timing;
- solve or bypass CAPTCHAs, challenges or reauthentication;
- rotate accounts, IP addresses or device identities to avoid enforcement;
- autonomously generate or send cover conversations, fake engagement or decoy activity;
- conceal unattended automation from a platform.

If a service presents a challenge, changes an enforcement boundary or rejects an action, OSL stops that capability, preserves the user's local draft and explains the safe next step. Account safety is more important than pretending feature parity.

## What OSL is competing with

OSL should not try to clone every mature privacy product at once. Its advantage is the layer none of them currently combine well: one understandable privacy policy applied across existing social accounts, native encrypted communication and verified cleanup receipts.

| Category | Strong current products | What users already get | OSL's opening |
|---|---|---|---|
| Privacy suite | Proton | Mail, VPN, drive, aliases, passwords and one account | Cross-social protection, overlays, retention and native private social in one policy |
| Private email | Proton Mail, Tuta | Mature delivery, encrypted mailboxes and aliases | A simpler social-plus-mail identity and consistent contacts/policies |
| Private messenger | Signal, SimpleX, Session | Strong E2EE and mature chat behavior | OSL contacts spanning both native chat and connected platforms |
| Social cleanup | Redact | Bulk deletion across many services | Ongoing retention rules, before-send protection and verified receipts |
| Data removal | Incogni, DeleteMe, Optery, Privacy Bee | Broker discovery and recurring removal requests | Combine external exposure cleanup with the user's real account privacy state |
| Identity safety bundle | Aura, Cloaked and similar suites | Breach, alias, identity and scam protection | Less alarm-heavy, more local-first and directly tied to communication actions |
| Network privacy | Mullvad and other VPNs | Mature network protection | Use the VPN as an optional policy signal instead of building a VPN |
| Early privacy super-apps | Private.Ki, Haven, Veil | Chat/email or broad all-in-one claims | Ship verifiable guarantees, connector isolation and clear capability labels |

The positioning should be:

> **OSL is the privacy layer for the accounts you already use, plus a private network for the conversations that matter most.**

It should not initially position itself as a replacement for Proton, Signal or Mullvad. It can connect to or coexist with them while OSL's own services mature.

## Usability rules

These are release requirements, not visual preferences:

- A new user reaches a useful protected state in under three minutes.
- **Balanced** is selected by default and is safe without further configuration.
- No automatic deletion is enabled during onboarding.
- Every screen has one obvious primary action.
- Common settings use plain language; protocols, classifiers and connector internals live under Advanced.
- A warning explains the risk and offers a fix. It does not shame, block or threaten the user.
- Protection state is always icon plus text, never color alone.
- OSL never promises that a platform, recipient, screenshot, export or backup has forgotten data when that cannot be verified.
- The interface never describes ordinary external email or public content as end-to-end encrypted.
- Free and paid states are visible but never interrupt a safety warning or destructive confirmation with an upsell.

## Information architecture

The desktop app uses six primary destinations. Settings remains a fixed item at the bottom of the sidebar rather than a seventh competing destination.

| Destination | User's question | Main content | Primary action |
|---|---|---|---|
| **Home** | Am I protected, and what needs attention? | Overall state, one recommended action, connected-service health and recent protection | Fix the most important issue |
| **Inbox** | Where are my conversations? | Optional unified view, OSL Chat, OSL Circles, OSL Mail and supported connected-account views | Start a private conversation |
| **People** | Who do I trust and where do I know them? | Verified OSL contacts, platform identities, groups, audiences and whitelist policy | Add or verify a person |
| **Privacy** | What will OSL do for me? | Preset, global policy, platform exceptions, cleanup and solo privacy tools | Review or change protection |
| **Activity** | What did OSL actually do? | Warnings, scheduled jobs, deletion verification, connection failures and receipts | Review an item needing attention |
| **Connections** | Which accounts and devices are connected? | Platform accounts, OSL services, Mullvad and Android Workspace | Connect a service |

Settings contains appearance, accessibility, notifications, local storage, recovery, updates, diagnostic sharing and Advanced.

## Desktop shell

The main layout stays stable between screens:

```text
┌──────────────────────┬─────────────────────────────────────────────────────┐
│ OSL                  │ Current account / workspace       Protection status │
│                      ├─────────────────────────────────────────────────────┤
│ Home                 │                                                     │
│ Inbox                │ Main screen                                         │
│ People               │                                                     │
│ Privacy              │                                                     │
│ Activity             │                                                     │
│ Connections          │                                      Detail drawer  │
│                      │                                      when selected   │
│ Settings             │                                                     │
└──────────────────────┴─────────────────────────────────────────────────────┘
```

- Sidebar width: approximately 224-240 px.
- Main content uses a readable maximum width; it does not fill a wide monitor with tiny cards.
- A right-side detail drawer handles inspection and light editing without losing place.
- Destructive confirmation uses a focused modal only after the user has reviewed the affected items.
- Account and workspace identity remains visible in the header to prevent cross-login mistakes.

## Home

Home is a quiet launcher, not a dashboard. After onboarding it contains no setup hero, marketing copy, health score, activity wall or mandatory widget.

The stable desktop layout is:

- a compact OSL title and Settings control;
- a grid of sharp, near-square service and OSL-module tiles;
- a Friends rail on the right, with requests separated from approved contacts;
- at most one small, contextual status row when something actually needs attention.

Tiles launch installed native Windows clients, open fixed official sites in external Firefox or open an OSL-native module. Official service marks keep their original proportions and colors inside OSL's consistent square tile frame. Home shows only the service name, not implementation labels such as `Installed`, `No Windows app` or `Web`; native-versus-Firefox routing appears on the service detail screen.

The user can enter `Edit Home` to reorder, pin or hide tiles. This is an explicit edit mode with keyboard-accessible Move controls and an undo action; it does not depend on drag gestures or imitate iOS's icon-wiggle behavior. Hidden tiles remain available in the module library. Layout preferences are local, encrypted and scoped to the active OSL identity.

Optional OSL modules include Chats, Circles/groups, Notifications, Notes, Files, Network privacy and, later, Protected Passwords and Android Workspace. Notifications are off the Home screen by default and may be enabled per connected app. No third-party message content is copied into the notification hub unless the user explicitly enables a locally stored preview.

Connection state still uses four honest labels when details are opened: **Protected**, **Needs attention**, **Degraded** and **Offline**. Home itself communicates these states with a small icon-plus-text badge on the affected tile, never a giant banner or vague score.

## Onboarding

The first-run flow contains six short steps:

1. **Welcome:** explain that OSL protects existing accounts and can also provide private OSL communication.
2. **Choose protection:** Basic, Balanced or Maximum, with Balanced recommended.
3. **Choose apps:** detect installed supported Windows clients, offer reviewed one-click installers and allow skipping everything. The user signs in only inside the platform's native client.
4. **Choose how Send works:** recommend Manual, explain Clipboard and Double Enter in one sentence each, and allow a per-service or per-account override.
5. **Review defaults:** show exactly what may warn, sanitize, retain or delete. All destructive automation begins off.
6. **Secure recovery:** establish recovery, then optionally connect Mullvad or an Android device.

The Send step offers three ordinary modes:

| Mode | First-run explanation | Send authority |
|---|---|---|
| **Manual (recommended)** | OSL prepares the protected message; the user places it and sends it | The user performs placement and Send |
| **Clipboard** | OSL encrypts and copies; the user pastes into the verified conversation and sends | The user performs paste and Send |
| **Double Enter** | The first user Enter encrypts and places the protected capsule; a second, distinct user Enter sends it | Two separate, deliberate user key presses |

No mode silently sends. The selected default is always visible in Settings under `Send behavior`, with overrides at global, service and account level. Conversation-level temporary changes expire when that conversation closes. `Reset to inherited` and `Undo last change` are available from the first-run summary and Settings.

`Experimental Single Enter / one-click` is not one of the ordinary three modes. It is a separate Advanced opt-in, off by default, with a service-specific warning before enablement. After all target checks pass, one explicit user Enter or one explicit click may encrypt, atomically place and submit the capsule. It does not add anti-detection, simulated-human input or unattended sending.

### Presets

| Preset | Default behavior |
|---|---|
| **Basic** | Account health, email tracker blocking, attachment metadata warnings and exposure alerts |
| **Balanced** | Basic plus local before-send warnings, one-click attachment cleaning, monthly cleanup review and OSL E2EE suggestions for verified contacts |
| **Maximum** | Balanced plus stricter public-post checks, optional VPN-required actions and OSL E2EE required for chosen contacts |

Each setting shows `Inherited from Balanced` until the user overrides it. A single `Return to preset` action clears overrides.

## Optional OSL home modules

These modules share one OSL identity and recovery experience, but they do not share one universal content key.

- **OSL Notes:** encrypted local-first notes, offline by default, with explicit per-note sharing to verified OSL contacts. Search indexes remain encrypted at rest and are rebuilt locally after recovery.
- **OSL Files:** encrypted local vault and friend-to-friend transfer. File names, previews, OCR, metadata cleaning and content indexes stay local; relays see only padded ciphertext and bounded routing metadata. Import/export is explicit and includes a private encrypted archive option.
- **OSL Protected Passwords:** later, separately audited module using OS-backed unlock, per-record encryption, clipboard expiry, origin-bound autofill and a dedicated recovery threat model. It must not ship merely by reusing chat keys or generic Notes storage.
- **Notifications:** optional local aggregator for selected installed apps and OSL services. Each app has `Off`, `Count only` and `Local preview` choices; OSL servers never receive notification content.
- **Chats and Circles:** shortcuts into first-party OSL DMs and groups, configurable like any other Home tile.

Files and Notes may ship before Protected Passwords. Password management is a higher-risk product and requires an independent cryptographic, recovery, autofill and supply-chain review before release.

## Inbox and native OSL services

Inbox is opt-in. It must not pretend every connector can read or render a full conversation history.

The top-level filters are **All**, **OSL**, **Connected** and **Requests**. Within OSL, users can switch between Chat, Circles and Mail.

Every conversation row shows its real source and protection:

- `OSL Chat · End-to-end encrypted`
- `X DM · Platform encryption only`
- `Instagram · OSL overlay active`
- `Email · External recipient, not OSL E2EE`

### OSL Chat

OSL Chat is a first-party service, not an overlay. Its first release includes:

- DMs and group chats.
- Text, files, images, voice notes, replies, reactions and search on the local device.
- Verified contact safety numbers or QR verification.
- Per-chat retention, disappearing messages and view-once protected media.
- Multiple linked devices with explicit device approval and revocation.
- Encrypted backups that OSL cannot decrypt.
- Delivery states that do not leak more presence information than necessary.

Calls, large public communities, bots and third-party mini-apps come later. They expand the attack surface and are not required to prove the product.

### OSL Circles

OSL Circles is the private social layer:

- A user posts to named audiences such as Close Friends, Family or Work.
- Posts and comments are E2EE to the selected audience.
- Chronological feed only at launch; no behavioral advertising or engagement-ranking profile.
- Audience membership and changes are visible before posting.
- Forwarding and screenshots can be discouraged and watermarked, but never claimed to be technically impossible.
- A public OSL profile is optional and contains no private-circle membership data.

OSL should not launch as a broad public social network. A private network is differentiated and achievable; a global feed introduces moderation, abuse, discovery and surveillance problems before the core product is ready.

### Encrypted audience posts on other platforms

The composer can package an encrypted post and publish a carrier object to X, Instagram or another supported platform. Only selected OSL contacts can decrypt it.

The preview must state:

- The host platform can still see the account, time, audience interaction and carrier object.
- Non-OSL users see a clean invitation or unavailable-content card, not broken ciphertext.
- Deleting the carrier does not prove recipients deleted decrypted copies.

### OSL Mail

OSL Mail should be a staged product, not part of the first OSL Privacy release.

**Stage A - private email client:** connect existing Gmail, Outlook, Proton, Tuta or standard mailboxes; block trackers, sanitize links and attachments, support retention rules and label encryption honestly.

**Stage B - OSL aliases and relay:** disposable aliases, reply relay and breach isolation without requiring OSL to operate a complete mailbox.

**Stage C - OSL mailbox:** optional `@osl` address, custom domains, calendar and encrypted storage after deliverability, spam/abuse, recovery and operations are proven.

Messages between OSL Mail users can be automatically E2EE. Mail to an external address remains ordinary interoperable email unless the recipient opens an OSL encrypted message portal or uses a compatible encryption method. The composer must show this distinction before send.

This sequencing lets OSL deliver email privacy value early without risking the brand on unreliable delivery or a rushed mail server.

## People, identity and the universal whitelist

People is the human center of OSL.

An `OSL Contact` can contain multiple claimed identities, such as Discord, Instagram, X, email and an OSL Chat identity. OSL never silently merges two people because their names or profile images look similar.

Each contact shows:

- Verified, unverified or changed identity state.
- Connected platform accounts.
- OSL key and approved devices.
- Shared Circles and groups.
- Inherited protection policy and any exceptions.

The whitelist uses positive language such as **Trusted contacts**. Trust can mean `allow OSL E2EE`, `reduce warning sensitivity`, `allow files` or `exclude from cleanup`, rather than one dangerous all-or-nothing bypass.

## Privacy

Privacy begins with the active preset, then reveals five understandable groups:

1. **Before I send** - risky-content warning, Public Post Guard and attachment cleaning.
2. **After I send** - timers, view-once behavior and retention.
3. **Incoming content** - link, scam, tracker and file protection.
4. **My history** - cleanup scans, scheduled review and account archive.
5. **My exposure** - breaches, old accounts, data brokers and privacy drift.

Global policy appears first. A user can drill into a platform, account or conversation only when they need an exception. The hierarchy is always visible:

`Balanced preset → Instagram → @account → conversation exception`

## Before-send warning design

Warnings are optional and run locally by default. They cover user-selected risks such as accidental personal information, threats, harassment, discriminatory language, illicit-activity discussion, company secrets or a public post revealing location.

The interruption appears only for meaningful confidence:

```text
Check before sending

This post includes your current location and is visible to everyone.
[highlighted text]

[Edit post]   [Change audience]   [Send anyway]
```

- Never say a message `will be used by law enforcement` or declare it illegal.
- Use `could create legal, employment, safety or reputation risk` when appropriate.
- Show why it fired and highlight the relevant span.
- Remember dismissals locally to reduce repeated false positives.
- Never auto-report, auto-delete or secretly transmit flagged text.

For X and other public surfaces, Public Post Guard also shows audience, searchability, location/media metadata and whether the post is ordinary public content or an OSL encrypted-audience carrier.

## Platform overlay

OSL uses two related surfaces:

1. **Desktop overlay/sidecar:** a separate transparent or compact OSL window snapped to a verified native app. It follows the app window without injecting code, loading a DLL, modifying the platform client or embedding the service website.
2. **Full OSL Privacy:** the complete settings, People, Activity, cleanup and account-management experience.

The companion next to a message or post composer stays compact:

```text
[ Mode: OSL Protected ▾ ]
```

An optional **Focus workspace** arranges the real native client to the Windows work area and pins the same compact OSL toolbar above it or beside it. OSL does not reparent, embed or imitate the client. Entering Focus saves the client's exact monitor, bounds and window state; leaving it, closing OSL, crashing or pressing the global restore shortcut returns those bounds. The toolbar can auto-collapse, always remains visibly OSL-owned and never covers native account or recipient identity.

The mode control is persistent and scoped to the active conversation or composer. Its menu always distinguishes `Native` from `OSL Protected`, names the actual encryption scope and shows whether all recipients and linked endpoints are verified. Selecting Native returns control to the installed platform client without reducing its feature set. OSL Protected is enabled only for capabilities supported by the active service and participants.

### Secure Composer layer

The preferred interaction is an OSL text composer positioned directly over the platform's native composer. It matches the underlying box's geometry, theme, typography, corner radius and normal interaction rhythm, so it feels native to the surrounding app. It still includes a persistent, accessible `OSL Protected` pill or lock edge; the plaintext field must never be visually indistinguishable from the platform field.

```text
┌─────────────────────────────────────────────┐
│ OSL Protected                         [▾]  │
│ Type a message...                          │
│ [Encrypt] [Timer] [Check]            [→]  │
└─────────────────────────────────────────────┘
  Encryption: On · Recipient verified
```

The flow is:

1. Plaintext is typed into OSL-controlled memory, not the platform composer.
2. Local policy warns, translates, sanitizes or encrypts as requested.
3. OSL produces the protected capsule locally.
4. OSL follows the configured Manual, Clipboard or Double Enter handoff into the real platform composer.
5. Sending still requires the user's configured final action. No timeout, focus change, repeated key event or recovery routine may silently complete Send.

### Double Enter state machine

Double Enter is a deliberate two-step interaction, not synthetic typing:

```text
Idle
  -> Drafting
  -> First user Enter
  -> Verify focus + service + account + conversation + recipients + mode
  -> Encrypt and place capsule
  -> Awaiting second user Enter
  -> Reverify the same context
  -> Second distinct user Enter passes through to native Send
  -> Verify outcome, or report Unknown
```

- The first Enter is consumed by OSL and cannot send the plaintext. It creates an immutable target snapshot before placing the locally generated capsule.
- The second Enter must be a separate, trusted user key press after key-up. Key repeat, a held key, a synthetic event or OSL's own placement action cannot satisfy it.
- The armed state has a short, visible timeout. Expiry returns to a safe draft state; it does not send. The timeout may be adjusted within a bounded Advanced range, but cannot be disabled.
- Immediately before both placement and Send, OSL verifies native-composer focus, service, account epoch, conversation, recipients and protection mode. Any mismatch cancels the handoff, clears the native capsule if safely possible and preserves the encrypted local draft.
- Switching service, account, conversation, recipient, window or protection mode cancels the armed state. Losing focus pauses it; ambiguous focus cancels it.
- `Cancel`, `Undo placement` and `Reset send behavior` remain reachable by keyboard and pointer. Crash recovery may restore the draft, never the armed-to-send state.
- Verification after the second Enter reports `Sent`, `Not sent` or `Outcome unknown`. OSL never converts an unverified outcome into `Sent` and never retries automatically.

Manual and Clipboard use the same immutable target snapshot and pre-send checks. Clipboard clears its temporary protected payload after a bounded interval or confirmed paste, while preserving the encrypted draft for recovery.

### Send-mode account-risk display

The setup screen and each per-service/account override show two separate facts:

1. **Terms status:** `No known restriction`, `May conflict with terms`, `Explicitly restricted`, or `Unknown`.
2. **Enforcement likelihood:** `Low`, `Medium`, `High`, or `Unknown`, followed by the evidence source and last review date when evidence exists.

OSL must not invent a ban probability. If there is no reliable, current enforcement evidence, the likelihood is `Unknown`; terms wording alone does not justify a percentage or enforcement prediction. The explanation uses realistic language such as `This interaction may violate the service's automation rules and could put this account at risk`, then names which action creates that risk. It does not claim that a user gesture makes an otherwise restricted action compliant.

Experimental Single Enter/one-click stays a distinct Advanced option. Enabling it requires a service/account-specific acknowledgement and keeps a persistent `Experimental` state beside the composer. It never enables unattended sending, ban bypass, fingerprint spoofing, simulated timing or concealment from the service.

Atomic insertion is the default placement method: OSL places the completed protected payload in one verified operation. A separate `Compatibility typing` option may deterministically enter the same completed capsule character by character only when atomic placement is incompatible with a verified composer. It is visibly user-initiated, uses fixed behavior, has no randomized words-per-minute, pauses, mistakes, mouse motion or other human mimicry, and is never described as undetectable or terms-safe. Any focus, account, conversation or content mismatch stops it immediately without Send. Immediately before either placement method, an expandable `Exact platform payload` panel shows precisely what OSL will place, including carrier text, capsule version, length and any formatting degradation. Copying that preview is explicit and never sends it.

Formatting fidelity is part of connector capability, not a best-effort surprise. The preview compares the local draft with the platform payload and identifies any changed line breaks, mentions, emoji, links, Markdown/rich-text spans, replies, quotes or attachment references. Unsupported formatting blocks protected Send until the user accepts a clearly previewed conversion or edits the draft; OSL never silently changes meaning.

Inbound protected capsules render through an OSL-owned decrypted-message layer aligned with the native message location. The plaintext has a subtle but persistent OSL lock indicator, sender verification state and a control to reveal the original carrier. It must not obscure the actual sender, service or conversation. If identity, conversation binding, decryption or layout confidence is uncertain, OSL leaves the native carrier visible and reports the failure instead of rendering untrusted plaintext.

Protected transport offers two honest options:

- **Inline capsule:** the full ciphertext travels in the platform message and OSL stores no message payload for delivery.
- **Sealed relay token:** an optional padded, short-TTL encrypted object is fetched through an opaque token and deleted on successful fetch or expiry. OSL cannot read it, but the relay necessarily observes limited transient metadata such as access time, network address and object size; setup and the payload preview disclose this.

Carrier text may contain only a note written by the user, explicitly selected by the user or individually approved through the future Pro Cover Draft Relay described below. OSL never sends generated text autonomously, manufactures engagement, or runs unattended decoy activity.

### AI-assisted Cover Draft Relay (future Pro)

This optional feature is a user-controlled drafting and rendering layer, not an autonomous conversation bot. The canonical message is always the locally decrypted OSL plaintext; the cover draft is only the visible carrier that the user may edit, approve and send through the normal protected-message flow.

The complete flow is:

1. The sender writes plaintext in the OSL Secure Composer. OSL encrypts it locally for the verified recipient and pads the resulting E2EE blob into a coarse size bucket.
2. The encrypted blob is stored either on infrastructure controlled by the user or in a blind OSL relay with a short published TTL and delete-on-fetch behavior. The relay receives neither plaintext nor decryption keys.
3. A local model prepares a cover draft. The outbound preview shows the cover text, the canonical plaintext, the exact opaque OSL token and the platform payload together. The user must review and may edit the cover draft before every send.
4. The platform-visible message contains an explicit, authenticated opaque OSL token or pointer alongside the approved cover draft. The token is visibly attributable to OSL in the payload preview; it is not hidden in punctuation, whitespace, pixels, wording choices or other steganography.
5. The sender deliberately completes the configured Manual, Clipboard, Double Enter or separately opted-in Experimental Single Enter flow. Generating or approving a draft never grants Send authority.
6. On the recipient device, a visible-UIA connector notices the newly visible explicit OSL token, authenticates its service, account, conversation and sender binding, fetches the sealed object and decrypts it locally.
7. OSL immediately aligns an OSL-owned overlay containing the canonical plaintext with the native message. The overlay keeps the OSL lock, verified sender and real platform context visible; revealing the platform carrier remains one action away.
8. With the recipient's permission, the local model may use that user-authorized decrypted message to prepare the next cover draft and protected reply. The recipient must still review or edit and deliberately send every outward message. There is no automatic response loop.

Each relay token is an unguessable, high-entropy, single-use capability. It is authenticated, expires after a short TTL, is bound to sender identity, recipient device set, connector, account epoch and conversation context, and carries replay protection. A token copied to another conversation, sender, account or service fails authentication. Fetch is idempotent only for the authorized recipient transaction; successful delivery consumes the capability, and expiry or revocation makes it permanently unusable.

The UI discloses that the platform sees the user-approved cover text, explicit OSL token, sender, recipient, timing and ordinary platform metadata. A blind relay can additionally observe transient network address, access time, padded size class and delivery result. OSL publishes the exact retention interval for those fields, minimizes or aggregates them where possible and never calls the cover text anonymous or metadata-free.

Offline and failure behavior is fail-closed:

- If the relay is unavailable, the recipient sees the native carrier plus `Protected message unavailable - try again`; no guessed or partial plaintext is rendered.
- If the token expired or was already consumed, OSL shows `Protected message expired` and never reuses it.
- If authentication, sender/context binding, decryption or overlay placement fails, OSL leaves the native message visible, shows the specific safe error locally and does not generate a response draft.
- A sender who is offline may keep the encrypted local draft, but OSL cannot claim the message is deliverable until relay upload or inline placement is verified.
- Draft generation defaults to an on-device model. A user-selected external model is a separate explicit setting with a precise data-disclosure screen; OSL's own servers do not receive plaintext for generation.

This feature uses the same minimal-touch connector boundary as the ordinary Secure Composer: visible accessibility/UI Automation observation and an OSL-owned overlay only. It does not use webhooks, private APIs, page injection or hidden platform networking, and it never stores platform credentials, plaintext or keys on the relay.

Before handoff, OSL rechecks recipient and endpoint capability. A recipient change, newly unsupported attachment or verification failure blocks the protected send and presents a plain choice: fix verification, remove the unsupported item, or deliberately switch to Native/assist. Draft content is preserved, but OSL never silently downgrades, strips protection or sends through another mode.

For an ordinary unencrypted message, the same layer can provide local translation, spelling, metadata warnings and Public Post Guard before handing the user's text back to the native composer.

The layer follows the native composer using Windows UI Automation/accessibility anchors and local window geometry as the primary placement signals. A native transparent overlay may sample visible pixels, colors, typography and dimensions locally to align with light/dark themes, but pixel matching never replaces semantic target verification or authorizes Send. If the platform layout changes and anchor confidence falls, the overlay disengages instead of covering the wrong control, preserves the draft locally and opens the sidecar fallback with `Layout changed - review placement`.

Keyboard navigation, IME composition, dictation, screen readers, paste, drag/drop and draft recovery must work before the Secure Composer is considered supported on a platform.

Opening it shows only context-relevant controls:

- Protection state and reason.
- Send with OSL E2EE when the recipient is verified.
- Delete after: Off / 1 hour / 1 day / 1 week / Custom.
- View once for supported media.
- Clean attachment metadata.
- Check before send.

The companion can observe only the user-visible context needed for the active feature through the operating system's visible accessibility/UI Automation surface. Context parsing happens locally. It does not bulk scrape hidden history, export a contact graph or send composer text to OSL servers.

For protected text, OSL encrypts locally and places a clearly identifiable OSL capsule in the composer. On assist-only platforms, the user presses the platform's Send button. The capsule may include a normal user-written note, but OSL will not generate fake conversations or disguise automated traffic to evade platform detection. Metadata resistance belongs inside the OSL protocol through padding, batching and minimized identifiers, not deceptive public cover text.

Unsupported controls remain visible only when an explanation is useful. For example, `View once - Instagram does not expose a reliable control here` is better than silently hiding a feature the user expects.

The companion inherits the app policy. It is not a second settings app. A small collapsed settings drawer can appear directly below it for quick changes to encryption, timer, warning sensitivity and attachment handling; deeper settings open the app.

## Deletion, retention and view-once

Every destructive workflow uses the same sequence:

`Scan → Preview → Confirm → Execute → Verify → Receipt`

Activity states are **Scheduled**, **Running**, **Verified**, **Failed**, **Unsupported** or **Held**. `Sent request` is never displayed as `Deleted`.

OSL exposes three distinct guarantees:

1. **Platform removal:** OSL asked the platform to remove the account's copy and verified the platform no longer returns it where possible.
2. **OSL content expiry:** encrypted OSL content becomes undecryptable after key expiry, subject to recipient capture and already-open copies.
3. **Local removal:** OSL deleted the local cached copy from this device.

View-once protects ordinary reuse and local persistence. It cannot prevent a screenshot, camera, rooted device or compromised recipient. That limitation appears once during setup and remains available in details, without nagging on every send.

For company accounts, retention can apply to labeled secrets, projects and workspaces, but an administrator must preview policy impact. Legal hold and audit obligations override ordinary cleanup and are clearly shown.

The paid remote option is named **Always-On Retention Agent**, not `Cloud Delete`. At launch it runs on a user-controlled PC, NAS or customer-owned cloud VM. OSL can provide the installer and health UI, but the customer controls the machine, social sessions and encryption keys.

Where a platform permits unattended actions, the agent can execute and verify scheduled cleanup. Where it does not, the agent prepares a guided queue and sends the user directly to each item. It does not bypass platform controls merely because no official API exists.

An OSL-managed cloud worker is not offered at launch. A worker capable of acting while the user's devices are offline must hold a usable session or execution secret somewhere; that conflicts with the rule that OSL servers must not retain social-account access. A future confidential-compute design would require a separate threat model and could still not honestly promise that no linkable operational metadata ever exists.

### Sensitive-history scan and guided deletion

OSL can scan user-selected visible history locally for categories such as phone numbers, home addresses, current location, identity documents, financial details, credentials, private images, company secrets or other user-defined terms. Context is used to reduce false positives; a number in a game score should not be treated like a bank account.

The default result is a calm action list, not an account-risking deletion bot:

```text
3 items may reveal your home address

Discord · DM with Rose · March 4
[Open message]  [Show deletion steps]  [Ignore]
```

- OSL opens the exact item or closest supported search view.
- It provides platform-specific deletion directions when direct linking is unavailable.
- Nothing is deleted until the user acts or explicitly enables an authorized cleanup method.
- Findings and ignore decisions stay local.
- The UI avoids alarmist language and explains uncertainty.
- The scan and category summary are Free; the organized guided-cleanup workspace, manual cleanup queue and bulk tools are Pro.

## Solo privacy tools

These provide immediate value even when no friend installs OSL. They live under Privacy rather than crowding Home:

- **History Cleanup:** inventory and safely reduce old posts, messages and email.
- **Attachment Guard:** remove location, device and document metadata before upload.
- **Email Privacy:** block tracking pixels, identify redirect trackers and sanitize links.
- **Exposure Inventory:** show old accounts, breached identifiers and public data exposure.
- **Privacy Drift Watch:** detect when a platform changes settings, permissions or connection state.
- **Scam Shield:** warn about suspicious links, impersonation and payment requests.
- **Data Removal:** guide or automate broker requests and verify results rather than counting requests as success.
- **Encrypted Capsule:** send protected files or notes to a recipient through a browser when they do not use OSL.

Tools are shown through one recommended action and a searchable Tools list, not as eight competing dashboard tiles.

## Connections

Each service card shows:

- Official service icon and exact account identity.
- Connected, reconnect required, degraded, unsupported or paused.
- What OSL can currently protect.
- Last successful check.
- Manage and disconnect actions.

### Multiple accounts per service

Every OSL user may connect multiple accounts to the same service, including on Free. A service card expands into account rows rather than treating a platform as one global session.

Each account has its own:

- exact platform identity and optional local label such as Personal, Work or Test;
- local session/profile reference; platform credentials remain inside the official client or Firefox and are never copied into OSL's keyserver or application state;
- connector health and capability state;
- trusted contacts, exceptions and retention policy;
- local drafts, scan findings, cleanup queue and receipts;
- account epoch used to reject stale callbacks after a switch.

The active service and account remain visible above the Secure Composer. Switching accounts cancels pending handoffs, clears plaintext from the previous view and loads only that account's locally encrypted draft. OSL never infers that two accounts belong to the same person from names, avatars, email addresses or shared contacts.

Cleanup is always scoped to one explicitly selected account unless the user builds and confirms a multi-account job. The preview groups every affected item by service and account before execution.

Connector self-healing is visible but quiet. OSL can repair placement, update a local layout recipe or disable one broken capability without failing the whole service. A degraded card explains what still works and what changed.

The initial native-client companion matrix is:

| Service | Initial surface | Default action level |
|---|---|---|
| Discord | Installed Discord for Windows | Local protection and user-assisted handoff |
| Telegram | Installed Telegram Desktop | Local protection and user-assisted handoff |
| Signal | Installed Signal Desktop | Local protection and user-assisted handoff |
| WhatsApp | Installed WhatsApp for Windows | Local protection and user-assisted handoff |
| Outlook | Fixed official Outlook web origin in a dedicated local Firefox profile; reviewed desktop connector later | Native use now; protected overlay later |
| Slack / Teams | Installed desktop clients, later launch | Coming soon; no hidden browser fallback |
| Instagram / Snapchat / X / Messenger | Fixed official origin in a dedicated local Firefox profile | Native site use now; protected overlay later |
| Gmail / Yahoo / AOL / GMX / Mail.com / Proton | Fixed official webmail origin in a dedicated local Firefox profile | Native webmail use now; protected overlay later |

The launch does not depend on official APIs. OSL companion capabilities are not promised to match every native feature. Full platform functionality remains in the official Windows client or the service's real site in Firefox while each OSL tile exposes only capabilities OSL can actually enforce. Browser-backed services are explicit Firefox routes, never a silent WebView fallback. OSL uses only fixed reviewed HTTPS origins, keeps the Firefox profile local, and does not embed, inject into, inspect, or intercept the remote page. A permitted official integration can be added later without changing the user's privacy policy or interface.

### Self-healing companion system

Every companion is built from a versioned platform recipe and a shared semantic contract rather than one brittle CSS selector or screen coordinate.

1. **Multi-anchor discovery:** identify the composer from accessibility roles, editable state, nearby send/attachment controls, window geometry and stable semantic relationships.
2. **Local appearance matching:** sample only visible layout properties needed for placement and theme. Message content is excluded from the structural model.
3. **Capability probes:** test read, focus, handoff, draft recovery and placement separately without sending or deleting anything.
4. **Content-free fingerprint:** record a redacted structural signature so OSL can recognize that a platform layout changed without uploading the user's DOM, messages or contacts.
5. **Constrained repair:** choose or locally derive a compatible recipe only within declared anchors and permissions. A repair cannot add new account actions.
6. **Confidence gate:** high confidence enables the Secure Composer; medium confidence uses the sidecar; low confidence disables the affected feature and preserves the draft.
7. **Signed recipe updates:** OSL may distribute generic, signed compatibility recipes and rollback instructions. These contain no customer data and are independently reviewable.
8. **Automatic rollback:** rising placement failures, accessibility failures or wrong-target detections revert the recipe immediately.

Platform development changes should degrade one capability, not the whole app. OSL never uses self-healing to bypass anti-bot checks, rate limits, authentication or platform enforcement.

## Mullvad integration

Mullvad appears as a **Network privacy** card, not as an OSL-built VPN.

States are Protected, Connecting, Not protected and Unavailable. Depending on user policy, OSL can:

- Show the current state.
- Open or request connection through supported local integration.
- Warn before a sensitive action.
- Pause a connector until the protected network returns.

OSL never stores Mullvad account credentials or claims that VPN use makes message content private from the recipient or platform.

## Privacy.com integration (optional Pro)

Privacy.com is an off-by-default Pro connector. Enabling it opens Privacy.com in a dedicated local service profile and can surface quick access from payment warnings; disabling it removes the OSL shortcut without touching the user's Privacy.com account.

OSL does not proxy or retain card numbers, bank credentials, merchant transactions or Privacy.com login data. Card creation/autofill is offered only through Privacy.com's supported browser extension or documented API, with its own consent and eligibility checks. Until that supported bridge is implemented and tested, the UI labels the connector `Coming soon` rather than simulating card controls.

## Android

### Android Companion

The companion app provides OSL Chat, Circles, Mail, approvals, warnings, key management and supported share-sheet sanitization on a real phone.

### Android Mobile Workspace

The paid desktop feature runs selected Android-only apps inside a clearly isolated workspace. Its card exposes:

- Start or stop workspace.
- Installed apps and storage use.
- Network/VPN state.
- Lock, snapshot and wipe.
- Clipboard, file and notification permissions.

It is marketed as a privacy workspace, not a cloud phone and not a stealth automation environment. The default is local execution. A future hosted workspace would require a separate threat model and explicit customer consent.

This is a future Pro feature, not a dependency of the Windows launch. The virtual device gets its own encrypted disk, Android identity, app permissions, clipboard boundary, network policy and wipe key per OSL identity. Snapshots are encrypted locally before any optional backup; OSL infrastructure cannot mount the workspace. Host-to-Android clipboard, files, notifications, camera, microphone and location are denied until individually enabled. A hosted version is a different product boundary and cannot inherit the local product's privacy claims without a new audit.

## Windows installation and updates

OSL uses one signed per-user Windows installer, a Start-menu/Desktop entry and an atomic updater. The next release keeps the existing Tauri NSIS/updater pipeline so the service-architecture pivot and installer migration do not land together. Legacy Squirrel.Windows is not adopted. Velopack remains the preferred later migration candidate once the native-launcher release is stable.

- Installation and update manifests are signed; the client rejects an invalid signature, downgrade and wrong channel.
- Updates download in the background but installation remains visible and recoverable. Security-critical updates may be strongly recommended, never silently described as installed.
- Stable, Beta and local-development channels are cryptographically separated.
- Failed updates roll back to the prior signed build without deleting the OSL identity or encrypted local data.
- Native-app installers are a separate explicit user action. OSL invokes only reviewed fixed package identifiers and never executes a user-provided path or shell command.
- Release CI verifies the Authenticode publisher and exact artifact hash after build and after download; updater-key signatures do not substitute for Windows publisher signing.

## Activity and receipts

Activity is a human-readable proof surface, not a developer log.

Each event answers:

- What did OSL attempt?
- For which account and platform?
- What was the result?
- What evidence supports that result?
- What can the user do next?

Examples include `Attachment location removed`, `Instagram connection degraded`, `12 posts verified deleted`, `Message timer could not be enforced on recipient copy` and `OSL Chat device revoked`.

Raw diagnostic details are available through Copy diagnostics under Advanced and exclude message content, tokens and keys by default.

## Visual system

The interface should look calm, trustworthy and modern:

- Match the operating-system theme by default and remember the user's choice.
- Use the Windows system UI font stack; do not imitate Discord typography or ship a decorative web font for the shell.
- Use OSL cyan-blue for primary actions, neutral near-black/white surfaces for structure, green only for verified success, amber for caution and red only for destructive or failed states.
- Use 4/8 px spacing, square OSL-owned surfaces with zero corner radius and low-contrast borders. Home tiles are sharp squares with enough internal clear space for official marks. Official brand artwork is not distorted, but OSL never adds a rounded wrapper around it.
- Use one consistent SVG icon family plus official service marks; no emoji as structural controls.
- Keep ordinary transitions around 150-200 ms and respect reduced motion.
- Minimum interactive target: 44 px desktop/touch, 48 dp on Android.
- Text contrast meets WCAG AA; keyboard navigation and a visible focus indicator cover every flow.

Avoid cyberpunk decoration, terminal grids, glowing locks, giant setup banners, marketing typography, Discord-like blurple surfaces and constant animated shields. Privacy should feel dependable, not theatrical.

## Free and Pro presentation

Monetization should remain understandable:

### Free

- All initial launch service companions, with no artificial connector or account cap.
- OSL Chat, text-only Circles and text E2EE across supported companions.
- Before-send warnings, translation, Public Post Guard and quick privacy settings.
- Timers and retention for OSL-native text.
- Email tracker warnings and link sanitization.
- Sensitive-history scanning and a category summary.
- No encrypted images, image attachments, image cleanup or view-once media.
- No manual, guided, bulk or unattended history cleanup execution.

### Pro

- Encrypted images, files, attachment sanitization and view-once media.
- Guided manual cleanup, bulk cleanup where supported and verified receipts.
- Advanced retention schedules and the customer-controlled Always-On Retention Agent.
- Full exposure and cleanup workspace.
- OSL Mail aliases, then mailbox when available.
- Android Mobile Workspace.
- Encrypted archive and expanded device support.
- AI-assisted Cover Draft Relay, with local generation by default and approval required for every outward message.

There is no Business tier at initial launch. Team policy, legal hold and enterprise administration remain future product research rather than a purchasable plan.

Premium badges may label features in Privacy or Connections. Safety dialogs, warning screens and deletion confirmations never contain upgrade ads.

## Security requirements expressed through the GUI

- Platform credentials live in the operating-system secret store, never the shared UI database.
- Each connector has a separate process and least-privilege capability grant.
- Decrypted content is isolated by OSL account and platform account; switching accounts cancels stale work and clears the previous view.
- The UI renders only the result belonging to the active account epoch, preventing late callbacks from another login.
- OSL Chat, Circles and Mail use separate service keys under one recoverable identity rather than one universal encryption key.
- Recovery is explicit, tested and revocable. OSL cannot silently recover content it claims not to be able to decrypt.
- Optional analytics are content-free, transparent and off for sensitive diagnostic payloads by default.
- Every connector displays an exact capability matrix so UI promises cannot drift beyond what the backend can enforce.

### OSL server data rule

The enforceable server promise is:

> OSL servers never retain plaintext messages, media, social-account credentials, encryption keys, warning contents or scanned-history findings. Current relays do retain bounded ciphertext-routing metadata and replay receipts for published TTLs; that metadata is not described as anonymous, unlinkable or risk-free.

- OSL-native relays carry end-to-end encrypted envelopes and delete queued ciphertext after delivery or a short published expiry.
- Compatibility recipes, app updates and health rules are generic and contain no customer content.
- Cleanup findings and detailed receipts remain on the user's device or customer-controlled agent.
- Until opaque rotating inbox capabilities replace stable sender/recipient routing, the public data inventory must disclose the linkable identifiers, timestamps, access logs and deletion schedule used by each relay.
- Subscription authorization should use unlinkable or minimally identifying entitlement tokens where practical; payment records stay with the payment processor.
- Application logs exclude content, tokens, recipient identifiers and full IP addresses. Operational counters are aggregated and short-lived.
- There is no server-side AI training on user messages, drafts, warnings or history.

`OSL servers retain nothing that could possibly be used against a person` is not a technically honest absolute: network providers, payment systems and abuse controls can create some operational metadata. The product should instead publish an exact data inventory, retention duration and deletion schedule, then offer a self-hosted coordinator or customer-controlled agent for users who require the smallest possible trust boundary.

## GUI implementation sequence after approval

1. Freeze shared vocabulary, state names, capability labels and design tokens.
2. Build the shell, navigation, account header, responsive behavior and accessibility primitives.
3. Build onboarding, presets and Connections with real health states.
4. Build Home and Activity from the same typed state model and receipts.
5. Build the overlay contract and Discord reference overlay.
6. Build People, OSL identities and universal trusted-contact policy.
7. Build OSL Chat and small-group Circles in the Inbox.
8. Add solo privacy tools and the consistent destructive-action flow.
9. Add Instagram, X, Telegram and email connectors behind capability flags.
10. Add Mullvad and Android Companion, then the local Mobile Workspace.
11. Ship OSL Mail Stage A and B; approve Stage C only after a separate mail operations review.
12. Add the AI-assisted Cover Draft Relay only after ordinary sealed-relay delivery, visible-token parsing, local overlay provenance and per-message approval have passed security review.

Every step produces a usable vertical slice. The UI must never show a feature as active before the connector, cryptography and verification path exist.

## Prototype and test plan

The earlier `docs/prototypes/osl-hub/index.html` artifact is a retired interaction reference only; it is not the product shell and must not be presented as the OSL app. The current implementation prototype is an installed Rust/Tauri desktop build with packaged local assets and a native window lifecycle. The trusted local shell uses the operating system's WebView2 renderer, and allowlisted service sites open in separate embedded WebView2 child views with isolated per-account profiles. Remote child views receive no Tauri capability and must never become the privileged shell. Their cookies and cache remain ordinary OS-protected browser-profile data, not OSL-encrypted state. If the product requirement becomes literally zero WebView2, the shell and service surfaces must migrate to a native Rust/Windows UI toolkit in a separately tested rewrite. The app starts without a localhost server and never requires the user to open a browser URL.

Before production implementation:

1. Produce static desktop and mobile wireframes for every primary screen and the overlay.
2. Build a clickable prototype using realistic content and failure states.
3. Test it with the Rose and OSL accounts across login switching, DM, group chat and server/community scopes.
4. Run five first-time users through onboarding without help; at least four should reach Balanced protection and understand what OSL can and cannot delete.
5. Test keyboard-only, screen-reader, 200% zoom, reduced-motion and high-contrast use.
6. Test low-memory and disconnected states so the UI remains responsive while connectors recover.
7. Conduct a privacy-language review: every encryption, deletion, VPN and warning claim must match its actual guarantee.

## Acceptance criteria

The GUI plan is ready to implement when these statements are all accepted:

- One OSL Privacy plus thin overlays is the core form.
- The Windows-first companion list includes Discord, Telegram, WhatsApp, Instagram, Snapchat, Signal, selected email providers, X and Facebook Messenger. Slack and LinkedIn Messaging are later integrations.
- Every service supports multiple isolated accounts with the active identity always visible.
- Every connected service preserves its full native feature set through the installed platform client; OSL does not need to reimplement every platform feature in the app.
- Every service with a supported protected capability presents a clear Native versus OSL Protected choice, and the active mode remains visible per conversation or composer.
- OSL Protected is labeled E2EE only when all participating OSL-capable endpoints are verified; ordinary external platform messages and email are never described as OSL E2EE.
- Unsupported recipients, endpoints and features block protected send and offer a plain Native or user-assisted fallback explanation.
- Recipient, endpoint, attachment or capability changes trigger a fresh protection check; no downgrade, fallback or mode switch occurs silently.
- First run configures Manual, Clipboard or Double Enter, with Manual recommended; global choices can be overridden and reset per service/account.
- Double Enter requires two distinct user key presses, reverifies focus/account/conversation before placement and Send, expires safely and never restores an armed state after restart.
- Experimental Single Enter/one-click is a separate per-service/account opt-in with a realistic terms warning; it remains visibly experimental and cannot become unattended or anti-detection automation.
- Terms status and enforcement likelihood are shown separately; absent reliable current evidence, enforcement likelihood is `Unknown`, never a fabricated percentage.
- The exact capsule payload and any formatting changes are previewable before placement. Atomic insertion is the default; deterministic Compatibility typing is explicit, fixed, fail-closed and never human-mimicking or marketed as undetectable/terms-safe.
- Inbound decrypted overlays retain a visible OSL lock, sender verification and native-service context, and fail closed when binding or layout is uncertain.
- Semantic anchors authorize placement; local pixel/theme sampling may align the transparent overlay visually but never identifies a target or authorizes Send.
- Connectors leave service networking untouched: no private APIs, token capture, fetch/XHR interception, CSP stripping, runtime injection or background scraping; the platform still sees ciphertext and may observe accessibility automation.
- Users can choose inline ciphertext or an optional sealed, padded, short-TTL delete-on-fetch relay with its transient metadata disclosed. The future Pro Cover Draft Relay may prepare user-reviewed carrier drafts, but the token is explicit and authenticated, every outward message requires approval and Send, and autonomous or steganographic cover traffic remains prohibited.
- Balanced is the default and automatic deletion is off at first run.
- Home uses clear states, not a made-up privacy score.
- Inbox is optional and always labels the real platform and encryption scope.
- OSL Chat and private Circles come before a broad public social network.
- OSL Mail begins as client protection and aliases before OSL operates a full mailbox.
- Destructive actions always preview, confirm, verify and generate a receipt.
- Sensitive-history scan defaults to direct links and deletion directions instead of unattended account automation.
- Self-healing repairs layout compatibility but never spoofs fingerprints, human input or platform identity.
- OSL never promises deletion from recipient captures or E2EE for ordinary external email.
- Mullvad remains an integration, not an OSL VPN.
- Android Workspace is local-first and isolated.
- Advanced settings stay hidden until requested.
- The visual style remains calm, accessible and low-motion.
- Free includes all text/core connectors and accounts but no image features or manual cleanup; Pro adds media and cleanup. No Business tier ships initially.

## Final recommendation

The best first product is not a giant privacy super-app launched all at once. It is a very simple OSL Privacy with excellent Discord protection, a trustworthy overlay, OSL contacts and a small native encrypted Chat/Circles network. That creates a differentiated core people can understand.

Email protection, cleanup, more connectors, Mullvad and Android then add value around that core. A full OSL mailbox comes last, after OSL has earned trust through reliable encryption, recovery, connector health and honest receipts.
