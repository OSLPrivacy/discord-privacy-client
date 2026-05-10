# Layer 10 — Discord internals (Vencord + BetterDiscord reference analysis)

This doc analyses how Vencord and BetterDiscord hook Discord's webpack
internals so we can choose an approach for our send/receive
encryption hooks. **Reference clones live in `reference/`** (gitignored)
and are read-only — not part of our build.

The two projects represent two paradigms:

- **Vencord** — *eager source-rewrite*. Patches the webpack module
  factory **source string** with regex before `eval()` ever runs the
  factory. Plugins ship `find` strings + `match` regexes + `replace`
  strings. By the time Discord's code executes, our injection is
  already baked into the function body.
- **BetterDiscord** — *runtime monkey-patching*. Lets webpack run the
  factories normally, then walks `webpack_require.c` (the module
  cache) and `webpack_require.m` (the module factories), wraps target
  exports with `Patcher.before` / `Patcher.after` / `Patcher.instead`
  proxies. No source rewriting; the original code stays as-is.

Both find modules using filter functions (`byProps`, `byCode`,
`byStoreName`, etc.). They diverge on **how** they get a hook in
place once the module is found.

This is reference material gathered from the upstream repos — not
ground truth for current Discord. The webpack module shapes we cite
here are documented as upstream filters at the time of clone; **all
of them must be re-verified against live Discord via DevTools on the
Tauri shell before we lock an implementation**. Discord's webpack
output is rebuilt continuously; signatures rot.

---

## 1. Webpack module discovery

### 1.1 Vencord — `Function.prototype.m` setter

`reference/vencord/src/webpack/patchWebpack.ts:97`:

```ts
define(Function.prototype, "m", {
    enumerable: false,
    set(this: AnyWebpackRequire, originalModules) {
        define(this, "m", { value: originalModules });
        // ... heuristics to skip non-Discord webpack instances
        // (sentry, libdiscore, fast-connect)
        // ... patchThisInstance() wraps wreq.m in a Proxy
    }
});
```

**Mechanism:** every webpack require object (`wreq`) sets `wreq.m`
(the module-factory dictionary) early in initialisation. By
defining a setter on `Function.prototype.m`, Vencord intercepts the
moment `wreq.m` is assigned, before any factory is run. It then:

1. Filters out Discord-internal webpack instances we don't want
   (sentry, libdiscore, fast-connect) by checking the call-stack
   filename and `wreq.p` (bundle path).
2. Wraps `wreq.m` itself in a `Proxy`. The proxy's `set` trap
   triggers patch evaluation when each new factory is registered.
3. Wraps every individual factory in a per-factory `Proxy` whose
   `apply` trap runs the patch logic (see §3.1) before forwarding
   to the real factory.

**Pre-condition:** the patch script must run **before**
`webpack_require` is initialised. In Discord's web app this means
injecting before the main bundle's `<script>` tag. Vencord
accomplishes this via Electron's `BrowserWindow.preload` script.
For us (Tauri shell loading remote `discord.com/app`), we'd need an
equivalent early-injection mechanism — see §6.

### 1.2 BetterDiscord — `webpackChunkdiscord_app.push` interception

`reference/better-discord/src/betterdiscord/webpack/require.ts:10`:

```ts
let __ORIGINAL_PUSH__ = (window.webpackChunkdiscord_app ??= []).push;

Object.defineProperty(window.webpackChunkdiscord_app, "push", {
    configurable: true,
    get: () => handlePush,
    set: (newPush) => { __ORIGINAL_PUSH__ = newPush; /* ... */ }
});

window.webpackChunkdiscord_app.push([
    [Symbol("BetterDiscord")],
    {},
    (__webpack_require__) => {
        if ("b" in __webpack_require__) {
            webpackRequire = __webpack_require__;
            listenToModules(__webpack_require__.m);
        }
    }
]);
```

**Mechanism:** Webpack chunks are pushed to a global array
`window.webpackChunkdiscord_app`. BetterDiscord:

1. Replaces the `.push` method with its own wrapper.
2. Pushes a sentinel chunk whose third element is a callback. Webpack
   invokes the callback with its own `__webpack_require__` to satisfy
   the chunk's "dependencies", which leaks the require function out
   to BetterDiscord.
3. From there, BetterDiscord wraps each module factory to install a
   listener that fires after the factory's `module.exports` is
   populated.

**Pre-condition:** the global must exist when this script runs but
the main webpack runtime must not yet have been used. Same
early-injection requirement as Vencord. BetterDiscord's actual
injection happens via Electron preload + an `early` preload that
runs *before* Discord's bundle.

### 1.3 Cross-reference

Both projects need to inject **before** Discord's webpack runtime
boots. Vencord intercepts at the `Function.prototype.m` setter
level; BetterDiscord intercepts at the chunk-push level. The two
approaches converge once they have a `webpack_require` reference.

For our project:

- The Tauri shell (Layer 9) loads `https://discord.com/app` directly.
  We don't control Discord's HTML, so we can't add a `<script>` tag
  to its document.
- Tauri 2 supports webview script injection via
  `WebviewBuilder::initialization_script(...)` (runs on every
  navigation, before page scripts) or
  `WebviewWindow::eval(...)` post-load. The former is the equivalent
  of Electron's preload; documented at the Tauri API level.
- We will need an init script that runs **before**
  `window.webpackChunkdiscord_app` is created. That's a hard
  ordering requirement; verify it with a console.log timestamp on
  the live shell before committing.

---

## 2. Module identification

Both projects use **filter functions** that test each candidate
export until one matches. The filters fall into a small set of
shapes.

### 2.1 Vencord filters

`reference/vencord/src/webpack/webpack.ts:61`:

| Filter           | What it tests                                          |
| ---              | ---                                                    |
| `byProps`        | All listed properties are `!== undefined` on `m`.      |
| `byCode`         | `Function.prototype.toString.call(m)` includes all     |
|                  | listed strings/regexes (m must be a function).         |
| `byStoreName`    | `m.constructor?.displayName === name`.                 |
| `byClassNames`   | One of `m`'s string-typed values matches each regex.   |
| `componentByCode`| Walks `.type` and `.render` for memos / forwardRefs    |
|                  | then runs `byCode` on the inner function.              |

`mapMangledModuleLazy(searchString, filterMap)` is a heavier hammer
for modules where Discord exports a tree of helpers under mangled
names — find the module by a string in its source, then assign each
helper by running a sub-filter against each export.

### 2.2 BetterDiscord filters

`reference/better-discord/src/betterdiscord/webpack/filter.ts`:

| Filter              | What it tests                                       |
| ---                 | ---                                                 |
| `byKeys`            | All listed keys are `in` the module.                |
| `byPrototypeKeys`   | All listed keys are `in m.prototype`.               |
| `byRegex`           | `m.toString()` matches the regex.                   |
| `bySource`          | The full *factory source* (from `wreq.m[id]`)       |
|                     | includes / matches each search.                     |
| `byStrings`         | `m.toString()` includes all listed strings.         |
| `byDisplayName`     | `m.displayName === name`.                           |
| `byStoreName`       | `m._dispatchToken && m.getName?.() === name`.       |

**Note the divergence on `byStoreName`.** Vencord checks
`m.constructor.displayName`; BetterDiscord checks
`m._dispatchToken && m.getName()`. Discord has changed how store
names surface over time. **Both must be tested live** before we
rely on either.

BetterDiscord additionally uses an `firstId` hint cached per module:

```ts
get MessageUtils() {
    return getByKeys(["sendMessage"], {
        firstId: 843472,
        cacheId: "core-MessageUtils"
    });
}
```

The `firstId` is the last-known module ID. The lookup checks that
module first; if it still matches the filter, no full scan. This
makes warm starts fast but is only an optimisation — the full filter
still runs on miss.

### 2.3 Modules we need (per upstream — verify before relying on)

The two repos identify the message send/receive modules as follows
*at the time of these clones*. Live Discord may have moved or
renamed any of this; the verification table in §6 is the gate.

| Purpose            | Vencord                                          | BetterDiscord                                   |
| ---                | ---                                              | ---                                             |
| Message send/edit  | `findByPropsLazy("editMessage", "sendMessage")`  | `getByKeys(["sendMessage"], {firstId: 843472})` |
|                    | exposed as `MessageActions`                      | exposed as `MessageUtils`                       |
| Message store      | `waitForStore("MessageStore", ...)`              | (would use `byStoreName("MessageStore")`)       |
| FluxDispatcher     | `waitFor(["dispatch", "subscribe"], ...)`        | `getBulkKeyed` filter                           |
|                    |                                                  | `byKeys(["dispatch", "subscribe", "register"])` |
| ChannelStore       | `waitForStore("ChannelStore", ...)`              | `byStoreName("ChannelStore")`                   |
| UserStore          | `waitForStore("UserStore", ...)`                 | `byStoreName("UserStore")`                      |
| DraftStore         | `waitForStore("DraftStore", ...)`                | similar                                         |
| Selected channel   | `waitForStore("SelectedChannelStore", ...)`      | similar                                         |

The `MessageActions`/`MessageUtils` distinction is just naming; both
target the **same** Discord module — the one with `sendMessage`,
`editMessage`, `receiveMessage`, `deleteMessage`, etc. on it.
Verifying which property name set is current is part of §6.

---

## 3. Send hook

### 3.1 Vencord — regex source-rewrite of the chat input module

`reference/vencord/src/plugins/_api/messageEvents.ts:37`:

```ts
{
    find: ".handleSendMessage,onResize:",
    replacement: {
        match: /let (\i)=\i\.\i\.parse\((\i),.+?\.getSendMessageOptions\(\{.+?\}\)?;(?=.+?(\i)\.flags=)(?<=\)\(({.+?})\)\.then.+?)/,
        replace: (m, parsedMessage, channel, replyOptions, extra) => m +
            `if(await Vencord.Api.MessageEvents._handlePreSend(${channel}.id,${parsedMessage},${extra},${replyOptions}))` +
            "return{shouldClear:false,shouldRefocus:true};"
    }
}
```

**How it works:**

- `find: ".handleSendMessage,onResize:"` is a unique-enough source
  string that identifies the chat-input component module. Any
  factory whose source includes that string is considered a match.
- `match` is a regex over the matched module's source. It captures
  variables (`parsedMessage`, `channel`, `replyOptions`, `extra`)
  that Discord's minifier renamed.
- `replace` is a function that takes the regex match plus captures
  and returns a string. The replacement appends an `await`-checked
  call to `Vencord.Api.MessageEvents._handlePreSend(...)`. If that
  returns `true` (the listener cancelled), the function early-returns
  with `{shouldClear: false, shouldRefocus: true}` — Discord's own
  handler never runs.
- The patched source is `eval`'d (`patchedFactory = (0, eval)(patchedSource)`)
  and the resulting function replaces the original factory in
  `wreq.m` (`patchWebpack.ts:600`).

**Why source-rewrite, not monkey-patch?** Vencord wants to inject
*inside* the function before Discord's local variables (channel,
parsed message, reply options, attachments, getSendMessageOptions
result) are consumed. By the time the function returns and a
monkey-patcher could intercept, those variables have already been
captured into a Promise chain. Source-rewrite gives access to the
local scope; monkey-patching only sees the public function
signature.

For our use case (pre-send encryption that needs the message
content + channel + reply context), this in-scope access is
essential.

**Public API the patch routes to:**
`reference/vencord/src/api/MessageEvents.ts`. Plugins register
`MessageSendListener`s via `addMessagePreSendListener(listener)`.
`_handlePreSend` iterates listeners, awaits each, and returns
`true` if any listener returns `{cancel: true}`. Listeners can also
**mutate** `messageObj.content` to modify the outgoing message
in-place — which is the path our encryption layer would use.

### 3.2 BetterDiscord — runtime monkey-patch of MessageActions.sendMessage

BetterDiscord doesn't ship a built-in send hook in the same shape;
plugins typically reach into `DiscordModules.MessageUtils` (=
`MessageActions`) and call `Patcher.instead("plugin-name", MessageUtils, "sendMessage", (_, args, original) => { ... })`.

This wraps the module's `sendMessage` property with a function that
gets `args = [channel_id, message, ...]` and an `original`
callable. The plugin can mutate `message.content`, choose to call
`original(...args)` or skip, etc.

**Trade-offs vs Vencord's source-rewrite:**

- **Pro (BD):** simpler. Just function wrapping. No regex, no eval.
- **Con (BD):** runs *after* the chat-input component has built up
  the message object. `getSendMessageOptions` has already populated
  `flags`, `nonce`, `tts`, etc. — anything that requires
  intercepting *before* that step is out of reach.
- **Pro (BD):** survives Discord moving things around inside the
  chat-input component, as long as `MessageActions.sendMessage`
  itself stays.
- **Con (Vencord):** every refactor of the chat-input component
  potentially breaks the `match` regex. Vencord's plugins ship
  build-number gates (`fromBuild` / `toBuild` per patch) for
  exactly this reason.

For our use case, monkey-patching `MessageActions.sendMessage` is
likely sufficient because the encryption transform we need is just:

```
plaintext = message.content
ciphertext = stego.encode(crypto.encrypt(plaintext, ...))
message.content = ciphertext
original.apply(this, args)
```

We don't need access to the chat input component's internal state.

### 3.3 Receive

Both projects use **`FluxDispatcher`** for inbound messages.
Discord's gateway → store path emits `MESSAGE_CREATE`,
`MESSAGE_UPDATE`, `MESSAGE_DELETE` (and similar) Flux events. Stores
subscribe to these and update their state; UI re-renders from
stores.

#### 3.3.1 Vencord — declarative `flux` map per plugin

Plugin shape, e.g. `reference/vencord/src/plugins/xsOverlay/index.tsx:204`:

```ts
export default definePlugin({
    name: "XSOverlay",
    flux: {
        MESSAGE_CREATE({ message, optimistic }) {
            if (optimistic) return;
            const channel = ChannelStore.getChannel(message.channel_id);
            // ...
        },
        CALL_UPDATE({ call }) { /* ... */ }
    }
});
```

`PluginManager.subscribePluginFluxEvents`
(`reference/vencord/src/api/PluginManager.ts:161`) iterates each
plugin's `flux` object and calls
`fluxDispatcher.subscribe(event, wrappedHandler)`. Handlers run
synchronously in dispatch order. Returning early or throwing does
**not** prevent Discord's own subscribers from receiving the event.

**Important:** the `optimistic` flag is `true` when the local
client emits `MESSAGE_CREATE` for a message it just sent (before
the server ack). Plugins should typically `if (optimistic) return`
unless they explicitly want to handle locally-originated messages.

#### 3.3.2 BetterDiscord — direct Dispatcher.subscribe

BetterDiscord exposes `DiscordModules.Dispatcher` and plugins call:

```ts
Dispatcher.subscribe("MESSAGE_CREATE", handler);
// later
Dispatcher.unsubscribe("MESSAGE_CREATE", handler);
```

Same primitive, no declarative wrapper. The lifetime management is
the plugin's responsibility.

#### 3.3.3 Where decryption fits

For our project, decryption needs to happen *before* the message
hits the UI. Two candidate hook points:

1. **`MESSAGE_CREATE` Flux subscription, mutate message.content
   in-place before it lands in MessageStore.** Risk: dispatch order
   isn't guaranteed; if MessageStore already cached the message
   before our handler ran, the UI gets the unmodified ciphertext.
   Need to verify dispatch order or use the `Flux.intercept` API
   if Discord still exposes one.
2. **Patch `MessageStore.getMessage(channel_id, message_id)` to
   transform on read.** Cleaner separation — decryption happens
   lazily at render time. But the "burn" semantics from our design
   need writes too (for the re-validation cycle). And every store
   consumer would have to go through the patched path.
3. **Patch the message render component (the `<Message>` React
   component or its content-renderer).** Latest interception point.
   Decrypt-on-render. UI consistency is naturally preserved because
   re-renders re-call the patched component. Pairs well with the
   re-validation cycle from C1.

