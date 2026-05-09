# Design: Group messaging (v1 sender keys)

Status: **Draft.** Pairwise fan-out (the previous v1 design) has been
replaced by sender keys per user direction in this design round.

## v1 approach: sender keys

In v1, group messaging (Discord group DMs and server-channel members
with the app) uses **sender keys** with mandatory rotation. See
[`sender-keys.md`](sender-keys.md) for the full construction, threat
model, and open questions.

Highlights:

- One symmetric AEAD per send (no per-recipient encryption).
- Rotation distribution to each member is O(N) at rotation time, but
  amortized over hundreds of messages.
- Forward-secure ratchet within each rotation window.
- **Bounded blast radius**: rotations heal compromise within at most
  1 hour, 500 messages, on any membership change, or on suspicious
  events — whichever fires first.
- **No 50-recipient cap.** The previous v1 limit is removed.

## Off-app members

Channels mix app-using and non-app members. v1 behaviour:

- When channel encryption is **on**, the message is encrypted to app-
  using members only via the active sender chain. Non-app members do
  not see the message at all (the cover text decrypts only with the
  wrapped sender key).
- The sender's UI must show clearly which members will receive the
  message before send.
- Sending plaintext to non-app members requires explicitly disabling
  encryption for that send.

## Burn rendering behaviour

When a recipient's client receives a `404 Not Found` from the key
server attempting to fetch a wrapped key, rendering depends on
`content_type`:

| `content_type` | Rendering on 404                                                |
| ---            | ---                                                              |
| `text`         | Render the original stego'd cover text directly from Discord. **No** "[message unavailable]" or "[deleted]" marker. The message simply appears as whatever Discord stored. Observers cannot distinguish burned messages from messages that were always cover text. |
| `attachment`   | Subtle `[attachment no longer available]` placeholder. Binary attachments cannot render as readable cover. |
| `system`       | `[notification no longer available]` placeholder. **NOT** cover text — system messages (e.g. burn alerts) have a known semantic role; rendering them as cover gibberish would be conspicuous. |

This applies equally to DM and group conversations. Authoritative
spec lives in `key-server-api.md`.

### Cache and re-validation

- Recipients' clients **zero their local wrapped-key cache** on the
  next user interaction (input, focus change, scroll, click) **and**
  on a 5-minute timer regardless of interaction.
- For currently-visible conversations, periodic key re-validation
  runs **every 5 minutes**. Visible messages whose keys have been
  burned re-render as cover text.
- Currently-rendered messages may persist on screen until the user
  scrolls or the 5-minute re-validation cycle fires.

### Burn exposure window

Documented in [`THREAT_MODEL.md`](../THREAT_MODEL.md):

- ≤ ~5 minutes for messages currently visible at the moment of burn.
- Until next interaction or 5-min cycle for messages in inactive
  conversations.
- Indefinite for content already screenshotted, copied, or captured
  outside the app.
- Zero for recipients who were offline at the time of burn and haven't
  yet fetched the wrapped keys.

## Burn-and-alert vs burn-silently

Per-burn-action choice. **Default: silent.**

- **Silent burn**: server deletes wrapped keys; recipients see cover
  text after re-validation; no active notification.
- **Alerted burn** (opt-in, per action): sender sends a signed system
  message announcing the burn.

### Anti-abuse constraints (mandatory)

- **Per-burn-action choice only.** No global "always alert" setting.
  Every burn dialog asks silent or alert. Coercers cannot pre-
  configure an account to always alert.
- **Silent is the default selected option in the UI.** Alert is a
  secondary radio button.
- **Panic button and duress passphrase always trigger silent burn** —
  never alerted, regardless of any prior selection.
- **Alert text is fixed, system-generated.** Sender cannot customize:
    - DM burn:
      `"@sender cleared message history with you on [date]"`
    - Channel burn:
      `"@sender cleared their messages in this channel on [date]"`
    - Burn-all: optional per-affected-conversation alert delivery;
      sender chooses "alert each affected contact" or "skip all
      alerts".
- **Alert is signed by sender's `IK_X25519`** so recipients can verify
  authenticity. Server cannot fake alerts on a user's behalf.
- **Alert is delivered as an encrypted system message** through the
  same conversation channel and rendered in the recipient's UI as a
  system message. Wire-level: a regular `POST /v1/wrapped-keys` with
  reserved `content_type = "system"` and
  `system_message_kind = "burn-alert"`. See
  [`key-server-api.md`](key-server-api.md).
- **Burn-and-uninstall defaults to silent across all burns.** The
  user is leaving; no need to announce departures unless they
  explicitly choose to.

### Burn dialog UI

```
Burn messages with @username

What gets burned:
☑ All messages I sent to this person
  (forever unreadable to anyone fetching keys after now)

How:
● Burn silently  [default]
○ Burn and alert recipient

[Hold to confirm]
```

When alert is selected, the dialog expands:

```
The alert will say:
"@you cleared message history with @username on [date]"

Visible to: @username only (or all members of #channel)
```

## Duress wipe

When the duress flow runs (see
[`unlock-and-duress.md`](unlock-and-duress.md) — entering the duress
password OR exceeding the failed-attempt threshold), all sender-key
state is wiped as part of the broader Phase 2 local burn:

- Active and prior `RotationRoot` material.
- All sender chain keys (`CK_n`).
- Receiver-side caches of fetched sender chains for all peer senders.
- Skipped-key caches for all sender chains.

This is bundled into the Phase 2 local burn list documented in
`unlock-and-duress.md`. After duress, the device cannot decrypt any
past group messages, and the user re-registers fresh keys after
reinstall — group recipients then see a key-rotation event and must
re-verify the user's identity fingerprint before trusting new
sender-key distributions.

## Open questions

1. **Off-app member transitions.** When a non-app member installs the
   app mid-conversation, prior cover-text messages remain readable as
   the cover (Discord saw them as plain stego covers). **This is by
   design.** Document so users understand encrypted history is not
   retroactively readable to new members.
2. **Per-server vs per-channel toggle.** Default: per-channel. UX
   surface for per-server defaults under user control.
3. **Group-membership manifest.** Trust Discord's member list, or
   maintain our own membership manifest signed by a channel admin?
   Inherits from `sender-keys.md` open question 7.
4. **Re-validation cycle for very large channels.** 5-minute polling
   for every active conversation × every visible recipient × every
   key = potentially heavy load. Throttle when window is unfocused
   (default: pause re-validation after 60 s of no focus, resume on
   focus). See `key-server-api.md` re-validation section.
5. **Burn-alert delivery for burn-all.** Sender's UI exposes a single
   "alert each affected contact" toggle that fans out to N system
   messages. Confirm UX is clear at high N.

## Review gate

- [ ] UX review: how the user understands "this message is encrypted
      to these N members and not these M others."
- [ ] UX review: burn-and-alert flow, particularly the anti-abuse
      constraints around panic / duress.
- [ ] UX review: burn-rendering as cover text — does this confuse
      users who expect "[deleted]" placeholders? (We chose this
      deliberately so that an observer cannot tell which messages
      were burned vs which were always cover text.)
- [ ] Verify sender-keys memory and rotation cost stays bounded under
      heavy group use (cross-ref `sender-keys.md`).
- [ ] Burn dialog accessibility (hold-to-confirm timing, screen-
      reader support).
- [ ] Stego Mode 1 quality bar: passes Discord's automated scanning
      AND looks like plausible chat when read by a human scrolling
      history (since burned messages now render as their cover text).
