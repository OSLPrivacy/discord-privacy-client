# Phase 7c — Discord UI Selectors

Reference document. Phase 7c implementation prompts pull selectors from
here. When Discord ships a UI refresh that breaks something, re-run the
survey, update this doc, regenerate fixes.

---

## Capture metadata

| Field | Value |
|---|---|
| **Discord build** | 541436 (stable channel) |
| **Version hash** | `28c300eef21ff861f54a265f19c97fdfa40094f1` |
| **Captured** | 2026-05-11 |
| **Method** | `scripts/survey-discord-selectors.js` + manual follow-up probes in DevTools |
| **Host** | Tauri WebView2 inside `cargo tauri dev` |
| **Channel contexts sampled** | DM, GC, server text channel |

---

## How to use this document

### Selector strategy

Every Discord class name carries a **hashed suffix** Discord regenerates
on each frontend deploy — `title__9293f` today may be `title__a47b1`
next week. The stable parts are the **semantic prefixes** (`title__`,
`container__`, `iconWrapper__`, …). All selectors below use prefix
matching via `[class*="prefix"]` so they survive suffix churn:

```js
// Brittle — breaks on next Discord deploy:
document.querySelector(".title__9293f");

// Stable — survives suffix churn:
document.querySelector('[class*="title_"]');
```

When a single prefix is ambiguous, **combine multiple `[class*="…"]`
attribute selectors** rather than escalating to the full hashed name.
The channel header pattern below
(`section[class*="title_"][class*="container__"]`) is the canonical
example.

### Data extraction via React fiber walks

Discord renders with React; every DOM element carries a fiber node at
a key matching `^__reactFiber\$` (or `__reactInternalInstance\$` on
older builds). Walking `fiber.return` upward and inspecting
`fiber.memoizedProps` recovers the underlying state: channel id, guild
id, user objects, recipient arrays.

Pattern:

```js
function fiberOf(el) {
    return el[Object.keys(el).find(k => k.startsWith("__reactFiber$"))];
}

function findInFiber(anchor, predicate, maxDepth = 30) {
    let fiber = fiberOf(anchor);
    for (let depth = 0; depth < maxDepth && fiber; depth++) {
        const v = predicate(fiber);
        if (v !== undefined) return { value: v, depth };
        fiber = fiber.return;
    }
    return null;
}
```

Depths below are **typical**, not guaranteed — Discord can change
component nesting in any release. Always walk up to `maxDepth=30`
and bail on first non-`undefined` hit. Wrap predicates in try/catch:
a hostile fiber shape (e.g. `memoizedProps` is null on a host
component) should never abort the walk.

---

## Surface 1: User profile popup / sidebar

Discord renders user profiles in **two modes** depending on the user
setting "Hide User Profile" in the channel header:

| Mode | Selector prefix | When |
|---|---|---|
| Sidebar (persistent right panel) | `[class*="user-profile-sidebar"]` | "Hide User Profile" OFF |
| Popout (floating modal) | `[class*="user-profile-popout"]` | "Hide User Profile" ON |

**Combined selector** — always use this so OSL code handles both:

```js
'[class*="user-profile-sidebar"], [class*="user-profile-popout"]'
```

### Container class (sidebar mode, observed verbatim)

```
outer_c0bea0 theme-dark theme-midnight images-dark user-profile-sidebar
```

### Inner DOM structure

```
.user-profile-sidebar
└─ .inner_c0bea0
   └─ .wrapper_da5890                ← top banner row with action buttons
      ├─ .bannerButton_fb7f94        ← Friend  (circular icon)
      └─ .bannerButton_fb7f94        ← More    (circular icon)
```

### Action buttons (in render order)

| Class prefix | Label | Style |
|---|---|---|
| `bannerButton_fb7f94` | Friend | Circular icon, top banner |
| `bannerButton_fb7f94` | More | Circular icon, top banner |
| `focusTarget__54e4b` | View Full Profile | Large rectangular |
| `clickable__26b1f` | — | Generic wrapper |
| `button__0f074` | Add Note (private) | Rectangular |
| `clickable__26b1f` | — | Generic wrapper |
| `header__1f6ca clickable__1f6ca` | — | Section header |
| `footerButton__272c7` | — | Footer action |

**Style template for OSL "Whitelist user…" button:** mimic
`.bannerButton_fb7f94`. Circular icon shape, fits alongside Friend /
More in the top banner row without disrupting layout.

### User ID extraction (React fiber walk)

```js
const el = document.querySelector(
    '[class*="user-profile-sidebar"], [class*="user-profile-popout"]'
);
const fiberKey = Object.keys(el).find(k => k.startsWith("__reactFiber"));
let fiber = el[fiberKey];
while (fiber) {
    if (fiber.memoizedProps?.user?.id) return fiber.memoizedProps.user;
    fiber = fiber.return;
}
```

- **Path:** `memoizedProps.user.id`
- **Typical depth:** 1
- **Returns:** `{ id, username, … }` (full user object)
- **Verified against three real accounts:**
  - `oslprivacy` → `900000000000000001`
  - `j5h0` → `264283974111723530`
  - `bestestperson2` → `1212181525739601990`