**Vencord's `messageLogger` plugin** mutates messages by both
subscribing to `MESSAGE_CREATE`/`MESSAGE_UPDATE`/`MESSAGE_DELETE`
*and* patching `MessageStore` directly via the `patches:` array —
two-pronged. We should expect to do similar.

---

## 4. Lifecycle: when do hooks become safe?

Both projects gate plugin code behind a "ready" promise:

- **Vencord** — `onceReady` resolves when FluxDispatcher fires
  `CONNECTION_OPEN` (gateway connected → core webpack modules have
  initialised). Plugins that touch webpack commons must `await
  onceReady` first.
- **BetterDiscord** — `allModulesLoaded` resolves on idle after the
  last chunk has loaded.

For our send/receive hooks:

- The patch / monkey-patch needs to be **registered** before the
  target module factory runs (Vencord) or before
  `MessageActions.sendMessage` is first called (BetterDiscord).
- The Flux subscription can be registered any time *after*
  FluxDispatcher is found.
- We probably want to gate our crypto IPC commands behind
  `onceReady`-equivalent so a half-initialised state doesn't
  silently lose messages.

---

## 5. Mangled identifiers and resilience

Discord's webpack output is minified. Variable names look like `t`,
`e`, `r`, etc. Both projects work around this by:

1. **String matching on stable strings.** Translation keys
   (`#{intl::EDIT_TEXTAREA_HELP}`), error messages
   (`"Trying to open a changelog for an invalid build number"`),
   API endpoints (`"/users/@me"`), event types
   (`"MESSAGE_CREATE"`) tend to outlive variable renames.
2. **Property-set matching.** `editMessage` + `sendMessage` on the
   same object is structurally distinctive even after minification.
3. **Build-number gating.** Vencord stores a build-number reading
   helper (`getBuildNumber()` in `patchWebpack.ts:29`) that finds
   the build number from a stable error message and lets each
   patch declare `fromBuild` / `toBuild` ranges. Patches outside
   the supported range are dropped silently.

For our project: we should expect to write structural filters
(property sets, code substrings) and version-pin them. Plan for
breakage on Discord webpack rebuilds.

---

## 6. Verification plan against live Discord

Per the user's instruction — *"We'll verify the analysis against
actual current Discord (via dev tools on the Tauri shell) before
committing to an implementation approach"* — these are the checks
to run on the Layer 9 Tauri shell with DevTools open.

Each row gives a question, a way to answer it, and what we'd do if
the answer differs from upstream's snapshot.

| # | Question                                                                                  | DevTools probe                                                                                                                              | If different |
| - | ---                                                                                       | ---                                                                                                                                         | --- |
| 1 | Does `window.webpackChunkdiscord_app` exist after navigation to discord.com/app?          | Console: `typeof window.webpackChunkdiscord_app` — expect `"object"`, an Array.                                                            | If gone, BetterDiscord-style chunk push is dead. Prefer Vencord-style `Function.prototype.m` setter. |
| 2 | Can we leak `webpack_require` via the chunk-push trick?                                   | Run BD's snippet from §1.2 in console, check that the callback fires and we get a `__webpack_require__` with `b in __webpack_require__`.   | If `b` no longer present, swap detection key (BD has heuristic logic for this). |
| 3 | Does `MessageActions` / `MessageUtils` still expose `sendMessage` and `editMessage` together? | After step 2: `Object.keys(Object.values(__webpack_require__.c).find(m => m?.exports?.sendMessage && m.exports.editMessage)?.exports || {})` | If split across modules, our hook needs two filters. |
| 4 | Is `FluxDispatcher` still detected by `["dispatch", "subscribe"]`?                       | Same walk, filter on `m.exports.dispatch && m.exports.subscribe && m.exports.register` — if non-empty, ✓.                                  | Update filter to whatever the current shape is. |
| 5 | Does `byStoreName` use `m.constructor.displayName` (Vencord) or `m._dispatchToken && m.getName()` (BD)? | Find a known store (e.g. UserStore: filter for `getCurrentUser`) and check both shapes.                                                    | Use whichever still works. |
| 6 | Does `MESSAGE_CREATE` flux event still carry `{ message, optimistic, channelId }`?       | `FluxDispatcher.subscribe("MESSAGE_CREATE", e => console.log(e))`, send a message, observe.                                               | Field renames are minor; doc and adapt. |
| 7 | Is `.handleSendMessage,onResize:` still a unique substring identifying the chat-input module? | Filter all factories' source: `Object.values(__webpack_require__.m).filter(f => String(f).includes(".handleSendMessage,onResize:"))`.    | If 0 matches, this is dead — Vencord-style send patch needs a new `find`. We may need to abandon source-rewrite and fall back to monkey-patching `MessageActions.sendMessage` (§3.2). |
| 8 | Can a Tauri 2 `initialization_script` run *before* `webpackChunkdiscord_app` is created? | Set initialization_script that does `console.log("init", typeof window.webpackChunkdiscord_app)` and check the timestamp + value vs the first chunk push. | If init script runs after, we need a different injection point — likely `WebviewBuilder::on_navigation` redirecting to a local intermediate page, or a CDP-style attach. |
| 9 | What's the current Discord build number?                                                  | Vencord's `getBuildNumber()` regex: search factories for `"Trying to open a changelog for an invalid build number"`, extract integer.       | Document it in our patch metadata; gate our patches with `fromBuild`. |
| 10| Does the chat input still wire through a single `getSendMessageOptions(...)` call we can identify? | Search factories for `getSendMessageOptions(`. Number of results, surrounding shape.                                                       | Informs whether Vencord's `match` regex needs adapting (almost certainly yes — it's brittle by design). |

The verification should be done **once interactively** against the
running Tauri shell, with the results recorded back into this doc
(or a sibling) before any production code lands.

---

## 7. Decision space (do not commit yet)

Three plausible architectures emerge from the reference reading.
Listing them so the verification results in §6 can choose between
them:

**A. Vencord-style source-rewrite.** Inject before webpack runtime
boots; intercept `wreq.m` setter; ship a small set of `find`
strings + regex patches. Highest power (intercept inside chat-input
local scope), most maintenance burden.

**B. BetterDiscord-style runtime monkey-patch.** Wait for modules
to load; wrap `MessageActions.sendMessage` and the message render
component. Cannot intercept chat-input internals. Lower
maintenance burden, but encryption-on-send loses access to the
component-local message-options pipeline (probably fine for our
needs).

**C. Hybrid — Flux subscription + render-time patch.** No
chat-input intercept at all. Hook send via `MessageActions` monkey-
patch (encrypt on outbound), hook receive via render-component
monkey-patch (decrypt on inbound). Clean separation; the receive
side composes naturally with the C1 re-validation cycle and the
"render cover text on burn" semantics.

§6 verification result (especially row 7 — is `.handleSendMessage,onResize:`
still unique?) drives the choice between A and B/C. Rows 1–8 also
inform whether the early-injection foothold needed by A is even
available to us in the Tauri shell.

---

## 8. Open questions / risks

- **Anti-cheat / anti-tamper.** Discord ships server-side telemetry
  for client modifications. Vencord and BetterDiscord users have
  occasionally been reported. Our threat model puts us
  in a worse-than-Vencord position because we don't just modify
  rendering — we send wire content that doesn't match what a
  vanilla Discord client would emit (stego-encoded ciphertext). This
  is acknowledged in `docs/THREAT_MODEL.md`. Layer 10's risk: if
  Discord starts hash-checking the chat-input component's source
  string at runtime, source-rewrite (A) breaks immediately while
  monkey-patch (B/C) lasts longer.
- **Re-injection on navigation.** Tauri's WebView2 may navigate
  multiple times during the session (login flow, channel switches
  inside the SPA — though SPA navigations don't reload the bundle,
  they should be transparent). Verify whether
  `initialization_script` re-runs on SPA-internal navigations vs
  only on full document loads.
- **CSP.** Tauri's `csp: null` means Discord's own CSP applies.
  Discord's CSP does **not** allow inline scripts via `unsafe-inline`,
  but injection-via-init runs before the document's CSP attaches,
  so our injected JS executes regardless. Verify in §6 step 8.
- **Devtools accessibility.** WebView2 DevTools availability in a
  Tauri release build can be turned off. Make sure debug builds
  expose DevTools so we can run the §6 probes.
- **Stego decode failure surface.** Decryption on inbound side will
  fail for non-stego'd messages from regular Discord users; the
  receive hook must detect that and pass through as plain text. The
  C3 `Mode1ParseError` branch is exactly this signal — wire it to
  the render-component patch.

---

## 9. Files referenced (for re-reading)

Vencord:
- `reference/vencord/src/webpack/patchWebpack.ts` — webpack interception + patch eval engine.
- `reference/vencord/src/webpack/webpack.ts` — module-finding API (`find`, `findByProps`, `findByCode`, filters, `waitFor`).
- `reference/vencord/src/webpack/common/utils.ts` — declarative bindings of common modules (`MessageActions`, `FluxDispatcher`, etc.).
- `reference/vencord/src/webpack/common/stores.ts` — store bindings via `waitForStore`.
- `reference/vencord/src/webpack/common/internal.tsx` — `waitForStore` definition (delegates to `filters.byStoreName`).
- `reference/vencord/src/api/MessageEvents.ts` — public listener registration API for pre-send / pre-edit / click.
- `reference/vencord/src/plugins/_api/messageEvents.ts` — the patches that wire `_handlePreSend` into the chat-input source.
- `reference/vencord/src/api/PluginManager.ts` — `flux:` map subscription via `subscribeAllPluginsFluxEvents`.
- `reference/vencord/src/plugins/xsOverlay/index.tsx` — example of `flux: { MESSAGE_CREATE: ... }`.
- `reference/vencord/src/plugins/messageLogger/index.tsx` — example of multi-pronged store + flux + patch combination.

BetterDiscord:
- `reference/better-discord/src/betterdiscord/webpack/require.ts` — `webpackChunkdiscord_app.push` interception.
- `reference/better-discord/src/betterdiscord/webpack/filter.ts` — filter primitives.
- `reference/better-discord/src/betterdiscord/webpack/webpack.ts` — public API surface (`getByKeys`, `getBySource`, ...).
- `reference/better-discord/src/betterdiscord/modules/discordmodules.ts` — declarative `MessageUtils`, `Dispatcher`, etc. bindings with `firstId` cache hints.

Both clones are pinned at the time of clone; for current upstream,
re-pull the references. Both are under their own licenses (Vencord
GPL-3.0-or-later, BetterDiscord Apache-2.0); we don't ship any of
their code, only learn from it.

---

## 10. Current Vencord approach (post-obfuscation)

Live Discord verification on the Tauri shell (4136 cached webpack
modules / 3559 factories) found:

- **0** modules expose `sendMessage` as a runtime property.
- **0** factory sources contain `sendMessage` defined as a property
  assignment (e.g. `sendMessage:function...` or `sendMessage=`).
- **0** factory sources contain `"sendMessage"` as a quoted string
  literal.
- **6** factory sources contain `.sendMessage(` call syntax against
  some minified alias `e.sendMessage(...)`.

The function still exists at runtime — Discord can send messages —
but the **export name has been obfuscated**. The 6 call sites
preserve the dotted method-access syntax in their source (Discord's
obfuscator didn't rewrite property-access expressions), but the
*defining* module no longer has a property literally named
`sendMessage` exposed for Vencord-style property-set walking.

This invalidates §2.3's `MessageActions` row. What follows
documents what Vencord *actually* does today vs what its public API
*claims* — diverging since Discord's most recent obfuscation pass.

### 10.1 The MessageActions binding is stale, but Vencord hasn't migrated

`reference/vencord/src/webpack/common/utils.ts:181`:

```ts
export const MessageActions = findByPropsLazy("editMessage", "sendMessage");
export const MessageCache = findByPropsLazy("clearCache", "_channelMessages");
```

`findByPropsLazy` reduces to `m["editMessage"] !== void 0 && m["sendMessage"] !== void 0` over every cached module export
(`reference/vencord/src/webpack/webpack.ts:62`):

```ts
byProps: (...props: PropsFilter): FilterFn =>
    props.length === 1
        ? m => m[props[0]] !== void 0
        : m => props.every(p => m[p] !== void 0),
```

If 0 modules expose `sendMessage` at runtime, this filter matches 0
modules. The `proxyLazy` wrapper means the failure is silent until a
caller dereferences the binding — at which point the error is an
unintuitive "Cannot read properties of null" from inside the proxy.

`reference/vencord/src/utils/discord.tsx:164` is exposed as a public
helper:

```ts
return MessageActions.sendMessage(channelId, messageData,
                                  waitForChannelReady, options);
```

and is called from at least 4 plugins (`customCommands`,
`spotifyShareCommands`, `voiceMessages`, `greetStickerPicker`,
`fullSearchContext`). Per the verification, **all of these are
currently broken** unless they bypass MessageActions.

**Conclusion:** Vencord's *programmatic-send* path (plugin calls
`MessageActions.sendMessage` directly) is broken on current Discord
and has not been migrated. The Vencord codebase hasn't caught up to
the obfuscation. **We must not rely on this path.**

### 10.2 The actual send hook does NOT touch `sendMessage`

The user-typed-in-chat send hook (the one that matters for our
encryption layer) lives in `reference/vencord/src/plugins/_api/messageEvents.ts:37`
and uses **source-string substrings unrelated to `sendMessage`**:

```ts
{
    find: ".handleSendMessage,onResize:",
    replacement: {
        match: /let (\i)=\i\.\i\.parse\((\i),.+?\.getSendMessageOptions\(\{.+?\}\)?;(?=.+?(\i)\.flags=)(?<=\)\(({.+?})\)\.then.+?)/,
        replace: (m, parsedMessage, channel, replyOptions, extra) => m +
            `if(await Vencord.Api.MessageEvents._handlePreSend(${channel}.id,${parsedMessage},${extra},${replyOptions}))` +
            "return{shouldClear:false,shouldRefocus:true};"
    }
}
```

Identifiers it depends on:

| Identifier              | Where it appears                                          | Why it survives obfuscation |
| ---                     | ---                                                       | --- |
| `handleSendMessage`     | React class component method on the chat-input component  | Class method names show in React DevTools and the React reconciler keys debug output by them — Discord likely keeps them unobfuscated for their own ops. |
| `onResize:`             | Object literal key adjacent to `handleSendMessage`        | Object literal keys *for objects passed to React class definitions* are visible to React's display logic — same reasoning. |
| `getSendMessageOptions` | Internal helper called inside `handleSendMessage`         | Function name in source — survives if Discord's obfuscator only renames *exports* and not *internal local function names*. |
| `.flags=`               | Property assignment on the message-options object         | Property *names* on internal objects passed downstream tend to survive aggressive obfuscation because the receivers (gateway, REST API) require specific shapes. |
| `.parse(...).then(...)` | Pattern on the parsed-message Promise chain               | Stable JS shape, not an identifier. |

**None of these is the string `sendMessage`.** This is the hook's
load-bearing fact: Vencord's actual send interception doesn't depend
on the obfuscated property name surviving. It reads the chat-input
component's source as a whole and patches a specific spot identified
by the surrounding `handleSendMessage` / `onResize:` /
`getSendMessageOptions` / `flags` neighbourhood.

If even *those* identifiers got obfuscated, the patch breaks. Per
the user's probe (which only checked `sendMessage`), it's an open
question whether any of these survive. **The next verification
question is "does `.handleSendMessage,onResize:` appear as a
substring in any factory source?"** That's a single grep over
factory sources — see §10.5.

### 10.3 Edit hook uses an i18n key

Same file, same pattern, different identifier:

```ts
{
    find: "#{intl::EDIT_TEXTAREA_HELP}",
    replacement: {
        match: /(?<=,channel:\i\}\)\.then\().+?(?=\i\.content!==this\.props\.message\.content&&\i\((.+?)\)\})/,
        replace: (match, args) => "" +
            `async ${match}` +
            `if(await Vencord.Api.MessageEvents._handlePreEdit(${args}))` +
            "return Promise.resolve({shouldClear:false,shouldRefocus:true});"
    }
}
```

