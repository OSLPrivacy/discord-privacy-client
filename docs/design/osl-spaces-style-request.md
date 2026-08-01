# OSL Spaces — style request for T7

**Owner:** T7. **Requested stylesheet:** `apps/osl-hub-ui/src/styles.css`.

This is a handoff, not CSS. T21 must not edit `styles.css`; add the eventual rules there when the
T21 Spaces markup lands. The app's CSP is `style-src 'self'`: runtime `<style>` elements and inline
`style=` attributes are dropped, so neither TypeScript nor markup may carry Spaces styling.

## Machine-readable handoff

```json
{
  "stylesheet": "apps/osl-hub-ui/src/styles.css",
  "forbidden": ["inline-style", "runtime-style"],
  "namespace": "osl-space",
  "requiredStateClasses": [
    "osl-space-state--offline",
    "osl-space-state--stale-roster",
    "osl-space-state--burn-queued",
    "osl-space-state--removal-unconfirmed",
    "osl-space-state--ack-unconfirmed"
  ],
  "prohibitedIndicators": ["presence", "last-seen", "online-dot"]
}
```

## Visual direction

Use the dark, compact three-column community layout from
`docs/prototypes/osl-chats-lab/` only as a layout reference: a Space picker, a channel rail, and a
message pane. Match the established OSL UI tokens, focus treatment, keyboard-visible controls, and
reduced-motion behavior in `styles.css`; do not copy the prototype's gradients, presence counts,
online labels, or voice controls. Voice is gated separately, and a Space must not show presence,
last-seen, or online-dot affordances.

Scope the rules below `osl-space` so they cannot alter DMs, Circles, or connected-platform pages.
The class names are the proposed markup-to-CSS contract for H1–H6; T7 may add nested selectors, but
should not rename these state classes without coordinating with T21.

## Required surfaces

- `osl-space`, `osl-space-picker`, `osl-space-channel-rail`, and `osl-space-message-pane`: stable
  desktop columns with independently scrollable rails and a message pane that can shrink without
  horizontal overflow.
- `osl-space-channel`: clear selected, hover, focus-visible, disabled, and unread treatments.
  Selection must not depend on colour alone.
- `osl-space-members`: a membership-only list. It may show the member identity and role, but never
  activity, device count, presence, or last-seen information.
- `osl-space-disclosure`: an always-visible, quiet but readable disclosure panel. It must remain
  distinct from success UI: admins see the same content as members; moderation is a request; and a
  removed member may retain content already held.
- `osl-space-governance-log`: an ordinary shared timeline, visible to every member. Do not style it
  like an admin-only audit console.
- `osl-space-join-pending`: pending admission has its own neutral, non-success treatment. Do not
  use a delivered/complete icon before rotate-before-admit completes; the explanatory copy that no
  earlier messages are available must stay readable.

## Honest-state treatment

Each state needs an icon plus text, adequate contrast, and a semantic distinction from completed
or delivered states. Do not use an endless spinner as the only signal.

- `osl-space-state--offline`: networking is unavailable; sending or receiving that needs the
  network is unavailable or queued, never presented as complete.
- `osl-space-state--stale-roster`: membership information is not current. Make posting controls
  visibly unavailable until the roster is current; do not imply that a message has reached the
  intended members.
- `osl-space-state--burn-queued`: local destruction and the server request are distinct. Show the
  remote request as queued, not as removed or confirmed.
- `osl-space-state--removal-unconfirmed`: removal stops future access but does not prove deletion
  of content already held. This must be a warning/outstanding state, never a green completion.
- `osl-space-state--ack-unconfirmed`: an expected acknowledgement has not arrived. Use an
  outstanding treatment and count-friendly layout; never equate no acknowledgement with refusal
  or compliance.

These names and treatments derive from T21-H2–H6, `features.md` §6.1 A2–A3 and §8.11, and owner
decision D66. They intentionally keep the prototype's community navigation while excluding the
tracking and premature-capability cues that its demo contains.