---

## Surface 2: Channel header

Top `<section>` of the chat column. Stable selector shape across DM,
GC, and server channel contexts.

### Container

Observed verbatim:

```html
<section class="title_f75fb0 theme-dark theme-midnight images-dark
                container__9293f themed__9293f">
```

**Selector:**

```js
'section[class*="title_"][class*="container__"]'
```

The two-prefix combination is required — `[class*="title_"]` alone
matches non-header elements that happen to start with `title`.

### Heading

```js
'h1[class*="title__"][class*="titleClickable__"]'
```

Heading class observed:

```
defaultColor__4bd52 text-md/medium_cf4812 defaultColor__5345c
cursorPointer_f75fb0 title__9293f titleClickable__9293f
```

### Icon buttons

**Style template:** `.iconWrapper__9293f.clickable__9293f` — prefix
match selector:

```js
'[class*="iconWrapper__"][class*="clickable__"]'
```

Existing icons observed (order varies by channel type):

| Position | Label |
|---|---|
| 1 | Voice Call |
| 2 | Video Call |
| 3 | Pinned Messages |
| 4 | Add Friends to DM |
| 5 | Hide User Profile |
| 6 | Members *(server channels only)* |
| 7 | Inbox |

**Style template for OSL encrypt-toggle + burn buttons:** mimic the
`iconWrapper__ clickable__` pair. Insert as additional children of
the header section; Discord's flex layout handles spacing.

### Channel ID extraction (fiber walk from header)

- **Path:** `memoizedProps.channelId`
- **Typical depth:** 2

```js
findInFiber(headerSection, f =>
    typeof f.memoizedProps?.channelId === "string"
        ? f.memoizedProps.channelId
        : undefined
);
```

---

## Surface 3: Channel type detection

Discord's three channel types map to OSL scopes:

| Discord `channel.type` | Channel kind | OSL scope kind |
|---:|---|---|
| `0` | Server text channel | `server_channel` or `server_full` |
| `1` | DM | `dm` |
| `3` | Group DM (GC) | `gc` |

### Extraction (fiber walk from any anchor)

```js
findInFiber(anchor, f =>
    typeof f.memoizedProps?.channel?.type === "number"
        ? f.memoizedProps.channel.type
        : undefined
);
```

- **Path:** `memoizedProps.channel.type`
- **Typical depth:** 11
- **Anchors that work:** channel header `<section>`, any rendered
  `[id^="message-content-"]` div

### Guild ID

For server channels (`type === 0`), `memoizedProps.channel.guild_id`
is also populated. For DM (`type === 1`) and GC (`type === 3`) it
is `null`.

Example (server channel `chat` in server `Smileland`):

```
id        = 1501872086690431006
type      = 0
name      = "chat"
guild_id  = 1501742059327983686
```

### Cross-check anchors

The channel-id fiber walk produces identical values regardless of
anchor:

- From channel header `<section>` → depth ~2 for `channelId`
- From `[id^="message-content-"]` → depth ~1 for `channel.id`,
  full `channel` object reachable up the chain

Disagreement between these two paths would be a bug in OSL's scope
detection. Cross-check in tests.

---

## Surface 4: Channel members / recipients

Three extraction paths, one per channel type.

### A. Server channels (`type === 0`) — members panel

Right-side members panel. Container selectors, in fallback order:

```js
'[aria-label="Members"]'         // most stable (aria contract)
'[class*="membersWrap"]'          // wraps the panel
'[class*="members-"]'             // legacy class hook
```

Aria-labelled container class observed: `content_d125d2`.
`membersWrap` container class observed: `membersWrap_c8ffbb hiddenMembers_c8ffbb`.

**Member row selector:**

```js
'[class*="member_"][class*="container__"]'
```

Row class observed: `member__5d473 member_c8ffbb container__91a9d clickable__91a9d`.

Row count matches visible online members. Example: 19 rows for the
`Smileland` `#chat` channel.

**User ID per row:** walk fiber up from the row element until
`memoizedProps.user.id` is reached (depth varies — start small, cap
at 30).

### B. Group DMs (`type === 3`) — recipients array on channel

No members panel. Fiber walk for `channel.recipients`:

- **Path:** `memoizedProps.channel.recipients`
- **Shape:** **array of Discord ID strings**, not user objects:

```js
[
  "1212181525739601990",
  "900000000000000001"
]
```

Usernames are NOT included — fetch them from Discord's user store
separately if needed for display.

### C. DMs (`type === 1`)

No enumeration needed. Exactly one recipient. Extract the peer's
Discord ID from `channel.recipients[0]` (single-element array) or
directly from the `channel` object.

---

## Surface 5: Bottom-left user area (OSL gear injection target)

Discord's "account" panel sits in the bottom-left of the viewport,
above the channel sidebar. Walking up from the current user's own
avatar (a stable anchor — the avatar always renders when logged in):