`#{intl::EDIT_TEXTAREA_HELP}` is Discord's i18n template-string
syntax (the `#{intl::...}` prefix is a Discord i18n compiler
artifact that survives in the bundled source). User-facing strings
are the most resilient identifier class — they're tied to
translation keys that have to be stable for translators. **i18n
markers like `#{intl::...}` are the gold-standard finder for
post-obfuscation Discord.**

### 10.4 What replaces property-set walking in current Vencord

For modules that aren't reachable via property-set walking,
Vencord's current toolkit:

**(a) `findByCodeLazy(...)`** — find a *function* whose
`Function.prototype.toString` includes specific stable substrings.
Examples from current Vencord source:

```ts
// reference/vencord/src/api/Commands/commandHelpers.ts:25
const createBotMessage = findByCodeLazy('username:"Clyde"');

// reference/vencord/src/plugins/roleColorEverywhere/index.tsx:27
const useMessageAuthor = findByCodeLazy(
    '"Result cannot be null because the message is not null"'
);

// reference/vencord/src/plugins/validReply/index.ts:31
const createMessageRecord = findByCodeLazy(
    ".createFromServer(", ".isBlockedForMessage", "messageReference:"
);

// reference/vencord/src/plugins/fullSearchContext/index.tsx:29
const useMessageMenu = findByCodeLazy(".MESSAGE,commandTargetId:");
```

The substrings are typically: error messages, asserts, internal
constants, dotted property accesses on minified locals (`.MESSAGE,`
is an enum reference), template literals.

**(b) `findComponentByCodeLazy(...)`** — same as `findByCodeLazy`
but walks `.type` and `.render` first to handle `React.memo` and
`React.forwardRef` wrappers. Used for finding React components by
their internal source.

**(c) `mapMangledModuleLazy(searchString, filterMap)`** — find a
module by a stable substring in its full source, then extract
multiple helpers from it by running per-helper sub-filters against
each export. From `reference/vencord/src/webpack/common/utils.ts`:

```ts
export const Constants = mapMangledModuleLazy('ME:"/users/@me"', {
    Endpoints: filters.byProps("USER", "ME"),
    UserFlags: filters.byProps("STAFF", "SPAMMER"),
    FriendsSections: m => m.PENDING === "PENDING" && m.ADD_FRIEND
});

export const NavigationRouter = mapMangledModuleLazy(
    "transitionTo - Transitioning to", {
    transitionTo: filters.byCode("transitionTo -"),
    transitionToGuild: filters.byCode("transitionToGuild -"),
    back: filters.byCode("goBack()"),
    forward: filters.byCode("goForward()"),
});
```

This is the pattern that *would* migrate the broken
`MessageActions` binding if Vencord chose to do so. A future
obfuscation-resilient `MessageActions` would look something like:

```ts
// hypothetical — not in Vencord's tree
export const MessageActions = mapMangledModuleLazy(
    /* some stable substring inside MessageActions's factory source */,
    {
        sendMessage: filters.byCode(/* stable internal substring */),
        editMessage: filters.byCode(/* ... */),
        getSendMessageOptionsForReply: filters.byCode(/* ... */),
    }
);
```

**(d) `findStore(name)`** — for Flux stores, Vencord's *current*
implementation prefers the dispatcher's own registry over property
walks. From `reference/vencord/src/webpack/webpack.ts:500`:

```ts
export function findStore(name: StoreNameFilter) {
    if (!fluxStores.has(name)) {
        populateFluxStoreMap();
    }

    if (fluxStores.has(name)) {
        return fluxStores.get(name);
    }

    const res = find(filters.byStoreName(name), { isIndirect: true });
    // ...
}
```

`populateFluxStoreMap` (`webpack.ts:473`) calls
`Flux.Store.getAll?.()` to enumerate every registered store via
Flux's own machinery, then falls back to the legacy
`m.constructor?.displayName === name` filter only if the Flux
registry didn't have it. **For receive-side wiring, this is the
modern path** — find the FluxDispatcher first via `["dispatch",
"subscribe"]` property set, then ask it to enumerate its stores.

### 10.5 Updated verification probes (the §6 table is partially stale)

Given §10.1's finding, four §6 rows need to be re-targeted at the
post-obfuscation reality. The new probes:

| #  | Question                                                                                | DevTools probe                                                                                                                  |
| -  | ---                                                                                     | ---                                                                                                                             |
| 7' | Does `.handleSendMessage,onResize:` survive as a substring in *any* factory source?     | `Object.values(__webpack_require__.m).filter(f => String(f).includes(".handleSendMessage,onResize:")).length` — expect 1.        |
| 7''| Does `getSendMessageOptions` survive as a substring in any factory source?              | Same shape, search for `"getSendMessageOptions"`. If 0, Vencord-style send patch has lost its anchor.                            |
| 7'''| Does `#{intl::EDIT_TEXTAREA_HELP}` (or any `#{intl::...}` token) survive as a substring? | Search for `"#{intl::"`. If matches > 0, i18n keys survive — most resilient anchor class.                                       |
| 11 | Does `Function.toString()` on the surviving 6 `.sendMessage(...)` callers reveal a discoverable upstream? | Walk those 6 modules: `String(factory)` → look for what they `require(...)` and chase the import chain to find the actual sender. |
| 12 | Are React class component method names obfuscated, or do `handleSendMessage`-shape names survive? | `Object.values(__webpack_require__.c).filter(m => m?.exports?.prototype?.handleSendMessage).length`.                            |
| 13 | Are i18n template strings the most-resilient finder class, as hypothesised?              | Sample 5 known i18n keys (e.g. `MESSAGE_DELETE_HEADER`, `EDIT_TEXTAREA_HELP`, `LOGIN_BUTTON`); check `#{intl::<KEY>}` substring presence. |
| 14 | Does the FluxDispatcher path still work end-to-end?                                     | After leaking `__webpack_require__`: find the FluxDispatcher via `["dispatch","subscribe","register"]`; call `.subscribe("MESSAGE_CREATE", e => console.log(e))`; send a message in any channel; observe console. |

If 7' / 7'' both fail, source-rewrite (architecture A) is dead and
we fall back to a completely different approach — likely **patching
inside the gateway-receive path** (which still has to handle
`MESSAGE_CREATE` payloads and is harder to obfuscate without
breaking Discord itself) or **patching at the React render layer**
(decrypt-on-render only — no send hook, but encryption can be done
in user-space via a slash command + `MessageActions` replacement).

If 7' passes but 7'' fails, the `.handleSendMessage,onResize:`
anchor still finds the chat-input module but the Vencord regex
won't match — we'd write our own regex against whatever stable
identifiers remain in that module.

If 13 confirms i18n keys are the most resilient class, we should
prefer them as primary anchors and treat React-class-method names
and internal-helper-function names as secondary.

### 10.6 Open question: how does Vencord stay alive on current Discord?

The contradiction this analysis surfaces:

- Per the user's verification, `findByPropsLazy("editMessage", "sendMessage")` finds 0 modules on current Discord.
- Per Vencord's current `webpack/common/utils.ts`, `MessageActions` is bound that way.
- Per Vencord's helper `sendMessage()` and 4+ plugins, `MessageActions.sendMessage(...)` is the call path.
- Vencord has an active user base (regular releases through 1.14.13).

Three hypotheses, in decreasing order of plausibility:

1. **Vencord users *are* hitting these bugs.** The plugins that
   programmatically send (`customCommands`, `spotifyShareCommands`,
   `voiceMessages`, `greetStickerPicker`, `fullSearchContext`) are
   silently broken; the *typed-in-chat* path (which is the only
   path the average user notices) still works because §10.2's
   chat-input source-rewrite is decoupled from MessageActions.
2. **Discord's obfuscation isn't uniform across users.** Stable
   release vs canary, A/B experiments, partial rollouts. If Vencord's
   user base skews toward stable and the user's verification was on
   canary (or vice versa), we'd see this asymmetry.
3. **There's a fix in flight.** A recent Vencord PR or branch we
   didn't pull may have migrated the binding to
   `mapMangledModuleLazy`. Re-cloning with full history (or pulling
   open PRs) would surface it. Worth a manual check on
   `github.com/Vendicated/Vencord` before designing our final
   approach.

For our purposes none of these hypotheses changes the current
plan: **don't rely on `findByPropsLazy("editMessage",
"sendMessage")` for our message hooks**. Use the source-rewrite
anchors from §10.2 if 7' / 7'' verify positive, fall back to one of
the alternatives outlined in §10.5 otherwise. Capture a fresh
diagnostic dump of the live shell's webpack state into a sibling
doc before locking the architecture choice in §7.

---

## 11. Phase 3 implementation notes

Verification probe results from Phase 2 picked architecture A
(Vencord-style eager source-rewrite) over B (BetterDiscord-style
runtime monkey-patch). Anchor and splice-point shape were nailed
down to:

| Layer            | Identifier                                                              |
| ---              | ---                                                                     |
| Outer anchor     | `.handleSendMessage,onResize:` — 1 hit, module **806202**.              |
| Secondary anchor | `getSendMessageOptions` — 2 hits (modules 352043 + 806202).             |
| Module gate      | Both anchors must be present → only 806202 qualifies (chat input).      |
| Splice site      | `E.A.sendMessage(h.id,_,void 0,I).then(...)` inside `handleSendMessage` |
| Splice regex     | `/(\w+(?:\.\w+)+)\.sendMessage\((\w+)\.id,(\w+),void 0,(\w+)\)/g`       |

Vencord's `findByPropsLazy("editMessage", "sendMessage")` finder
(stale per §10.1) is not used; the dual-anchor module gate
identifies the chat-input module by source-substring presence,
which is independent of the obfuscated runtime export shape.

### 11.1 What ships in Phase 3

**Goal:** prove the injection mechanism works end-to-end by
intercepting outbound chat messages and replacing them with a
visible marker. **No real crypto wiring** — that's Phase 4.

**Files:**

- `src-tauri/src/injection/mod.rs` — `pub const BOOT_SCRIPT: &str = include_str!("boot.js");`
- `src-tauri/src/injection/boot.js` — the bootloader (described in §11.2 below)
- `src-tauri/capabilities/main.json` — capability granting
  `allow-osl-encrypt-message` to the `main` window when loading
  `https://discord.com/*`. Without `remote.urls`, Tauri 2 rejects
  IPC from remote origins.
- `src-tauri/permissions/osl-encrypt-message.toml` — explicit
  permission declaration. Tauri 2 auto-generates these for plugins
  but not for app-level commands; the `tauri-build` resolver fails
  with "Permission allow-osl-encrypt-message not found" if this
  file is missing.
- `src-tauri/src/main.rs` — adds:
    - `osl_encrypt_message(channel_id, plaintext, _options) -> Result<String, String>`
      command. Phase 3 body: `Ok(format!("[OSL-STUB] {plaintext}"))`.
      Phase 4 swaps the body, not the signature.
    - Programmatic window construction via
      `WebviewWindowBuilder::new(app, "main", WebviewUrl::External(...))`
      chained with `.initialization_script(injection::BOOT_SCRIPT)`.
- `src-tauri/tauri.conf.json` — `app.windows: []`. The window
  config moved into Rust because `initialization_script` is only
  exposed on `WebviewWindowBuilder`, not on config-built windows.

### 11.2 Bootloader walkthrough (`boot.js`)

```js
(function () {
    "use strict";

    const ANCHOR_HANDLE = ".handleSendMessage,onResize:";
    const ANCHOR_OPTIONS = "getSendMessageOptions";
    const SEND_PATTERN =
        /(\w+(?:\.\w+)+)\.sendMessage\((\w+)\.id,(\w+),void 0,(\w+)\)/g;
    // ...
    function rewriteFactorySrc(id, src) {
        if (!src.includes(ANCHOR_HANDLE)) return null;
        if (!src.includes(ANCHOR_OPTIONS)) return null;
        // ...
        return src.replace(SEND_PATTERN, /* splice intercept call */);
    }
```

The dual-anchor gate is a **module filter, not a regex constraint** —
both substrings must appear *anywhere* in the factory source for the
module to be a rewrite candidate. The splice regex then runs over
the whole source; if it doesn't match (the call shape has drifted
from `<X>.<Y>.sendMessage(<chan>.id, <msg>, void 0, <opts>)`), we
log an error and leave the factory untouched. **Drift fails noisy,
not silent.**

The factory replacement uses Vencord's eval-an-expression pattern
(`patchWebpack.ts:600`) — prepend `"0,"` so a function declaration
parses as an expression, optionally insert `"function"` for
non-arrow factories, eval the result, swap into the chunk:

```js
const isArrow = newSrc.charAt(0) === "(";
const evalable =
    "0," + (isArrow ? "" : "function") + newSrc.slice(parenIdx);
const replacement = (0, eval)(evalable);
```

The push interception pattern matches BetterDiscord's chunk-leak
trick (§1.2) but goes the other direction — instead of pushing a
sentinel chunk to leak `__webpack_require__`, we wrap the array's
`push` so every chunk Discord pushes runs through us first:

```js
const arr = (window.webpackChunkdiscord_app =
    window.webpackChunkdiscord_app || []);
const origPush = Array.prototype.push;
arr.push = function () {
    for (let i = 0; i < arguments.length; i += 1) {
        processChunk(arguments[i]);
    }
    return origPush.apply(this, arguments);
};
```

A defensive pre-walk runs over `arr` first in case the
init-script-runs-first assumption ever drifts on some
Windows/WebView2 build — any chunks already in the array (typically
zero, but can't be guaranteed) get processed before the push wrapper
is installed.

The intercept itself:

```js
window.__OSL_INTERCEPT__ = function (
    target, methodName, channelId, message, third, options
) {
    // ... shape-check passthrough on unexpected types ...
    const tauriInvoke =
        (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke) ||
        (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke);
    return tauriInvoke("osl_encrypt_message", {
        channelId, plaintext: message.content, options: {},
    }).then(function (coverText) {
        message.content = coverText;
        return target[methodName](channelId, message, third, options);
    }).catch(function (err) {
        console.error("[OSL] osl_encrypt_message failed; passthrough plaintext", err);
        return target[methodName](channelId, message, third, options);
    });
};
```

`target[methodName].bind` is **not** used — the lookup happens at
call time inside the closure, which preserves Discord's `this`-binding
semantics correctly even if `E.A.sendMessage` is a getter or a
bound method.

The dual fallback for `tauriInvoke` (`__TAURI_INTERNALS__` first,
`__TAURI__.core` second) covers minor Tauri version drift in how
the invoke function is exposed. Both are checked because we don't
control which one is present in a given Tauri 2.x patch release.

### 11.3 Phase 3 trade-offs (vs Phase 4 scope)

**Fail-open passthrough.** Every error path in `__OSL_INTERCEPT__`
ends with `target[methodName](channelId, message, third, options)` —
the plaintext message goes through unchanged. This is **wrong** for
production (the user thinks they're sending encrypted, the wire is
plaintext) but **right** for Phase 3 (we want to prove the hook
fires; messages getting silently dropped on stub-error would be
worse than messages going through with a `[OSL-STUB]` prefix that
the user will obviously notice).

Phase 4 needs an explicit decision: **fail-closed (refuse to send,
show UI error)** vs **fail-with-warning (send plaintext but flag
loudly)**. Per `docs/design/THREAT_MODEL.md`, the project's stance
is "loud disclosure" — fail-with-warning is the likely choice, but
that's an architecture decision, not a Phase 3 implementation
detail.

**Stub command body.** `osl_encrypt_message`'s Phase 3 body is one
line:

```rust
Ok(format!("[OSL-STUB] {plaintext}"))
```

The signature is the stable contract:

```rust
async fn osl_encrypt_message(
    channel_id: String,
    plaintext: String,
    _options: serde_json::Value,
) -> Result<String, String>
```

Phase 4 changes the body to call into `crypto::pqxdh::*` +
`crypto::ratchet::*` + `stego::mode1::*` (or whichever stego mode
is active in the conversation). The wire shape stays unchanged
(`channel_id` / `plaintext` / `options` in, cover-text string out)
so the JS-side bootloader requires no edits.

**`options` is opaque.** Phase 3 takes
`_options: serde_json::Value` and ignores it. Phase 4 will
formalise the schema (reply context, attachment metadata, sticker
IDs, etc.) once we know what fields the crypto pipeline actually
needs from the chat-input frame.

**Single splice site.** Only outbound `sendMessage` is intercepted.
Inbound `MESSAGE_CREATE` decryption hooks (the receive side from
§3.3) aren't part of Phase 3 — they're a separate splice-site
verification + implementation, and our threat model doesn't break
catastrophically with send-only hooked (we just can't *read* what
we sent without the receive side). Phase 4 or 5 lands the receive
hook; sequencing TBD.

### 11.4 Verification (cross-compile)

`cargo check -p discord-privacy-client --target x86_64-pc-windows-gnu`
is green after Phase 3. WSL still needs the
`x86_64-w64-mingw32-windres` stub on PATH to bypass `tauri-winres`
during cross-compile — production Windows builds use MSVC's `rc.exe`
and don't hit this path.

End-to-end Windows verification has to run against
`cargo tauri dev` on a real Windows host:

1. Tauri shell launches; Discord loads normally.
2. DevTools console shows
   `[OSL] Boot script installed; hooking webpackChunkdiscord_app`
   immediately, then `[OSL] Hooked module 806202` shortly after
   (timing depends on when Discord's bundle pushes the chat-input
   chunk).
3. Sending a message in any channel logs
   `[OSL] intercept: channel=<id> plaintext_len=<N>` and the
   actually-sent message in Discord's UI is prefixed with
   `[OSL-STUB] `. **This is the load-bearing acceptance signal**:
   if the prefix appears in the chat history visible to the sender
   AND to other users in the channel, the hook is end-to-end
   functional.
4. No `[OSL] anchors matched but SEND_PATTERN did not splice`
   errors (would signal call-site shape drift).
5. No `[OSL] Tauri IPC bridge not present on window` errors (would
   signal capability misconfiguration).

Failure modes and fixes:

- **Hook silent** (no `[OSL] Hooked module` log). Most likely the
  init script runs *after* `webpackChunkdiscord_app` is created —
  the defensive pre-walk catches this only if pre-existing chunks
  are already in the array. Confirm via `console.log("[OSL] arr length at install:", arr.length)`
  added near the top of the boot script.
- **Hook fires, message not prefixed.** IPC permission failed
  silently. Look for `[OSL] osl_encrypt_message failed` in console;
  the catch path's error message will name the issue (typically
  "permission denied" or "command not allowed for this URL").
- **Discord crashes / chat sends silently fail with no log.** Our
  rewrite probably broke the chat-input function. The
  before/after debug log around the anchor (`console.debug`) shows
  the splice context — eyeball it for unbalanced parens or
  mis-captured variables.

### 11.5 What Phase 3 deliberately leaves to Phase 4

- Real `crypto::*` + `stego::*` integration in
  `osl_encrypt_message`'s body.
- A typed schema for the `options` parameter.
- Fail-closed-vs-fail-with-warning decision when crypto fails.
- Receive-side decryption hook (FluxDispatcher subscription or
  message-render component patch — see §3.3).
- Per-conversation salt management (where does the salt come from
  in the JS context? Probably another IPC roundtrip on conversation
  open).
- Cover-text fallback rendering (§3.3.3) wiring for inbound
  burned/tombstoned messages.
- Ratchet state hand-off between the JS hook and the Rust crypto
  state machine.

---

## 12. Round 4: pivot to fetch interception

Phase 3 ran four rounds of webpack source-rewriting before pivoting
to a network-layer hook. This section captures why the pivot happened
and what the new architecture looks like, so we don't redo the same
investigation in v2.

### 12.1 What rounds 1–3 proved (and didn't)

Rounds 1–3 progressively hardened the Vencord-style eager source-
rewrite harness:

| Round | Focus                                                                           |
| ---   | ---                                                                             |
| 1     | First attempt. Hooked module 806202 via `.handleSendMessage,onResize:` anchor.  |
| 2     | Honest hook logging (round 1's "Hooked" log fired without verifying the splice landed). Loosened regex for `_` / `$` identifiers. Dump on failure. |
| 3     | Two-rule architecture. Added `chat-input` rule with `invalidEmojis:[]` + `validNonShortcutEmojis:[]` anchors targeting module 249700. Kept `thread-reply` rule on 806202 as fallback. |

After round 3 the harness worked **mechanically** — every diagnostic
fired correctly, the `__OSL_INTERCEPT__` marker landed in eval'd
factory output, the IPC bridge was reachable, the substitution
regex captured the right call-site shape. End-to-end test: typed
"test" + Enter → no `[OSL] intercept:` log, no cover text in chat,
just Discord's own `[MessageQueue] Queueing message to be sent`.

The hooked modules' sendMessage call sites **were not on the runtime
execution path** of the typed-and-pressed-Enter user action.

### 12.2 The MessageQueue chokepoint and why webpack hooks miss it

Live-shell investigation of what fires on a real send turned up the
following:

- The runtime log `[MessageQueue] Queueing message to be sent` shows
  up on every typed send.
- Searching all 4136 force-loaded modules and all 3559 factories in
  `req.m` for the literal substrings "Queueing message", "Draining
  message", "MessageQueue" (and three other related strings):
  **0 hits across all sources**. The log string is dynamically
  constructed at runtime — it doesn't exist as a literal anywhere
  in the loaded webpack source.
- DevTools Sources tab Ctrl+Shift+F over all loaded JS files (not
  just webpack-managed): the MessageQueue logic lives in
  `sentry.f36cc9d429843670.js`, the **Sentry-wrapped Discord bundle**.
  This file is loaded directly by Discord's HTML, not via the
  webpack chunk loader, and is not enumerable through `__webpack_require__`.

Webpack-based hooks are blind to anything outside the standard
chunk loading path. The MessageQueue lives outside that path. Even
with a perfect anchor + regex, source-rewriting webpack factories
cannot reach the code we need to intercept.

The modules we *did* successfully hook (249700, 806202) contain
sendMessage call sites for adjacent code paths — thread creation,
gift sends, retry-flow scaffolding, GIF picker — but the typed-in-
chat send dispatches through MessageQueue first and never reaches
those call sites on the happy path.

### 12.3 Network-layer interception as the chokepoint

Regardless of how many internal abstractions Discord stacks
(MessageQueue → MessageActions → RetryQueue → whatever), the bytes
ultimately leave the client via `fetch` (or `XMLHttpRequest` —
Discord uses fetch). Wrapping `window.fetch` is a guaranteed
chokepoint: **every** outbound API call passes through it,
including any future internal restructuring Discord does on the
producer side.

Trade-off vs source-rewrite:

| Aspect                              | webpack source-rewrite       | fetch interception     |
| ---                                 | ---                          | ---                    |
| Visibility of internal call paths   | Yes (with the right module)  | No                     |
| Coverage guarantee                  | Per-module — depends on hooking the right one | Universal |
| Resilience to Discord refactors     | Anchors decay each rebuild   | Stable HTTP API        |
| Visibility before optimistic render | Yes                          | No (fetch fires after) |
| Mutation of local component state   | Possible                     | Impossible             |

The optimistic-render asymmetry is the main UX cost. Round 4's
fetch wrapper sees the request **after** Discord's local code has
already rendered the typed plaintext into the channel UI. The
sender sees their plaintext briefly, then the server-confirmed
view replaces it with the cover text. Other channel members only
ever see the cover text. Phase 3 acceptance tolerates this; Phase
4 / 5 will need a complementary local-render hook (likely
`MESSAGE_CREATE` Flux subscription per §3.3) so the sender's UI
matches what the wire actually carries.

For the v1 alpha threat model — which is "Discord can't read your
messages" — the wire-level guarantee is what matters. The
sender's-own-UI-momentarily-shows-plaintext window is not a
threat-model violation (the sender already had the plaintext, by
definition). It's purely a UX concern that can be addressed
without changing the wire-level intercept.

### 12.4 Round 4 architecture

`src-tauri/src/injection/boot.js` (full rewrite — webpack rules
removed):

```js
const SEND_RE = /\/api\/v\d+\/channels\/(\d+)\/messages\/?(?:\?|$)/;
const EDIT_RE = /\/api\/v\d+\/channels\/(\d+)\/messages\/(\d+)\/?(?:\?|$)/;

const origFetch = window.fetch.bind(window);
window.fetch = function (input, init) {
    const { url, method, isRequestObj } = resolveRequest(input, init);

    // PATCH /messages/<id> — Phase 4 territory; log + passthrough.
    const editMatch = EDIT_RE.exec(url);
    if (editMatch && method === "PATCH") { /* log */ return origFetch(input, init); }

    // POST /messages — the chokepoint we route through.
    const sendMatch = SEND_RE.exec(url);
    if (!sendMatch || method !== "POST") return origFetch(input, init);
    const channelId = sendMatch[1];

    // Extract body. init.body overrides Request body per spec.
    const initBody = init?.body;
    if (typeof initBody === "string") {
        return interceptBody(channelId, initBody, /* mutate */, /* passthrough */);
    }
    if (initBody != null) {
        // FormData / Blob / etc. — Phase 4 will handle multipart.
        return origFetch(input, init);
    }
    if (isRequestObj) {
        return input.clone().text().then(bodyText =>
            interceptBody(channelId, bodyText, ...)
        );
    }
    return origFetch(input, init);
};

function interceptBody(channelId, bodyText, onMutated, onPassthrough) {
    const parsed = JSON.parse(bodyText);
    if (typeof parsed.content !== "string" || parsed.content === "") {
        return onPassthrough();  // attachment-only / sticker-only
    }
    return window.__OSL_INTERCEPT__(channelId, parsed.content, parsed)
        .then(coverText => {
            parsed.content = coverText;
            return onMutated(JSON.stringify(parsed));
        })
        .catch(_ => onPassthrough());
}

window.__OSL_INTERCEPT__ = (channelId, plaintext, options) =>
    getTauriInvoke()("osl_encrypt_message", { channelId, plaintext, options });
```

Key design points:

- **Edit endpoint matched first** because `EDIT_RE` is a strict
  subset of `SEND_RE` (the edit URL also matches the send pattern
  up to `/messages`). Order matters.
- **Three body cases** explicitly handled: string in `init.body`
  (common), non-string in `init.body` (multipart — passthrough,
  Phase 4 territory), body in `Request` stream (clone + read async).
- **Five passthrough paths** all fail-open: body not JSON, no
  `content` field, empty content, IPC reject, IPC returns
  non-string. The user sees their plaintext go through unencrypted
  on any of these. Phase 3 tolerates that for the acceptance signal;
  Phase 4 needs a fail-closed-with-loud-warning policy decision per
  the threat model.
- **Idempotency guard** (`__OSL_FETCH_HOOK_INSTALLED__`). If the
  init script runs twice (round 2 observed this on some WebView2
  paths) we want one active wrapper, not a chain.
- **Tauri command stub unchanged** — same wire shape
  `(channel_id, plaintext, options) -> Result<String, String>`,
  same `Ok(format!("[OSL-STUB] {plaintext}"))` body. The contract
  between JS and Rust is stable across the architecture pivot.

### 12.5 Round 4 acceptance

Identical to round 3's acceptance, just with the interception point
moved:

1. Console on page load: `[OSL] Boot script installed; hooking fetch`.
2. Typing "test" + Enter in any channel:
   `[OSL] outgoing message: channel=<id> content_len=4`.
3. Channel UI shows `[OSL-STUB] test` (after the optimistic render
   flips to the server-confirmed view — see §12.3).
4. No `[OSL] outgoing /messages: body not JSON` warnings on the
   normal happy path.
5. Editing a message: `[OSL] outgoing edit (PATCH): channel=<id>
   message=<id>; passthrough (Phase 4 territory)` — and the edit
   goes through unmodified.
6. Uploading an attachment with content:
   `[OSL] outgoing /messages: non-string init.body (FormData);
   passthrough (Phase 4 will handle multipart)` — and the message
   goes through unmodified.

Failure modes worth eyeballing:

- **No fetch log on send** — Discord uses XHR for some path we
  didn't anticipate, OR Discord cached `fetch` before our wrapper
  installed. Add an XHR wrapper as a sibling intercept.
- **`outgoing message` log fires but channel doesn't show
  `[OSL-STUB]`** — the `__OSL_INTERCEPT__` call rejected (server
  rejected the modified body? IPC bridge broken?) and we
  passthrough'd. Look for the matching `__OSL_INTERCEPT__ rejected`
  log.
- **Channel shows `[OSL-STUB]` for the *first* message but not
  subsequent** — Discord might cache a fetch reference per
  component instance. If the wrapper installation timing is right
  for the initial bundle but Discord re-imports fetch later, we
  could miss subsequent calls. Verify by checking
  `window.fetch.toString()` in DevTools — should still be our
  wrapper.

### 12.6 Implications for Phase 4+

The intercept point moved but the IPC contract didn't:
`osl_encrypt_message(channel_id, plaintext, options) -> Result<String, String>`
remains the stable boundary. Phase 4 swaps the stub body for the
real `crypto::*` + `stego::*` pipeline; the JS-side fetch wrapper
needs no edits.

What Phase 4+ inherits from this round-4 pivot:

- **Multipart support** (attachments with content). FormData has
  a `payload_json` field that carries the message body; mutate
  that, rebuild the FormData, re-set the Request.
- **Edit endpoint hook**. Same shape but PATCH on a different URL,
  same JSON body schema with `content` key. Easy add.
- **Local optimistic-render hook**. Without it, the sender briefly
  sees their own plaintext. Likely a Flux `MESSAGE_CREATE`
  subscription with `optimistic: true` that mutates the message
  before it reaches the renderer.
- **Receive-side decryption hook**. Inbound `MESSAGE_CREATE`
  needs to route through `osl_decrypt_message` before the
  renderer sees it.
- **XHR fallback**. If Discord ever introduces an XHR-based
  send path, we'd miss it. Adding an XMLHttpRequest hook as a
  parallel chokepoint costs little.
- **Webpack hooks return for v2** if we need finer-grained
  interception (e.g. local state mutation before MessageQueue, or
  intercepting the chat-input keypress to apply per-character
  encryption). The harness scaffolding exists in git history;
  resurrecting it is a non-trivial but well-scoped exercise.

### 12.6 Round 5: XHR fallback added (the actual send path)

Round 4's fetch wrapper installed correctly — `[OSL] Boot script
installed; hooking fetch` fired on page load — but typing "test"
+ Enter fired Discord's `[MessageQueue] Queueing/Draining/Finished`
sequence (LogId:6617) without ever hitting the fetch wrapper. **The
typed-message-send path doesn't go through `fetch`.**

Likely cause: `libdiscore-wasm-fetch.js`. Discord's desktop client
(Electron-derived) ships a REST shim that uses `XMLHttpRequest`
under the hood for parity with Electron's network stack. The
WebView2 shell adopts the same code path even though `fetch` is
fully available — the shim was written for Electron compatibility
and Discord didn't refactor it to prefer `fetch` in browser/WebView
contexts. Other API paths (typing indicator, presence updates,
attachment uploads) may still use `fetch`; only some paths route
through the wasm-fetch XHR shim.

**Round 5 adds an XHR sibling wrapper alongside fetch.** Both
hooks are installed at script start; either can independently
match an outbound `/messages` POST.

#### Architecture

`XMLHttpRequest` interception is a two-step capture because
`open()` and `send()` are separate calls:

```js
const OSL_XHR_META = Symbol("OSL_XHR_META");
const origOpen = XMLHttpRequest.prototype.open;
const origSend = XMLHttpRequest.prototype.send;

XMLHttpRequest.prototype.open = function (method, url, async) {
    this[OSL_XHR_META] = {
        method: typeof method === "string" ? method.toUpperCase() : "GET",
        url: typeof url === "string" ? url : String(url || ""),
        async: async !== false,  // open()'s 3rd arg defaults to true
    };
    return origOpen.apply(this, arguments);
};

XMLHttpRequest.prototype.send = function (body) {
    const meta = this[OSL_XHR_META];
    if (!meta || !meta.async) return origSend.apply(this, arguments);
    // ... URL/method match ...
    // ... body-type check ...
    // ... defer origSend until IPC resolves ...
};
```

A `Symbol`-keyed property on the XHR instance carries the
URL/method/async-flag from `open()` to `send()`. `Symbol` is
chosen over a string property name to avoid any chance of
collision with an XHR property name Discord might use elsewhere.

#### Async deferral

`XMLHttpRequest.send()` is synchronous from the caller's POV
(returns `undefined` immediately, with the actual network call
queued asynchronously). Our IPC call to `osl_encrypt_message` is
also asynchronous (returns a `Promise`). We bridge the two by:

1. Returning `undefined` from our wrapper synchronously, matching
   `send()`'s spec'd return value. Discord's caller sees no
   apparent change.
2. Calling `interceptBody(...)` which kicks off the IPC roundtrip
   and returns a Promise.
3. Inside the Promise's `.then` callback, calling
   `origSend.call(xhr, newBody)` (or `origSend.call(xhr, origBody)`
   on passthrough paths).

The xhr instance enters its real network state (HEADERS_RECEIVED,
LOADING, DONE) some milliseconds later than it would have without
us; Discord's `onload` / `onreadystatechange` callbacks fire after
the deferred `origSend` completes. From Discord's perspective this
is functionally a slightly slower network — observable only as a
small latency bump (single-digit ms for the stub, larger once
Phase 4 wires real crypto).

#### Sync XHR

`xhr.open(method, url, false)` opens a synchronous XHR. We can't
defer a sync call onto a Promise without blocking the page (which
JS doesn't natively support). The wrapper passthrough's sync XHRs
unchanged. Sync XHR is deprecated in modern browsers and Discord
almost certainly doesn't use it.

#### Idempotency

Each hook has its own guard:
`window.__OSL_FETCH_HOOK_INSTALLED__` and
`window.__OSL_XHR_HOOK_INSTALLED__`. Round 4's IIFE-level guard
was replaced — if only one of the two has been installed previously,
the other still gets installed on this run. Final install log
reports which hooks fired:
`[OSL] Boot script installed; hooks: fetch + XHR`.

#### Shared helpers

The `interceptBody(source, channelId, bodyText, onMutated, onPassthrough)`
helper is shared between both wrappers, parameterised by `source`
("fetch" or "XHR") so the log line names which wrapper saw the
request. All log lines use the source label:

- Success: `[OSL] outgoing message (XHR): channel=<id> content_len=<N>`
- Edit:    `[OSL] outgoing edit (XHR PATCH): channel=<id> message=<id>; passthrough`
- Multipart: `[OSL] outgoing /messages (XHR): non-string body (FormData); passthrough`
- Failure: `[OSL] __OSL_INTERCEPT__ rejected (XHR); passthrough`

#### Passthrough symmetry

Both wrappers fail-open via the same five paths (body not JSON,
no `content` field, empty content, IPC reject, IPC returns
non-string). XHR-specific addition: if `origSend` itself throws
(e.g. state error from a deferred send into a closed connection),
we log and swallow rather than letting the error escape as an
unhandled rejection.

#### Double-intercept risk

If Discord ever uses an XHR-polyfill-via-fetch (unlikely — modern
browsers have native both), the XHR wrapper would intercept first,
mutate the body to `[OSL-STUB] hello`, then the polyfill's
internal `fetch` call would hit the fetch wrapper which would
intercept again, producing `[OSL-STUB] [OSL-STUB] hello`.

Phase 3 doesn't guard against this — observe the channel UI
during verification; if double prefixes appear, the polyfill
hypothesis is confirmed and we'd add a per-IIFE re-entry flag or
body-marker to skip the inner intercept.

#### Round 5 acceptance

Same as round 4 except the source label in the log changes:

1. Page-load console: `[OSL] Boot script installed; hooks: fetch + XHR`.
2. Typing "test" + Enter:
   `[OSL] outgoing message (XHR): channel=<id> content_len=4`
   (or `(fetch)` if some path uses fetch — both are valid).
3. Channel shows `[OSL-STUB] test` after the optimistic flip.
4. Editing logs `(XHR PATCH)` or `(fetch PATCH)` and goes through
   unmodified.
5. Attachment upload logs `(XHR)` non-string body and goes through
   unmodified.

If neither `(XHR)` nor `(fetch)` log fires on a typed send, the
send must be using a third path we haven't anticipated — possibly
WebSocket gateway, possibly a worker. WebSocket sends are real-time
events but not typically used for the message-create POST; that
would be a notable Discord refactor. Worker scope wouldn't see our
window-level hook installations and would need a different approach
(MessagePort interception or service-worker shim).

### 12.7 Round 6: detection mitigations

Round 5 verified — typed messages flow through the XHR hook,
`[OSL-STUB] test` lands in the channel. Phase 3 hooks work
end-to-end. Round 6 adds three layers of detection-resistance
hardening before we move on to Phase 4's real crypto. None of these
are about *making the hook work* — they're about *making the hook
harder to detect*.

This is **hardening against simple detection only**. Sophisticated
adversaries (Reflect introspection, Proxy detection via timing or
descriptor comparison, iframe-based fetch acquisition) are an
intentional v1 non-goal. The v2 overlay architecture sidesteps
detection entirely by not modifying Discord's runtime at all.

#### 12.7.1 Layer 1: Proxy-based wrappers

Round 5 used direct function override:

```js
const origFetch = window.fetch.bind(window);
window.fetch = function (input, init) { /* hook logic */ };
```

This shows up to descriptor-introspection checks as a JS-defined
function whose `.toString()` reveals the wrapper source. Replaced
with Proxy wrappers in round 6:

```js
window.fetch = new Proxy(origFetch, {
    get: makeToStringGetTrap("function fetch() { [native code] }"),
    apply: function (target, thisArg, args) { /* hook logic */ },
});
```

Property descriptor inspection
(`Object.getOwnPropertyDescriptor(window, 'fetch')`) returns the
same shape it would for the unwrapped native — Proxy is a function
exotic that proxies through to the target. The apply trap is the
hook logic; the get trap is layer 2.

`XMLHttpRequest.prototype.open` and `.send` are Proxy-wrapped the
same way.

#### 12.7.2 Layer 2: toString spoofing (two sublayers)

Detection-of-hooks 101:

```js
if (window.fetch.toString().includes('[native code]')) {
    // looks native
} else {
    // looks hooked — bail
}
```

To defeat this, `window.fetch.toString()` needs to return
`"function fetch() { [native code] }"`. Two distinct call patterns
need separate handling.

**Sublayer A — instance-level toString (`proxy.toString()`)**:

The fetch proxy's `get` trap intercepts `'toString'` access:

```js
function makeToStringGetTrap(spoofString) {
    return function (target, prop, receiver) {
        if (prop === "toString") {
            return function () { return spoofString; };
        }
        return Reflect.get(target, prop, receiver);
    };
}
```

When detection does `window.fetch.toString`, it triggers the get
trap which returns a fresh spoof function. Calling that spoof
function returns the native-code string. Other property accesses
fall through to `Reflect.get(target, prop, receiver)` so descriptor
inspection on every other property looks unchanged.

Why a get trap rather than direct property assignment?

`window.fetch.toString = () => '...'` on a Proxy without a `set`
trap forwards the assignment to the target — i.e. it modifies
`origFetch` itself. That's bad for two reasons: (a) it modifies the
underlying native function which we still call internally via
`Reflect.apply(target, ...)`, and (b) the spoof leaks if anything
else acquires a reference to `origFetch` somehow.

The get trap keeps the spoof local to the Proxy.

**Sublayer B — `Function.prototype.toString.call(proxy)`**:

This bypasses sublayer A entirely:

```js
// Sublayer A — sees our spoof:
window.fetch.toString();
//   → 'function fetch() { [native code] }' ✓

// Sublayer B — bypasses sublayer A:
Function.prototype.toString.call(window.fetch);
//   → without sublayer B: implementation-defined, may leak
//   → with sublayer B:    'function fetch() { [native code] }' ✓
```

`Function.prototype.toString` itself is wrapped in a Proxy whose
apply trap consults a `WeakMap<hookedFn, spoofString>`:

```js
const SPOOFED = new WeakMap();
SPOOFED.set(fetchProxy, "function fetch() { [native code] }");
SPOOFED.set(openProxy, "function open() { [native code] }");
SPOOFED.set(sendProxy, "function send() { [native code] }");

Function.prototype.toString = new Proxy(origFnToString, {
    apply: function (target, thisArg, args) {
        if (SPOOFED.has(thisArg)) {
            return SPOOFED.get(thisArg);
        }
        return Reflect.apply(target, thisArg, args);
    },
});
```

When detection does `Function.prototype.toString.call(window.fetch)`:
- Property access on `Function.prototype` returns the Proxy.
- `.call(window.fetch)` invokes the Proxy with `thisArg =
  window.fetch` (the fetch proxy).
- The apply trap sees `thisArg` in the WeakMap, returns the spoof.

Crucially, `Function.prototype.toString.call(someOtherFn)` (any
function not in our SPOOFED map) falls through to
`Reflect.apply(target, thisArg, args)` and returns the normal
toString — we don't lie about other functions, only about the ones
we hooked.

`WeakMap` over `Map` is intentional: WeakMap can't be iterated, so
attackers can't enumerate it to discover our hooked functions.
(`Object.getOwnPropertySymbols(Function.prototype.toString)` won't
reveal the closure either; closures aren't enumerable through
property introspection.)

#### 12.7.3 Layer 3: compile-time `DEBUG` strip

Round 5 emitted `[OSL]`-prefixed `console.log` lines on every
intercepted send and on install. A console-output scraper looking
for `'[OSL]'` substrings would identify a modified client.

Round 6 wraps every `console.log` and `console.warn` in
`if (DEBUG)`:

```js
const DEBUG = true;  // FLIP TO false BEFORE RELEASE BUILDS

// elsewhere:
if (DEBUG) console.log("[OSL] Boot script installed; hooks: ...");
if (DEBUG) console.log("[OSL] outgoing message (XHR): ...");
```

When `DEBUG = false`, V8 / SpiderMonkey dead-code-eliminate the
gated blocks during JIT optimisation. The `[OSL]` literal strings
don't appear in the executable JIT'd code — even a
`String.prototype.includes('[OSL]')` scan over function sources
finds nothing. (`window.fetch.toString()` returns the spoofed
native string, not the wrapper source, so this scan path is
already closed by sublayer A; the DEBUG strip closes the
console-output scraper path.)

`console.error` calls are intentionally **not** gated. Real
failures (IPC missing, encryption rejected, send errors) are
signal we want to surface even in production builds. The trade-off
is real-vs-zero `[OSL]`-prefix output: a console-scraper would see
`[OSL]` only on actual failure paths. The user's accepted stance:
errors are signal, not fingerprint to hide.

For Phase 3 verification we keep `DEBUG = true` so the existing
test signals still fire. **Flipping to `false` is a release-build
checklist item documented in the boot.js header comment.**

#### 12.7.4 What this defends against

| Detection technique                                                            | Defended? |
| ---                                                                            | ---       |
| `window.fetch.toString().includes('[native code]')`                            | Yes (sublayer A) |
| `Function.prototype.toString.call(window.fetch)` source check                  | Yes (sublayer B) |
| `Object.getOwnPropertyDescriptor(window, 'fetch')` shape comparison            | Yes (Proxy preserves function-exotic shape) |
| `Object.prototype.toString.call(window.fetch) === "[object Function]"`         | Yes (Proxy of callable returns "[object Function]") |
| `typeof window.fetch === "function"`                                           | Yes (Proxy of callable is `typeof "function"`) |
| `window.fetch instanceof Function`                                             | Yes (Proxy of callable passes instanceof Function) |
| Console output regex scan for `[OSL]` prefix                                   | Yes (DEBUG strip in release builds) |
| Same checks for `XMLHttpRequest.prototype.open` / `.send`                      | Yes (same machinery) |

#### 12.7.5 What this does NOT defend against (v1 non-goals)

| Detection technique                                                            | Notes                                                                        |
| ---                                                                            | ---                                                                          |
| Proxy detection via Reflect introspection or descriptor cycle observation      | Proxies are detectable by sufficiently determined adversaries.               |
| Iframe-based fetch acquisition (`iframe.contentWindow.fetch`)                  | An attacker can grab an untainted fetch from a fresh iframe; we'd need to also hook iframe creation. v2. |
| Timing attacks comparing Proxy vs native call latency                          | A Proxy hop costs measurable nanoseconds; consistent timing differences leak hook presence. |
| `Reflect.getPrototypeOf(window.fetch)` / `Object.getPrototypeOf` walks         | Could expose inconsistencies between the Proxy's pretend-shape and the underlying target. |
| WebAssembly-based detection                                                    | wasm code can read the page's globals at native speed and run more thorough comparisons. |
| Service Worker bypass                                                          | A service worker registered before our init script could intercept fetch at the SW layer, before our hook fires. |

These are sophisticated detection paths. The arms race against
them is unwinnable for a modify-Discord-in-place architecture
because Discord controls its runtime and we don't.

#### 12.7.6 v2 sidesteps detection entirely

v2 will use a separate **overlay window** that doesn't touch
Discord's runtime at all:

- Discord runs unmodified in its own webview / iframe.
- Encryption UI runs in our overlay, which has no influence over
  Discord's JS environment.
- Outbound: user types into our overlay → we encrypt → we use
  Discord's Web Worker / native API to actually send.
- Inbound: we observe the Discord webview's DOM (or Flux events,
  via a minimal touch) and decrypt visible messages in our overlay.

Detection becomes moot because Discord's runtime is identical to
an unmodified client. This is a much larger architectural shift
and lives in the v2 roadmap.

#### 12.7.7 Round 6 acceptance + the Sentry overwrite

##### Original acceptance plan

All round 5 acceptance signals still fire (`DEBUG = true`):

1. `[OSL] Boot script installed; hooks: fetch + XHR` on page load.
2. `[OSL] outgoing message (XHR): channel=<id> content_len=<N>` on send.
3. Channel UI shows `[OSL-STUB] test`.
4. Edit / multipart passthrough logs unchanged.

Round 6-specific verification (DevTools console, after page load):

1. `window.fetch.toString()` returns `"function fetch() { [native code] }"`.
2. `Function.prototype.toString.call(window.fetch)` returns the same.
3. `XMLHttpRequest.prototype.open.toString()` returns
   `"function open() { [native code] }"`.
4. `Function.prototype.toString.call(XMLHttpRequest.prototype.open)`
   returns the same.
5. Same for `.send` with `"function send() { [native code] }"`.
6. `typeof window.fetch === "function"` returns `true`.
7. `window.fetch instanceof Function` returns `true`.
8. `Object.prototype.toString.call(window.fetch) === "[object Function]"`
   returns `true`.

##### What actually happened (round 6 first verification)

Checks 3, 4, 5 (XHR-side) passed. Sanity controls (`Math.max`
unaffected, FPT trap falls through correctly for non-hooked
functions) passed. **Checks 1 and 2 leaked**: both
`window.fetch.toString()` and `Function.prototype.toString.call(window.fetch)`
returned Sentry's wrapper source containing
`function(...n){let r=Error()...nW("fetch",{...a},t.apply(M,n).then`
— Sentry's fetch instrumentation, identifiable by the `nW("fetch",...)`
hub-publish call.

XHR worked, fetch leaked. Same trap machinery, different runtime
fate. The diagnostic table from the previous round had a row for
exactly this: **case 2 — Sentry overwrites `window.fetch` after
our install**.

##### Diagnostic round (probe data)

Four immediate probes added at the install site (right after
`fetchProxy = new Proxy(...)`), plus a delayed +3s probe that
re-checks `window.fetch === fetchProxy` after Discord's bundle
has run.

Result on Windows verification:

- **All four immediate probes** showed our spoof. The Proxy was
  installed correctly and the get trap intercepted toString
  access end-to-end at install time.
- **Delayed +3s probe, webview context A**:
  `window.fetch === fetchProxy` → `false`; `toString()` returned
  Sentry's wrapper source.
- **Delayed +3s probe, webview context B** (Tauri runs two
  webview contexts on Windows for IPC isolation): canary still
  `true`, toString still spoofed.

Two webview contexts running with different Sentry-init timing
relative to our boot script. In one, Sentry got there second
(after us) and we won. In the other, Sentry's init somehow ran
between our install and the +3s mark, replacing our Proxy.

Diagnostic confirmed: **case 2, Sentry overwrites**. Our Proxy +
get trap + FPT trap + WeakMap machinery all work; they were just
displaced from `window.fetch` after the fact.

##### The fix: `Object.defineProperty` lock

Replaced:

```js
window.fetch = fetchProxy;
```

with:

```js
Object.defineProperty(window, "fetch", {
    value: fetchProxy,
    writable: false,
    configurable: false,
    enumerable: true,
});
```

Effect on Sentry's later `window.fetch = sentryWrapper` attempt:

- **Strict mode**: throws `TypeError`. Sentry wraps its
  instrumentation in try/catch (they have to — they can't crash
  apps), so the throw is swallowed and Sentry's wrapper simply
  doesn't install.
- **Sloppy mode**: silently fails. Same net effect; Sentry's
  wrapper doesn't install, our Proxy stays.
- **`Object.defineProperty` redefinition**: also fails because
  `configurable: false` forbids further redefinition.
- **`Reflect.defineProperty`**: returns `false` rather than
  throwing; Sentry's wrapper doesn't install.

`enumerable: true` is set explicitly to preserve original
behaviour (`Object.defineProperty`'s default is `enumerable:
false`, which would change `Object.keys(window).includes("fetch")`
results — that itself becomes a detection vector).

##### Side effect: Sentry's fetch telemetry is gone

Sentry's fetch-instrumentation collects breadcrumbs (request URLs,
response codes, error categorisation) for Discord's error
monitoring. With our defineProperty preventing the wrap, Sentry
can't observe fetch calls on this client. **What's affected**:

- Sentry **fetch breadcrumbs** for this client: gone.
- Sentry **XHR breadcrumbs**: still work. Sentry's XHR
  instrumentation is independent of fetch (it wraps
  `XMLHttpRequest.prototype` independently, and we wrap XHR
  ourselves but our wrap chains over Sentry's rather than
  displacing it because we install via plain assignment on a
  prototype property — Sentry's earlier wrap stays in the chain).
  Wait — actually we install XHR via plain assignment too, not
  via defineProperty. Need to verify XHR isn't displaced the
  same way at +3s. The round 6 first-verification probes
  confirmed XHR toString stays spoofed at +3s, so XHR coexists
  with whatever Sentry does. (Sentry's XHR instrumentation runs
  in a different code path that doesn't touch
  `XMLHttpRequest.prototype.open/send` after our wrap, or runs
  before our wrap and our wrap chains correctly over it.)
- Sentry **non-fetch observability** (unhandled rejections,
  console capture, error reporting): unaffected.

For the OSL threat model — Discord shouldn't be able to read
message content — losing Sentry's fetch breadcrumb dimension on
our clients is an acceptable trade. Discord's other observability
paths still function normally.

##### Concern: app code expecting writable `window.fetch`

If Discord's app code expects to swap `window.fetch` for any
legitimate reason (e.g. a feature-flag-gated re-instrumentation,
test harness shim), our defineProperty would break it.

**Mitigation in place**: the post-fix verification probes still
log `[OSL DIAG]` lines at install + +3s. If Discord's code does
ever try to reassign and expects the assignment to take, we'd
see the `[OSL DIAG] CANARY BROKEN` console.error on the +3s
probe (assignment failed silently in sloppy mode, our Proxy
stayed, but Discord-side code might be subtly malfunctioning).

If we observe symptoms (e.g. specific Discord features broken)
and the canary still holds, that's the signal that some Discord
code path expected to write `window.fetch` and silently failed.
We'd then need to widen the analysis — possibly accept Sentry
overwrite by NOT locking and instead re-establish our Proxy
periodically (worse trade, but preserves writability).

##### Post-fix verification

Probes already added at the install site (round 6 diagnostic
round, kept in place as ongoing self-verification):

- All four immediate probes: spoof returned.
- Delayed +3s `window.fetch === fetchProxy`: must now be `true`
  in **both** webview contexts (was `false` in one before fix).
- Delayed +3s `window.fetch.toString()`: must be the spoof.
- Delayed +3s `Function.prototype.toString.call(window.fetch)`:
  must be the spoof.
- `[OSL DIAG] CANARY BROKEN` error must NOT fire.

Plus all the original round 6 acceptance signals (1–8 above) must
pass.

Plus all Phase 3 functional acceptance: type "test" + Enter →
channel shows `[OSL-STUB] test`. The defineProperty lock changes
the property descriptor of `window.fetch` but doesn't change
behaviour for callers — they get the same Proxy they always did.

## 13. Phase 4: real crypto pipeline behind the same hook

Phase 3 proved end-to-end "typed plaintext → IPC → Rust → returned
cover string lands as outbound message body." Phase 4 swaps the
stub body for the real pipeline behind the **same wire shape**:
`(channel_id, plaintext, options) -> Result<String, String>`. The
JS bootloader doesn't know whether it's talking to the stub or the
real pipeline; only the Rust-side body changed.

This section is the spec for that body.

### 13.1 Phase 4 framing: dev/dogfooding milestone, not public

Phase 4 explicitly does **not** ship the full security envelope
the project is targeting. It ships a tractable, auditable,
end-to-end-working pipeline so the team can dogfood encrypted
messaging between two of their own dev accounts and shake out the
hook architecture under real Discord traffic. Specifically:

- **No PQXDH handshake.** Per-recipient X25519 ECDH only (the
  long-term identity X25519 leg). ML-KEM-768 is collected but not
  used in the encryption itself yet. Phase 5 introduces the
  handshake.
- **No Double Ratchet.** Each message uses a fresh random session
  key. There's no chain key, no message key derivation, no
  forward secrecy on the per-message granularity beyond what
  fresh nonces give you.
- **No receive-side decoder.** Sent messages are produced; the
  inverse `osl_decrypt_message` Tauri command and the
  receive-time DOM hook live in Phase 5.
- **No quality stego (Mode 0 only).** Cover text is base64 with
  the `DPC0::` magic prefix. NOT natural-looking; trivially
  flagged as "weird random-looking string" by anyone reading the
  channel. Mode 1 (template-based natural text) is deferred to
  the final pre-public-beta scope-out.
- **Static recipient config only.** `~/.config/osl/channels.json`
  (Linux/macOS) or `%APPDATA%\osl\channels.json` (Windows). No
  channel-membership introspection, no key-server-driven
  recipient discovery.
- **Fail-closed.** Any pipeline error rejects the IPC; the JS
  bootloader simulates a network failure rather than passing
  plaintext through. The Phase 3 fail-open passthrough on
  `__OSL_INTERCEPT__` rejection is gone.

Each of these gaps is documented and tracked. None is acceptable
for the public-beta version. The point of Phase 4 is to ship a
working honest E2E flow that exercises every layer (selectors →
keystore → crypto → stego → wire) under realistic dogfooding
conditions, so Phase 5+ replaces components with confidence rather
than building on un-exercised abstractions.

### 13.2 Recipient resolution

A new module `crates/keystore/src/recipients.rs` reads a JSON
config file and looks up the recipient list per Discord channel.

**Path:**

- Linux / macOS: `$XDG_CONFIG_HOME/osl/channels.json` if set,
  else `$HOME/.config/osl/channels.json`.
- Windows: `%APPDATA%\osl\channels.json` (Roaming, not Local —
  intent is the config follows the user across machines on
  roaming profiles).

**Schema:**

```json
{
  "channels": {
    "1234567890123456789": { "recipients": ["111", "222"] },
    "9876543210987654321": { "recipients": ["333"] }
  }
}
```

**API:**

```rust
pub fn get_recipients(channel_id: &str) -> Result<Vec<String>, RecipientError>;
```

`RecipientError` variants:

- `ConfigFileMissing { path }` — the file doesn't exist. We
  deliberately do NOT auto-create on first call: silent
  auto-creation would mask the "I forgot to configure recipients"
  bug as "encrypted to nobody."
- `ChannelNotConfigured { channel_id, path }` — file exists but
  this channel id isn't listed.
- `EmptyRecipients { channel_id, path }` — channel listed but
  array is empty. Distinct from `ChannelNotConfigured` so error
  messages can be precise.
- `FileReadFailed { path, source: io::Error }` — IO failure
  other than NotFound.
- `ParseFailed { path, source: serde_json::Error }` — JSON
  malformed.
- `NoConfigDir` — neither `APPDATA` nor `HOME` /
  `XDG_CONFIG_HOME` is set in the environment.

**No internal cache.** The file is reread on every send. The
file is small (one entry per dogfooded channel), and rereading
means a config edit lands without a client restart. This matters
for the alpha-prototype "I just added another test account"
workflow.

The Vencord-style alternative — parse the recipient list out of
Discord's React state for the active channel — was rejected. It
conflates "decryption client" with "DOM scraper," and a wrong
recipient set is a fail-open from a privacy standpoint
(encrypted to N-1 of N intended recipients still leaks plaintext
to Discord). A static config file is dumb, auditable, and forces
explicit per-channel opt-in.

Phase 5+ replaces `get_recipients` with a key-server-driven
channel-membership resolver. The wire shape (`Vec<String>`) is
the same so the call site in `cmd_osl_encrypt_message` doesn't
change.

### 13.3 Wire format inside the Mode 0 payload

Wire format chosen: **session-key wrap (KEM-then-DEM)**. A random
per-message session key encrypts the bulk message once; the
session key is wrapped per recipient via X25519 ECDH + HKDF +
AEAD. This minimises per-recipient overhead (73 bytes) so a
1000-byte plaintext fits up to N=5 recipients within the
1400-byte Mode 0 budget.

Layout (post-Mode-0-decode bytes; outer `DPC0::` prefix is the
Mode 0 magic and is stripped at decode):

```text
[
  version:    u8 = 0x01     // hard-coded; future formats bump this
  N:          u8            // recipient count, 1..=255
  per-recipient (N times, sorted by user_id ASCII for stable scan
                 order at the receiver):
    pub_hint: u8            // low byte of recipient's IK_X25519
                            // public key — receiver scans for the
                            // slot whose pub_hint matches their
                            // own and tries decrypt
    nonce_k:  [u8; 24]      // XChaCha20-Poly1305 nonce for the
                            // session-key wrap
    wrap_k:   [u8; 48]      // 32-byte session key + 16-byte tag
  nonce_msg:  [u8; 24]      // nonce for the bulk message AEAD
  ct_msg:     [u8; pt_len + 16]  // ciphertext + tag
]
```

Total bytes: `2 + N * 73 + 24 + plaintext_len + 16` =
`42 + N*73 + plaintext_len`.

Effective plaintext caps given the 1400-byte Mode 0 budget:

| N  | max plaintext bytes |
|----|---------------------|
| 1  | 1285 (full 1000 OK) |
| 2  | 1212                |
| 3  | 1139                |
| 5  | 993                 |
| 10 | 628                 |
| 18 | 44                  |

Combined with the soft 1000-byte plaintext cap declared at the
IPC boundary (`OSL_PHASE4_PLAINTEXT_BYTE_CAP`), the effective
cap is `min(1000, MODE0_MAX_RAW_LEN - framing)`. Smaller wins;
both apply.

#### 13.3.1 AEAD construction

XChaCha20-Poly1305 throughout (matches the rest of the project's
crypto crate; 24-byte nonces give us safe random-nonce semantics).

**Bulk message leg:**

- key: per-message random session key (32 bytes from
  `random_aead_key`)
- nonce: `random_nonce()` (24 bytes)
- ad: static `b"OSL/P4/msg/v1"`
- plaintext: UTF-8 chat-input bytes

**Per-recipient wrap leg:**

- shared = `x25519::diffie_hellman(my_x25519_secret, peer_x25519_pub)`
- wrap_key = `hkdf::derive_32(salt=b"", ikm=shared, info=b"OSL/P4/wrap-key/v1")`
- nonce: `random_nonce()` (24 bytes, fresh per recipient)
- ad: static `b"OSL/P4/wrap/v1"`
- plaintext: 32-byte session key

#### 13.3.2 Why static AD strings (no transcript binding)

Phase 4 has no receive-side decoder. Binding AD to a transcript
that nothing yet validates would be ceremonial and would lock us
into a spec we'd want to revisit when Phase 5 introduces PQXDH.
Static domain separators are sufficient for the Phase 4 goal
("don't accidentally cross-protocol-decrypt with another future
key derivation") and keep the wire format trivially explicable.

Phase 5 binds AD to the full PQXDH transcript (sender IK, peer
IK + SPK + OPK, ML-KEM ct, plus a session epoch).

### 13.4 IPC contract

Tauri command `osl_encrypt_message` keeps the Phase 3 wire
shape exactly:

```rust
async fn osl_encrypt_message(
    app: tauri::AppHandle,
    channel_id: String,
    plaintext: String,
    options: serde_json::Value,
) -> Result<String, String>;
```

Body lives in `ipc::commands::cmd_osl_encrypt_message`; the
attribute glue in `src-tauri/src/main.rs` runs it on a
`spawn_blocking` task because the underlying
`KeyServerClient::fetch_pubkeys` is synchronous (hand-rolled
HTTP/1.1 over `std::net::TcpStream`) and iterates once per
recipient over network IO.

`Result<String, String>` is intentional (versus the typed
`IpcResult` used by every other command). Per the comment block
on the Tauri command: a flat string error is the most
predictable shape across the JS-bootloader fail-closed boundary,
and the JS side immediately turns the rejection into a
network-failure simulation regardless of the message contents.

Error messages all start with `OSL: ` for log-grep ergonomics
and include the failing recipient `user_id` where applicable.

#### 13.4.1 Pure-encoder seam

`cmd_osl_encrypt_message` is split into two layers so the
cryptographic core is testable without a running keyserver:

```rust
pub fn encrypt_osl_phase4_to_pubkeys(
    sender_secret: &x25519::SecretKey,
    recipient_pubkeys: &[x25519::PublicKey],
    plaintext: &str,
) -> Result<String, String>;
```

The pure encoder takes pre-resolved pubkeys, applies all caps,
builds the wire format, and stego-encodes. No `AppState`, no
HTTP, no filesystem reads. The IO wrapper
(`cmd_osl_encrypt_message`) does:

1. `keystore::get_recipients(&channel_id)` → `Vec<user_id>`.
2. Lock `AppState` for identity + keyserver client.
3. Sort recipient `user_id`s ASCII for stable wire-slot order.
4. Per-recipient: `fetch_pubkeys` → decode IK_X25519 → typed
   `PublicKey`.
5. Hand the resolved pubkey vector to the pure encoder.

The Phase 4.5 round-trip integration test
(`crates/ipc/tests/osl_phase4_roundtrip.rs`) calls the pure
encoder directly with hand-built identity pairs, so it
exercises the full crypto pipeline without standing up the
Node keyserver. See §13.7.

### 13.5 Autostart bootstrap

`AppState` starts at `(identity: None, keyserver: None)`.
Without intervention, the very first call to
`osl_encrypt_message` from the Discord webview hits
`OSL: identity not loaded`. Bootstrap closes that gap by
populating `AppState` from on-disk config during
`tauri::Builder::setup`.

#### 13.5.1 What runs at boot

In order, all from `src-tauri/src/bootstrap.rs::run_autostart`:

1. **Resolve config dir** via `keystore::osl_config_dir()` — the
   same XDG / APPDATA fallback chain used for `channels.json`.
   Failure here (no `HOME` / `APPDATA`) skips the entire
   bootstrap with a `tracing::warn`.
2. **Read `<dir>/keyserver.json`** if present:

   ```json
   {
     "base_url": "http://<keyserver-host>:3000",
     "user_id": "alice"
   }
   ```

   Missing → `tracing::info` "no keyserver.json; populate to
   enable" and skip the keyserver leg. Malformed → `warn` and
   skip.
3. **Identity:**
   - `<dir>/identity.json` present → `keystore::load_identity`
     using `keystore::select_best_sealer()` (TPM →
     Keyring → NoOp). Failure logs a `warn` and continues
     without identity.
   - `<dir>/identity.json` missing AND `keyserver.json`
     present → `keystore::generate_identity(cfg.user_id)` then
     `save_identity`. Save failure leaves the identity in
     memory for this session but won't survive restart.
   - Both missing → skip; nothing to seed `generate_identity`
     with.
4. **Keyserver client + register** if both `keyserver.json` and
   identity are present: `KeyServerClient::new(base_url)` then
   `client.register(&identity)`. Register failure is
   non-fatal — the client stays installed so `fetch_pubkeys`
   can still work (the prototype keyserver doesn't require
   register-before-read).

Mismatch case: if `identity.json` exists with a different
`user_id` than `keyserver.json`, the loaded identity wins (it
was already registered at that user_id) and we log a
`warn`. Edit `keyserver.json` or regenerate the identity to
resolve.

#### 13.5.2 Why fail-loud-not-fatal

Every step warns and continues. Three reasons:

- **First-boot UX.** A user without `keyserver.json` should see
  the app start, log a clear actionable line, and let them
  populate the file before their first send. Refusing to start
  is hostile.
- **Partial-success states are useful.** Keyserver down at
  startup but identity loads → user can still open Discord;
  sends will fail at the IPC boundary with `OSL: fetch_pubkeys`
  error. Better than refusing to start.
- **The Tauri webview is the source of truth for working
  state.** Logs are diagnostic; the `osl_encrypt_message` IPC
  return value is what drives "did it work?" UI. Bootstrap
  just primes the cache.

#### 13.5.3 Manual setup for two-peer dogfood

For each peer (call them Alice and Bob):

```sh
mkdir -p ~/.config/osl   # or %APPDATA%\osl on Windows

# Pick the keyserver host (one peer hosts; both point at the same
# instance). Plain HTTP, port 3000 default.
cat > ~/.config/osl/keyserver.json <<'EOF'
{ "base_url": "http://<keyserver-host>:3000", "user_id": "alice" }
EOF

# Channel mapping (only after both peers have registered and
# learned each other's user_id).
cat > ~/.config/osl/channels.json <<'EOF'
{
  "channels": {
    "<discord_dm_channel_id>": { "recipients": ["bob"] }
  }
}
EOF
```

Bob does the symmetric thing with `user_id: "bob"` and
`recipients: ["alice"]`.

Keyserver host (one of the peers, or a separate machine):

```sh
cd keyserver
npm install
PORT=3000 npm start         # listens on 127.0.0.1:3000 by default
```

Then `cargo tauri dev` (or run the built binary). Autostart
generates the identity on first run, saves it, registers, and
becomes ready. Subsequent runs `load_identity`.

**Discord channel ID lookup**: in Discord webview, right-click
the DM → Copy Channel ID (developer mode required;
Settings → Advanced → Developer Mode).

### 13.6 JS bootloader: fail-closed on encryption-attempt failures

Phase 3's `interceptBody` had a single error-path callback
(`onPassthrough`) that fed back to `Reflect.apply(target,
thisArg, args)` — i.e. forward the original plaintext-bearing
request. That's correct for "no plaintext to encrypt" cases
(sticker-only / attachment-only sends) but **leaks plaintext**
on any encryption-attempt failure.

Phase 4 splits the callback set:

- `onMutated(newBody)` — encryption succeeded, send ciphertext.
- `onPassthrough()` — no plaintext to encrypt; safe forward.
- `onAbort(err)` — encryption was attempted but the pipeline
  rejected. Caller MUST simulate a network failure rather than
  forwarding plaintext.

Cases routed to `onAbort`:

1. `__OSL_INTERCEPT__` threw synchronously.
2. `__OSL_INTERCEPT__` returned a non-Promise.
3. Cover text was non-string.
4. Re-serialising the mutated body via `JSON.stringify` failed.
5. `__OSL_INTERCEPT__` rejected (this is the IPC-error path —
   the Phase 4 Rust pipeline rejecting on any failure mode).

Cases left on `onPassthrough` (Phase 4b refinement candidates):

- Body not JSON-parseable. `/messages` POST with non-JSON body
  is unusual but has historically been used by Discord for
  non-content payloads. Keeping passthrough avoids breaking
  unrelated features; tightening to abort is a Phase 4b
  refinement.
- `parsed.content` not a string (sticker-only, attachment-only).
- `parsed.content === ""` (same).

#### 13.5.1 Fetch abort: rejected Promise

Cleanest path. `Promise.reject(new TypeError("Failed to
fetch"))` produces the same exception type the Fetch API uses
for genuine network failures; Discord's `fetch` callers treat it
as a network error and the message UI shows "Failed to send."

#### 13.5.2 XHR abort: synthesised event sequence

XHR has no Promise to reject — the `send()` apply trap returned
`undefined` synchronously and there's no handle for a deferred
result. Phase 4 fail-closed approach:

1. Do **not** call `origSend`. (The plaintext-bearing request
   never leaves the box.)
2. Schedule a microtask that dispatches `error` and `loadend`
   `ProgressEvent`s on the XHR instance:

   ```js
   setTimeout(function () {
       xhrInst.dispatchEvent(new ProgressEvent("error"));
       xhrInst.dispatchEvent(new ProgressEvent("loadend"));
   }, 0);
   ```

Discord's `onerror` handler fires; the message UI shows "Failed
to send."

**Caveat:** `xhr.readyState` and `xhr.status` stay at their
pre-send values (1 / 0). Most callers gate on the error event
firing rather than reading those, but a Phase 4b refinement
could synthesize a more complete failed state by manipulating
the readyState/status getters or by returning a fully-faked
mock XHR. Acceptable for Phase 4 because the privacy property
(no plaintext over the wire) holds regardless of UI-level
oddities.

### 13.7 Acceptance criteria

Phase 4 is done when:

1. **Phase 4.5 round-trip tests pass** (see §13.8). This is the
   internal cryptographic correctness gate. Without Phase 5's
   receive-side hook, this is the only direct evidence the wire
   format is recoverable; we ship Phase 4 only after the
   round-trip is green.

2. Two devs each running the client, each with their identity
   registered to the prototype key server, can:
   a. Follow §13.5.3 setup (`keyserver.json` + `channels.json`
      on each peer; one peer or a separate machine running
      `npm start` in `keyserver/`).
   b. Boot the client; autostart logs `OSL bootstrap: identity
      loaded` (or `generated fresh identity` + `identity
      saved` on first run) and `OSL bootstrap: registered with
      key-server`.
   c. Type a message in the shared DM channel from device A;
      observe the channel show a `DPC0::<base64>` cover
      string sent by device A. (No receive-side decoder yet;
      "the cover text appears" is the visible success signal.)
   d. Repeat from device B; A's channel shows a fresh `DPC0::`
      cover string.

3. Pipeline failure modes all fail closed (verify each by
   running into the failure deliberately):
   a. Unconfigured channel → `OSL: recipient lookup: channel
      ... not configured` returned, JS rejects fetch, UI shows
      "Failed to send." No `/messages` POST hits the wire.
   b. Identity not loaded → `OSL: identity not loaded`,
      same outcome. (Reproduce: temporarily delete
      `identity.json` AND `keyserver.json` so autostart
      skips identity bootstrap.)
   c. Key-server unreachable → `OSL: fetch_pubkeys(...)`,
      same outcome. (Reproduce: stop the keyserver between
      autostart and first send; the cached client will fail
      to connect.)
   d. Plaintext > 1000 bytes → `OSL: plaintext is N bytes,
      exceeds soft cap of 1000`, same outcome.
   e. Plaintext + framing > 1400 bytes for the configured
      recipient count → `OSL: payload N bytes exceeds Mode 0
      cap 1400 ...`, same outcome.

4. Cross-compile to `x86_64-pc-windows-gnu` stays green for
   `cargo check -p discord-privacy-client` and the keystore /
   ipc / crypto / stego crates.

5. Phase 3 acceptance still passes: `[OSL]` boot log appears,
   detection-mitigation `[OSL DIAG]` probes still pass.

6. The 1000-char plaintext cap is enforced at the IPC boundary,
   not at the JS hook (defence in depth: even if the JS
   passes a 5000-char plaintext, Rust rejects it).

### 13.8 Phase 4.5: round-trip verification

Phase 5 lands the receive-side hook (Discord-message-render
DOM mutation that detects `DPC0::` cover strings, decodes
them, and renders the recovered plaintext in place of the
cover). Until then, the only proof that the encrypt half is
correct is a Rust-side round-trip test — generate two
identities, encrypt with one, decrypt with the other, assert
plaintext equality.

This is **Phase 4.5**, not Phase 5: Phase 5 is the JS-side
receive-and-render integration; Phase 4.5 is just the
Rust-side cryptographic round-trip that proves the wire
format is internally consistent and recoverable.

#### 13.8.1 What the test exercises

Located at `crates/ipc/tests/osl_phase4_roundtrip.rs`:

- `encrypt_osl_phase4_to_pubkeys` (the pure encoder; no
  `AppState`, no HTTP).
- The full wire format: version byte, recipient count, slot
  layout, bulk message AEAD.
- X25519 ECDH, HKDF-SHA256 wrap-key derivation,
  XChaCha20-Poly1305 AEAD on both legs.
- Mode 0 stego encode/decode.
- Recipient slot scan via `pub_hint` low-byte match.

Test cases:

- 1:1 round-trip (basic encrypt → decrypt → plaintext match).
- Multi-recipient: each of three recipients independently
  decodes the same cover string.
- Stranger-not-recipient: a fourth identity given the cover
  string + sender's pubkey CANNOT decode (negative test —
  proves the encryption is actually doing something).
- Empty plaintext rejected, oversized plaintext rejected,
  zero recipients rejected, over-budget recipient count
  rejected.
- Wire-format self-consistency: cover length matches predicted
  framing, header byte values match constants, slot 0
  `pub_hint` matches recipient pubkey low byte.

#### 13.8.2 What the test deliberately does NOT exercise

- `cmd_osl_encrypt_message` (the IO wrapper). That path's
  keyserver / channel-config IO is covered by the keystore
  unit tests + the Phase 4 §13.7 acceptance procedure;
  round-tripping it would require booting the Node keyserver
  and tempfile-rooting `keystore::osl_config_dir`, which is
  outside the scope of "did the cryptography round-trip?"
- Phase 5 detection of "this Discord message is for me." The
  test feeds the cover string directly into the decoder; the
  real receive path will pre-filter on the `DPC0::` prefix.

#### 13.8.3 The decoder is Phase 5's seed

The test-side `decode_phase4_wire` helper mirrors the
encoder's slot layout exactly. When Phase 5 lands, this
function moves into the production receive path almost
verbatim — the differences will be:

- Production reads sender's pubkey from a keyserver lookup
  keyed on the Discord message author's user_id (the test
  hands it in directly).
- Production attaches to a Discord-side message-render DOM
  hook (Phase 5's domain), not a unit test.
- Production handles "no slot matches" by leaving the cover
  string visible (Phase 5 also pre-filters on `DPC0::`
  prefix; non-OSL messages never hit the decoder).

Keeping the decoder in the test for Phase 4 means the wire
format has a working reference implementation, encoder
breakage is caught immediately, and the Phase 5 extraction
is mechanical.

### 13.9 Deferred to Phase 4b / 5+

- **Multi-message chunking.** Phase 4 hard-rejects messages
  whose ciphertext won't fit in a single Mode 0 payload. Phase
  4b would split across multiple stego'd messages with random
  inter-message delays (5–15 s) to defeat trivial timing
  correlation.
- **Mode 1 stego (natural-looking text).** Mode 0's `DPC0::`
  prefix is trivially scannable. Mode 1 produces template-based
  cover text indistinguishable from organic chat at a glance.
  Cap is much smaller (~80 bytes) so adoption requires the
  multi-message chunker.
- **Receive-side decoder.** `osl_decrypt_message` Tauri command
  + receive-time message-render DOM hook. Decodes Mode 0,
  scans wrap slots for our pub_hint, attempts decryption,
  substitutes the rendered message with the recovered
  plaintext.
- **Edits and deletes.** EDIT_RE is currently in
  passthrough-with-log mode in `boot.js`. Phase 5 brings
  message-edit-aware encryption.
- **Attachments.** Currently passthrough. Phase 5 brings
  attachment encryption (separate AEAD pipeline; cover-content
  is a stego'd link to a separately-encrypted blob).
- **PQXDH handshake + Double Ratchet.** Phase 5 — replaces
  the static X25519 ECDH leg with the full PQXDH handshake
  and per-message ratchet step. Wire format bumps to
  `version = 0x02`.
- **Tightening JS-side parse-failure path to abort.** Phase
  4b refinement: route "body not JSON-parseable" cases through
  `onAbort` rather than `onPassthrough` so a future Discord
  schema change can't reintroduce a leak via that path.

## 14. Phase 5: receive-side hook (decrypt + render plaintext)

Phase 4 + 4.5 ship the encrypt half plus a Rust-side round-trip
proof. Phase 5 ships the receive half: when Discord renders an
incoming message whose `content` carries the `DPC0::` prefix,
attempt decryption; on success replace the rendered text with
plaintext. **Goal:** when liam sends "hello" to henry, henry's
client shows "hello" in the channel — not the cover string.

### 14.1 Architectural choices (with rationale)

#### 14.1.1 Sender pubkey resolution: Discord-user-id == OSL-user-id

The Phase 4 wire format does NOT embed the sender's public key.
The decoder needs `(recipient_secret, sender_pub)` for ECDH;
without sender_pub baked into the wire, the recipient must
look it up.

**v1 closed-beta choice:** the OSL `user_id` registered with the
keyserver IS the sender's Discord user_id. Discord provides the
message author via `message.author.id` on every dispatcher
event; we hand that straight to `KeyServerClient::fetch_pubkeys`.
No client-side mapping table.

**Privacy trade-off:** the keyserver now sees a graph of "Discord
user_ids registered to OSL." For closed-beta this is fine — the
two-to-three peers already know each other's Discord IDs. For
v2 this changes: OSL identities should be client-generated UUIDs,
each peer keeps a local Discord-ID → OSL-UUID mapping, and the
keyserver only sees opaque OSL UUIDs. Tracked in
`docs/design/key-server-api.md`.

#### 14.1.2 Auto-include sender as recipient

The Phase 4 encoder is updated so the sender's own pubkey is
always added as an extra slot (deduped against the explicit
recipient list). Three reasons:

- **Optimistic-render UX**: when sender hits Enter, the server
  bounces the encrypted message back as a `MESSAGE_CREATE`. The
  receive hook decrypts using sender's own secret + sender's own
  pub (which IS in the wire as the auto-slot). Sender sees
  plaintext for their own message immediately.
- **Search consistency**: Discord Cmd-F searches the rendered
  text. Without a sender slot, sender can't search their own
  past messages.
- **Multi-device readiness**: future-proofs the wire for the
  case where one user has multiple devices, each with its own
  identity key — auto-encrypting to self is the seed of that.

Cost: 73 extra bytes per message (one slot). Effective Mode 0
plaintext budget drops from 1285 → 1212 bytes for N=1
encryptions.

#### Wire-size cost (informs v2 size optimization)

Phase 5's auto-include-sender pushes 1:1 messaging from N=1
to N=2 slots. Concretely:

- Phase 4 1:1 wire: 1 explicit recipient → N=1 → framing
  = 42 + 73 = **115 bytes** + plaintext + 16-byte msg AEAD tag.
- Phase 5 1:1 wire: 1 explicit recipient + auto-sender → N=2
  → framing = 42 + 146 = **188 bytes** + plaintext + 16-byte
  msg AEAD tag.

Per-message overhead nearly doubles for the most common case
(1:1 DMs). Still small relative to plaintext (a 200-byte
plaintext + 188-byte framing = 388 wire bytes, ~518 base64
chars including `DPC0::` prefix — well under Discord's
2000-char message cap). But worth flagging:

- Mode 0 effective plaintext budget for 1:1 DMs is now 1212
  bytes (was 1285).
- A future per-recipient slot compression scheme could
  collapse the auto-sender slot when sender is also one of
  the recipients (treat as a flag bit in the version byte
  rather than a full slot).
- v2's PQXDH ratchet design will probably encode sender
  state into a per-session header instead of per-message
  slots, sidestepping this cost entirely.

For closed-beta dogfood scale, 73 extra bytes per send is
unmeasurable. The trade-off vs. the optimistic-render UX
gain (sender sees plaintext for own messages) is heavily in
favour of keeping the auto-slot. Just informing v2.

#### 14.1.3 Async decrypt + sync render

Discord renders synchronously from `MessageStore`; the IPC
roundtrip to Rust + sender-pubkey lookup is async. We can't
`await` inline in a Flux subscriber. Strategy:

1. Subscribe to `MESSAGE_CREATE`. Synchronously kick off the
   decrypt Promise.
2. While Promise is pending: `MessageStore` commits the original
   `DPC0::` content; first paint shows the cover for ~10–100 ms
   (IPC + decrypt latency).
3. On Promise resolution: dispatch a synthetic `MESSAGE_UPDATE`
   with `{message: {id, channel_id, content: plaintext, __osl_decrypted: true}}`.
   `MessageStore` reducer updates the content; React re-renders
   with plaintext.
4. The synthetic dispatch's `__osl_decrypted: true` marker
   prevents our subscriber from re-entering the decrypt path.

The brief flash of `DPC0::` → plaintext is a deliberate UX cost
of async decryption. Phase 5 dev-milestone treats it as a
debugging signal: persistent `DPC0::` with no flip = config
error visible to the user (recipient pubkey not registered, key
rotation that hasn't refetched, etc.).

A synchronous-cache pre-paint approach (intercept React render,
consult a content cache) was considered and rejected as too
invasive. v2 may revisit if the flash becomes a UX problem.

#### 14.1.4 Receive-path interception layer: DOM (v1)

Three approaches were considered before locking the v1 design.
All three were prototyped end-to-end in `boot.js` (preserved
in git history; see commits leading to `v0.0.5-phaseb-keyserver-deployed`):

1. **FluxDispatcher store discovery** via
   `webpackChunkdiscord_app.push` — scan loaded modules for
   `dispatch + subscribe + _subscriptions` shape, attach a
   subscriber, mutate `MESSAGE_CREATE` payloads in flight.
   Outcome over three rounds (structural sentinel, behavioral
   detection, i18n exclusion + Flux-keyword filter): zero
   modules with the dispatcher shape were reachable through
   the chunk hook. All 22 `dispatch+subscribe` candidates
   were i18n adapters; FluxDispatcher itself initialises
   inside an unreachable code path.
2. **Gateway WebSocket intercept** via `WebSocket` constructor
   Proxy + `DecompressionStream('deflate')` for `zlib-stream`.
   Read-side worked: PROBE messages were captured and
   decompressed cleanly. Mutation didn't propagate: synthetic
   re-emit landed *after* Discord's reducer had already
   processed the original frame, so the UI rendered the cover
   string regardless. Per-frame ordering inside Discord's
   message queue isn't observable from the Proxy boundary.
3. **DOM MutationObserver on `document.body`** — observe
   `childList + subtree + characterData`, match elements
   whose first text child starts with `DPC0::`, request
   decryption from Rust, replace `textContent` on success.

**Locked v1 choice: option 3.** The DOM is the one
Discord-facing API that's observable, public, and stable
enough to bet on. Internal-hook approaches couple the mod
tightly to Discord's reducer ordering and obfuscated module
IDs; a single bundle refactor breaks them silently with no
diagnostic signal. The DOM trade-off is fragility against
*UI* refactors (manageable as ongoing maintenance) and a brief
flash of the cover string before async decrypt completes
(documented limitation; v2 sidesteps via overlay window).

#### 14.1.5 Sender pubkey cache

`AppState` gains a `SenderPubkeyCache: Mutex<HashMap<String,
CachedPubkey>>` with a 30-minute TTL (see
`SENDER_PUBKEY_CACHE_TTL` in `crates/ipc/src/state.rs`). First
message from a sender per 30-minute window pays a keyserver
round-trip; subsequent messages are local. Eviction is lazy
(on read).

Bounded staleness: when a peer rotates their identity key, our
cache holds the prior key for at most 30 minutes; after expiry
we refetch and pick up the new key. Identity-key rotation is
rare in practice (tied to duress reinstall or major-incident
response, NOT per-conversation lifecycle), so a half-hour
window keeps an active dogfood session at O(N) keyserver
requests rather than O(M). Long-term answer is keyserver-pushed
invalidation over a websocket (v2).

#### 14.1.6 Constant-time-ish slot iteration in decoder

The decoder iterates **all** wire slots (no early break), even
after a successful unwrap. Two slots can share a `pub_hint`
byte (1/256 collision probability per slot pair); breaking
early would let a timing-aware observer narrow down which slot
is ours. Cost: one extra AEAD attempt per legitimate hint
collision; usually zero in practice.

We do still skip slots whose `pub_hint` doesn't match ours —
the hint is public information embedded by the sender, so
iterating non-matching slots is wasted work, not a leak.

The "are we a recipient at all?" distinguisher is NOT made
constant-time. That state is externally observable via whether
we replace the rendered text afterwards; making the in-process
timing constant doesn't close that channel.

### 14.2 Rust-side architecture

#### 14.2.1 Pure decoder

```rust
pub fn decrypt_osl_phase4_from_wire(
    recipient_secret: &x25519::SecretKey,
    sender_pub: &x25519::PublicKey,
    wire: &[u8],
) -> Result<Vec<u8>, DecodeError>;

pub fn decrypt_osl_phase4_cover(
    recipient_secret: &x25519::SecretKey,
    sender_pub: &x25519::PublicKey,
    cover: &str,                      // "DPC0::<base64>"
) -> Result<Vec<u8>, DecodeError>;
```

`DecodeError` variants: `BadPrefix`, `Base64`, `TooShort`,
`UnsupportedVersion`, `ZeroRecipients`, `NoMatchingSlot`,
`MessageAeadFailed`, `Crypto`. `BadPrefix` and
`NoMatchingSlot` are common-path "leave cover alone" signals;
the others indicate corruption or a Discord-side schema change.

#### 14.2.2 IPC command

```rust
pub fn cmd_osl_decrypt_message(
    state: &AppState,
    _channel_id: String,
    sender_user_id: String,
    content: String,
) -> Result<String, String>;
```

The wrapper handles:
1. Identity lookup from `AppState`.
2. Sender pubkey lookup: cache-first, keyserver-fallback,
   cache-insert on miss.
3. Pure-decoder call.
4. UTF-8 conversion of the recovered bytes.

`channel_id` is unused on the receive side (any message we can
decrypt belongs to us regardless of channel); kept in the
signature for symmetry with encrypt and for future per-channel
ratchet state.

#### 14.2.3 Tauri command + capability

`osl_decrypt_message` Tauri command in `src-tauri/src/main.rs`
mirrors the encrypt-side shape: `spawn_blocking`,
`AppHandle::state::<AppState>()`. Permission file at
`src-tauri/permissions/osl-decrypt-message.toml` declares
`allow-osl-decrypt-message`; capability `main-capability` adds
it to the discord.com remote-URL grant list.

### 14.3 JS-side architecture (boot.js extension)

The receive hook lives in the same IIFE as the Phase 3 send-
side hooks in `src-tauri/src/injection/boot.js`. It runs after
`DOMContentLoaded` (or immediately if the document is already
past `loading`) and installs a single `MutationObserver` on
`document.body`.

**Observer config**:
```
{ childList: true, subtree: true, characterData: true }
```

**Element selection.** A target is an `Element` whose first
child is a `Text` node whose `nodeValue` starts with `DPC0::`.
This is exact-match on the prefix — no scan deeper into mixed
content, no regex over inner HTML. Discord renders message
content into a leaf-ish span where the text node is the first
child; that's our anchor.

**Disposition state**:

- `recvDone: WeakSet<Element>` — the element has been settled
  (decrypt succeeded OR a permanent reject like `BadPrefix` /
  `NoMatchingSlot` came back). No further requests.
- `recvRetries: WeakMap<Element, number>` — bounded retry
  counter (cap 3) to absorb React re-render churn without
  pathological IPC volume.

**Per-mutation handling**:

- *childList.addedNodes*: walk subtree, call `recvTryDecrypt`
  on each match.
- *characterData* on a text node whose `nodeValue` is back to
  starting with `DPC0::`: this is React clobbering our
  in-place plaintext replacement during a re-render. Clear
  the parent element's `recvDone` mark and call
  `recvTryDecrypt` again (the retry counter still bounds total
  attempts at 3 so this can't loop).

**Author / channel extraction.** Both are best-effort heuristics
on the rendered DOM:

- `recvExtractChannelId()`: regex `/\/channels\/[^/]+\/(\d{15,22})/`
  on `window.location.pathname`. Null if the user isn't on a
  channel route.
- `recvExtractAuthorId(el)`: walk up to 12 parents looking for
  `data-list-item-id` starting with `chat-messages___`; within
  that subtree try `data-author-id` first, then scan
  `<img src="…/avatars/<snowflake>/…">` to recover the
  snowflake from the avatar URL.

When either lookup returns null, `recvTryDecrypt` is a no-op —
safe default, no crash, no spurious request.

**IPC shape**:
```
invoke("osl_decrypt_message", {
  channelId, senderUserId, content
}).then(plaintext => { el.textContent = plaintext })
  .catch(_   => { recvDone.add(el); })
```

The success branch double-checks `el.isConnected` and that the
element's text *still* starts with `DPC0::` before mutating —
React may have re-rendered the element between request and
response. The catch branch logs at `DEBUG` (most rejections
are expected: messages we're not a recipient of, plain text
that happens to look like a cover, etc.).

**Initial sweep.** Right after attaching, walk
`document.body.querySelectorAll('*')` and call the same match
predicate on each — catches messages Discord rendered before
the observer attached (channel switches, scrollback loads).

**Detection posture.** The observer *itself* is detectable
from outside (any DOM scan that sees plaintext where wire-level
content was `DPC0::` knows something rewrote it). v1 doesn't
try to hide that — anti-detection in v1 covers the *send-side*
fetch/XHR hooks (Phase 3 round 6); receive-side rewriting is
inherently visible. v2's overlay-window architecture moves
both halves out of the discord.com webview entirely.

### 14.4 Acceptance criteria

1. **Phase 5 round-trip tests pass** (`crates/ipc/tests/osl_phase5_decrypt.rs`):
   each `DecodeError` variant produced under expected trigger;
   sender self-decrypts under auto-include-sender; cache
   semantics correct.
2. **Phase 4.5 round-trip still passes** with the auto-included
   sender slot accounted for (N = explicit_recipients + 1).
3. **Two-peer dogfood**: liam sends "hello" in a channel where
   henry is configured as a recipient. Within 100ms, henry's
   channel rendering changes from `DPC0::<base64>` to "hello".
   liam's own channel rendering similarly changes from `DPC0::`
   to "hello" (sender self-decrypt).
4. **Non-recipient sees cover gracefully**: a third user in the
   channel who is NOT a recipient sees the `DPC0::` cover
   without errors and without app crashes.
5. **All Phase 3 / Phase 4 / Phase B acceptance signals still
   pass**: send-side hook works, detection mitigations active,
   keyserver auth + allowlist + rate limit unaffected,
   Railway-hosted instance reachable via HTTPS.
6. **Cross-compile to `x86_64-pc-windows-gnu` stays green**.

### 14.5 Recon report — concluded

Before locking the v1 receive-side strategy, three rounds of
live-runtime recon ran in `boot.js` behind a `RECON` flag.
This section captures the conclusions; the gory probe details
live in git history (commits leading up to the
`v0.0.6-phase5-dom-receive` tag) — pulled out of the design
doc to keep it focused on the shipped architecture.

**Recon outcome (decisions feeding §14.1.4):**

1. **FluxDispatcher discovery via `webpackChunkdiscord_app.push`
   was unreachable.** Three filter generations (structural
   sentinel, behavioral detection, i18n exclusion + Flux
   keywords + dispatch-source-length filter) all returned the
   same result: 22 candidates with `dispatch + subscribe`
   shape, all i18n adapters, zero matched the FluxDispatcher
   sentinel. Discord's bundle initialises FluxDispatcher inside
   a code path that never resolves through the chunk hook.
2. **Gateway WebSocket capture works for read; doesn't work
   for mutate.** The constructor-Proxy + `DecompressionStream
   ('deflate')` path captures and decompresses zlib-stream
   frames cleanly (`PROBE FIRE` confirmed end-to-end). But
   synthetic re-emit lands *after* Discord's reducer has
   already consumed the original frame, and the duplicate is
   deduped or ignored. Per-frame ordering inside Discord's
   message queue isn't observable from outside, so this path
   is structurally incompatible with Discord's reducer
   ordering — not a code-level fix.
3. **DOM MutationObserver works.** The DOM is a public,
   observable, reasonably stable Discord-facing surface.
   Trade-offs are scoped and documented (see §14.6).

**What's preserved in git history (not in this doc):**

- Per-round detection logs, candidate-shape statistics,
  source-length distributions, the i18n-adapter signature
  family that polluted v1+v2 results, the call-twice
  diagnosis for why mutation didn't propagate, and the
  detailed Q1–Q7 probe scaffolding.

**What lives in `boot.js` instead:** the implementation
(§14.3) — no recon, no `RECON` flag, no `OSLPROBE` gate.
Stripping the recon block was a precondition for the v1 ship.

### 14.6 v1 limitations (accepted) and v2 deferred work

**Accepted v1 limitations** (documented behaviour, not bugs):

- **Brief flash of cover string** before async decrypt
  resolves. Typical observed window: tens to a few hundred
  milliseconds. The user sees `DPC0::<base64>…` then the
  plaintext replaces it in place. Acceptable for a closed-
  beta dogfood; eliminated entirely in v2.
- **DOM-mutation fragility.** A major Discord refactor of the
  message-renderer DOM shape (the `chat-messages___…` list-
  item structure or the avatar URL pattern that
  `recvExtractAuthorId` falls back to) can break the observer.
  Treated as ongoing maintenance — a regression surfaces as
  cover strings staying visible, which is a loud failure mode.
- **Best-effort author_id extraction.** When neither
  `data-author-id` nor a parseable avatar URL is in the
  rendered DOM (rare — primarily for system messages or
  peculiar non-user authors), `recvTryDecrypt` is a no-op.
  The cover stays visible. Safer than guessing.
- **Sender's own messages flash too.** The encoder auto-
  includes the sender as a recipient slot (§14.1.2) so the
  sender CAN decrypt their own bounced message, but the cover
  still renders before the observer fires. Optimistic-render
  fix is a separate UX layer (deferred).
- **Receive-side rewriting is detectable from outside.** A
  page-level scan that compares wire-level message content
  with rendered DOM text knows something rewrote it. v1
  doesn't hide this; anti-detection covers the send path
  (Phase 3 round 6) only.

**Deferred to later phases:**

- **Edits and deletes** (Phase 6).
- **Attachment encryption** (Phase 6).
- **Optimistic-render fix beyond what falls out for free** —
  the auto-include-sender slot fixes the primary case
  (sender → server → bounce → decrypt). Pre-bounce instant
  rendering (showing plaintext in the local channel before
  the server confirms) is a separate UX layer.
- **UI polish** (separate phase).
- **PQXDH handshake + Double Ratchet** (Phase 7+).
- **Multi-device support** (Phase 7+; requires session
  coordination across keys).
- **v2 overlay-window architecture** — separate Tauri window
  with its own message store; receive-decrypt happens at the
  gateway WebSocket inside our own runtime, not Discord's.
  Eliminates DOM coupling, eliminates the cover flash,
  eliminates receive-side detection surface, and removes the
  "Discord-user-id == OSL-user-id" privacy linkage by giving
  each peer a client-generated OSL UUID.