```
avatar (.avatar__44b0c)
└─ div.avatarStack__44b0c
   └─ <foreignobject>
      └─ <svg>.mask__44b0c.svg__44b0c
         └─ div.wrapper__44b0c.avatar__37e49
            └─ div.accountPopoutButtonWrapper__37e49     ← clickable wrapper
               └─ div.container__37e49                    ← user account block
                  └─ <section>.panels__5e434              ← INJECTION TARGET
                     └─ div.sidebar__5e434
```

**Injection target:**

```js
'section[class*="panels__"]'
```

**Strategy for OSL gear icon:** insert as an additional child of this
section, after the existing account / voice-control children. The
section uses flex layout; appending a new icon-shaped child slots
into the row without disrupting Discord's controls.

---

## Surface 6: CSS custom properties

Source: `getComputedStyle(document.documentElement)`.

156 properties matching the survey prefixes (`--background`, `--text`,
`--brand`, `--status`, `--button`). Inject OSL chrome via
`var(--…)` references so theme changes (dark / light / midnight /
high-contrast) propagate automatically.

### Key variables for OSL UI

| Variable | Sample value |
|---|---|
| `--background-brand` | `hsl(234.935 calc(1*85.556%) 64.706%/1)` |
| `--brand-560` | `hsl(233.115 calc(1*49.194%) 51.373%/1)` |
| `--brand-760` | `hsl(228.197 calc(1*57.009%) 20.98%/1)` |
| `--brand-430-hsl` | `231.739 calc(1*88.462%) 69.412%` |
| `--text-overlay-light` | `hsl(240 calc(1*3.846%) 89.804%/1)` |
| `--background-tile-gradient-pink-end` | *(gradient string)* |
| `--text-code-decorator` | *(mono color)* |
| `--text-code-variable` | *(mono color)* |
| `--text-code-tag` | *(mono color)* |

The full set is captured in
`scripts/survey-discord-selectors.js` output. Use:

```css
.osl-injected-button {
    color: var(--text-overlay-light);
    background: var(--background-brand);
    /* … */
}
```

Don't hard-code colors — Discord redefines these per theme and per
high-contrast mode.

---

## Known limitations / open questions

### Bare ID strings in GC recipient arrays

`memoizedProps.channel.recipients` is an array of Discord snowflake
**strings**, not full user objects. If the UI needs usernames /
avatars (e.g. for the GC member picker in 7c's whitelist UI), fetch
them from Discord's user store separately — the snowflakes alone are
sufficient for the OSL scope/whitelist logic and that's what 7b's
`recipients_for_scope` works with.

### Hashed suffixes will rotate

Every class hash in this document (`__9293f`, `__44b0c`, `__5e434`,
`__c0bea0`, …) **will change** across Discord builds. Always use
`[class*="prefix"]` matching. If an OSL surface breaks after a Discord
deploy, the fix is almost always "the hash rotated, but the prefix is
the same."

Selectors that combine multiple prefixes
(`section[class*="title_"][class*="container__"]`) are more
resilient than single-prefix selectors — even if one prefix is
renamed, the other usually anchors the match.

### "Hide User Profile" toggle changes profile mode

The channel header carries a toggle that switches between popout and
sidebar profile rendering. OSL code that touches the user profile
surface (Surface 1) MUST use the combined selector
(`'[class*="user-profile-sidebar"], [class*="user-profile-popout"]'`)
and handle both DOM trees. Inner structure (`.inner_c0bea0`,
`.wrapper_da5890`) is the same in both modes — only the outermost
container class differs.

### Members panel layout drift

Discord has redesigned the right-side members panel at least three
times in the last two years. The fallback chain
(`[aria-label="Members"]` → `[class*="membersWrap"]` →
`[class*="members-"]`) is ordered from most stable (aria contract is
an explicit accessibility commitment) to least stable. Add new
fallbacks to the front of the chain when Discord ships a redesign;
keep older selectors as the tail.

### `Hide User Profile` interaction with the "Whitelist user…" button

If the user has "Hide User Profile" ON (popout mode), the popout
auto-closes when clicking outside it — including when clicking
OSL's injected button if it's not a child of the popout root.
Inject the OSL button as a **descendant of the popout / sidebar
container**, not as a sibling, so clicks don't dismiss the surface.

### Self-anchor for the bottom-left injection

The walked path in Surface 5 starts from the user's own avatar.
On a not-yet-logged-in state, the avatar isn't rendered — Surface 5
will return `found: false`. OSL's gear injector must defer
installation until the user area mounts. Phase 7c can either
poll on a short interval or hook into the same MutationObserver
that watches for the channel header.

---

## Re-running the survey

When Discord ships a UI change that breaks an OSL surface, re-run:

1. Open `cargo tauri dev`. Log in.
2. Navigate to a DM with another user. Click their avatar so the
   profile popout / sidebar is open.
3. Open DevTools console. Paste `scripts/survey-discord-selectors.js`.
4. Save the printed JSON.
5. Navigate to a GC. Re-run. Save.
6. Navigate to a server text channel (one with a populated members
   panel). Re-run. Save.
7. Update this doc with the new class prefixes / fiber depths / DOM
   shapes from the three outputs.
8. Commit the doc; bump the "Captured" date + Discord build number at
   the top. Phase 7c selectors regenerate from this single source of
   truth.
