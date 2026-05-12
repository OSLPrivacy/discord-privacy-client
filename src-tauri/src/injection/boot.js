/* OSL boot script â€” Layer 10 / Phase 3 round 6.
 *
 * Runs as the Tauri WebView's initialization_script BEFORE Discord's
 * bundle initialises. Wraps both `window.fetch` and
 * `XMLHttpRequest.prototype.open/send`; for POSTs to
 * `/api/v<N>/channels/<channel_id>/messages` (the message-send
 * endpoint) we route the JSON body's `content` field through our
 * Tauri command `osl_encrypt_message`, replace it with the returned
 * cover text, and forward.
 *
 * Phase 3 round 6: `osl_encrypt_message` is a stub returning
 * `"[OSL-STUB] " + plaintext`. Phase 4 wires the real crypto path.
 *
 * ## Round 6: anti-detection mitigations
 *
 * Three layers stacked to defeat the most common ways a remote
 * client-side scan would detect a hooked client:
 *
 *   1. **Proxy-based wrappers** instead of direct function override.
 *      `window.fetch = new Proxy(origFetch, { apply, get })` reads
 *      to descriptor checks as a function exotic with the native
 *      target, rather than a JS-defined function whose source is
 *      visible. Property descriptor introspection (e.g. via
 *      `Object.getOwnPropertyDescriptor`) returns the same shape it
 *      would for the unwrapped native.
 *
 *   2. **toString spoofing** at two levels:
 *      a. The proxy's `get` trap intercepts `'toString'` access and
 *         returns a spoof function that produces
 *         `'function fetch() { [native code] }'`. This handles
 *         `window.fetch.toString()`.
 *      b. `Function.prototype.toString` itself is wrapped in a
 *         Proxy whose apply trap consults a `WeakMap<hookedFn,
 *         spoofString>`. This handles
 *         `Function.prototype.toString.call(window.fetch)`, which
 *         bypasses instance-level toString overrides â€” without this
 *         layer, naive detection would still see the wrapper source.
 *
 *   3. **Compile-time DEBUG strip** (the `DEBUG` const at the top
 *      of the IIFE). All `[OSL]`-prefixed `console.log` /
 *      `console.warn` calls are gated by `if (DEBUG)`. With
 *      `DEBUG = false` in release builds, V8/SpiderMonkey dead-code
 *      eliminate the gated blocks during optimisation â€” the
 *      `[OSL]` literals don't even appear in the JIT'd code, so a
 *      console-output scan finds nothing. `console.error` calls are
 *      NOT gated: real failures (IPC missing, encryption rejected)
 *      are signal we want to surface even in production.
 *
 * What this defends against:
 *   - `window.fetch.toString()` / `XMLHttpRequest.prototype.open.toString()`
 *     scanning for non-native source strings.
 *   - `Function.prototype.toString.call(...)` scanning the same.
 *   - `if (window.fetch.toString().includes('[native code]'))` checks.
 *   - Console-output regex scrapers looking for telltale prefixes.
 *
 * What this does NOT defend against (intentional v1 non-goals;
 * v2 overlay architecture sidesteps detection entirely):
 *   - `Reflect.getPrototypeOf` / Proxy introspection â€” Proxies are
 *     detectable by sufficiently-determined adversaries (e.g.
 *     measuring the wall-clock cost of a Proxy hop, comparing
 *     descriptor shapes deeply, observing Reflect handler
 *     side-effects).
 *   - Iframe-based fetch acquisition: an attacker can create an
 *     `<iframe>`, grab its untainted `contentWindow.fetch`, and
 *     bypass our hook entirely. v2 would need to also hook
 *     iframe creation to close this.
 *   - `Object.prototype.toString.call(proxy)` returns
 *     `"[object Function]"` for our function-Proxy (correct), so
 *     this isn't a detection vector â€” but it's worth noting the
 *     classification is preserved, not spoofed.
 *
 * Sophisticated detection is an arms race that v1's modify-Discord-
 * in-place architecture inherently loses. v2 will use a separate
 * overlay window that doesn't touch Discord's runtime at all,
 * making detection moot.
 */

(function () {
    "use strict";

    // ============================================================
    // IIFE-level idempotency guard.
    //
    // The boot script has been observed running in two contexts
    // for the same Discord page (e.g. once injected by Tauri's
    // `initialization_script`, once re-evaluated through
    // Discord's Sentry instrumentation that re-emits scripts as
    // it captures them). Each evaluation builds its own scope
    // with its own `recvPlaintext` Map, its own observer, and
    // its own `setInterval` registration. The visible symptoms:
    //
    //   - Every `[OSL]` log line appears twice (once with VM16:
    //     source, once with `sentry.<hash>.js:15` source).
    //   - Two periodic sweep ticks per second; one always shows
    //     `cached=0` because that scope's decrypt success path
    //     populates a different Map than the one its sweep
    //     queries on the next tick (the closures aren't shared).
    //   - decrypt RPCs fire 2Ã— per message, and over a long
    //     session the dispatched count grows unboundedly.
    //
    // The guard is keyed on a single `window` flag â€” both
    // contexts share `window` because they evaluate in the same
    // frame â€” so the second evaluation short-circuits before
    // installing anything.
    // ============================================================
    if (window.__OSL_BOOT_INSTALLED__) {
        return;
    }
    window.__OSL_BOOT_INSTALLED__ = true;

    // ============================================================
    // Phase 7d-B2 Stealth gate.
    //
    // The boot-gate page navigates to `discord.com/app#osl-stealth`
    // after a successful stealth-password match. Boot.js sees the
    // hash synchronously here (before fetch/XHR hooks install) and
    // bails — vanilla Discord runs with zero OSL features for this
    // session.
    //
    // We strip the hash via `history.replaceState` so Discord's
    // own SPA routing doesn't see it on subsequent route changes
    // (Discord uses path-based routing, not hash-based, but the
    // hash would show in the URL bar of in-page links / share
    // dialogs — keeping it private).
    // ============================================================
    try {
        if (typeof window !== "undefined" && window.location.hash === "#osl-stealth") {
            try {
                history.replaceState(
                    null,
                    "",
                    window.location.pathname + window.location.search
                );
            } catch (_) {}
            console.log("[OSL] Stealth session — feature install skipped");
            return;
        }
    } catch (_) {}

    // ============================================================
    // 7d-FIX2: CSP allowance for Tauri IPC origins.
    //
    // Discord ships a strict Content-Security-Policy that omits
    // Tauri's IPC fetch endpoint (ipc.localhost / tauri.localhost)
    // from `connect-src`. `window.__TAURI__.event.listen` registers
    // listeners via that endpoint internally, so without this patch
    // the very first call rejects with a CSP violation and (in some
    // Tauri versions) throws synchronously, halting downstream init.
    //
    // We try the immediate meta-tag patch here. If Discord's CSP
    // meta hasn't been written yet, install a one-shot
    // MutationObserver on `document.head` to patch it the moment
    // it arrives. If Discord uses HTTP-header CSP instead (no meta
    // tag), neither path works — log and rely on the defensive
    // try/catch wrapping inside `oslInstallCrossWindowListeners`.
    //
    // Over-allow rather than under-allow: we add both http+https
    // variants of `ipc.localhost` AND `tauri.localhost` because the
    // exact scheme/host depends on platform and Tauri version.
    // ============================================================
    function oslPatchDiscordCsp() {
        const ORIGINS =
            "http://ipc.localhost https://ipc.localhost " +
            "http://tauri.localhost https://tauri.localhost";

        function patch(meta) {
            try {
                let csp = meta.getAttribute("content") || "";
                if (csp.indexOf("ipc.localhost") !== -1) {
                    // Already patched (idempotency, e.g. on Discord
                    // re-render). Nothing to do.
                    return true;
                }
                if (/connect-src\s/i.test(csp)) {
                    csp = csp.replace(
                        /connect-src ([^;]+)/i,
                        "connect-src $1 " + ORIGINS
                    );
                } else {
                    // No connect-src directive present — append one.
                    // CSP merges by intersection if there's no
                    // existing connect-src, so the default falls
                    // back to default-src; we want explicit allow.
                    csp = csp.replace(/;?\s*$/, "") + "; connect-src 'self' " + ORIGINS;
                }
                meta.setAttribute("content", csp);
                console.log("[OSL] CSP modified (connect-src extended)");
                return true;
            } catch (e) {
                console.warn(
                    "[OSL] CSP patch failed:",
                    (e && e.message) || e
                );
                return false;
            }
        }

        const existing = document.querySelector(
            'meta[http-equiv="Content-Security-Policy"]'
        );
        if (existing) {
            patch(existing);
            return;
        }

        // Meta tag not present yet — Discord may write it after the
        // boot script's first synchronous tick. Late-patch via a
        // one-shot MutationObserver on document.head.
        try {
            const headObs = new MutationObserver(function (mutations) {
                for (const m of mutations) {
                    for (const node of m.addedNodes) {
                        if (
                            node &&
                            node.tagName === "META" &&
                            (node.getAttribute("http-equiv") || "").toLowerCase() ===
                                "content-security-policy"
                        ) {
                            if (patch(node)) {
                                headObs.disconnect();
                                return;
                            }
                        }
                    }
                }
            });
            if (document.head) {
                headObs.observe(document.head, {
                    childList: true,
                    subtree: false,
                });
            } else {
                console.warn(
                    "[OSL] CSP patch: document.head not yet available — relying on defensive wrapping"
                );
            }
            // Self-disconnect after 10s to avoid leaking an observer
            // when Discord's CSP is HTTP-header-only and no meta tag
            // ever arrives.
            setTimeout(function () {
                try {
                    headObs.disconnect();
                } catch (_) {}
                if (
                    !document.querySelector(
                        'meta[http-equiv="Content-Security-Policy"]'
                    )
                ) {
                    console.warn(
                        "[OSL] CSP meta tag not found after 10s; " +
                            "cross-window events may fail if Discord's " +
                            "CSP is HTTP-header-only — defensive wrapping " +
                            "in oslInstallCrossWindowListeners is the fallback"
                    );
                }
            }, 10000);
        } catch (e) {
            console.warn(
                "[OSL] CSP modification skipped (observer setup threw):",
                (e && e.message) || e
            );
        }
    }
    try {
        oslPatchDiscordCsp();
    } catch (e) {
        console.warn(
            "[OSL] CSP patch entry threw:",
            (e && e.message) || e
        );
    }

    // ============================================================
    // Compile-time DEBUG switch.
    //
    // PHASE 3 VERIFICATION: leave at `true` so console output stays
    // visible for debugging. **FLIP TO `false` BEFORE RELEASE BUILDS.**
    //
    // When `false`: V8/SpiderMonkey dead-code-eliminate every
    // `if (DEBUG) { ... }` block during JIT optimisation, leaving no
    // `[OSL]` literals in the executable. Console-output scanners
    // for log fingerprints find nothing.
    //
    // `console.error` calls are intentionally NOT gated â€” failures
    // are signal we want to surface even in production builds.
    // ============================================================
    const DEBUG = true;

    // ============================================================
    // Phase 7c round-3 (safe subset): fine-grained debug flags for
    // high-volume logs that drown the DevTools console during
    // normal use.
    //
    // - OSL_DEBUG_SWEEP: gates the 250ms periodic sweep tick log
    //   (`[OSL] periodic sweep tick (msgs=N, cached=M,
    //   dispatched=K)`). Fires every tick whether anything
    //   happened or not. Turn on when debugging the sweep loop.
    //
    // - OSL_DEBUG_RECV: gates per-message `recvHandleDiv ENTRY`
    //   and the high-volume `reason=no_DPC0_prefix` SKIP log.
    //   Fires for every message rendered, OSL or not — unusable
    //   in a busy server channel. Turn on when investigating
    //   recv-side cover detection.
    //
    // Important `[OSL]` logs (control-message handling, decrypt
    // success/failure, intercept abort/passthrough, whitelist UI
    // events, recvHandleDiv DISPATCH/APPLY/other SKIP reasons)
    // stay un-gated so real activity remains visible.
    // ============================================================
    const OSL_DEBUG_SWEEP = false;
    const OSL_DEBUG_RECV = false;

    // ============================================================
    // Captured native timers.
    //
    // Discord's bundle loads AFTER this IIFE (we run as a Tauri
    // `initialization_script`, before any document scripts). Some
    // sites' bundles override `window.setInterval` or
    // `setTimeout` for instrumentation / virtual scheduling.
    // Capture the native references here so the receive
    // observer's periodic sweep keeps using browser timers
    // regardless of what Discord installs later. Bound to
    // `window` because the spec lets some engines crash on
    // detached `this` for these natives.
    // ============================================================
    const nativeSetInterval = window.setInterval.bind(window);
    const nativeClearInterval = window.clearInterval.bind(window);
    const nativeSetTimeout = window.setTimeout.bind(window);
    const nativeClearTimeout = window.clearTimeout.bind(window);


    // ============================================================
    // Helpers (IIFE-scoped, recreated each invocation).
    // ============================================================

    /**
     * Returns the live Tauri IPC `invoke` function or undefined.
     * Both window globals are checked because Tauri 2.x patches drift
     * between exposing one or the other.
     */
    function getTauriInvoke() {
        if (
            window.__TAURI_INTERNALS__ &&
            typeof window.__TAURI_INTERNALS__.invoke === "function"
        ) {
            return window.__TAURI_INTERNALS__.invoke;
        }
        if (
            window.__TAURI__ &&
            window.__TAURI__.core &&
            typeof window.__TAURI__.core.invoke === "function"
        ) {
            return window.__TAURI__.core.invoke;
        }
        return undefined;
    }

    // Phase 7c bug-fix #1: cached local Discord ID.
    //
    // The injection layer has no reliable React-fiber source for
    // self-id — fiber walks from message anchors miss it most of
    // the time. The Rust shell already holds the loaded
    // identity's `user_id` (== local Discord snowflake) in
    // `AppState`; `osl_get_self_user_id` surfaces it. Result is
    // cached for the session — identity changes require a restart
    // anyway. Concurrent callers share a single in-flight invoke.
    let oslSelfDiscordIdCache = null;
    let oslSelfDiscordIdInFlight = null;
    let oslSelfDiscordIdLastError = null;
    let oslSelfDiscordIdToastShown = false;
    function oslSelfDiscordId() {
        if (typeof oslSelfDiscordIdCache === "string") {
            return Promise.resolve(oslSelfDiscordIdCache);
        }
        if (oslSelfDiscordIdInFlight) return oslSelfDiscordIdInFlight;
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") {
            return Promise.resolve(null);
        }
        oslSelfDiscordIdInFlight = invoke("osl_get_self_user_id", {})
            .then(function (id) {
                // Discord snowflakes are 17–20 digits; the previous
                // 15–22 range was too loose AND too tight at the
                // edges. The round-3 Rust impl now returns the
                // peer_map-resolved snowflake, not identity.user_id.
                if (typeof id === "string" && /^\d{17,20}$/.test(id)) {
                    oslSelfDiscordIdCache = id;
                    oslSelfDiscordIdLastError = null;
                    return id;
                }
                // Log the actual rejected value so we can diagnose
                // shape mismatches (snowflake-as-empty-string, wrong
                // field, etc.).
                oslSelfDiscordIdLastError =
                    "osl_get_self_user_id returned non-snowflake value " +
                    JSON.stringify(id);
                console.error("[OSL] " + oslSelfDiscordIdLastError);
                return null;
            })
            .catch(function (err) {
                const msg =
                    typeof err === "string"
                        ? err
                        : err && err.message
                            ? err.message
                            : String(err);
                oslSelfDiscordIdLastError = msg;
                console.error("[OSL] osl_get_self_user_id failed: " + msg);
                // One-shot toast so the user sees the actionable Rust
                // error (e.g. "add to peer_map.json") without it
                // re-spamming on every click. Helper-internal so all
                // callers benefit uniformly.
                if (!oslSelfDiscordIdToastShown) {
                    oslSelfDiscordIdToastShown = true;
                    try {
                        if (typeof oslToast === "function") {
                            oslToast(msg, { durationMs: 8000 });
                        }
                    } catch (e) {}
                }
                return null;
            })
            .finally(function () {
                oslSelfDiscordIdInFlight = null;
            });
        return oslSelfDiscordIdInFlight;
    }

    /**
     * Thin wrapper around the Tauri command. Resolves to the cover-
     * text string on success, rejects on IPC-level failure. Phase 3:
     * command body is stubbed to `[OSL-STUB] <plaintext>`. Phase 4
     * swaps in the real crypto pipeline behind the same wire shape.
     */
    window.__OSL_INTERCEPT__ = function (channelId, plaintext, options) {
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") {
            return Promise.reject(
                new Error(
                    "[OSL] Tauri IPC bridge not present on window â€” check " +
                        "capabilities/main.json grants `allow-osl-encrypt-message` " +
                        "and `remote.urls` includes `https://discord.com/*`."
                )
            );
        }
        return invoke("osl_encrypt_message", {
            channelId: channelId,
            plaintext: plaintext,
            options: options || {},
        });
    };

    /**
     * Debug-only manual decrypt invoker for DevTools.
     *
     * Discord's main world (where boot.js runs to hook fetch/XHR)
     * does NOT see `window.__TAURI__` because Tauri 2 keeps that
     * API in the isolated world. So a developer can't open the
     * console and call `__TAURI__.core.invoke('osl_decrypt_message',
     * â€¦)` directly â€” the global is undefined. This wrapper
     * exposes the same IPC path the receive observer uses, but
     * callable interactively, for diagnosing the "first decrypt
     * succeeds, subsequent hang" symptom.
     *
     * Usage from DevTools console:
     *
     *   await window.__OSL_DEBUG_DECRYPT__(
     *     "DPC0::AQFC...",          // raw cover string
     *     "1477008451799482419",    // sender Discord user_id
     *     "1502771310428819569"     // optional channel_id; defaults
     *                               // to current URL if omitted
     *   );
     *
     * Resolves to the decrypted plaintext, rejects with the same
     * `OSL: â€¦` strings the receive observer sees. Includes a
     * `[OSL] __OSL_DEBUG_DECRYPT__ id=N` log on dispatch + result
     * so the user can correlate with the receive observer's
     * `el#N` logs and tell whether the IPC layer is alive.
     */
    let oslDebugDecryptId = 0;
    window.__OSL_DEBUG_DECRYPT__ = function (cover, senderDiscordId, channelId) {
        const id = ++oslDebugDecryptId;
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") {
            console.log(
                "[OSL] __OSL_DEBUG_DECRYPT__ id=" +
                    id +
                    " ERROR: Tauri IPC bridge not present"
            );
            return Promise.reject(new Error("Tauri IPC bridge not present"));
        }
        let resolvedChannel = channelId;
        if (!resolvedChannel) {
            const m = window.location.pathname.match(
                /\/channels\/[^/]+\/(\d{15,22})/
            );
            resolvedChannel = m ? m[1] : "manual-debug";
        }
        console.log(
            "[OSL] __OSL_DEBUG_DECRYPT__ id=" +
                id +
                " dispatching (sender=" +
                senderDiscordId +
                ", channel=" +
                resolvedChannel +
                ", cover_len=" +
                (cover ? cover.length : 0) +
                ")"
        );
        const t0 = (typeof performance !== "undefined" && performance.now)
            ? performance.now()
            : Date.now();
        return invoke("osl_decrypt_message", {
            channelId: resolvedChannel,
            senderDiscordId: senderDiscordId,
            content: cover,
        }).then(
            function (plaintext) {
                const dt = (typeof performance !== "undefined" && performance.now)
                    ? performance.now() - t0
                    : Date.now() - t0;
                console.log(
                    "[OSL] __OSL_DEBUG_DECRYPT__ id=" +
                        id +
                        " ok in " +
                        Math.round(dt) +
                        "ms"
                );
                return plaintext;
            },
            function (err) {
                const dt = (typeof performance !== "undefined" && performance.now)
                    ? performance.now() - t0
                    : Date.now() - t0;
                const msg = err && err.message ? err.message : String(err);
                console.log(
                    "[OSL] __OSL_DEBUG_DECRYPT__ id=" +
                        id +
                        " err in " +
                        Math.round(dt) +
                        "ms: " +
                        msg
                );
                throw err;
            }
        );
    };

    // Path patterns for the Discord REST API. The `/v\d+/` segment
    // tolerates Discord rolling the API version (currently v9).
    const SEND_RE =
        /\/api\/v\d+\/channels\/(\d+)\/messages\/?(?:\?|$)/;
    const EDIT_RE =
        /\/api\/v\d+\/channels\/(\d+)\/messages\/(\d+)\/?(?:\?|$)/;

    /**
     * Attempt JSON parse + content-mutation against the body text.
     *
     * Three exit callbacks, distinct by privacy meaning:
     *
     * - `onMutated(newBodyJson)` â€” encryption succeeded, send the
     *   ciphertext-bearing body.
     * - `onPassthrough()` â€” there was no plaintext to encrypt
     *   (sticker-only / attachment-only sends with a missing or
     *   empty `content` field). Original body is forwarded as-is.
     *   This is **safe**: nothing was meant to be encrypted.
     * - `onAbort(err)` â€” Phase 4 fail-closed. We **tried** to
     *   encrypt but the pipeline rejected (IPC throw, non-Promise,
     *   non-string result, JSON re-serialisation failure, or
     *   `__OSL_INTERCEPT__` rejected). The caller MUST simulate a
     *   network failure rather than passing the plaintext through.
     *
     * Phase 3 fail-open routed every error to `onPassthrough()`,
     * which would have leaked plaintext on any pipeline failure. Phase 4
     * splits these paths so encryption-attempted-but-failed paths
     * fail closed.
     *
     * The "body not JSON-parseable" path stays on `onPassthrough()`
     * for now: it's almost always a Discord-side schema change for
     * non-content payloads, and the entrypoint already URL-matched
     * on `/messages` POST so a future change that sends plaintext
     * via a non-JSON body would be a regression we'd want to
     * catch by bisection rather than mask. Tightening this to abort
     * is a Phase 4b refinement.
     *
     * `source` is "fetch" or "XHR", woven into log lines.
     */
    function interceptBody(
        source,
        channelId,
        bodyText,
        onMutated,
        onPassthrough,
        onAbort
    ) {
        let parsed;
        try {
            parsed = JSON.parse(bodyText);
        } catch (e) {
            if (DEBUG)
                console.warn(
                    "[OSL] outgoing /messages (" +
                        source +
                        "): body not JSON-parseable; passthrough",
                    e
                );
            return onPassthrough();
        }

        // 8c step 3: attachment cover-envelope injection. If
        // /messages references uploaded files that match our
        // pending reservations (i.e. step 1+2 were intercepted),
        // build the v=2 attachment cover and use it as `content`
        // — replacing whatever the user typed alongside the
        // attachment (Discord allows arbitrary content alongside
        // attachments). When no attachments match a reservation,
        // fall through to the normal text-encrypt path.
        const attachmentResult = oslMaybeBuildAttachmentCover(parsed);
        if (attachmentResult && attachmentResult.handled) {
            if (attachmentResult.error) {
                console.error(
                    "[OSL] step3 attachment cover failed (" +
                        source +
                        "): " +
                        attachmentResult.error
                );
                return onAbort(new Error(attachmentResult.error));
            }
            return attachmentResult.promise.then(
                function (mutatedBody) {
                    return onMutated(mutatedBody);
                },
                function (err) {
                    console.error(
                        "[OSL] step3 attachment cover rejected (" +
                            source +
                            "):",
                        err
                    );
                    return onAbort(err);
                }
            );
        }

        if (typeof parsed.content !== "string") {
            return onPassthrough();
        }
        if (parsed.content === "") {
            return onPassthrough();
        }
        // Phase 7b safety-net: content already starts with the
        // DPC0:: cover prefix. Either the caller pre-encrypted
        // (e.g. `oslSendControlMessage` shipping a v=2 control
        // message via the regular fetch path), or Discord echoed
        // back our own cover in a follow-up request. Either way,
        // re-encrypting would double-wrap and corrupt; pass the
        // body through untouched.
        if (parsed.content.indexOf("DPC0::") === 0) {
            if (DEBUG)
                console.log(
                    "[OSL] outgoing /messages (" +
                        source +
                        "): content already DPC0::; passthrough (pre-encrypted)"
                );
            return onPassthrough();
        }

        const plaintext = parsed.content;
        if (DEBUG)
            console.log(
                "[OSL] outgoing message (" +
                    source +
                    "): channel=" +
                    channelId +
                    " content_len=" +
                    plaintext.length
            );

        // Phase 7c send-path gate: if the current channel scope has
        // encrypt_toggle ON and a non-empty whitelist, encrypt to
        // the v=2 wire format (scope-whitelist recipients) and skip
        // the v=1 per-channel-share path. Coexistence: when toggle
        // is off OR whitelist empty OR scope-state read fails, fall
        // through to v=1 (`__OSL_INTERCEPT__`) so pre-7c senders
        // and not-yet-whitelisted scopes keep working.
        function v1Send() {
            let interceptResult;
            try {
                interceptResult = window.__OSL_INTERCEPT__(
                    channelId,
                    plaintext,
                    parsed
                );
            } catch (e) {
                console.error(
                    "[OSL] __OSL_INTERCEPT__ threw synchronously (" +
                        source +
                        "); ABORT (fail-closed)",
                    e
                );
                return onAbort(e);
            }
            if (!interceptResult || typeof interceptResult.then !== "function") {
                console.error(
                    "[OSL] __OSL_INTERCEPT__ did not return a Promise (" +
                        source +
                        "); ABORT (fail-closed)",
                    { actualType: typeof interceptResult }
                );
                return onAbort(
                    new Error("__OSL_INTERCEPT__ did not return a Promise")
                );
            }

            return interceptResult.then(
                function (coverText) {
                    if (typeof coverText !== "string") {
                        console.error(
                            "[OSL] __OSL_INTERCEPT__ returned non-string (" +
                                typeof coverText +
                                ", source=" +
                                source +
                                "); ABORT (fail-closed)"
                        );
                        return onAbort(new Error("non-string cover text"));
                    }
                    parsed.content = coverText;
                    let newBody;
                    try {
                        newBody = JSON.stringify(parsed);
                    } catch (e) {
                        console.error(
                            "[OSL] re-serialising mutated body failed (" +
                                source +
                                "); ABORT (fail-closed)",
                            e
                        );
                        return onAbort(e);
                    }
                    return onMutated(newBody);
                },
                function (err) {
                    // Phase 7c bug-fix #2: "OSL: recipient lookup:
                    // channel ... not configured in recipients file"
                    // means this channel has no v=1 entry in
                    // channels.json AND already (by virtue of being
                    // in v1Send) no v=2 whitelist/toggle. There's
                    // nothing to encrypt — fall through to plaintext
                    // passthrough rather than aborting the send.
                    // The v=2 gate above still fails closed for its
                    // own error paths; only the unconfigured-channel
                    // case lands here.
                    const msg = err && err.message ? err.message : String(err);
                    if (msg.indexOf("OSL: recipient lookup:") === 0) {
                        if (DEBUG)
                            console.log(
                                "[OSL] v=1 path: channel not OSL-configured (" +
                                    source +
                                    "); passthrough plaintext"
                            );
                        return onPassthrough();
                    }
                    console.error(
                        "[OSL] __OSL_INTERCEPT__ rejected (" +
                            source +
                            "); ABORT (fail-closed)",
                        err
                    );
                    return onAbort(err);
                }
            );
        }

        let v7cCtx = null;
        try {
            v7cCtx =
                typeof oslCurrentChannelContext === "function"
                    ? oslCurrentChannelContext()
                    : null;
        } catch (e) {
            v7cCtx = null;
        }
        let v7cScope = null;
        if (v7cCtx && v7cCtx.channelId === channelId) {
            try {
                v7cScope =
                    typeof oslScopeForCurrentContext === "function"
                        ? oslScopeForCurrentContext(v7cCtx)
                        : null;
            } catch (e) {
                v7cScope = null;
            }
        }
        if (!v7cScope || typeof oslInvoke !== "function") {
            // 7d-PIVOT-FIX4: no scope detected (or Tauri bridge
            // missing) means the user couldn't have toggled
            // encrypt on for this send — passthrough the plain
            // body. v1Send was the legacy per-channel-share path;
            // under PIVOT we never want to silently fall into it.
            console.log(
                "[OSL] send-gate (" +
                    source +
                    "): no scope / no Tauri — passthrough"
            );
            return onPassthrough();
        }

        // 7d-PIVOT-FIX3 Bug E: the composer toggle's
        // `data-osl-encrypt-state` is the authoritative source for
        // "encrypt this send" at send time. PIVOT-FIX2 tried
        // aria-checked here, but observed in DevTools that the
        // attribute returns null on many React reconciliations —
        // the FIX2 log said "source: aria-checked" while reading
        // null, so the gate fell through to "encrypt off" even
        // with the visual showing ON. data-osl-encrypt-state is
        // stamped by oslComposerToggleStyle on every visual
        // change and persists across reconciliation.
        //
        // Fallback chain: data-osl-encrypt-state → synchronous IPC
        // → default-off (fail-closed).
        const composerToggle = document.querySelector(
            "[" + COMPOSER_TOGGLE_DATA_ATTR + "='1']"
        );
        const stateAttr =
            composerToggle &&
            composerToggle.getAttribute("data-osl-encrypt-state");
        const togglePromise =
            stateAttr === "on" || stateAttr === "off"
                ? Promise.resolve({
                      ok: true,
                      value: {
                          encrypt_toggle: stateAttr === "on",
                          // has_whitelist is unused by the gate
                          // under PIVOT — encrypt_toggle is the
                          // only signal — but we set it so the
                          // shape matches the IPC fallback.
                          has_whitelist: false,
                      },
                      source: "data-osl-encrypt-state",
                  })
                : oslInvoke("osl_get_scope_encryption_state", {
                      scopeInput: v7cScope,
                  }).then(function (r) {
                      return Object.assign({}, r, {
                          source: r && r.ok ? "ipc-fallback" : "default-off",
                      });
                  });

        return togglePromise.then(function (stateRes) {
            // 7d-PIVOT: encrypt_toggle is now independent of
            // whitelist. Send-time gate is just encrypt_toggle —
            // if no peer whitelist exists, the Rust send path
            // encrypts to self only (you alone decrypt).
            const encryptOn = !!(
                stateRes &&
                stateRes.ok &&
                stateRes.value &&
                stateRes.value.encrypt_toggle
            );
            console.log(
                "[OSL] send-gate (" +
                    source +
                    "): encrypt_toggle = " +
                    (encryptOn ? "on" : "off") +
                    " (source: " +
                    (stateRes.source || "?") +
                    ") — " +
                    (encryptOn ? "encrypting" : "skipping encrypt")
            );
            if (!encryptOn) {
                // 7d-PIVOT-FIX4: toggle OFF means the user wants
                // the message plain. PIVOT-FIX3 routed this through
                // v1Send() which calls __OSL_INTERCEPT__, which
                // invokes the legacy `osl_encrypt_message` (v=1)
                // per-channel-share path — that still produced a
                // DPC0:: ciphertext on the wire. PIVOT replaced the
                // v=1 model with the toggle-gated v=2 model, so the
                // only correct "encrypt off" behaviour is to
                // forward the original plaintext untouched.
                return onPassthrough();
            }
            const memberIds = (v7cCtx.members || [])
                .map(function (m) {
                    if (typeof m === "string") return m;
                    if (m && typeof m.id === "string") return m.id;
                    if (m && typeof m.user_id === "string") return m.user_id;
                    return null;
                })
                .filter(function (x) {
                    return typeof x === "string" && x.length > 0;
                });
            return oslSelfDiscordId().then(function (selfId) {
                if (!selfId) {
                    console.error(
                        "[OSL] v=2 send gate (" +
                            source +
                            "): osl_get_self_user_id returned null; ABORT (fail-closed)"
                    );
                    return onAbort(new Error("v2_send_no_self_id"));
                }
                return oslEncryptV2(
                    plaintext,
                    v7cScope,
                    memberIds,
                    selfId
                ).then(
                function (wire) {
                    if (typeof wire !== "string") {
                        console.error(
                            "[OSL] v=2 send gate (" +
                                source +
                                "): oslEncryptV2 returned non-string; ABORT (fail-closed)"
                        );
                        return onAbort(new Error("v2_encrypt_failed"));
                    }
                    parsed.content = wire;
                    // 7d-PIVOT-FIX3 Bug F: inline post-burn re-engage.
                    // A successful encrypt-send into a burned scope
                    // un-burns it. PIVOT-FIX2 routed this through the
                    // `osl:scope_unburned` cross-window event, but
                    // Discord's CSP can drop those events before they
                    // reach this origin's listener — leaving the JS
                    // `__oslBurnedScopes` cache stale and the receive
                    // observer continuing to skip decrypts for the
                    // re-engaged scope. Do it locally and persist via
                    // a fire-and-forget IPC call. The Rust command is
                    // idempotent so the cross-window path (still wired
                    // for Settings Window initiated unburns) is safe
                    // to keep alongside this.
                    try {
                        oslInlineUnburnAfterEncrypt(v7cScope);
                    } catch (_) {}
                    let newBody;
                    try {
                        newBody = JSON.stringify(parsed);
                    } catch (e) {
                        console.error(
                            "[OSL] v=2 send gate (" +
                                source +
                                "): re-serialising mutated body failed; ABORT (fail-closed)",
                            e
                        );
                        return onAbort(e);
                    }
                    if (DEBUG)
                        console.log(
                            "[OSL] v=2 send gate (" +
                                source +
                                "): wire=DPC0:: channel=" +
                                channelId +
                                " scope=" +
                                v7cScope.kind +
                                ":" +
                                v7cScope.id
                        );
                    return onMutated(newBody);
                },
                function (err) {
                    console.error(
                        "[OSL] v=2 send gate (" +
                            source +
                            "): oslEncryptV2 rejected; ABORT (fail-closed)",
                        err
                    );
                    return onAbort(err);
                }
            );
            });
        });
    }

    /**
     * Phase 6a: fire-and-forget persist call for an edited
     * message. The original plaintext (what the user typed) is
     * the source of truth â€” we don't round-trip through decrypt.
     * Failures log but never propagate; the receive observer's
     * normal decrypt-and-persist path provides a fallback.
     */
    function runPersistEdit(messageId, plaintext) {
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") return;
        invoke("osl_persist_edit", {
            discordMessageId: messageId,
            newPlaintext: plaintext,
        })
            .then(function () {
                console.log(
                    "[OSL] selfEdit persist msg=" +
                        messageId +
                        " plaintext_len=" +
                        plaintext.length
                );
            })
            .catch(function (err) {
                console.error(
                    "[OSL] selfEdit persist failed msg=" +
                        messageId +
                        ": " +
                        (err && err.message ? err.message : String(err))
                );
            });
    }

    // ===== Phase 7b: v=2 send + control message plumbing =====
    //
    // No UI surface — that's 7c. These helpers exist so 7b can be
    // exercised end-to-end via `window.__oslDebugSendV2` and so
    // the recv path's control-message dispatch sentinels surface
    // as console logs.
    //
    // OSL_RESULT_* sentinel strings must match the Rust constants
    // in `crates/ipc/src/commands.rs`. If those change, update
    // here.
    const OSL_RESULT_BURN_APPLIED = "__OSL_CONTROL_BURN_APPLIED__";
    const OSL_RESULT_INVITATION_RECEIVED =
        "__OSL_CONTROL_INVITATION_RECEIVED__";
    const OSL_RESULT_RESPONSE_RECEIVED = "__OSL_CONTROL_RESPONSE_RECEIVED__";
    // Phase 8: recv-side sentinel for an attachment envelope. The
    // full result string is `OSL_RESULT_ATTACHMENT_PREFIX + json`,
    // where the JSON carries the per-attachment AEAD key + filenames
    // + MIME for boot.js to feed into `osl_open_attachment`.
    const OSL_RESULT_ATTACHMENT_PREFIX = "__OSL_CONTROL_ATTACHMENT__|";

    /**
     * Phase 7b: encrypt `plaintext` for the whitelist-resolved
     * recipients of `scopeInput` across `channelMembers`. Returns
     * the wire-format string (`DPC0::<base64>`) or null on error.
     *
     * Failure modes (all logged + null-returned, never thrown):
     *   - no Tauri invoke
     *   - no identity loaded
     *   - empty whitelist for the scope
     *   - inner crypto error
     *
     * `selfDiscordId` is required because the Rust layer needs to
     * exclude self from the channel-member recipient walk (we
     * include self_pubkey via identity, separately). Boot.js
     * pulls our Discord id from the page state.
     */
    async function oslEncryptV2(
        plaintext,
        scopeInput,
        channelMembers,
        selfDiscordId
    ) {
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") {
            console.log("[OSL] oslEncryptV2 FAIL reason=no_invoke");
            return null;
        }
        // 7d-PIVOT-FIX4 diagnostic: log every actual encrypt
        // invocation. If a "CALLING" line ever appears with a
        // matching "encrypt_toggle = off" send-gate line, the gate
        // didn't actually short-circuit — that would be a regression.
        const scopeKey =
            scopeInput && scopeInput.kind && scopeInput.id
                ? scopeInput.kind + ":" + scopeInput.id
                : "?";
        console.log(
            "[OSL] CALLING osl_encrypt_message_v2 for scope=" +
                scopeKey +
                ", plaintext_len=" +
                (plaintext ? plaintext.length : 0)
        );
        try {
            const wire = await invoke("osl_encrypt_message_v2", {
                plaintext: plaintext,
                scopeInput: scopeInput,
                channelMembers: channelMembers,
                selfDiscordId: selfDiscordId,
            });
            if (typeof wire === "string" && wire.indexOf("DPC0::") === 0) {
                console.log(
                    "[OSL] osl_encrypt_message_v2 returned, body now DPC0:: prefix"
                );
                return wire;
            }
            console.log(
                "[OSL] oslEncryptV2 FAIL reason=non_string_or_missing_prefix"
            );
            return null;
        } catch (err) {
            const msg = err && err.message ? err.message : String(err);
            console.log("[OSL] oslEncryptV2 FAIL reason=" + msg);
            return null;
        }
    }

    /**
     * Phase 7b: ship a pre-encrypted wire string to `channelId`
     * via Discord's REST API. The send-side `interceptBody` has a
     * DPC0:: passthrough guard so this fetch doesn't get
     * re-encrypted on the way out. Authenticated via the same
     * token capture used by the Phase 6a edit overlay (we re-read
     * `editOverlayAuthToken` from the IIFE scope).
     *
     * Fire-and-forget: returns the fetch Response on success or
     * null on failure (logged).
     */
    async function oslSendControlMessage(channelId, wireString) {
        if (!editOverlayAuthToken) {
            console.log(
                "[OSL] oslSendControlMessage FAIL reason=no_auth_token"
            );
            return null;
        }
        if (
            typeof wireString !== "string" ||
            wireString.indexOf("DPC0::") !== 0
        ) {
            console.log(
                "[OSL] oslSendControlMessage FAIL reason=not_dpc0_wire"
            );
            return null;
        }
        const url = "/api/v9/channels/" + channelId + "/messages";
        try {
            const resp = await fetch(url, {
                method: "POST",
                headers: {
                    Authorization: editOverlayAuthToken,
                    "Content-Type": "application/json",
                },
                body: JSON.stringify({ content: wireString }),
            });
            if (resp && resp.ok) {
                console.log(
                    "[OSL] oslSendControlMessage OK channel=" +
                        channelId +
                        " wire_len=" +
                        wireString.length
                );
            } else {
                console.log(
                    "[OSL] oslSendControlMessage FAIL channel=" +
                        channelId +
                        " status=" +
                        (resp ? resp.status : "?")
                );
            }
            return resp;
        } catch (err) {
            console.error(
                "[OSL] oslSendControlMessage threw channel=" + channelId,
                err
            );
            return null;
        }
    }

    /**
     * Phase 7b: build a ScopeInput from Discord channel metadata.
     *
     * `channelType` is the standard Discord channel-type number:
     *   0 = server text channel
     *   1 = DM
     *   3 = group DM (GC)
     *
     * For DM: caller passes `peerDiscordId` as `gcMembers[0]`
     * (the lone peer's id). We extract that as the scope id.
     *
     * For server_full: not derivable from a single channel —
     * returns null. The 7c UI will set server_full scopes via
     * an explicit picker.
     *
     * Returns a ScopeInput object or null when the scope can't
     * be inferred from this channel alone.
     */
    function oslDetectScope(channelId, channelType, serverId, gcMembers) {
        if (channelType === 1) {
            // DM: peer's discord_id is the scope id.
            const peer =
                Array.isArray(gcMembers) && gcMembers.length > 0
                    ? gcMembers[0]
                    : null;
            if (!peer) return null;
            return {
                kind: "dm",
                id: peer,
                channel_id: channelId,
            };
        }
        if (channelType === 3) {
            return {
                kind: "gc",
                id: channelId,
                channel_id: channelId,
            };
        }
        if (channelType === 0) {
            if (!serverId) return null;
            return {
                kind: "server_channel",
                id: serverId + ":" + channelId,
                server_id: serverId,
                channel_id: channelId,
            };
        }
        return null;
    }

    /**
     * Phase 7b: dispatch on a v=2 decrypt result string. The
     * Rust recv path returns either a plaintext content string
     * (for `msg_type=0x00`) or one of the OSL_RESULT_* sentinel
     * strings for control messages. For 7b this is a logging
     * stub — 7c's UI consumes the sentinels and updates the
     * banner / channel-header state accordingly.
     *
     * Returns `true` if the result was a control sentinel
     * (caller should NOT render as plaintext), `false` for
     * normal content.
     */
    function oslHandleDecryptResult(msgId, result) {
        if (typeof result !== "string") return false;
        // Phase 8: attachment-envelope sentinel uses a `<prefix>|<json>`
        // shape so we can't switch/case on the exact string.
        if (result.indexOf(OSL_RESULT_ATTACHMENT_PREFIX) === 0) {
            console.log(
                "[OSL] v=2 attachment envelope received msg=" + msgId
            );
            return true;
        }
        switch (result) {
            case OSL_RESULT_BURN_APPLIED:
                console.log("[OSL] v=2 burn applied msg=" + msgId);
                return true;
            case OSL_RESULT_INVITATION_RECEIVED:
                console.log(
                    "[OSL] v=2 invitation received msg=" + msgId
                );
                return true;
            case OSL_RESULT_RESPONSE_RECEIVED:
                console.log(
                    "[OSL] v=2 response received msg=" + msgId
                );
                return true;
            default:
                return false;
        }
    }

    /**
     * Phase 7b debug-only: send a v=2 message from DevTools to
     * verify wire format end-to-end.
     *
     *     window.__oslDebugSendV2(
     *         "1234567890",                // channel_id
     *         "hello from v2",             // plaintext
     *         { kind: "dm", id: "5678" },  // scope override (optional)
     *         ["5678"],                    // channel members (optional)
     *         "9999"                       // self discord id (required)
     *     );
     *
     * Returns the fetch Response on success or null on failure.
     */
    window.__oslDebugSendV2 = async function (
        channelId,
        plaintext,
        scopeOverride,
        channelMembers,
        selfDiscordId
    ) {
        const scope = scopeOverride || null;
        if (!scope) {
            console.log("[OSL] __oslDebugSendV2 FAIL reason=no_scope");
            return null;
        }
        if (!selfDiscordId) {
            console.log("[OSL] __oslDebugSendV2 FAIL reason=no_self_id");
            return null;
        }
        const members = Array.isArray(channelMembers) ? channelMembers : [];
        const wire = await oslEncryptV2(plaintext, scope, members, selfDiscordId);
        if (!wire) return null;
        return await oslSendControlMessage(channelId, wire);
    };

    // ============================================================
    // Phase 8: attachment send pipeline (helpers + intercept hook)
    //
    // Discord's attachment upload is a three-step flow:
    //   1. POST /api/v9/channels/{cid}/attachments — Discord-side
    //      pre-upload; response carries `upload_url` (GCS-signed
    //      PUT URL) + `upload_filename` for each file in the
    //      request's `files[]` body.
    //   2. PUT to the GCS URL with the raw file bytes.
    //   3. POST /messages with `attachments[]` referencing the
    //      uploaded files + `content`.
    //
    // The OSL intercept replaces step-2 body bytes with the
    // sealed-attachment blob (decoy PNG + OSL-ATT1 + AEAD wire),
    // and overrides step-3's `content` with a v=2 envelope
    // (MSG_TYPE_ATTACHMENT) carrying the per-attachment AEAD key.
    //
    // Per-build URL patterns for steps 1/2 drift across Discord
    // releases (different CDN providers, signed-URL shapes); the
    // helper `oslSealAttachmentForUpload` below does the encryption
    // + envelope build in pure JS and is callable from DevTools, so
    // a manual end-to-end test works regardless. The
    // `oslAttachmentUploadHooks` block then wires it into Discord's
    // actual fetch traffic — adjust the URL-matchers in that block
    // if Discord's flow changes.
    // ============================================================

    /**
     * Phase 8: seal a File (as picked by Discord's composer) for
     * upload. Returns `{ sealedBlob, sealedFilename, envelopeWire,
     * mimeType, originalFilename }` — the caller PUTs `sealedBlob`
     * to Discord's CDN under `sealedFilename` and POSTs to
     * /messages with `content = envelopeWire`.
     */
    async function oslSealAttachmentForUpload(file, scopeInput, channelMembers, selfDiscordId) {
        if (!file || typeof file.arrayBuffer !== "function") {
            throw new Error("oslSealAttachmentForUpload: invalid File");
        }
        const buf = await file.arrayBuffer();
        const bytes = new Uint8Array(buf);
        let binary = "";
        const CHUNK = 32 * 1024;
        for (let i = 0; i < bytes.length; i += CHUNK) {
            const slice = bytes.subarray(i, Math.min(i + CHUNK, bytes.length));
            binary += String.fromCharCode.apply(null, slice);
        }
        const b64 = btoa(binary);
        const sealRes = await oslInvoke("osl_seal_attachment", {
            originalBytesB64: b64,
            originalFilename: file.name,
        });
        if (!sealRes.ok) {
            throw new Error("osl_seal_attachment failed: " + sealRes.error);
        }
        const sealed = sealRes.value;
        // Build the v=2 envelope wire that carries the AEAD key
        // to whitelisted recipients. 8b: always sends an array
        // (length 1 for the single-attachment debug-upload path).
        const envRes = await oslInvoke("osl_encrypt_attachment_envelope", {
            scopeInput: scopeInput,
            channelMembers: channelMembers,
            selfDiscordId: selfDiscordId,
            attachments: [
                {
                    attKeyB64: sealed.attKeyB64,
                    originalFilename: file.name,
                    randomFilename: sealed.randomFilename,
                    mimeType: sealed.mimeType,
                },
            ],
        });
        if (!envRes.ok) {
            throw new Error(
                "osl_encrypt_attachment_envelope failed: " + envRes.error
            );
        }
        // Decode the sealed blob b64 back into a Blob ready for
        // Discord's CDN PUT.
        const sealedBinary = atob(sealed.fileBlobB64);
        const sealedBytes = new Uint8Array(sealedBinary.length);
        for (let i = 0; i < sealedBinary.length; i++) {
            sealedBytes[i] = sealedBinary.charCodeAt(i);
        }
        const sealedBlob = new Blob([sealedBytes], { type: "image/png" });
        return {
            sealedBlob: sealedBlob,
            sealedFilename: sealed.randomFilename,
            envelopeWire: envRes.value,
            mimeType: sealed.mimeType,
            originalFilename: file.name,
        };
    }

    // ============================================================
    // Phase 8c: 3-step GCS upload-flow state coordination.
    //
    // Discord's attachment upload is:
    //   1. POST /api/v9/channels/{cid}/attachments — request
    //      `{files: [{filename, file_size, ...}]}`, response
    //      `{attachments: [{id, upload_filename, upload_url}]}`
    //      where `upload_url` is a presigned GCS PUT URL with an
    //      `upload_id=` query token.
    //   2. PUT to that GCS URL with the raw file bytes.
    //   3. POST /api/v9/channels/{cid}/messages with
    //      `{content, attachments: [{id, filename,
    //      uploaded_filename, original_content_type}]}` that
    //      references the GCS-uploaded files.
    //
    // We intercept all three. Step 1 reserves a slot (a
    // `PendingUploadEntry`) keyed temporarily by the random
    // upload-filename we generate; the load listener rekeys to the
    // GCS `upload_id` once Discord's response lands. Step 2 looks
    // up by `upload_id` and replaces the binary body with the
    // sealed-attachment bundle. Step 3 looks up by
    // `upload_filename` to build the v=2 attachment cover that
    // replaces `content`. Entries auto-expire 5 minutes after
    // creation so a forgotten upload doesn't pin memory.
    //
    // Reservation cap of 10 matches Discord's per-message attachment
    // limit — exceeding it aborts.
    if (!window.__oslPendingUploads) {
        window.__oslPendingUploads = new Map();
    }
    const OSL_PENDING_UPLOADS_CAP = 10;
    const OSL_PENDING_UPLOAD_TTL_MS = 5 * 60 * 1000;

    function oslPendingUploadsPurgeStale() {
        const now = Date.now();
        for (const [k, v] of window.__oslPendingUploads.entries()) {
            if (now - v.createdAt > OSL_PENDING_UPLOAD_TTL_MS) {
                window.__oslPendingUploads.delete(k);
            }
        }
    }
    function oslPendingUploadsRoom() {
        oslPendingUploadsPurgeStale();
        return (
            OSL_PENDING_UPLOADS_CAP - window.__oslPendingUploads.size
        );
    }
    function oslPendingUploadsByFilename(uploadFilename) {
        for (const v of window.__oslPendingUploads.values()) {
            if (v.uploadFilename === uploadFilename) return v;
        }
        return null;
    }

    /**
     * 8c: streaming-AEAD bucket table. JS-side mirror of
     * `crypto::attachment::ATTACHMENT_BUCKETS`. Used at step 1 to
     * declare a generous GCS reservation size before we've actually
     * encrypted — GCS allows under-fill so over-declaring is safe.
     *
     * Bundle overhead on top of the padded bucket: per-chunk 16-byte
     * AEAD tags (~64KB worst case at 25MB / 16KB chunks), the
     * stream header, the decoy PNG (~3KB for solid-color), and the
     * magic + filename header (~50 bytes). 128KB headroom covers
     * everything with margin.
     */
    const OSL_ATT_BUCKETS = [
        256 * 1024,
        1024 * 1024,
        5 * 1024 * 1024,
        10 * 1024 * 1024,
        25 * 1024 * 1024,
    ];
    function oslBucketFor(plaintextSize) {
        for (const b of OSL_ATT_BUCKETS) {
            if (plaintextSize <= b) return b;
        }
        return OSL_ATT_BUCKETS[OSL_ATT_BUCKETS.length - 1];
    }
    function oslReservedSizeFor(plaintextSize) {
        return oslBucketFor(plaintextSize) + 128 * 1024;
    }

    /**
     * Phase 8b (DEPRECATED in 8c): the legacy single-step multipart
     * upload path. Discord's real attachment flow is the 3-step
     * GCS upload (POST /attachments → PUT to GCS → POST /messages),
     * so this branch never fires in production — but kept as a
     * passthrough so a future Discord build that swaps back to
     * multipart doesn't silently break. Logged once per call so
     * regressions show up in DevTools.
     */
    async function oslInterceptMultipartXhr(origSend, xhr, args, channelId, _formData) {
        console.log(
            "[OSL] FormData on /messages (XHR): unexpected in 8c flow, passthrough channel=" +
                channelId
        );
        return Reflect.apply(origSend, xhr, args);
    }

    // ============================================================
    // Phase 8c: 3-step GCS upload interception helpers.
    // ============================================================

    /**
     * URL patterns for the three Discord attachment-upload steps.
     * `ATTACHMENTS_RE` matches step 1; `GCS_UPLOAD_RE` matches the
     * GCS-hosted PUT URL Discord's pre-upload response hands out.
     */
    const ATTACHMENTS_RE =
        /\/api\/v\d+\/channels\/(\d+)\/attachments(?:\?|$)/;
    const GCS_UPLOAD_RE =
        /^https:\/\/discord-attachments-uploads-prd\.storage\.googleapis\.com\/.+\?.*upload_id=([^&]+)/;

    const ATT_SUPPORTED_RE = /\.(jpe?g|png|gif|webp|mp4|webm|mov)$/i;

    // FIX5: relative-time prefix (+Nms) so step1/step2/step3 timing
    // is visible in DevTools without needing the performance panel.
    // Baseline is module load; rolls over via `performance.now()`
    // which is monotonic and unaffected by wall-clock changes.
    const __oslAttT0 =
        typeof performance !== "undefined" && performance.now
            ? performance.now()
            : Date.now();
    function oslLogTime() {
        const now =
            typeof performance !== "undefined" && performance.now
                ? performance.now()
                : Date.now();
        return "+" + Math.round(now - __oslAttT0) + "ms";
    }

    function oslAbortXhr(xhr, reason, channelId) {
        console.warn(
            "[OSL] att abort: " +
                reason +
                (channelId ? " (channel=" + channelId + ")" : "")
        );
        setTimeout(function () {
            try {
                xhr.dispatchEvent(new ProgressEvent("error"));
                xhr.dispatchEvent(new ProgressEvent("loadend"));
            } catch (_) {}
        }, 0);
    }

    /**
     * 8c step 1: pre-process the POST /channels/{cid}/attachments
     * request when encrypt is ON for the channel's scope. Modifies
     * each `files[N]` entry's `filename` + `file_size` to claim the
     * encrypted bundle's shape (bucket-rounded + 128KB AEAD
     * headroom), reserves a slot in `__oslPendingUploads`, and
     * installs a one-shot `load` listener that rekeys each
     * reservation from its tempId to the GCS-supplied upload_id
     * once Discord's response lands.
     */
    async function oslInterceptStep1Attachments(origSend, xhr, args, channelId, bodyString) {
        let body;
        try {
            body = JSON.parse(bodyString);
        } catch (e) {
            console.log("[OSL] step1: body not JSON, passthrough");
            return Reflect.apply(origSend, xhr, args);
        }
        if (!body || !Array.isArray(body.files) || body.files.length === 0) {
            return Reflect.apply(origSend, xhr, args);
        }

        // Encrypt-toggle gate. Off → no reservation, no body
        // mutation, no load listener; the subsequent steps
        // naturally cascade to plain upload.
        const composerToggle = document.querySelector(
            "[" + COMPOSER_TOGGLE_DATA_ATTR + "='1']"
        );
        const stateAttr =
            composerToggle &&
            composerToggle.getAttribute("data-osl-encrypt-state");
        const isOn = stateAttr === "on";
        console.log(
            "[OSL] " +
                oslLogTime() +
                " step1 attachments: channel=" +
                channelId +
                " files=" +
                body.files.length +
                " encrypt_toggle=" +
                (isOn ? "on" : "off")
        );
        if (!isOn) {
            return Reflect.apply(origSend, xhr, args);
        }

        // Resolve scope. Passthrough if unresolvable — text path's
        // posture for unknown scopes.
        let ctx = null;
        try {
            ctx = oslCurrentChannelContext();
        } catch (_) {}
        if (!ctx || ctx.channelId !== channelId) {
            console.log("[OSL] step1: scope unresolvable, passthrough");
            return Reflect.apply(origSend, xhr, args);
        }
        const scope = oslScopeForCurrentContext(ctx);
        if (!scope) {
            console.log("[OSL] step1: scope unresolvable, passthrough");
            return Reflect.apply(origSend, xhr, args);
        }
        if (oslBurnedScopesShouldSkip(scope.channel_id || scope.id)) {
            oslToast(
                "Cannot send attachment to burned scope. Re-engage encryption first by sending a text message."
            );
            return oslAbortXhr(xhr, "burned scope", channelId);
        }

        // Pre-validate every file. Reject the whole batch on any
        // failure (strict all-or-nothing).
        if (oslPendingUploadsRoom() < body.files.length) {
            oslToast(
                "Too many pending encrypted uploads; wait for the previous ones to finish."
            );
            return oslAbortXhr(xhr, "pending-uploads cap exceeded", channelId);
        }
        for (const f of body.files) {
            if (typeof f.filename !== "string" || typeof f.file_size !== "number") {
                return oslAbortXhr(xhr, "step1: malformed file entry", channelId);
            }
            if (!ATT_SUPPORTED_RE.test(f.filename)) {
                oslToast(
                    "Unsupported file type for encryption: " +
                        f.filename +
                        ". Disable encrypt to send this file."
                );
                return oslAbortXhr(xhr, "unsupported ext: " + f.filename, channelId);
            }
            if (f.file_size > 24 * 1024 * 1024) {
                oslToast("File too large for encrypted upload (max 24 MB)");
                return oslAbortXhr(
                    xhr,
                    "oversize: " + f.filename + " " + f.file_size + "B",
                    channelId
                );
            }
        }

        // Mint a reservation per file and mutate the request body
        // to claim the random_filename + bucket-rounded size.
        // tempId is replaced with the real GCS upload_id once the
        // response load listener fires.
        const indexToTempId = {};
        body.files.forEach(function (f, i) {
            const reservedSize = oslReservedSizeFor(f.file_size);
            const randomFilename = (function () {
                const a = new Uint8Array(4);
                crypto.getRandomValues(a);
                let s = "";
                for (const x of a)
                    s += x.toString(16).padStart(2, "0");
                return s + ".png";
            })();
            const tempId =
                "tmp:" +
                Date.now().toString(36) +
                ":" +
                Math.random().toString(36).slice(2, 10) +
                ":" +
                i;
            const reservation = {
                tempId: tempId,
                channelId: channelId,
                scope: scope,
                channelMembers: (ctx.members || [])
                    .map(function (m) {
                        return typeof m === "string"
                            ? m
                            : (m && (m.id || m.user_id)) || null;
                    })
                    .filter(Boolean),
                originalFilename: f.filename,
                originalContentType: f.content_type || null,
                originalSize: f.file_size,
                reservedSize: reservedSize,
                randomFilename: randomFilename,
                uploadFilename: null, // set by load listener
                uploadId: null, // set by load listener
                fileIndex: i,
                status: "pending_bytes",
                createdAt: Date.now(),
            };
            window.__oslPendingUploads.set(tempId, reservation);
            indexToTempId[i] = tempId;
            f.filename = randomFilename;
            f.file_size = reservedSize;
            console.log(
                "[OSL] " +
                    oslLogTime() +
                    " step1 reserved: original=" +
                    reservation.originalFilename +
                    " random=" +
                    randomFilename +
                    " reserved_size=" +
                    reservedSize
            );
        });

        // Install a one-shot load listener to rekey reservations
        // from tempId to upload_id once Discord answers. Discord
        // registers its own `load` handler BEFORE our send proxy
        // runs, so its handler fires first and may dispatch step 2
        // (the GCS PUT) before our rekey runs. step 2 has a poll-
        // with-retry to handle that race; here we just make sure
        // the rekey itself stays synchronous (no awaits, no
        // setTimeouts) so the gap stays measured in milliseconds.
        function onAttachmentsResponse() {
            try {
                if (xhr.readyState !== 4) return;
                console.log(
                    "[OSL] " +
                        oslLogTime() +
                        " step1 response listener entered for channel=" +
                        channelId +
                        " status=" +
                        xhr.status
                );
                if (xhr.status < 200 || xhr.status >= 300) {
                    // Discord rejected the pre-upload request —
                    // clean up our reservations so they don't leak.
                    for (const tempId of Object.values(indexToTempId)) {
                        window.__oslPendingUploads.delete(tempId);
                    }
                    console.log(
                        "[OSL] " +
                            oslLogTime() +
                            " step1 response listener exiting (non-2xx, reservations cleaned)"
                    );
                    return;
                }
                const resp = JSON.parse(xhr.responseText);
                const respAtts = Array.isArray(resp && resp.attachments)
                    ? resp.attachments
                    : [];
                let rekeyed = 0;
                respAtts.forEach(function (att, i) {
                    const tempId = indexToTempId[i];
                    if (!tempId) return;
                    const reservation = window.__oslPendingUploads.get(tempId);
                    if (!reservation) return;
                    const m = (att.upload_url || "").match(
                        /[?&]upload_id=([^&]+)/
                    );
                    const uploadId = m ? m[1] : null;
                    reservation.uploadFilename = att.upload_filename || null;
                    reservation.uploadId = uploadId;
                    if (uploadId) {
                        window.__oslPendingUploads.delete(tempId);
                        window.__oslPendingUploads.set(uploadId, reservation);
                        rekeyed++;
                        console.log(
                            "[OSL] " +
                                oslLogTime() +
                                " step1 mapped upload_id=" +
                                uploadId.substring(0, 20) +
                                "... for random=" +
                                reservation.randomFilename
                        );
                    } else {
                        console.warn(
                            "[OSL] step1: response missing upload_id for " +
                                reservation.randomFilename
                        );
                    }
                });
                console.log(
                    "[OSL] " +
                        oslLogTime() +
                        " step1 response listener exiting after rekeying " +
                        rekeyed +
                        " reservation(s)"
                );
            } catch (e) {
                console.error("[OSL] step1 load listener threw:", e);
            }
        }
        try {
            xhr.addEventListener("load", onAttachmentsResponse);
        } catch (e) {
            console.error("[OSL] step1: failed to attach load listener:", e);
        }

        // Re-serialize the mutated body and dispatch.
        let newBody;
        try {
            newBody = JSON.stringify(body);
        } catch (e) {
            // Clean up the reservations on failure.
            for (const tempId of Object.values(indexToTempId)) {
                window.__oslPendingUploads.delete(tempId);
            }
            return oslAbortXhr(xhr, "step1: body re-serialize failed", channelId);
        }
        return Reflect.apply(origSend, xhr, [newBody]);
    }

    /**
     * 8c step 2: replace the PUT body bytes (the raw file) with
     * our sealed-attachment bundle before the GCS upload runs.
     * Called from the XHR send proxy when the URL matches
     * `GCS_UPLOAD_RE`. If no reservation is registered for the
     * upload_id, this is a non-OSL upload (encrypt was off when
     * step 1 ran) — passthrough.
     */
    /**
     * FIX5: returns true iff at least one reservation is still
     * keyed by its tempId (i.e. step 1's response listener hasn't
     * rekeyed it yet). Used by step 2 to decide whether the
     * upload_id miss is plausibly a race or a genuine non-OSL
     * upload — only the former is worth polling for.
     */
    function oslHasPendingRekey() {
        for (const k of window.__oslPendingUploads.keys()) {
            if (typeof k === "string" && k.startsWith("tmp:")) return true;
        }
        return false;
    }

    async function oslInterceptStep2GcsPut(origSend, xhr, args, uploadId) {
        let reservation = window.__oslPendingUploads.get(uploadId);

        // FIX5: rekey race. Discord registers its own `load`
        // handler on the step 1 XHR BEFORE our send proxy runs, so
        // Discord's handler fires first — and Discord then
        // synchronously kicks off step 2's GCS PUT. Our load
        // handler (which does the tempId → upload_id rekey) hasn't
        // run yet, so the lookup misses. Poll briefly to give it
        // a chance to land. Only poll if there's at least one
        // outstanding tempId reservation — otherwise this is a
        // genuine non-OSL upload (encrypt was off) and we want
        // the passthrough to stay fast.
        if (
            (!reservation || reservation.status !== "pending_bytes") &&
            oslHasPendingRekey()
        ) {
            const startMs = Date.now();
            console.log(
                "[OSL] " +
                    oslLogTime() +
                    " step2 await begin: upload_id=" +
                    uploadId.substring(0, 20) +
                    "..."
            );
            const MAX_ATTEMPTS = 10;
            const INTERVAL_MS = 25;
            for (let i = 1; i <= MAX_ATTEMPTS; i++) {
                await new Promise(function (r) {
                    setTimeout(r, INTERVAL_MS);
                });
                reservation = window.__oslPendingUploads.get(uploadId);
                if (reservation && reservation.status === "pending_bytes") {
                    console.log(
                        "[OSL] " +
                            oslLogTime() +
                            " step2 await success: upload_id=" +
                            uploadId.substring(0, 20) +
                            "... matched after " +
                            (Date.now() - startMs) +
                            "ms (attempt " +
                            i +
                            "/" +
                            MAX_ATTEMPTS +
                            ")"
                    );
                    break;
                }
                console.log(
                    "[OSL] step2 polling for upload_id=" +
                        uploadId.substring(0, 20) +
                        "... attempt=" +
                        i +
                        "/" +
                        MAX_ATTEMPTS
                );
            }
            if (!reservation || reservation.status !== "pending_bytes") {
                console.warn(
                    "[OSL] " +
                        oslLogTime() +
                        " step2 await timeout: upload_id=" +
                        uploadId.substring(0, 20) +
                        "... gave up after " +
                        (Date.now() - startMs) +
                        "ms"
                );
            }
        }

        if (!reservation || reservation.status !== "pending_bytes") {
            console.log(
                "[OSL] " +
                    oslLogTime() +
                    " step2 passthrough: upload_id=" +
                    uploadId.substring(0, 20) +
                    "... " +
                    (reservation
                        ? "status=" + reservation.status
                        : "no reservation")
            );
            return Reflect.apply(origSend, xhr, args);
        }

        const body = args[0];
        if (
            !body ||
            (typeof Blob !== "undefined" && !(body instanceof Blob) &&
                !(body instanceof ArrayBuffer) &&
                !ArrayBuffer.isView(body))
        ) {
            console.warn(
                "[OSL] step2: unexpected body type, passthrough; type=" +
                    (body && body.constructor && body.constructor.name)
            );
            return Reflect.apply(origSend, xhr, args);
        }

        // Read the original file bytes.
        let originalBytes;
        try {
            if (body instanceof Blob) {
                originalBytes = new Uint8Array(await body.arrayBuffer());
            } else if (body instanceof ArrayBuffer) {
                originalBytes = new Uint8Array(body);
            } else {
                originalBytes = new Uint8Array(body.buffer);
            }
        } catch (e) {
            window.__oslPendingUploads.delete(uploadId);
            return oslAbortXhr(xhr, "step2: arrayBuffer() failed: " + e);
        }

        // Base64-encode for IPC (chunked to avoid call-stack limit
        // on multi-MB strings).
        let binary = "";
        const CHUNK = 32 * 1024;
        for (let i = 0; i < originalBytes.length; i += CHUNK) {
            const slice = originalBytes.subarray(
                i,
                Math.min(i + CHUNK, originalBytes.length)
            );
            binary += String.fromCharCode.apply(null, slice);
        }
        const b64 = btoa(binary);

        // Phase 8d: use the combined seal command that embeds the
        // cover envelope INSIDE the file. JS never sees the per-
        // attachment AEAD key — it lives only inside the embedded
        // cover, encrypted to the scope's recipients. Step 3 just
        // needs to set message content to "" and rewrite the
        // filename; no separate cover-on-the-wire.
        const selfId = await oslSelfDiscordId();
        if (!selfId) {
            window.__oslPendingUploads.delete(uploadId);
            oslToast("Encryption failed, attachment not sent");
            return oslAbortXhr(xhr, "step2: no self_discord_id");
        }
        const sealRes = await oslInvoke("osl_seal_attachment_with_cover_v2", {
            scopeInput: reservation.scope,
            channelMembers: reservation.channelMembers,
            selfDiscordId: selfId,
            originalBytesB64: b64,
            originalFilename: reservation.originalFilename,
            randomFilename: reservation.randomFilename,
        });
        if (!sealRes.ok) {
            window.__oslPendingUploads.delete(uploadId);
            oslToast("Encryption failed, attachment not sent");
            return oslAbortXhr(
                xhr,
                "step2: seal_with_cover_v2 failed: " + sealRes.error
            );
        }
        const sealed = sealRes.value;
        const sealedBinary = atob(sealed.sealedB64);
        const sealedBytes = new Uint8Array(sealedBinary.length);
        for (let i = 0; i < sealedBinary.length; i++) {
            sealedBytes[i] = sealedBinary.charCodeAt(i);
        }
        if (sealedBytes.length > reservation.reservedSize) {
            window.__oslPendingUploads.delete(uploadId);
            oslToast("Encryption failed, attachment not sent");
            return oslAbortXhr(
                xhr,
                "step2: encrypted size " +
                    sealedBytes.length +
                    " exceeds reservation " +
                    reservation.reservedSize
            );
        }

        // Phase 8d: nothing for step 3 to look up via attKey — the
        // cover lives in the file. Flip status so step 3 can find
        // the reservation by uploaded_filename and set content="".
        reservation.mimeType = sealed.mimeType;
        reservation.encryptedSize = sealedBytes.length;
        reservation.status = "pending_cover";

        console.log(
            "[OSL] " +
                oslLogTime() +
                " step2 GCS PUT: upload_id=" +
                uploadId.substring(0, 20) +
                "... original_size=" +
                originalBytes.length +
                " encrypted_size=" +
                sealedBytes.length
        );

        // Replace the PUT body with our sealed bundle. GCS uses
        // octet-stream; preserve that content-type (Discord's XHR
        // already set it before send).
        const newBody = new Blob([sealedBytes], { type: "image/png" });
        return Reflect.apply(origSend, xhr, [newBody]);
    }

    /**
     * 8c step 3: build the v=2 attachment-envelope cover for a
     * /messages POST body. Inspects `parsed.attachments[]` for any
     * `uploaded_filename` matching a pending reservation; if any
     * match, returns `{ handled: true, promise }` where the promise
     * resolves to the mutated JSON body string. If no entries
     * match, returns null so the caller falls through to normal
     * text-encrypt.
     *
     * Phase 8d: the cover envelope now lives INSIDE each attachment
     * file (embedded by step 2 via osl_seal_attachment_with_cover_v2)
     * — so step 3 just blanks `parsed.content` and rewrites
     * `attachments[i].filename`. No envelope IPC. Non-OSL viewers
     * see only the decoy PNG and zero text.
     */
    function oslMaybeBuildAttachmentCover(parsed) {
        if (!parsed || !Array.isArray(parsed.attachments)) return null;
        if (parsed.attachments.length === 0) return null;

        const matched = [];
        parsed.attachments.forEach(function (att, i) {
            const ufn = att && (att.uploaded_filename || att.uploadedFilename);
            if (!ufn) return;
            const reservation = oslPendingUploadsByFilename(ufn);
            if (!reservation) return;
            if (reservation.status !== "pending_cover") return;
            matched.push({ index: i, reservation: reservation });
        });
        if (matched.length === 0) return null;

        // 8d: synchronous body mutation — no IPC for cover build.
        parsed.content = "";
        matched.forEach(function (m) {
            const att = parsed.attachments[m.index];
            if (att) {
                att.filename = m.reservation.randomFilename;
            }
        });
        // Consume the reservations.
        for (const m of matched) {
            for (const [k, v] of window.__oslPendingUploads.entries()) {
                if (v === m.reservation) {
                    window.__oslPendingUploads.delete(k);
                    break;
                }
            }
        }
        const randomNames = matched
            .map(function (m) {
                return m.reservation.randomFilename;
            })
            .join(",");
        console.log(
            "[OSL] " +
                oslLogTime() +
                " step3 /messages: attachments=" +
                parsed.attachments.length +
                " encrypted_count=" +
                matched.length +
                " random_filenames=[" +
                randomNames +
                "] cover=embedded-in-file"
        );
        return {
            handled: true,
            promise: Promise.resolve(JSON.stringify(parsed)),
        };
    }

    /**
     * Phase 8 DevTools-side end-to-end debug helper. Pick a File
     * (e.g. via a `<input type=file>` in DevTools) and call:
     *
     *     await window.__oslDebugUploadAttachment(channelId, file)
     *
     * The helper seals the file, sends the v=2 envelope as the
     * message content, and POSTs the sealed blob as a plain
     * multipart upload via the message-send endpoint (Discord's
     * legacy single-step path). Doesn't go through the new
     * pre-upload+CDN flow — that's where the production hook
     * needs to land — but it lets you verify recv-side decoding
     * end-to-end before the upload hook is wired.
     */
    window.__oslDebugUploadAttachment = async function (channelId, file) {
        if (!channelId || !file) {
            throw new Error(
                "usage: __oslDebugUploadAttachment(channelId, file)"
            );
        }
        const ctx = oslCurrentChannelContext();
        const scope = ctx && oslScopeForCurrentContext(ctx);
        if (!scope) {
            throw new Error("could not resolve current scope");
        }
        const selfId = await oslSelfDiscordId();
        if (!selfId) {
            throw new Error("could not resolve self discord id");
        }
        const members = (ctx.members || []).map(function (m) {
            return typeof m === "string" ? m : (m && (m.id || m.user_id)) || null;
        }).filter(Boolean);
        const sealed = await oslSealAttachmentForUpload(
            file,
            scope,
            members,
            selfId
        );
        console.log(
            "[OSL] debug-upload sealed file=" +
                sealed.originalFilename +
                " → upload=" +
                sealed.sealedFilename +
                " bytes=" +
                sealed.sealedBlob.size +
                " envelope_len=" +
                sealed.envelopeWire.length
        );
        // Build a multipart payload Discord accepts on the
        // /messages endpoint (legacy single-step upload).
        const fd = new FormData();
        const payload = {
            content: sealed.envelopeWire,
            attachments: [
                {
                    id: "0",
                    filename: sealed.sealedFilename,
                },
            ],
        };
        fd.append("payload_json", JSON.stringify(payload));
        fd.append("files[0]", sealed.sealedBlob, sealed.sealedFilename);
        const url =
            "https://discord.com/api/v9/channels/" + channelId + "/messages";
        const resp = await fetch(url, {
            method: "POST",
            credentials: "include",
            body: fd,
        });
        console.log(
            "[OSL] debug-upload posted message HTTP " + resp.status
        );
        return resp;
    };

    /**
     * 7d-PIVOT-FIX3 diagnostic: dump the composer toggle's attributes
     * + computed visual style for debug-time inspection from DevTools.
     * Returns an object so it round-trips through `JSON.stringify`
     * cleanly. Logs to console so a console-only inspection still
     * surfaces the data.
     */
    window.__oslDiagDumpToggle = function () {
        const btn = document.querySelector(
            "[" + COMPOSER_TOGGLE_DATA_ATTR + "='1']"
        );
        if (!btn) {
            console.log("[OSL][diag] toggle: not mounted");
            return { mounted: false };
        }
        const track = btn.querySelector("[data-osl-track='1']");
        const knob = btn.querySelector("[data-osl-knob='1']");
        const snapshot = {
            mounted: true,
            composer_attr: btn.getAttribute(COMPOSER_TOGGLE_DATA_ATTR),
            encrypt_state: btn.getAttribute("data-osl-encrypt-state"),
            aria_checked: btn.getAttribute("aria-checked"),
            track_background: track
                ? getComputedStyle(track).backgroundColor
                : null,
            knob_left: knob ? knob.style.left : null,
        };
        console.log("[OSL][diag] toggle:", snapshot);
        return snapshot;
    };

    /**
     * 7d-PIVOT-FIX3 diagnostic: dump the current __oslBurnedScopes
     * cache as a plain array. Useful for verifying Bug F's inline
     * unburn fired (cache should NOT contain the scope post-encrypt).
     */
    window.__oslDiagDumpBurned = function () {
        const map = window.__oslBurnedScopes;
        if (!(map instanceof Map)) {
            console.log("[OSL][diag] burned: cache not initialised");
            return [];
        }
        const keys = Array.from(map.keys());
        console.log(
            "[OSL][diag] burned (" + keys.length + " entries):",
            keys
        );
        return keys;
    };

    /**
     * Phase 8d diagnostic: everything we know about a message's
     * attachments. Cache hit, blob URLs, mime types, plus the rendered
     * DOM elements pointing at the Discord CDN. Useful from DevTools
     * when an attachment renders as the decoy instead of the original.
     */
    window.__oslDiagInspectAttachment = function (msgId) {
        if (!msgId) {
            console.log(
                "[OSL][diag] usage: __oslDiagInspectAttachment(msgId)"
            );
            return null;
        }
        const cacheEntry =
            window.__oslAttachmentDecrypted &&
            window.__oslAttachmentDecrypted.get(msgId);
        const li = document.querySelector(
            'li[id$="-' + msgId + '"]'
        );
        const cdnEls = li
            ? Array.from(
                  li.querySelectorAll(
                      "img[src*='discord'], video[src*='discord'], a[href*='discord']"
                  )
              ).map(function (el) {
                  const url =
                      el.tagName === "A"
                          ? el.getAttribute("href")
                          : el.getAttribute("src");
                  return {
                      tag: el.tagName.toLowerCase(),
                      url:
                          typeof url === "string"
                              ? url.substring(0, 120)
                              : null,
                  };
              })
            : [];
        const snapshot = {
            msgId: msgId,
            li_present: !!li,
            cache_hit: !!cacheEntry,
            cached_random_names: cacheEntry
                ? Object.keys(cacheEntry.byRandomName || {})
                : [],
            cache_size: window.__oslAttachmentDecrypted
                ? window.__oslAttachmentDecrypted.size
                : 0,
            cdn_elements: cdnEls,
        };
        console.log("[OSL][diag] attachment:", snapshot);
        return snapshot;
    };

    // ============================================================
    // Phase 7c: whitelist UI (profile button, channel-header toggle
    // + burn button, persistent invitation banner). No settings
    // menu, no keybinds — those are 7d.
    //
    // All selectors below match the survey in
    // `docs/phase-7c-selectors.md` via [class*="prefix"] so
    // hash-suffix rotation across Discord builds doesn't break us.
    //
    // Section layout:
    //   1. Toast + dialog helpers (shared by all surfaces).
    //   2. Current-channel-context helper (fiber walk).
    //   3. Profile popout/sidebar Whitelist button + scope dropdown.
    //   4. Channel header encrypt-toggle + burn button + modal.
    //   5. Pending-invitation banner system.
    //   6. Recv-path glue (toasts on burn/response, banner refresh).
    //   7. Install + sweep tick.
    // ============================================================

    // ---- Section 1: toast + dialog helpers ----

    /**
     * Phase 7c: fire a short bottom-right toast. Stacks multiple
     * toasts vertically (newest on top). Auto-dismiss after
     * `opts.durationMs` (default 3000ms).
     *
     * Returns the toast DOM element so the caller can dismiss
     * early via `el.remove()` if needed.
     */
    function oslToast(message, opts) {
        opts = opts || {};
        const durationMs =
            typeof opts.durationMs === "number" ? opts.durationMs : 3000;
        // Build the stack container lazily.
        let stack = document.getElementById("__osl_toast_stack");
        if (!stack) {
            stack = document.createElement("div");
            stack.id = "__osl_toast_stack";
            stack.style.position = "fixed";
            stack.style.bottom = "16px";
            stack.style.right = "16px";
            stack.style.display = "flex";
            stack.style.flexDirection = "column-reverse";
            stack.style.gap = "8px";
            stack.style.zIndex = "100000";
            stack.style.pointerEvents = "none";
            document.body.appendChild(stack);
        }
        const toast = document.createElement("div");
        toast.style.background =
            "var(--background-floating, #18191c)";
        toast.style.color = "var(--text-normal, #dbdee1)";
        toast.style.padding = "12px 16px";
        toast.style.borderRadius = "6px";
        toast.style.fontSize = "14px";
        toast.style.lineHeight = "1.4";
        toast.style.maxWidth = "360px";
        toast.style.boxShadow = "0 4px 12px rgba(0, 0, 0, 0.32)";
        toast.style.pointerEvents = "auto";
        toast.textContent = String(message);
        stack.appendChild(toast);
        nativeSetTimeout(function () {
            if (toast.parentNode) toast.parentNode.removeChild(toast);
        }, durationMs);
        return toast;
    }

    /**
     * Phase 7c: modal confirmation dialog. Returns a Promise that
     * resolves to `true` on confirm, `false` on cancel /
     * backdrop click / Escape. Used by the burn-button flow.
     *
     * `opts`:
     *   - title       : string
     *   - body        : string (multi-line OK; rendered in a <p>)
     *   - confirmText : string ("Burn")
     *   - cancelText  : string ("Cancel")
     *   - danger      : bool — colours the confirm button red
     */
    function oslConfirm(opts) {
        return new Promise(function (resolve) {
            const backdrop = document.createElement("div");
            backdrop.style.position = "fixed";
            backdrop.style.inset = "0";
            backdrop.style.background = "rgba(0, 0, 0, 0.5)";
            backdrop.style.zIndex = "100000";
            backdrop.style.display = "flex";
            backdrop.style.alignItems = "center";
            backdrop.style.justifyContent = "center";

            const modal = document.createElement("div");
            modal.style.background =
                "var(--background-floating, #18191c)";
            modal.style.color = "var(--text-normal, #dbdee1)";
            modal.style.padding = "20px";
            modal.style.borderRadius = "8px";
            modal.style.maxWidth = "400px";
            modal.style.boxShadow = "0 8px 24px rgba(0, 0, 0, 0.5)";
            modal.style.fontSize = "14px";
            modal.style.lineHeight = "1.4";

            const title = document.createElement("h3");
            title.style.margin = "0 0 8px 0";
            title.style.fontSize = "18px";
            title.style.fontWeight = "600";
            title.textContent = opts.title || "Are you sure?";
            modal.appendChild(title);

            const body = document.createElement("p");
            body.style.margin = "0 0 16px 0";
            body.textContent = opts.body || "";
            modal.appendChild(body);

            const row = document.createElement("div");
            row.style.display = "flex";
            row.style.justifyContent = "flex-end";
            row.style.gap = "8px";

            const cancel = document.createElement("button");
            cancel.textContent = opts.cancelText || "Cancel";
            cancel.style.padding = "6px 14px";
            cancel.style.borderRadius = "4px";
            cancel.style.border = "1px solid var(--background-modifier-accent, #4f545c)";
            cancel.style.background = "transparent";
            cancel.style.color = "inherit";
            cancel.style.cursor = "pointer";
            cancel.style.fontSize = "14px";

            const confirm = document.createElement("button");
            confirm.textContent = opts.confirmText || "Confirm";
            confirm.style.padding = "6px 14px";
            confirm.style.borderRadius = "4px";
            confirm.style.border = "none";
            confirm.style.background = opts.danger
                ? "#ed4245"
                : "var(--brand-560, #5865f2)";
            confirm.style.color = "white";
            confirm.style.cursor = "pointer";
            confirm.style.fontSize = "14px";
            confirm.style.fontWeight = "500";

            const close = function (result) {
                document.removeEventListener("keydown", onKey, true);
                if (backdrop.parentNode)
                    backdrop.parentNode.removeChild(backdrop);
                resolve(result);
            };
            const onKey = function (e) {
                if (e.key === "Escape") {
                    e.preventDefault();
                    e.stopPropagation();
                    close(false);
                }
            };
            cancel.addEventListener("click", function () {
                close(false);
            });
            confirm.addEventListener("click", function () {
                close(true);
            });
            backdrop.addEventListener("click", function (e) {
                if (e.target === backdrop) close(false);
            });
            document.addEventListener("keydown", onKey, true);

            row.appendChild(cancel);
            row.appendChild(confirm);
            modal.appendChild(row);
            backdrop.appendChild(modal);
            document.body.appendChild(backdrop);
        });
    }

    // ---- Section 2: current-channel-context helper ----

    /**
     * Phase 7c: walk the React fiber from the channel header (or
     * any rendered message div) to recover:
     *   - channelId   : the Discord channel snowflake
     *   - channelType : 0 server text, 1 DM, 3 GC
     *   - guildId     : populated for server channels, null otherwise
     *   - members     : array of Discord user_ids in this channel
     *                   (DM/GC: from channel.recipients; server: empty
     *                    for 7c since we don't enumerate the members
     *                    panel here — Task 4 send path skips v=2 for
     *                    server channels until members are populated)
     *   - selfId      : the current user's Discord id (walked from
     *                   the same anchor; usually surfaces a few
     *                   frames up in a session / authentication
     *                   provider)
     *
     * Returns `null` if no anchor is mounted (e.g. settings open).
     */
    function oslCurrentChannelContext() {
        const anchor =
            document.querySelector(
                'section[class*="title_"][class*="container__"]'
            ) || document.querySelector('[id^="message-content-"]');
        if (!anchor) return null;

        let fiber;
        try {
            const key = Object.keys(anchor).find(function (k) {
                return k.indexOf("__reactFiber") === 0;
            });
            fiber = key ? anchor[key] : null;
        } catch (e) {
            return null;
        }
        if (!fiber) return null;

        let channelId = null;
        let channelType = null;
        let guildId = null;
        let members = null;
        let selfId = null;
        let f = fiber;
        for (let depth = 0; depth < 30 && f; depth++) {
            try {
                const p = f.memoizedProps;
                if (p && typeof p === "object") {
                    if (channelId == null && typeof p.channelId === "string") {
                        channelId = p.channelId;
                    }
                    if (p.channel && typeof p.channel === "object") {
                        if (channelId == null && typeof p.channel.id === "string") {
                            channelId = p.channel.id;
                        }
                        if (
                            channelType == null &&
                            typeof p.channel.type === "number"
                        ) {
                            channelType = p.channel.type;
                        }
                        if (guildId == null && typeof p.channel.guild_id === "string") {
                            guildId = p.channel.guild_id;
                        }
                        if (
                            members == null &&
                            Array.isArray(p.channel.recipients)
                        ) {
                            members = p.channel.recipients.slice();
                        }
                    }
                    if (guildId == null && typeof p.guildId === "string") {
                        guildId = p.guildId;
                    }
                    if (
                        selfId == null &&
                        p.currentUser &&
                        typeof p.currentUser.id === "string"
                    ) {
                        selfId = p.currentUser.id;
                    }
                }
            } catch (e) {
                // keep walking
            }
            f = f.return;
        }
        // Fallback: pull selfId from any rendered self-attribute on
        // the user area at the bottom-left. We never block on this —
        // commands that need selfDiscordId surface a clear error.
        return {
            channelId: channelId,
            channelType: channelType,
            guildId: guildId,
            members: members || [],
            selfId: selfId,
        };
    }

    /**
     * Phase 7c: human-readable scope label for UI strings.
     */
    function oslScopeLabel(scopeInput) {
        if (!scopeInput) return "this scope";
        switch (scopeInput.kind) {
            case "dm":
                return "DM";
            case "gc":
                return "this group chat";
            case "server_channel":
                return "this channel";
            case "server_full":
                return "this server";
            default:
                return "this scope";
        }
    }

    /**
     * Phase 7c: invoke a Tauri command with a uniform error
     * shape. Returns `{ ok: true, value }` or `{ ok: false, error }`.
     */
    async function oslInvoke(name, args) {
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") {
            return { ok: false, error: "no_invoke" };
        }
        try {
            const value = await invoke(name, args || {});
            return { ok: true, value: value };
        } catch (err) {
            const msg = err && err.message ? err.message : String(err);
            return { ok: false, error: msg };
        }
    }

    // ---- Section 3: profile popout/sidebar Whitelist button ----

    const PROFILE_BUTTON_DATA_ATTR = "data-osl-whitelist-btn";

    /**
     * SVG lock icon used by the Whitelist button + encrypt toggle.
     * `state` is "open" or "closed". Renders at 16x16; inherits
     * currentColor so theme-aware CSS variables apply.
     */
    function oslLockSvg(state) {
        const open = state === "open";
        return (
            '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" ' +
            'stroke="currentColor" stroke-width="2" stroke-linecap="round" ' +
            'stroke-linejoin="round" aria-hidden="true">' +
            '<rect x="4" y="11" width="16" height="10" rx="2"/>' +
            (open
                ? '<path d="M8 11V7a4 4 0 0 1 8 0"/>'
                : '<path d="M8 11V7a4 4 0 0 1 8 0v4"/>') +
            "</svg>"
        );
    }

    /**
     * SVG flame icon for the burn button.
     */
    function oslFlameSvg() {
        return (
            '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" ' +
            'stroke="currentColor" stroke-width="2" stroke-linecap="round" ' +
            'stroke-linejoin="round" aria-hidden="true">' +
            '<path d="M8.5 14.5A2.5 2.5 0 0 0 11 17c1.38 0 2.5-1 2.5-2.5 ' +
            "0-1.5-1-2.5-1-4 0-1.5 1-2 1-3 0-1.5-1-2.5-2.5-2.5C9 5 8 6.5 8 8c0 " +
            "1 .5 2 .5 3 0 1-.5 2-.5 3.5z\"/>" +
            '<path d="M12 2c1 3 3 4 3 7a3 3 0 0 1-6 0c0-1 1-2 1-4-2 1.5-4 4-4 ' +
            "7a7 7 0 0 0 14 0c0-5-4-7-8-10z\"/>" +
            "</svg>"
        );
    }

    function oslFindProfileSurface() {
        return document.querySelector(
            '[class*="user-profile-sidebar"], [class*="user-profile-popout"]'
        );
    }

    function oslExtractUserFromProfile(surfaceEl) {
        try {
            const key = Object.keys(surfaceEl).find(function (k) {
                return k.indexOf("__reactFiber") === 0;
            });
            let fiber = key ? surfaceEl[key] : null;
            for (let d = 0; d < 30 && fiber; d++) {
                const p = fiber.memoizedProps;
                if (p && p.user && typeof p.user.id === "string") {
                    return {
                        id: p.user.id,
                        username:
                            typeof p.user.username === "string"
                                ? p.user.username
                                : p.user.global_name ||
                                  p.user.id,
                    };
                }
                fiber = fiber.return;
            }
        } catch (e) {
            // fall through
        }
        return null;
    }

    function oslInjectProfileButton(surfaceEl) {
        if (!surfaceEl) return;
        if (surfaceEl.querySelector("[" + PROFILE_BUTTON_DATA_ATTR + "='1']")) {
            return; // already injected
        }
        // The action-button banner row uses .wrapper_da5890 in the
        // surveyed build (2026-05-11, build 541436). Prefix-match
        // against `wrapper_` to absorb hash rotation. If multiple
        // wrappers exist, pick the one that contains the Friend
        // banner button (`bannerButton_` prefix).
        const wrappers = surfaceEl.querySelectorAll(
            '[class*="wrapper_"]'
        );
        let banner = null;
        for (const w of wrappers) {
            if (w.querySelector('[class*="bannerButton_"]')) {
                banner = w;
                break;
            }
        }
        if (!banner) {
            // Banner row not present yet — observer will retry on
            // next mutation. Log once for diagnosis.
            console.log(
                "[OSL] profile injection deferred: no bannerButton wrapper yet"
            );
            return;
        }
        const sample = banner.querySelector('[class*="bannerButton_"]');
        const btn = document.createElement("div");
        btn.setAttribute("role", "button");
        btn.setAttribute("tabindex", "0");
        btn.setAttribute("aria-label", "Whitelist with OSL");
        btn.setAttribute(PROFILE_BUTTON_DATA_ATTR, "1");
        // Mirror the Friend/More button's class list so Discord's
        // existing CSS handles hover / focus / sizing for us.
        btn.className = sample ? sample.className : "";
        btn.style.cursor = "pointer";
        btn.innerHTML = oslLockSvg("closed");
        btn.addEventListener("click", function (e) {
            e.preventDefault();
            e.stopPropagation();
            const user = oslExtractUserFromProfile(surfaceEl);
            if (!user) {
                oslToast("OSL: could not resolve user id from profile");
                return;
            }
            oslOpenScopeDropdown(btn, user);
        });
        banner.appendChild(btn);
        console.log(
            "[OSL] profile whitelist button injected (peer id resolved at click time)"
        );
    }

    /**
     * Phase 7c: scope-pick dropdown anchored beneath the Whitelist
     * button. Options vary by current channel context (DM is
     * always shown; GC / server options surface only when
     * applicable per design doc §6.1).
     */
    function oslOpenScopeDropdown(anchorBtn, user) {
        // Close any existing dropdown first.
        const existing = document.getElementById("__osl_scope_dropdown");
        if (existing) existing.remove();

        const ctx = oslCurrentChannelContext();
        const dd = document.createElement("div");
        dd.id = "__osl_scope_dropdown";
        const rect = anchorBtn.getBoundingClientRect();
        dd.style.position = "fixed";
        dd.style.top = rect.bottom + 6 + "px";
        dd.style.left = Math.min(rect.left, window.innerWidth - 280) + "px";
        dd.style.background =
            "var(--background-tertiary, #1e1f22)";
        dd.style.color = "var(--text-normal, #dbdee1)";
        dd.style.borderRadius = "6px";
        dd.style.boxShadow = "0 4px 12px rgba(0, 0, 0, 0.32)";
        dd.style.padding = "8px 0";
        dd.style.minWidth = "240px";
        dd.style.maxWidth = "320px";
        dd.style.zIndex = "100000";
        dd.style.fontSize = "14px";
        dd.style.lineHeight = "1.4";

        // Header showing the user we're whitelisting.
        const head = document.createElement("div");
        head.style.padding = "4px 12px 8px";
        head.style.borderBottom =
            "1px solid var(--background-modifier-accent, #2e3035)";
        head.style.color = "var(--text-muted, #b5bac1)";
        head.style.fontSize = "12px";
        head.textContent = "User: " + user.username + " (" + user.id + ")";
        dd.appendChild(head);

        // Build the options list per context.
        const options = [];
        // DM is always available.
        options.push({
            label: "Whitelist in DM",
            kind: "dm",
            scopeInput: { kind: "dm", id: user.id, channel_id: user.id },
            broadenCheckbox: true,
        });
        if (ctx) {
            if (ctx.channelType === 3 && ctx.channelId) {
                options.push({
                    label: "Whitelist in this group chat",
                    kind: "gc",
                    scopeInput: {
                        kind: "gc",
                        id: ctx.channelId,
                        channel_id: ctx.channelId,
                    },
                });
            }
            if (ctx.channelType === 0 && ctx.channelId && ctx.guildId) {
                options.push({
                    label: "Whitelist in this channel",
                    kind: "server_channel",
                    scopeInput: {
                        kind: "server_channel",
                        id: ctx.guildId + ":" + ctx.channelId,
                        server_id: ctx.guildId,
                        channel_id: ctx.channelId,
                    },
                });
                options.push({
                    label: "Whitelist in entire server",
                    kind: "server_full",
                    scopeInput: {
                        kind: "server_full",
                        id: ctx.guildId,
                        server_id: ctx.guildId,
                    },
                });
            }
        }

        let broadenChecked = false;
        for (const opt of options) {
            const row = document.createElement("div");
            row.style.padding = "8px 12px";
            row.style.cursor = "pointer";
            row.style.display = "flex";
            row.style.alignItems = "center";
            row.style.gap = "8px";
            row.addEventListener("mouseenter", function () {
                row.style.background =
                    "var(--background-modifier-hover, #4e5058)";
            });
            row.addEventListener("mouseleave", function () {
                row.style.background = "transparent";
            });
            const label = document.createElement("span");
            label.textContent = opt.label;
            label.style.flex = "1";
            row.appendChild(label);

            if (opt.broadenCheckbox) {
                const cbWrap = document.createElement("label");
                cbWrap.style.display = "inline-flex";
                cbWrap.style.alignItems = "center";
                cbWrap.style.gap = "4px";
                cbWrap.style.fontSize = "12px";
                cbWrap.style.color =
                    "var(--text-muted, #b5bac1)";
                cbWrap.style.cursor = "pointer";
                const cb = document.createElement("input");
                cb.type = "checkbox";
                cb.style.cursor = "pointer";
                cb.addEventListener("change", function () {
                    broadenChecked = !!cb.checked;
                });
                cb.addEventListener("click", function (e) {
                    e.stopPropagation();
                });
                cbWrap.appendChild(cb);
                const cbLabel = document.createElement("span");
                cbLabel.textContent = "broaden";
                cbWrap.appendChild(cbLabel);
                row.appendChild(cbWrap);
            }

            row.addEventListener("click", async function () {
                close();
                await oslSendWhitelistInvitation(
                    user,
                    opt.scopeInput,
                    opt.kind === "dm" ? broadenChecked : false,
                    ctx
                );
            });
            dd.appendChild(row);
        }

        // Cancel.
        const cancel = document.createElement("div");
        cancel.style.padding = "8px 12px";
        cancel.style.cursor = "pointer";
        cancel.style.color =
            "var(--text-muted, #b5bac1)";
        cancel.style.borderTop =
            "1px solid var(--background-modifier-accent, #2e3035)";
        cancel.textContent = "Cancel";
        cancel.addEventListener("mouseenter", function () {
            cancel.style.background =
                "var(--background-modifier-hover, #4e5058)";
        });
        cancel.addEventListener("mouseleave", function () {
            cancel.style.background = "transparent";
        });
        cancel.addEventListener("click", function () {
            close();
        });
        dd.appendChild(cancel);

        const close = function () {
            document.removeEventListener("mousedown", outsideClick, true);
            document.removeEventListener("keydown", onKey, true);
            if (dd.parentNode) dd.parentNode.removeChild(dd);
        };
        const outsideClick = function (e) {
            if (!dd.contains(e.target) && e.target !== anchorBtn) close();
        };
        const onKey = function (e) {
            if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                close();
            }
        };
        nativeSetTimeout(function () {
            document.addEventListener("mousedown", outsideClick, true);
            document.addEventListener("keydown", onKey, true);
        }, 0);

        document.body.appendChild(dd);
    }

    /**
     * Phase 7c: send a whitelist invitation. Issues
     * `osl_set_whitelist` (Rust mutates local state + returns the
     * wire) then `oslSendControlMessage` (Discord delivery).
     */
    async function oslSendWhitelistInvitation(user, scopeInput, broadened, ctx) {
        const selfId = await oslSelfDiscordId();
        if (!selfId) {
            oslToast(
                "OSL: could not resolve your Discord id (identity not loaded?)"
            );
            return;
        }
        const setResult = await oslInvoke("osl_set_whitelist", {
            peerDiscordId: user.id,
            scopeInput: scopeInput,
            broadened: broadened,
            fromDiscordId: selfId,
        });
        if (!setResult.ok) {
            oslToast("OSL: whitelist failed: " + setResult.error);
            console.log(
                "[OSL] osl_set_whitelist FAIL user=" +
                    user.id +
                    " reason=" +
                    setResult.error
            );
            return;
        }
        const wire = setResult.value;
        // For DM scope: deliver via the DM channel (channelId IS the
        // user id in our representation; Discord's DM channel id is
        // different — fall back to the current channelId if it
        // matches a DM with this user, else log a deferral).
        const deliveryChannelId =
            scopeInput.kind === "dm"
                ? ctx.channelId
                : ctx.channelId;
        if (!deliveryChannelId) {
            oslToast(
                "OSL: invitation wire built but no delivery channel; open the target channel and re-invite"
            );
            console.log(
                "[OSL] invitation queued (no delivery channel) user=" +
                    user.id
            );
            return;
        }
        await oslSendControlMessage(deliveryChannelId, wire);
        // 7d-FIX1 decision-B: the Rust set_whitelist call above
        // auto-removed the scope from the burned-scopes ledger; mirror
        // that to the in-memory JS cache so the recv observer
        // immediately resumes decrypting (new) messages in this
        // scope on the next sweep tick. Old ciphertext stays
        // unreadable (wrapped_keys gone).
        try {
            oslBurnedScopesRemove(scopeInput.kind, scopeInput.id);
            console.log(
                "[OSL][burn] scope " +
                    scopeInput.kind +
                    ":" +
                    scopeInput.id +
                    " re-whitelisted, removed from burned list"
            );
        } catch (_) {}
        oslToast("Invitation sent to " + user.username);
        // Refresh the channel header (encrypt toggle may have
        // become available).
        oslRefreshHeaderState();
    }

    // ---- Section 4: channel header encrypt toggle + burn ----

    const HEADER_ENCRYPT_DATA_ATTR = "data-osl-encrypt-toggle";
    const HEADER_BURN_DATA_ATTR = "data-osl-burn-btn";

    /** Last-known scope state for the header (so toggle clicks have
     *  current values without round-tripping every time). */
    let oslHeaderState = {
        scopeKey: null,
        encryptToggle: false,
        hasWhitelist: false,
    };

    function oslFindHeaderIconContainer(header) {
        const firstIcon = header.querySelector('[class*="iconWrapper__"]');
        return firstIcon && firstIcon.parentElement
            ? firstIcon.parentElement
            : null;
    }

    // 7d-D: account burn icon, distinct from the scope-burn button
    // above. Account burn destroys ALL OSL state via `osl_burn_engage`,
    // not just one scope. Appears at the RIGHTMOST end of the header
    // icon row on ALL channel types (DM, GC, server channel). Confirm
    // flow is a 3-second double-tap (Q2-C decision): first click arms
    // + shows a tooltip + tints the icon red; second click within
    // the window executes the burn.
    const HEADER_ACCOUNT_BURN_DATA_ATTR = "data-osl-account-burn";
    const ACCOUNT_BURN_TOOLTIP_ID = "__osl_account_burn_tooltip";
    const ACCOUNT_BURN_ARM_MS = 3000;
    let oslAccountBurnArmed = false;
    let oslAccountBurnArmTimer = null;
    let oslAccountBurnCancelHandler = null;

    /**
     * 7d-D: bigger flame, sized to match Discord's other header
     * icons (20x20 displayed, viewBox 24). Kept as currentColor so
     * we can swap the icon's color when armed without re-rendering.
     */
    function oslAccountBurnSvg() {
        return (
            '<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" ' +
            'aria-hidden="true">' +
            '<path d="M12 2.5c.4 1.6 1.6 2.7 2.6 3.6 1.6 1.5 2.9 3 2.9 5.6 ' +
            "0 1.4-.5 2.6-1.4 3.4.4-.6.6-1.3.6-2.1 0-2-1.6-3.4-3.3-4.3.4 1.4 " +
            "-.1 2.8-1 3.8-.7.7-1.4 1-1.4 1.8 0 .8.7 1.3 1.5 1.3.6 0 1.1-.2 " +
            "1.5-.6-.3 1.4-1.5 2.3-3 2.3-1.9 0-3.4-1.4-3.4-3.3 0-1.7 1.2-2.6 " +
            "2.4-3.6 1.4-1.2 2.5-2.6 2-4.8-.7 1.1-1.9 1.6-3.1 1.6.5-1.8.4-3.7" +
            '-.4-5.3-.7 1.7-2 3-3.5 4.1A6.6 6.6 0 0 0 4.5 12c0 3.9 3.2 7 7.5 ' +
            '7s7.5-3.1 7.5-7c0-3.6-1.7-5.6-3.5-7.2-1.4-1.3-2.7-2.5-4-4.3z"/>' +
            "</svg>"
        );
    }

    function oslAccountBurnRemoveTooltip() {
        const t = document.getElementById(ACCOUNT_BURN_TOOLTIP_ID);
        if (t && t.parentNode) t.parentNode.removeChild(t);
    }

    function oslAccountBurnSetVisual(btn, armed) {
        if (!btn) return;
        if (armed) {
            btn.style.color = "var(--status-danger, #ed4245)";
        } else {
            btn.style.color = "var(--interactive-normal, #b5bac1)";
        }
    }

    function oslAccountBurnCancelArm() {
        oslAccountBurnArmed = false;
        if (oslAccountBurnArmTimer) {
            clearTimeout(oslAccountBurnArmTimer);
            oslAccountBurnArmTimer = null;
        }
        oslAccountBurnRemoveTooltip();
        if (oslAccountBurnCancelHandler) {
            document.removeEventListener(
                "click",
                oslAccountBurnCancelHandler,
                true
            );
            oslAccountBurnCancelHandler = null;
        }
        const btn = document.querySelector(
            "[" + HEADER_ACCOUNT_BURN_DATA_ATTR + "='1']"
        );
        oslAccountBurnSetVisual(btn, false);
    }

    function oslAccountBurnShowTooltip(anchor) {
        oslAccountBurnRemoveTooltip();
        if (!anchor) return;
        const rect = anchor.getBoundingClientRect();
        const tt = document.createElement("div");
        tt.id = ACCOUNT_BURN_TOOLTIP_ID;
        tt.textContent = "Click again within 3 seconds to burn account";
        tt.style.position = "fixed";
        tt.style.top = rect.bottom + 6 + "px";
        // Slightly left of the icon so the tooltip stays on-screen
        // when the icon is near the right edge of the channel header.
        tt.style.left = Math.max(8, rect.left - 200) + "px";
        tt.style.background = "#18191c";
        tt.style.color = "#fff";
        tt.style.padding = "8px 10px";
        tt.style.borderRadius = "4px";
        tt.style.fontSize = "12px";
        tt.style.fontWeight = "500";
        tt.style.boxShadow = "0 4px 12px rgba(0,0,0,0.4)";
        tt.style.zIndex = "1000000";
        tt.style.pointerEvents = "none";
        tt.style.whiteSpace = "nowrap";
        document.body.appendChild(tt);
    }

    async function oslAccountBurnExecute() {
        // 7d-D Task 5: close the settings window first so it doesn't
        // end up orphaned reading from now-wiped state files.
        try {
            await oslInvoke("osl_close_settings_window_if_open", {});
        } catch (_) {}
        const result = await oslInvoke("osl_burn_engage", {});
        if (!result.ok) {
            console.error(
                "[OSL] account burn: osl_burn_engage failed: " + result.error
            );
            oslToast("Burn failed: " + result.error);
            return;
        }
        // Same post-burn navigation as the gate-side burn flow:
        // bounce to plain discord.com so the freshly-wiped on-disk
        // state has no UI re-attaching to it on this tick.
        window.location.href = "https://discord.com/app";
    }

    function oslAccountBurnOnActivate(btn) {
        if (oslAccountBurnArmed) {
            // Second tap within the window — execute.
            oslAccountBurnCancelArm();
            oslAccountBurnExecute();
            return;
        }
        oslAccountBurnArmed = true;
        oslAccountBurnSetVisual(btn, true);
        oslAccountBurnShowTooltip(btn);
        oslAccountBurnArmTimer = setTimeout(function () {
            oslAccountBurnCancelArm();
        }, ACCOUNT_BURN_ARM_MS);
        // Capture-phase document click: if user clicks anywhere
        // that isn't the icon, cancel the arm. Idempotent setup.
        oslAccountBurnCancelHandler = function (e) {
            const onIcon =
                e.target &&
                (e.target === btn ||
                    (typeof e.target.closest === "function" &&
                        e.target.closest(
                            "[" + HEADER_ACCOUNT_BURN_DATA_ATTR + "='1']"
                        )));
            if (onIcon) return;
            oslAccountBurnCancelArm();
        };
        document.addEventListener(
            "click",
            oslAccountBurnCancelHandler,
            true
        );
    }

    function oslAccountBurnInject(header) {
        if (!header) return;
        const container = oslFindHeaderIconContainer(header);
        if (!container) return;
        // Idempotent: bail if the account burn already exists.
        if (
            container.querySelector(
                "[" + HEADER_ACCOUNT_BURN_DATA_ATTR + "='1']"
            )
        ) {
            return;
        }
        const sample = header.querySelector('[class*="iconWrapper__"]');
        const sampleClass = sample ? sample.className : "";
        const btn = document.createElement("div");
        btn.setAttribute("role", "button");
        btn.setAttribute("tabindex", "0");
        btn.setAttribute(HEADER_ACCOUNT_BURN_DATA_ATTR, "1");
        btn.setAttribute("aria-label", "Account Burn");
        btn.title = "Account Burn — double-tap to destroy all OSL data";
        btn.className = sampleClass;
        btn.style.display = "inline-flex";
        btn.style.alignItems = "center";
        btn.style.justifyContent = "center";
        btn.style.cursor = "pointer";
        oslAccountBurnSetVisual(btn, false);
        btn.addEventListener("mouseenter", function () {
            if (!oslAccountBurnArmed) {
                btn.style.color = "var(--interactive-hover, #dbdee1)";
            }
        });
        btn.addEventListener("mouseleave", function () {
            if (!oslAccountBurnArmed) {
                oslAccountBurnSetVisual(btn, false);
            }
        });
        btn.innerHTML = oslAccountBurnSvg();
        btn.addEventListener("click", function (e) {
            e.preventDefault();
            e.stopPropagation();
            oslAccountBurnOnActivate(btn);
        });
        btn.addEventListener("keydown", function (e) {
            if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                e.stopPropagation();
                oslAccountBurnOnActivate(btn);
            }
        });
        // Rightmost position: append as last child of the icon row.
        // Discord's search box lives in a sibling container after
        // this one, so appending here keeps the icon inside the icon
        // group and before the search.
        container.appendChild(btn);
    }

    // ---- 7d-PIVOT: composer encrypt toggle ----
    //
    // Pill button injected into Discord's composer toolbar. The pill
    // is the new control surface for the per-scope `encrypt_toggle`
    // (decoupled from whitelist in PIVOT — turning ON without a
    // whitelist means "encrypt-to-self-only"). The previous
    // channel-header lock stays put as a separate, view-only
    // indicator of whitelist coverage.
    //
    // Injection strategy: fallback inline-composer position (Task
    // 2c). The floating-strip-above-composer position (Task 2b)
    // turned out to fight Discord's typing-indicator container on
    // some channel layouts (different React tree depending on
    // server vs DM); inline-toolbar is consistent across all
    // channel kinds.
    const COMPOSER_TOGGLE_DATA_ATTR = "data-osl-composer-encrypt";

    function oslFindComposerToolbar() {
        // The composer form has a buttons__ row holding GIF / gift /
        // sticker / emoji icons. We prepend our pill there.
        // Selector targets a stable suffix pattern Discord ships.
        return (
            document.querySelector(
                'form[class*="form_"] [class*="buttons__"]'
            ) ||
            document.querySelector('[class*="channelTextArea_"] [class*="buttons__"]')
        );
    }

    // 7d-PIVOT-FIX: iOS-style toggle visual. Track + sliding knob,
    // no text inside. Two nested divs we update in place — keeping
    // the same outer `btn` keeps the click handler bound stable.
    function oslComposerToggleStyle(btn, on) {
        // Lazily create the inner track + knob on first paint.
        let track = btn.querySelector("[data-osl-track='1']");
        let knob = btn.querySelector("[data-osl-knob='1']");
        if (!track) {
            track = document.createElement("div");
            track.setAttribute("data-osl-track", "1");
            track.style.position = "relative";
            track.style.width = "36px";
            track.style.height = "20px";
            track.style.borderRadius = "10px";
            track.style.transition = "background 0.15s ease-out";
            track.style.flexShrink = "0";
            knob = document.createElement("div");
            knob.setAttribute("data-osl-knob", "1");
            knob.style.position = "absolute";
            knob.style.top = "2px";
            knob.style.width = "16px";
            knob.style.height = "16px";
            knob.style.borderRadius = "50%";
            knob.style.background = "#ffffff";
            knob.style.boxShadow = "0 1px 2px rgba(0,0,0,0.30)";
            knob.style.transition = "left 0.15s ease-out";
            track.appendChild(knob);
            // Replace whatever text was previously in `btn` (the
            // pre-FIX visual was "ENC ON"/"ENC OFF" text) with the
            // track. Keep btn as inline-flex so the track centers.
            btn.textContent = "";
            btn.appendChild(track);
        }
        // 7d-PIVOT-FIX3 Bug E: `data-osl-encrypt-state` is the
        // authoritative source for the send gate. Set it BEFORE the
        // visual changes so a racing send between the click handler
        // and the next paint frame still observes the new state.
        // aria-checked is kept in sync for accessibility, but the
        // send path no longer trusts it (Discord/React occasionally
        // reset aria-* on the wrapping switch element).
        btn.setAttribute("data-osl-encrypt-state", on ? "on" : "off");
        if (on) {
            track.style.background = "#3ba55d"; // Discord green
            knob.style.left = "18px";
            btn.setAttribute("aria-checked", "true");
            btn.title =
                "Encryption ON for this channel — click to disable";
        } else {
            track.style.background = "var(--background-modifier-accent, #3a3c43)";
            knob.style.left = "2px";
            btn.setAttribute("aria-checked", "false");
            btn.title =
                "Encryption OFF — click to enable (encrypt-to-self if no whitelist)";
        }
    }

    // 7d-PIVOT-FIX: track last-seen scope storage_key so we only
    // fire osl_get_scope_encryption_state on actual scope changes,
    // not on every header-observer tick (Discord fires hundreds
    // per second during a send + the IPC round-trip was the cause
    // of "slow send" symptoms in 7d-PIVOT).
    let oslComposerToggleLastScopeKey = null;
    async function oslComposerToggleRefresh(btn, opts) {
        const ctx = oslCurrentChannelContext();
        if (!ctx) return;
        const scope = oslScopeForCurrentContext(ctx);
        if (!scope) return;
        const key = oslScopeStorageKey(scope);
        const force = opts && opts.force === true;
        if (!force && key === oslComposerToggleLastScopeKey) {
            return;
        }
        oslComposerToggleLastScopeKey = key;
        const r = await oslInvoke("osl_get_scope_encryption_state", {
            scopeInput: scope,
        });
        if (!r.ok) return;
        oslComposerToggleStyle(btn, !!(r.value && r.value.encrypt_toggle));
    }

    async function oslComposerToggleOnClick(btn) {
        const ctx = oslCurrentChannelContext();
        if (!ctx) {
            oslToast("Cannot resolve channel scope");
            return;
        }
        const scope = oslScopeForCurrentContext(ctx);
        if (!scope) {
            oslToast("Cannot resolve channel scope");
            return;
        }
        // 7d-PIVOT-FIX: derive desired state from the toggle's
        // current visual instead of round-tripping to Rust just
        // to flip it. Visual is the source of truth — set
        // optimistically + write through to disk.
        // 7d-PIVOT-FIX3 Bug E: read `data-osl-encrypt-state` for
        // the same reason the send path does — aria-checked turned
        // out to be unreliable. Falls back to aria-checked only if
        // the data attr is missing (pre-FIX3 install path).
        const stateAttr = btn.getAttribute("data-osl-encrypt-state");
        const currentOn =
            stateAttr === "on" ||
            (stateAttr === null && btn.getAttribute("aria-checked") === "true");
        oslComposerToggleStyle(btn, !currentOn); // optimistic
        const set = await oslInvoke("osl_set_scope_encrypt", {
            scopeInput: scope,
            enabled: !currentOn,
        });
        if (!set.ok) {
            oslComposerToggleStyle(btn, currentOn); // rollback
            oslToast("Encrypt toggle failed: " + set.error);
            return;
        }
        // Confirm visual matches the result (no-op if optimistic
        // matched).
        oslComposerToggleStyle(btn, !!set.value);
        // Refresh the header lock too — coverage display may shift.
        try {
            oslRefreshHeaderState();
        } catch (_) {}
    }

    function oslComposerToggleInject() {
        const toolbar = oslFindComposerToolbar();
        if (!toolbar) return;
        // Idempotency.
        if (
            toolbar.querySelector(
                "[" + COMPOSER_TOGGLE_DATA_ATTR + "='1']"
            )
        ) {
            return;
        }
        const btn = document.createElement("div");
        btn.setAttribute("role", "switch");
        btn.setAttribute("tabindex", "0");
        btn.setAttribute(COMPOSER_TOGGLE_DATA_ATTR, "1");
        btn.setAttribute("aria-label", "Encrypt messages in this channel");
        btn.setAttribute("aria-checked", "false");
        // 7d-PIVOT-FIX3 Bug E: ensure data-osl-encrypt-state is never
        // null while the toggle is mounted — even before the first
        // oslComposerToggleStyle() call below or the async refresh
        // below resolves. Send-gate readers depend on this attribute
        // existing.
        btn.setAttribute("data-osl-encrypt-state", "off");
        btn.style.display = "inline-flex";
        btn.style.alignItems = "center";
        btn.style.justifyContent = "center";
        btn.style.padding = "4px 6px";
        btn.style.marginRight = "4px";
        btn.style.cursor = "pointer";
        btn.style.userSelect = "none";
        oslComposerToggleStyle(btn, false);
        btn.addEventListener("click", function (e) {
            e.preventDefault();
            e.stopPropagation();
            oslComposerToggleOnClick(btn);
        });
        btn.addEventListener("keydown", function (e) {
            if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                e.stopPropagation();
                oslComposerToggleOnClick(btn);
            }
        });
        // Prepend so we're left of GIF/gift/sticker icons.
        toolbar.insertBefore(btn, toolbar.firstChild);
        // Reflect actual scope state (async).
        oslComposerToggleRefresh(btn).catch(function () {});
    }

    function oslComposerToggleRefreshIfMounted(opts) {
        const btn = document.querySelector(
            "[" + COMPOSER_TOGGLE_DATA_ATTR + "='1']"
        );
        if (!btn) return;
        oslComposerToggleRefresh(btn, opts).catch(function () {});
    }

    function oslHeaderInjectButtons(header) {
        if (!header) return;
        const container = oslFindHeaderIconContainer(header);
        if (!container) return;
        const sample = header.querySelector('[class*="iconWrapper__"]');
        const sampleClass = sample ? sample.className : "";

        // Don't double-inject; if the buttons exist already we just
        // refresh their state.
        let encryptBtn = container.querySelector(
            "[" + HEADER_ENCRYPT_DATA_ATTR + "='1']"
        );
        let burnBtn = container.querySelector(
            "[" + HEADER_BURN_DATA_ATTR + "='1']"
        );

        if (!encryptBtn) {
            encryptBtn = document.createElement("div");
            encryptBtn.setAttribute("role", "button");
            encryptBtn.setAttribute("tabindex", "0");
            encryptBtn.setAttribute(HEADER_ENCRYPT_DATA_ATTR, "1");
            encryptBtn.className = sampleClass;
            encryptBtn.style.display = "inline-flex";
            encryptBtn.style.alignItems = "center";
            encryptBtn.style.justifyContent = "center";
            encryptBtn.style.cursor = "pointer";
            encryptBtn.addEventListener("click", oslOnEncryptToggleClick);
            container.insertBefore(encryptBtn, container.firstChild);
        }

        if (!burnBtn) {
            burnBtn = document.createElement("div");
            burnBtn.setAttribute("role", "button");
            burnBtn.setAttribute("tabindex", "0");
            burnBtn.setAttribute(HEADER_BURN_DATA_ATTR, "1");
            burnBtn.setAttribute("aria-label", "OSL burn scope");
            burnBtn.className = sampleClass;
            burnBtn.style.display = "inline-flex";
            burnBtn.style.alignItems = "center";
            burnBtn.style.justifyContent = "center";
            burnBtn.style.cursor = "pointer";
            burnBtn.style.color = "#ed4245";
            burnBtn.innerHTML = oslFlameSvg();
            burnBtn.addEventListener("click", oslOnBurnClick);
            container.insertBefore(burnBtn, encryptBtn.nextSibling);
        }

        // Refresh state.
        oslRefreshHeaderState();
    }

    /**
     * Phase 7c: re-read scope-encryption state from Rust and
     * update both header buttons. Called after every mutation
     * that could change state (whitelist set, toggle flip, burn,
     * channel switch).
     */
    // 7d-PIVOT-FIX: throttle. The header MutationObserver fires
    // hundreds of times per second during a normal send; without
    // a scope-key cache the Tauri IPC round-trip becomes the
    // dominant cost and surfaces as multi-second "slow send" UX.
    // Pass `{ force: true }` from cross-window-event listeners
    // (Rust side mutated state we couldn't otherwise see).
    let oslHeaderStateLastScopeKey = null;
    async function oslRefreshHeaderState(opts) {
        const ctx = oslCurrentChannelContext();
        if (!ctx || !ctx.channelId) {
            // Out of a channel context — nothing to refresh.
            return;
        }
        const scopeInput = oslScopeForCurrentContext(ctx);
        if (!scopeInput) return;
        const encryptBtn = document.querySelector(
            "[" + HEADER_ENCRYPT_DATA_ATTR + "='1']"
        );
        const burnBtn = document.querySelector(
            "[" + HEADER_BURN_DATA_ATTR + "='1']"
        );
        if (!encryptBtn) return;
        const key = oslScopeStorageKey(scopeInput);
        const force = opts && opts.force === true;
        if (!force && key === oslHeaderStateLastScopeKey) {
            return;
        }
        oslHeaderStateLastScopeKey = key;
        const result = await oslInvoke("osl_get_scope_encryption_state", {
            scopeInput: scopeInput,
        });
        if (!result.ok) {
            console.log(
                "[OSL] header state refresh failed: " + result.error
            );
            return;
        }
        const st = result.value;
        oslHeaderState.scopeKey = oslScopeStorageKey(scopeInput);
        oslHeaderState.encryptToggle = !!st.encrypt_toggle;
        oslHeaderState.hasWhitelist = !!st.has_whitelist;
        // 7d-PIVOT: header lock now indicates WHITELIST coverage,
        // not encrypt_toggle. The composer pill is the new
        // encrypt-toggle control surface; this icon shows whether
        // any peer in this scope is whitelisted to decrypt your
        // messages. Two-state for this phase — gray (no peers
        // whitelisted) vs green (≥ 1 peer whitelisted). The full
        // tri-state (gray / yellow / green) tied to participant
        // overlap is a follow-up.
        encryptBtn.innerHTML = oslLockSvg(st.has_whitelist ? "closed" : "open");
        encryptBtn.setAttribute(
            "aria-label",
            "OSL whitelist: " +
                (st.has_whitelist ? "active" : "none")
        );
        encryptBtn.style.opacity = "1";
        encryptBtn.style.pointerEvents = "auto";
        encryptBtn.style.color = st.has_whitelist
            ? "var(--status-positive, #23a559)"
            : "var(--text-muted, #87898c)";
        encryptBtn.title = st.has_whitelist
            ? "Whitelist active in " +
              oslScopeLabel(scopeInput) +
              " — click for whitelist details"
            : "No whitelist in " +
              oslScopeLabel(scopeInput) +
              " — encrypted messages here go to self only. " +
              "Add a peer via their profile popup.";
        if (burnBtn) {
            burnBtn.title =
                "Burn your messages in " + oslScopeLabel(scopeInput);
        }
    }

    /**
     * Phase 7c: assemble a ScopeInput from the current
     * channel-context fiber walk. For 7c we don't yet support
     * `server_full` from a single channel (per design doc §6.1 +
     * Task 6 of 7b boot.js); that's an explicit user choice
     * from the profile-popup dropdown.
     */
    function oslScopeForCurrentContext(ctx) {
        if (!ctx) return null;
        if (ctx.channelType === 1) {
            // DM: peer is members[0].
            const peer =
                Array.isArray(ctx.members) && ctx.members.length > 0
                    ? ctx.members[0]
                    : null;
            if (!peer) return null;
            return { kind: "dm", id: peer, channel_id: ctx.channelId };
        }
        if (ctx.channelType === 3) {
            return {
                kind: "gc",
                id: ctx.channelId,
                channel_id: ctx.channelId,
            };
        }
        if (ctx.channelType === 0 && ctx.guildId) {
            return {
                kind: "server_channel",
                id: ctx.guildId + ":" + ctx.channelId,
                server_id: ctx.guildId,
                channel_id: ctx.channelId,
            };
        }
        return null;
    }

    function oslScopeStorageKey(scopeInput) {
        if (!scopeInput) return null;
        switch (scopeInput.kind) {
            case "dm":
                return "dm:" + scopeInput.id;
            case "gc":
                return "gc:" + scopeInput.id;
            case "server_channel":
                return "server_channel:" + scopeInput.id;
            case "server_full":
                return "server_full:" + scopeInput.id;
            default:
                return null;
        }
    }

    // 7d-PIVOT: header-lock click is now a passive informational
    // surface. The composer pill is the encrypt control; the header
    // icon indicates whitelist coverage. Click opens a hint toast
    // pointing at the whitelist management surface (settings window
    // or profile popup). Full tri-state + bulk-toggle behavior
    // (whitelist all participants on click) is a follow-up phase —
    // it needs participant enumeration from Discord's React fiber
    // which deserves a focused implementation.
    async function oslOnEncryptToggleClick(e) {
        e.preventDefault();
        e.stopPropagation();
        const ctx = oslCurrentChannelContext();
        const scopeInput = oslScopeForCurrentContext(ctx);
        if (!scopeInput) {
            oslToast("OSL: cannot determine current scope");
            return;
        }
        oslToast(
            "Whitelist management: open Settings → Whitelist Manager, " +
                "or click a user's avatar + their OSL whitelist button. " +
                "Use the composer pill to toggle encryption."
        );
    }

    async function oslOnBurnClick(e) {
        e.preventDefault();
        e.stopPropagation();
        const ctx = oslCurrentChannelContext();
        const scopeInput = oslScopeForCurrentContext(ctx);
        if (!scopeInput) {
            oslToast("OSL: cannot determine current scope");
            return;
        }
        const ok = await oslConfirm({
            title: "Burn " + oslScopeLabel(scopeInput) + "?",
            body:
                "Your messages in " +
                oslScopeLabel(scopeInput) +
                " will become permanent ciphertext for everyone. " +
                "This cannot be undone.",
            confirmText: "Burn",
            cancelText: "Cancel",
            danger: true,
        });
        if (!ok) return;
        const selfId = await oslSelfDiscordId();
        if (!selfId) {
            oslToast(
                "OSL: cannot burn — local identity not loaded"
            );
            return;
        }
        // Ship the burn marker. Recipients = members of current
        // channel (for DM/GC, ctx.members already populated; for
        // server channels 7c doesn't yet enumerate the right-panel
        // members list — Task 4 send path notes this limitation
        // and 7d's settings UI will let users pick members).
        const sendResult = await oslInvoke("osl_send_burn_marker", {
            scopeInput: scopeInput,
            channelMembers: ctx.members,
            selfDiscordId: selfId,
        });
        if (sendResult.ok) {
            await oslSendControlMessage(ctx.channelId, sendResult.value);
        } else if (sendResult.error !== "no_whitelisted_recipients") {
            // Real error — burn marker not shipped. Still proceed
            // with local apply so the user's own state is wiped.
            console.log(
                "[OSL] burn marker send failed: " + sendResult.error
            );
        }
        // Apply locally regardless of whether the wire shipped.
        const applyResult = await oslInvoke("osl_apply_burn", {
            scopeInput: scopeInput,
        });
        if (!applyResult.ok) {
            oslToast("OSL: burn apply failed: " + applyResult.error);
            return;
        }
        // 7d-FIX1: actually destroy local data + mark burned. The
        // existing apply_burn only set wrapped_key = NULL on the
        // sqlite rows; the receive observer's next 1000ms sweep
        // would still re-decrypt the wire. Now we (a) DELETE the
        // rows for this channel, (b) add scope to the burned-scopes
        // ledger, and (c) update the JS-side __oslBurnedScopes
        // cache so the recv observer skips dispatch. Each call is
        // best-effort — partial burn is still better than no burn.
        let rowsDestroyed = 0;
        const dataResult = await oslInvoke("osl_burn_scope_data", {
            scopeKind: scopeInput.kind,
            scopeId: scopeInput.id,
            serverId: scopeInput.server_id || null,
        });
        if (dataResult.ok && dataResult.value) {
            rowsDestroyed = dataResult.value.rows_destroyed || 0;
        } else if (!dataResult.ok) {
            console.log("[OSL][burn] burn_scope_data failed: " + dataResult.error);
        }
        const markResult = await oslInvoke("osl_mark_scope_burned", {
            scopeKind: scopeInput.kind,
            scopeId: scopeInput.id,
            serverId: scopeInput.server_id || null,
            channelId: scopeInput.channel_id || ctx.channelId || null,
        });
        if (!markResult.ok) {
            console.log("[OSL][burn] mark_scope_burned failed: " + markResult.error);
        } else {
            // Sync the in-memory skip cache so the next recv sweep
            // honours the burn without an extra round-trip.
            oslBurnedScopesAdd(scopeInput.kind, scopeInput.id);
        }
        oslBurnAftermath(ctx.channelId);
        // 7d-PIVOT-FIX: force header state refresh after burn. The
        // throttled refresh would otherwise skip this re-read
        // because the scope key hasn't changed; burn doesn't move
        // us off the channel, so without `force` the lock visual
        // could lag if whitelist_state was mutated by a side path.
        try {
            oslRefreshHeaderState({ force: true });
        } catch (_) {}
        console.log(
            "[OSL][burn] scope " +
                scopeInput.kind +
                ":" +
                scopeInput.id +
                " burned, " +
                rowsDestroyed +
                " messages destroyed"
        );
        oslToast(
            "Burn applied to " +
                oslScopeLabel(scopeInput) +
                " (" +
                rowsDestroyed +
                " messages destroyed)"
        );
        oslRefreshHeaderState();
    }

    /**
     * Phase 7c bug-fix #3: post-burn DOM + cache cleanup.
     *
     * `cmd_osl_apply_burn` wipes the at-rest wrapped_keys, but
     * boot.js holds three caches that would otherwise keep
     * serving the plaintext we already decrypted earlier in the
     * session:
     *   - `loadedHistory`  (filled from on-disk store at channel
     *                       switch — stale post-burn)
     *   - `recvPlaintext`  (this-session decrypts)
     *   - `recvDone`       (one-shot dispatch guard)
     *
     * For every visible message <li> in `channelId`, we drop the
     * cache entries and repaint the content div with the cached
     * cover text (`recvCovers`). The recv observer's next tick
     * sees DPC0:: text in the div, the caches are empty, it
     * dispatches a fresh decrypt — which now fails (wrapped key
     * gone) — and the cover stays in place. Net effect: the
     * user's view of their own old messages becomes ciphertext
     * immediately, matching what peers see post-burn.
     *
     * Limitations:
     *   - Only operates on the supplied `channelId`. For
     *     server_full scopes spanning many channels, only the
     *     current channel repaints immediately; the others will
     *     show ciphertext on next channel switch (cache repopulates
     *     from on-disk store, which now has no wrapped_keys).
     *   - If a message has no `recvCovers` entry (e.g. it was
     *     loaded from history before this session and its cover
     *     was never observed live), we clear the visible content
     *     (empty span). 7d-PIVOT removed the `[burned]` placeholder
     *     — failed-decrypt messages stay as raw ciphertext when
     *     we have it, and as empty bubbles when we don't, but
     *     never as the "[burned]" string.
     */
    function oslBurnAftermath(channelId) {
        if (!channelId) return;
        const items = document.querySelectorAll(
            'li[id^="chat-messages-' + channelId + '-"]'
        );
        let repainted = 0;
        let blanked = 0;
        items.forEach(function (li) {
            const div = li.querySelector(
                '[id^="' + RECV_MESSAGE_ID_PREFIX + '"]'
            );
            if (!div) return;
            const messageId = recvMessageIdOf(div);
            loadedHistory.delete(messageId);
            recvPlaintext.delete(messageId);
            recvDone.delete(messageId);
            // 7d-PIVOT-FIX2 Bug G: prefer the DOM-persisted original
            // ciphertext over the in-memory `recvCovers` map. The
            // attribute survives across the cache wipes above and lets
            // us repaint even if this is the first burn observation in
            // this session. Falls back to recvCovers, then to blank.
            const origCipher = div.getAttribute("data-osl-orig-cipher");
            const lastCover = recvCovers.get(messageId);
            const span = document.createElement("span");
            if (typeof origCipher === "string" && origCipher.length > 0) {
                span.textContent = origCipher;
                repainted++;
            } else if (typeof lastCover === "string" && lastCover.length > 0) {
                // 7d-PIVOT-FIX3 Bug G: data-osl-orig-cipher was the
                // primary source post-FIX2. Falling through to
                // recvCovers means the attribute wasn't stamped on
                // this div — either the message was a self-send
                // (we never observed it in ciphertext form) or a
                // change to recvHandleDiv regressed the stamp.
                // recvCovers is still the right fallback, but log
                // it so future regressions stand out.
                console.warn(
                    "[OSL] burn aftermath: msg=" +
                        messageId +
                        " had no data-osl-orig-cipher attribute;" +
                        " falling back to recvCovers"
                );
                span.textContent = lastCover;
                repainted++;
            } else {
                // 7d-PIVOT: no "[burned]" placeholder. Blank the
                // content rather than leak plaintext.
                span.textContent = "";
                blanked++;
            }
            div.replaceChildren(span);
        });
        console.log(
            "[OSL] burn aftermath: channel=" +
                channelId +
                " items=" +
                items.length +
                " repainted=" +
                repainted +
                " blanked=" +
                blanked
        );
    }

    // ---- Phase 8: attachment recv pipeline ----
    //
    // On a v=2 attachment-envelope decrypt, the message-text result
    // is the sentinel `OSL_RESULT_ATTACHMENT_PREFIX + <json>` and the
    // actual encrypted bytes live in the Discord CDN. The envelope
    // JSON carries the per-attachment AEAD key + original filename +
    // random upload filename + MIME. We:
    //   1. Find the message's `<li>` in the DOM.
    //   2. Scan for `<img>` / `<a>` elements whose src/href points
    //      at the Discord CDN AND whose filename matches the
    //      envelope's random_filename (so we don't accidentally
    //      swap an unrelated attachment in a multi-attachment
    //      message).
    //   3. Fetch the URL → arrayBuffer → base64.
    //   4. Call `osl_open_attachment` with the envelope's AEAD key
    //      and the file bytes; the Rust side scans for OSL-ATT1,
    //      decrypts, returns plaintext + MIME.
    //   5. Build a Blob, mint a blob URL, swap the rendered
    //      element's src/href.
    //
    // Cache: `__oslAttachmentDecrypted` is a Map keyed by message_id
    // with `{ blobUrl, mime }`. We cap at 50 entries (LRU eviction
    // via insertion order) so heavy scrolling doesn't pin large
    // video Blobs forever.
    if (!window.__oslAttachmentDecrypted) {
        window.__oslAttachmentDecrypted = new Map();
    }
    const OSL_ATT_CACHE_CAP = 50;

    function oslAttachmentCacheEvictIfFull() {
        const m = window.__oslAttachmentDecrypted;
        while (m.size > OSL_ATT_CACHE_CAP) {
            // Map iteration is insertion order; first key is the
            // oldest entry.
            const first = m.keys().next().value;
            const old = m.get(first);
            if (old && Array.isArray(old.blobUrls)) {
                for (const u of old.blobUrls) {
                    try {
                        URL.revokeObjectURL(u);
                    } catch (_) {}
                }
            }
            m.delete(first);
        }
    }

    /**
     * Find the message `<li>` for `msgId` in the current DOM.
     * Returns null if not yet rendered (channel switched, message
     * scrolled out, etc.) — caller drops the decrypt in that case.
     */
    function oslFindMessageListItem(msgId) {
        if (!msgId) return null;
        return document.querySelector('li[id$="-' + msgId + '"]');
    }

    /**
     * Inside `li`, find every CDN-hosted media element (<img> for
     * images, <a> with class containing "originalLink" for video
     * thumbnails) whose filename matches `randomFilename`. The
     * filename match is conservative: we look for the last
     * URL path segment so query strings (Discord auth tokens) are
     * ignored.
     */
    function oslFindAttachmentTargets(li, randomFilename) {
        if (!li || !randomFilename) return [];
        const out = [];
        const candidates = li.querySelectorAll(
            "img, video, a[href*='discord']"
        );
        candidates.forEach(function (el) {
            const url =
                el.tagName === "A"
                    ? el.getAttribute("href")
                    : el.getAttribute("src");
            if (typeof url !== "string") return;
            // Extract the last path segment, drop any query string.
            const path = url.split("?")[0];
            const segs = path.split("/");
            const last = segs[segs.length - 1] || "";
            if (last === randomFilename) {
                out.push({ el, url });
            }
        });
        return out;
    }

    /**
     * Promise-returning helper: fetch the attachment URL as bytes,
     * convert to base64 (chunked to avoid blowing the string-arg
     * size for `String.fromCharCode` on large videos).
     */
    async function oslFetchAttachmentBase64(url) {
        const resp = await fetch(url, { credentials: "omit" });
        if (!resp.ok) {
            throw new Error(
                "attachment fetch HTTP " + resp.status + " for " + url
            );
        }
        const buf = await resp.arrayBuffer();
        // Chunked base64 — 32 KB at a time so a 24 MB blob doesn't
        // synthesize a 24 MB argument string for fromCharCode.
        const bytes = new Uint8Array(buf);
        let binary = "";
        const CHUNK = 32 * 1024;
        for (let i = 0; i < bytes.length; i += CHUNK) {
            const slice = bytes.subarray(i, Math.min(i + CHUNK, bytes.length));
            binary += String.fromCharCode.apply(null, slice);
        }
        return btoa(binary);
    }

    /**
     * Swap a single rendered element to point at `blobUrl`. For
     * <img> we just replace src; for <a> linking to a video we
     * mutate the closest containing tile to inline a <video> element.
     */
    function oslSwapAttachmentElement(target, blobUrl, mime) {
        try {
            if (target.el.tagName === "IMG") {
                target.el.setAttribute("src", blobUrl);
                target.el.removeAttribute("srcset");
                return;
            }
            if (target.el.tagName === "VIDEO") {
                target.el.setAttribute("src", blobUrl);
                target.el.setAttribute("type", mime);
                return;
            }
            if (target.el.tagName === "A" && mime.indexOf("video/") === 0) {
                // Replace the link with an inline video element.
                const video = document.createElement("video");
                video.setAttribute("src", blobUrl);
                video.setAttribute("controls", "controls");
                video.setAttribute("preload", "metadata");
                video.style.maxWidth = "100%";
                video.style.maxHeight = "440px";
                video.style.borderRadius = "4px";
                target.el.replaceWith(video);
                return;
            }
            // <a> linking to an image: append the decrypted image
            // alongside the link so it shows inline without
            // disturbing other rendering.
            if (target.el.tagName === "A") {
                const img = document.createElement("img");
                img.setAttribute("src", blobUrl);
                img.style.maxWidth = "100%";
                img.style.borderRadius = "4px";
                target.el.replaceWith(img);
            }
        } catch (e) {
            console.warn(
                "[OSL] attachment swap failed for " + target.url + ":",
                e
            );
        }
    }

    /**
     * Top-level attachment recv handler. Called from the
     * `oslHandleDecryptResult` wrapper when the recv sentinel
     * matches the attachment prefix. Idempotent: cached entries
     * are short-circuited.
     */
    async function oslHandleAttachmentEnvelope(msgId, env) {
        if (!msgId || !env || !Array.isArray(env.attachments)) {
            console.warn(
                "[OSL] attachment envelope missing attachments[] msg=" + msgId,
                env
            );
            return;
        }
        // Burned scope: don't decrypt — the receive observer
        // already treats this scope as "leave ciphertext alone."
        try {
            const ctx = oslCurrentChannelContext();
            const scope = ctx && oslScopeForCurrentContext(ctx);
            if (
                scope &&
                oslBurnedScopesShouldSkip(scope.channel_id || scope.id)
            ) {
                console.log(
                    "[OSL] attachment decrypt skipped: msg=" +
                        msgId +
                        " reason=scope_burned"
                );
                return;
            }
        } catch (_) {}

        const li = oslFindMessageListItem(msgId);
        if (!li) {
            console.log(
                "[OSL] attachment decrypt deferred: msg=" +
                    msgId +
                    " reason=li_not_rendered"
            );
            return;
        }

        // Cache hit: replay the swap on the current DOM. The cache
        // stores one (blobUrl, mime) tuple per random_filename so
        // repeated renders skip both the network fetch and the
        // decrypt.
        let cacheEntry = window.__oslAttachmentDecrypted.get(msgId);
        if (cacheEntry && cacheEntry.byRandomName) {
            env.attachments.forEach(function (att) {
                const cached = cacheEntry.byRandomName[att.randomFilename];
                if (!cached) return;
                const targets = oslFindAttachmentTargets(
                    li,
                    att.randomFilename
                );
                targets.forEach(function (t) {
                    oslSwapAttachmentElement(t, cached.blobUrl, cached.mime);
                });
            });
            return;
        }

        cacheEntry = { byRandomName: {}, blobUrls: [] };
        // Decrypt + swap each attachment independently. One fails
        // → log + continue with the rest (so a partial Discord
        // outage doesn't block all attachments in the same message).
        for (const att of env.attachments) {
            if (
                !att.attKey ||
                !att.randomFilename ||
                !att.originalFilename ||
                !att.mimeType
            ) {
                console.warn(
                    "[OSL] attachment entry missing fields msg=" + msgId,
                    att
                );
                continue;
            }
            const targets = oslFindAttachmentTargets(li, att.randomFilename);
            if (targets.length === 0) {
                console.log(
                    "[OSL] attachment decrypt skipped: msg=" +
                        msgId +
                        " random=" +
                        att.randomFilename +
                        " reason=no_matching_cdn_element"
                );
                continue;
            }
            const url = targets[0].url;
            console.log(
                "[OSL] attachment detected: msg=" +
                    msgId +
                    " original=" +
                    att.originalFilename +
                    " random=" +
                    att.randomFilename +
                    " mime=" +
                    att.mimeType +
                    " url=" +
                    url.substring(0, 80)
            );
            let fileB64;
            try {
                fileB64 = await oslFetchAttachmentBase64(url);
            } catch (err) {
                console.error(
                    "[OSL] attachment fetch failed msg=" +
                        msgId +
                        " random=" +
                        att.randomFilename +
                        ":",
                    err
                );
                continue;
            }
            const decRes = await oslInvoke("osl_open_attachment", {
                attKeyB64: att.attKey,
                fileBytesB64: fileB64,
            });
            if (!decRes.ok) {
                console.error(
                    "[OSL] attachment decrypt failed msg=" +
                        msgId +
                        " random=" +
                        att.randomFilename +
                        " error=" +
                        decRes.error
                );
                continue;
            }
            const plain = decRes.value;
            const binary = atob(plain.plaintextB64);
            const bytes = new Uint8Array(binary.length);
            for (let i = 0; i < binary.length; i++) {
                bytes[i] = binary.charCodeAt(i);
            }
            const blob = new Blob([bytes], { type: plain.mimeType });
            const blobUrl = URL.createObjectURL(blob);
            cacheEntry.byRandomName[att.randomFilename] = {
                blobUrl: blobUrl,
                mime: plain.mimeType,
            };
            cacheEntry.blobUrls.push(blobUrl);
            targets.forEach(function (t) {
                oslSwapAttachmentElement(t, blobUrl, plain.mimeType);
            });
            console.log(
                "[OSL] attachment decrypted: msg=" +
                    msgId +
                    " original=" +
                    plain.originalFilename +
                    " mime=" +
                    plain.mimeType +
                    " size=" +
                    bytes.length
            );
        }
        if (cacheEntry.blobUrls.length > 0) {
            window.__oslAttachmentDecrypted.set(msgId, cacheEntry);
            oslAttachmentCacheEvictIfFull();
        }
    }

    /**
     * 8d V2 receive scan. Fires on every `li[id^="chat-messages-"]`
     * the observer sees. For each CDN-hosted image/video/anchor
     * inside the li, attempts `osl_open_attachment_v2`. The Rust
     * side scans for the OSL-ATT2 (or OSL-ATT1 with legacy key)
     * magic — if absent, returns MagicNotFound and we leave the
     * element alone. So this scan is cheap on non-OSL attachments.
     *
     * Idempotent via `__oslAttachmentDecrypted` cache (re-applies
     * cached blob URLs without re-fetching / re-decrypting).
     */
    async function oslScanLiAttachmentsV2(li) {
        if (!li || !li.id) return;
        const m = /chat-messages-(?:\d{15,22})-(\d{15,22})/.exec(li.id);
        if (!m) return;
        const msgId = m[1];

        // 8d-FIX1: capture channel_id alongside scope. We need it
        // to construct the cdn.discordapp.com URL directly —
        // media.discordapp.net (the URL on `<img src>`) is a
        // re-encoding proxy that strips everything after the PNG
        // IEND chunk including our OSL-ATT2 magic + cover + payload.
        // Only cdn.discordapp.com serves the original bytes.
        let scope = null;
        let channelId = null;
        try {
            const ctx = oslCurrentChannelContext();
            channelId = (ctx && ctx.channelId) || null;
            scope = ctx && oslScopeForCurrentContext(ctx);
            if (
                scope &&
                oslBurnedScopesShouldSkip(scope.channel_id || scope.id)
            ) {
                return;
            }
        } catch (_) {}

        // Cache hit: replay swap.
        const cacheEntry = window.__oslAttachmentDecrypted.get(msgId);
        if (cacheEntry && cacheEntry.byRandomName) {
            for (const randomName of Object.keys(cacheEntry.byRandomName)) {
                const cached = cacheEntry.byRandomName[randomName];
                const targets = oslFindAttachmentTargets(li, randomName);
                targets.forEach(function (t) {
                    oslSwapAttachmentElement(t, cached.blobUrl, cached.mime);
                });
            }
            return;
        }

        // Discover CDN media elements. The V2 scan doesn't know
        // random_filename in advance, so collect every plausible
        // candidate keyed by its URL's last path segment.
        const candidates = [];
        const els = li.querySelectorAll(
            "img[src*='discord'], video[src*='discord'], a[href*='discord']"
        );
        els.forEach(function (el) {
            const url =
                el.tagName === "A"
                    ? el.getAttribute("href")
                    : el.getAttribute("src");
            if (typeof url !== "string") return;
            const path = url.split("?")[0];
            const last = path.split("/").pop() || "";
            // Phase 8 / 8c / 8d uploads always carry a `.png`
            // upload-filename (random 8-hex). Anchor on that
            // suffix to filter out unrelated discord.com
            // referenced URLs like profile images.
            if (!/^[0-9a-f]{8}\.png$/i.test(last)) return;
            candidates.push({ el: el, url: url, name: last });
        });
        if (candidates.length === 0) return;

        // Sender id is required for the cover decrypt (v=2 wrap is
        // bound to sender's pubkey on the recv side). Pull from
        // li's data-author-id, walking up if needed.
        let senderId = null;
        try {
            senderId = recvExtractAuthorId(li);
        } catch (_) {}
        if (!senderId) return;

        const newCache = { byRandomName: {}, blobUrls: [] };
        for (const cand of candidates) {
            try {
                // 8d-FIX1: try cdn.discordapp.com first. Discord's
                // media.discordapp.net proxy re-encodes uploads to
                // strip trailing bytes (which is where our OSL-ATT2
                // magic + cover envelope + payload live). cdn.disc-
                // ordapp.com serves the originals untouched.
                const cdnUrl = channelId
                    ? "https://cdn.discordapp.com/attachments/" +
                      channelId +
                      "/" +
                      msgId +
                      "/" +
                      cand.name
                    : null;
                console.log(
                    "[OSL] attachment scan: msg=" +
                        msgId +
                        " channel=" +
                        (channelId || "?") +
                        " random=" +
                        cand.name +
                        " cdn_url=" +
                        (cdnUrl
                            ? cdnUrl.substring(0, 100)
                            : "n/a") +
                        " dom_url=" +
                        cand.url.substring(0, 100)
                );
                let fileB64 = null;
                let fetchedFrom = null;
                if (cdnUrl) {
                    try {
                        fileB64 = await oslFetchAttachmentBase64(cdnUrl);
                        fetchedFrom = "cdn";
                    } catch (cdnErr) {
                        console.warn(
                            "[OSL] attachment fetch: cdn_url failed msg=" +
                                msgId +
                                " — falling back to dom_url. err=" +
                                (cdnErr && cdnErr.message
                                    ? cdnErr.message
                                    : cdnErr)
                        );
                    }
                }
                if (fileB64 === null) {
                    fileB64 = await oslFetchAttachmentBase64(cand.url);
                    fetchedFrom = "dom";
                }
                // Quick magic-presence diagnostic — base64-decoded
                // length + look for OSL-ATT[12] in the first 64KB
                // so a future re-encoding regression at the CDN
                // shows up clearly in logs.
                try {
                    const sniff = atob(fileB64.substring(0, 1024 * 96));
                    const hasV2 = sniff.indexOf("OSL-ATT2") >= 0;
                    const hasV1 = sniff.indexOf("OSL-ATT1") >= 0;
                    console.log(
                        "[OSL] attachment fetch: source=" +
                            fetchedFrom +
                            " msg=" +
                            msgId +
                            " b64_len=" +
                            fileB64.length +
                            " magic_present=" +
                            (hasV2 ? "OSL-ATT2" : hasV1 ? "OSL-ATT1" : "no")
                    );
                } catch (_) {}
                const decRes = await oslInvoke("osl_open_attachment_v2", {
                    senderDiscordId: senderId,
                    scopeInput: scope || null,
                    fileBytesB64: fileB64,
                    legacyAttKeyB64: null,
                });
                if (!decRes.ok) {
                    // Most common: MagicNotFound for non-OSL files.
                    // Logged at low volume to avoid spam on
                    // unencrypted channels.
                    if (decRes.error && decRes.error.indexOf("MagicNotFound") < 0) {
                        console.log(
                            "[OSL] attachment decrypt skip: msg=" +
                                msgId +
                                " random=" +
                                cand.name +
                                " reason=" +
                                decRes.error
                        );
                    }
                    continue;
                }
                const plain = decRes.value;
                const binary = atob(plain.plaintextB64);
                const bytes = new Uint8Array(binary.length);
                for (let i = 0; i < binary.length; i++) {
                    bytes[i] = binary.charCodeAt(i);
                }
                const blob = new Blob([bytes], { type: plain.mimeType });
                const blobUrl = URL.createObjectURL(blob);
                newCache.byRandomName[cand.name] = {
                    blobUrl: blobUrl,
                    mime: plain.mimeType,
                };
                newCache.blobUrls.push(blobUrl);
                const targets = oslFindAttachmentTargets(li, cand.name);
                targets.forEach(function (t) {
                    oslSwapAttachmentElement(t, blobUrl, plain.mimeType);
                });
                console.log(
                    "[OSL] attachment decrypt: msg=" +
                        msgId +
                        " original=" +
                        plain.originalFilename +
                        " mime=" +
                        plain.mimeType +
                        " size=" +
                        bytes.length
                );
            } catch (err) {
                console.warn(
                    "[OSL] attachment scan threw msg=" +
                        msgId +
                        " random=" +
                        cand.name +
                        ":",
                    err
                );
            }
        }
        if (newCache.blobUrls.length > 0) {
            window.__oslAttachmentDecrypted.set(msgId, newCache);
            oslAttachmentCacheEvictIfFull();
        }
    }

    // ---- Section 4e (7d-FIX1): burned-scopes ledger cache ----
    //
    // The receive observer's 1000ms sweep otherwise re-decrypts
    // every DPC0:: message it sees. After a scope burn, the wire
    // ciphertext is still on screen but the on-disk message rows
    // are gone (cmd_osl_burn_scope_data) and the user wants the
    // ciphertext to STAY as ciphertext.
    //
    // `__oslBurnedScopes` is a synchronous Map keyed by storage_key
    // ("dm:<peer>", "gc:<id>", "server_channel:<server>:<channel>",
    // "server_full:<server>"). Filled at install via
    // `osl_list_burned_scopes`, mutated synchronously on
    // local-scope-burn via `oslBurnedScopesAdd`, and on re-whitelist
    // via `oslBurnedScopesRemove`. The receive observer checks via
    // `oslBurnedScopesShouldSkip(channelId)` before dispatching.
    //
    // Per-channel "we already logged the skip once this session" set
    // suppresses log spam — the spec asks for one-per-channel logging.

    if (!window.__oslBurnedScopes) {
        window.__oslBurnedScopes = new Map();
    }
    const oslBurnedScopesLoggedChannels = new Set();

    function oslBurnedScopesKey(scopeKind, scopeId) {
        // Normalize JS-side kind strings ("dm"/"gc"/"server_channel"/
        // "server_full") to the same storage_key form Rust uses.
        switch (scopeKind) {
            case "dm":
                return "dm:" + scopeId;
            case "gc":
            case "gc_full":
            case "gc_per_user":
                return "gc:" + scopeId;
            case "server_channel":
            case "server_channel_full":
            case "server_channel_per_user":
                return "server_channel:" + scopeId;
            case "server_full":
            case "server_full_per_user":
                return "server_full:" + scopeId;
            default:
                return scopeKind + ":" + scopeId;
        }
    }

    function oslBurnedScopesAdd(scopeKind, scopeId) {
        window.__oslBurnedScopes.set(
            oslBurnedScopesKey(scopeKind, scopeId),
            true
        );
    }

    function oslBurnedScopesRemove(scopeKind, scopeId) {
        window.__oslBurnedScopes.delete(
            oslBurnedScopesKey(scopeKind, scopeId)
        );
        // Allow re-logging if the scope gets burned again later.
        for (const ch of Array.from(oslBurnedScopesLoggedChannels)) {
            if (ch.endsWith(":" + scopeId)) {
                oslBurnedScopesLoggedChannels.delete(ch);
            }
        }
    }

    /**
     * 7d-PIVOT-FIX3 Bug F: drop `scopeInput` from `__oslBurnedScopes`
     * locally (synchronous, no event round-trip) and persist the
     * unburn to disk via `osl_unburn_scope`. Called from the send
     * gate after a successful encrypt, so a re-engaged scope's next
     * inbound DPC0 message is decrypted instead of being skipped by
     * `oslBurnedScopesShouldSkip`.
     *
     * Idempotent: if the scope wasn't burned, the local Map delete
     * is a no-op and the Rust command returns Ok(false).
     */
    function oslInlineUnburnAfterEncrypt(scopeInput) {
        if (!scopeInput || !scopeInput.kind || !scopeInput.id) return;
        const key = oslBurnedScopesKey(scopeInput.kind, scopeInput.id);
        const wasBurned =
            window.__oslBurnedScopes && window.__oslBurnedScopes.has(key);
        if (wasBurned) {
            oslBurnedScopesRemove(scopeInput.kind, scopeInput.id);
            console.log(
                "[OSL] inline unburn: removed " +
                    key +
                    " from __oslBurnedScopes (encrypt-send re-engaged)"
            );
        }
        // Always call through to Rust — the JS map can drift (a new
        // browser session repopulates from disk via osl_list_burned_scopes,
        // and Rust persistence is the source of truth across launches).
        // Idempotent on the Rust side too.
        try {
            oslInvoke("osl_unburn_scope", {
                scopeKind: scopeInput.kind,
                scopeId: scopeInput.id,
            }).then(
                function (r) {
                    if (r && r.ok && r.value === true) {
                        console.log(
                            "[OSL] inline unburn: persisted unburn for " + key
                        );
                    }
                },
                function (err) {
                    console.warn(
                        "[OSL] inline unburn IPC failed for " + key + ":",
                        err
                    );
                }
            );
        } catch (err) {
            console.warn(
                "[OSL] inline unburn IPC threw for " + key + ":",
                err
            );
        }
    }

    /**
     * Decide whether to skip dispatch for a message in `channelId`.
     * The recv-side only knows channel_id at this point, not the
     * full scope shape. We check every storage_key in the cache and
     * return true if any matches — DM scopes have scope_id ==
     * channel_id, GC scopes the same, server_channel scopes embed
     * channel_id in the second half. server_full is not yet
     * burnable per the Rust-side scope handling.
     */
    function oslBurnedScopesShouldSkip(channelId) {
        if (!channelId) return false;
        if (window.__oslBurnedScopes.size === 0) return false;
        // Fast paths: dm:<channelId>, gc:<channelId>.
        if (window.__oslBurnedScopes.has("dm:" + channelId)) return true;
        if (window.__oslBurnedScopes.has("gc:" + channelId)) return true;
        // server_channel storage_key is "server_channel:<server>:<channel>".
        // Walk keys for the suffix.
        for (const k of window.__oslBurnedScopes.keys()) {
            if (k.startsWith("server_channel:") && k.endsWith(":" + channelId)) {
                return true;
            }
        }
        return false;
    }

    function oslBurnedScopesLogOnceForChannel(channelId) {
        if (oslBurnedScopesLoggedChannels.has(channelId)) return;
        oslBurnedScopesLoggedChannels.add(channelId);
        console.log(
            "[OSL] channel " + channelId + " is burned, skipping decrypt"
        );
    }

    /**
     * Install-time hydration. Best-effort: a failed invoke leaves
     * the cache empty, which means the recv observer doesn't skip
     * anything (worst-case decrypt resumes for previously-burned
     * scopes until next install). A new local burn during the
     * session re-fills the cache via `oslBurnedScopesAdd`.
     */
    async function oslBurnedScopesInit() {
        const r = await oslInvoke("osl_list_burned_scopes", {});
        if (!r.ok) {
            console.log(
                "[OSL][burn] list_burned_scopes failed at init: " + r.error
            );
            return;
        }
        const entries = r.value || [];
        for (const e of entries) {
            window.__oslBurnedScopes.set(
                oslBurnedScopesKey(e.scope_kind, e.scope_id),
                true
            );
        }
        if (entries.length > 0) {
            console.log(
                "[OSL][burn] init: loaded " +
                    entries.length +
                    " burned scope(s) from disk"
            );
        }
        return entries.length;
    }

    // ---- Section 5: pending invitation banner ----

    const BANNER_STACK_ID = "__osl_invitation_banners";
    const BANNER_ATTR = "data-osl-invitation-banner";

    /**
     * Phase 7c: insert the banner stack inside Discord's chat
     * content area, above the message list. The stack is a
     * pinned-top sibling of the message scroller so banners stay
     * visible while the user scrolls back through history.
     */
    function oslEnsureBannerStack() {
        let stack = document.getElementById(BANNER_STACK_ID);
        if (stack && document.body.contains(stack)) return stack;
        // Discord's chat content sits in <main>; find the chat area
        // by looking for the channel header's parent.
        const header = document.querySelector(
            'section[class*="title_"][class*="container__"]'
        );
        const parent = header && header.parentElement;
        if (!parent) return null;
        stack = document.createElement("div");
        stack.id = BANNER_STACK_ID;
        stack.style.display = "flex";
        stack.style.flexDirection = "column";
        stack.style.gap = "4px";
        // Insert just below the header.
        parent.insertBefore(stack, header.nextSibling);
        return stack;
    }

    async function oslRefreshBanners() {
        const stack = oslEnsureBannerStack();
        if (!stack) return;
        const listResult = await oslInvoke(
            "osl_list_pending_invitations",
            {}
        );
        if (!listResult.ok) {
            console.log(
                "[OSL] list pending invitations failed: " + listResult.error
            );
            return;
        }
        // Clear existing banners and re-render. Cheap for the
        // expected size (≤ a handful of pending invitations).
        const existing = stack.querySelectorAll("[" + BANNER_ATTR + "='1']");
        for (const e of existing) e.remove();
        for (const inv of listResult.value) {
            stack.appendChild(oslRenderBanner(inv));
        }
    }

    function oslRenderBanner(inv) {
        const el = document.createElement("div");
        el.setAttribute(BANNER_ATTR, "1");
        el.setAttribute("data-osl-invitation-id", inv.id);
        el.style.padding = "12px 16px";
        el.style.background = "var(--background-tertiary, #1e1f22)";
        el.style.color = "var(--text-normal, #dbdee1)";
        el.style.display = "flex";
        el.style.alignItems = "center";
        el.style.justifyContent = "space-between";
        el.style.gap = "12px";
        el.style.borderBottom =
            "1px solid var(--background-modifier-accent, #2e3035)";

        const msg = document.createElement("div");
        msg.style.flex = "1";
        msg.style.fontSize = "14px";
        msg.textContent =
            "OSL: " +
            inv.from +
            " wants to send you encrypted messages in " +
            oslBannerScopeLabel(inv) +
            ".";
        el.appendChild(msg);

        const accept = document.createElement("button");
        accept.textContent = "Accept";
        accept.style.padding = "6px 14px";
        accept.style.borderRadius = "4px";
        accept.style.border = "none";
        accept.style.background = "var(--brand-560, #5865f2)";
        accept.style.color = "white";
        accept.style.cursor = "pointer";
        accept.style.fontSize = "13px";
        accept.addEventListener("click", function () {
            oslOnInvitationDecision(inv, true);
        });
        el.appendChild(accept);

        const decline = document.createElement("button");
        decline.textContent = "Decline";
        decline.style.padding = "6px 14px";
        decline.style.borderRadius = "4px";
        decline.style.border =
            "1px solid var(--background-modifier-accent, #4f545c)";
        decline.style.background = "transparent";
        decline.style.color = "inherit";
        decline.style.cursor = "pointer";
        decline.style.fontSize = "13px";
        decline.addEventListener("click", function () {
            oslOnInvitationDecision(inv, false);
        });
        el.appendChild(decline);

        return el;
    }

    function oslBannerScopeLabel(inv) {
        // inv.scope is the scope-kind string from
        // pending_invitations.json ("dm", "gc", "server_channel",
        // "server_full"); inv.scope_id is the storage_key.
        switch (inv.scope) {
            case "dm":
                return "this DM";
            case "gc":
                return "this group chat";
            case "server_channel":
                return "this server channel";
            case "server_full":
                return "an entire server";
            default:
                return "an OSL scope";
        }
    }

    async function oslOnInvitationDecision(inv, accepted) {
        const cmd = accepted
            ? "osl_accept_invitation"
            : "osl_decline_invitation";
        const result = await oslInvoke(cmd, { invitationId: inv.id });
        if (!result.ok) {
            oslToast(
                "OSL: " +
                    (accepted ? "accept" : "decline") +
                    " failed: " +
                    result.error
            );
            return;
        }
        // Build + ship the response wire so the inviter's UI
        // updates. Reconstruct the scope from inv.scope_id (the
        // storage_key embeds it).
        const ctx = oslCurrentChannelContext();
        if (ctx && ctx.channelId && inv.scope_id) {
            const scopeInput = oslScopeInputFromStorageKey(inv.scope_id);
            if (scopeInput) {
                const respResult = await oslInvoke(
                    "osl_send_whitelist_response",
                    {
                        toDiscordId: inv.from,
                        scopeInput: scopeInput,
                        accepted: accepted,
                    }
                );
                if (respResult.ok) {
                    await oslSendControlMessage(
                        ctx.channelId,
                        respResult.value
                    );
                }
            }
        }
        oslToast(
            accepted
                ? "Accepted. " +
                      inv.from +
                      "'s messages will now decrypt."
                : "Declined. " +
                      inv.from +
                      "'s messages will stay encrypted."
        );
        oslRefreshBanners();
        oslRefreshHeaderState();
    }

    function oslScopeInputFromStorageKey(key) {
        if (typeof key !== "string") return null;
        if (key.indexOf("dm:") === 0) {
            const id = key.slice(3);
            return id ? { kind: "dm", id: id, channel_id: id } : null;
        }
        if (key.indexOf("gc:") === 0) {
            const id = key.slice(3);
            return id ? { kind: "gc", id: id, channel_id: id } : null;
        }
        if (key.indexOf("server_channel:") === 0) {
            const rest = key.slice("server_channel:".length);
            const ix = rest.indexOf(":");
            if (ix <= 0 || ix === rest.length - 1) return null;
            const server = rest.slice(0, ix);
            const channel = rest.slice(ix + 1);
            return {
                kind: "server_channel",
                id: server + ":" + channel,
                server_id: server,
                channel_id: channel,
            };
        }
        if (key.indexOf("server_full:") === 0) {
            const id = key.slice("server_full:".length);
            return id ? { kind: "server_full", id: id, server_id: id } : null;
        }
        return null;
    }

    // ---- Section 6: recv-path glue ----

    // Extend the 7b sentinel handler so control-message recv
    // events trigger UI side-effects: toasts + banner refresh.
    // We wrap the existing `oslHandleDecryptResult` to keep its
    // boolean contract (true = control sentinel, false = plaintext).
    const _oslHandleDecryptResult_v7b = oslHandleDecryptResult;
    // eslint-disable-next-line no-func-assign
    oslHandleDecryptResult = function (msgId, result) {
        const handled = _oslHandleDecryptResult_v7b(msgId, result);
        if (!handled) return false;
        try {
            if (result === OSL_RESULT_BURN_APPLIED) {
                oslToast(
                    "OSL: a peer burned messages in this scope; affected messages will re-render as ciphertext."
                );
                // Phase 7c bug-fix #3: same JS-cache + DOM
                // cleanup the local-burn path runs. Rust already
                // wiped wrapped_keys in `cmd_osl_apply_burn`; we
                // need to evict the in-memory caches and repaint
                // the visible message divs so the user actually
                // sees the burn effect instead of stale plaintext.
                try {
                    const burnCtx = oslCurrentChannelContext();
                    if (burnCtx && burnCtx.channelId) {
                        oslBurnAftermath(burnCtx.channelId);
                    }
                } catch (e) {
                    console.log(
                        "[OSL] recv-side burn aftermath threw: " +
                            (e && e.message ? e.message : e)
                    );
                }
                oslRefreshHeaderState();
            } else if (result === OSL_RESULT_INVITATION_RECEIVED) {
                oslRefreshBanners();
            } else if (result === OSL_RESULT_RESPONSE_RECEIVED) {
                oslToast(
                    "OSL: a peer accepted/declined your whitelist invitation."
                );
                oslRefreshHeaderState();
            } else if (
                typeof result === "string" &&
                result.indexOf(OSL_RESULT_ATTACHMENT_PREFIX) === 0
            ) {
                // Phase 8: dispatch to the attachment recv handler.
                // Fire-and-forget — DOM swap happens asynchronously
                // once the CDN fetch + decrypt resolves. Errors are
                // logged inside the handler; we never block the
                // recv-observer thread here.
                const json = result.slice(OSL_RESULT_ATTACHMENT_PREFIX.length);
                try {
                    const env = JSON.parse(json);
                    oslHandleAttachmentEnvelope(msgId, env);
                } catch (parseErr) {
                    console.error(
                        "[OSL] attachment envelope JSON parse failed msg=" +
                            msgId +
                            ":",
                        parseErr
                    );
                }
            }
        } catch (e) {
            console.log(
                "[OSL] handle decrypt UI side-effect threw: " +
                    (e && e.message ? e.message : e)
            );
        }
        return true;
    };

    // ============================================================
    // Section 8 (Phase 7d-A → 7d-C): Settings gear + cross-window
    //
    // 7d-A through 7d-B4 rendered the settings UI (Identity,
    // Whitelist Manager, Passwords + recovery, Stealth, Burn) as a
    // modal inside the Discord-origin webview. Henry's PR review
    // flagged that hosting password / recovery / identity ops on
    // the remote origin gives Discord-delivered JS reachability
    // into the local Tauri command surface, so 7d-C moved that UI
    // to a trusted local Tauri window served from
    // `osl-gate://localhost/settings`.
    //
    // What stays here:
    //   - Gear icon injection into Discord's `panels__` row
    //     (oslSettingsFindIconRow / oslSettingsGearInject below).
    //   - The click handler now calls `osl_open_settings_window`
    //     instead of rendering a modal in-place.
    //   - Cross-window event listeners (registered at install
    //     time) that re-sync in-memory state when the settings
    //     window mutates the whitelist / burns a scope / changes
    //     password state.
    //
    // What moved (now lives in `assets/settings_window.html`):
    //   - oslSettingsEnsureCss / oslSettingsEnsureRoot / Open / Close
    //   - oslSettingsRenderIdentity / Whitelist / Passwords / About
    //   - oslSettingsConfirm + oslPasswordWizard + helpers
    //   - Stealth / Burn password sections
    // The reusable `oslConfirm` for channel-header burn stays put
    // (it's defined earlier in this file, around section 5).
    // ============================================================

    const SETTINGS_GEAR_ATTR = "data-osl-settings-btn";

    // ---- Gear icon injection ----

    function oslSettingsGearSvg() {
        return (
            '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
            '<circle cx="12" cy="12" r="3"></circle>' +
            '<path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>' +
            "</svg>"
        );
    }

    /**
     * 7d-B1 Task 1: locate the icon-row container inside panels__
     * (the inline row holding Mute / Deafen / User Settings) so the
     * OSL gear sits next to Discord's gear, not below the whole
     * panels__ section as a stray row.
     *
     * Try ordered selectors, first match wins:
     *   1. The User Settings button's parent — most semantic anchor.
     *   2. A panels__ descendant with class containing "container"
     *      that contains the Mute button.
     *   3. The last <div> child of panels__ that has at least one
     *      <button> — heuristic fallback.
     *
     * Returns null when none match; caller falls back to appending
     * to the panels__ section root (the old, ugly placement) and
     * logs once so future selector drift is observable.
     */
    function oslSettingsFindIconRow(panel) {
        const userSettingsBtn = panel.querySelector(
            'button[aria-label*="User Settings"]'
        );
        if (userSettingsBtn && userSettingsBtn.parentElement) {
            return userSettingsBtn.parentElement;
        }
        // Fallback 2: container with the Mute button inside.
        const muteBtn = panel.querySelector('button[aria-label*="Mute"]');
        if (muteBtn) {
            let cur = muteBtn.parentElement;
            while (cur && cur !== panel) {
                if (
                    typeof cur.className === "string" &&
                    cur.className.indexOf("container") !== -1
                ) {
                    return cur;
                }
                cur = cur.parentElement;
            }
            // Fallback within fallback: muteBtn's direct parent if no
            // container__ ancestor in the chain.
            if (muteBtn.parentElement) return muteBtn.parentElement;
        }
        // Fallback 3: last child <div> of panels__ that contains a <button>.
        const children = panel.querySelectorAll(":scope > div");
        for (let i = children.length - 1; i >= 0; i--) {
            if (children[i].querySelector("button")) return children[i];
        }
        return null;
    }

    let oslGearFallbackLogged = false;
    function oslSettingsGearInject() {
        // SAFETY (round-3 lesson): idempotency check FIRST, before
        // any DOM read/write that could trigger a React reconcile
        // loop. We bail before the rAF if the gear already exists.
        const panel = document.querySelector('section[class*="panels__"]');
        if (!panel) return;
        if (panel.querySelector("[" + SETTINGS_GEAR_ATTR + "='1']")) return;
        // SAFETY: defer the actual DOM append to the next animation
        // frame so we don't write into a tree React may still be
        // reconciling on this tick. The idempotency check above
        // re-runs inside the rAF in case a different code path
        // already injected between the two ticks.
        requestAnimationFrame(function () {
            const panelNow = document.querySelector(
                'section[class*="panels__"]'
            );
            if (!panelNow) return;
            if (panelNow.querySelector("[" + SETTINGS_GEAR_ATTR + "='1']"))
                return;
            // 7d-B1 Task 1: try to mount inside Discord's icon row so
            // the gear sits inline with Mute / Deafen / User Settings.
            // Fall back to panels__ root if the row anchor can't be
            // located, so the gear still appears somewhere.
            const row = oslSettingsFindIconRow(panelNow);
            const mountInline = !!row;
            if (!mountInline && !oslGearFallbackLogged) {
                oslGearFallbackLogged = true;
                console.log(
                    "[OSL] gear placement: panels__ icon row not found, " +
                        "falling back to panels__ append"
                );
            }
            // Sample an existing button in the icon row so we can
            // mirror its class list (gives us the right hover ring,
            // sizing, color from Discord's own CSS without us
            // hard-coding pixel values that drift between builds).
            const sample = row
                ? row.querySelector("button") ||
                  row.querySelector('[class*="iconWrapper"]')
                : null;
            const btn = document.createElement("div");
            btn.setAttribute("role", "button");
            btn.setAttribute("tabindex", "0");
            btn.setAttribute("aria-label", "OSL Settings");
            btn.setAttribute(SETTINGS_GEAR_ATTR, "1");
            btn.title = "OSL Settings";
            if (sample && sample.className) {
                btn.className = sample.className;
            }
            btn.style.display = "inline-flex";
            btn.style.alignItems = "center";
            btn.style.justifyContent = "center";
            btn.style.cursor = "pointer";
            // 7d-B2 Task 0.A: always paint the icon color even when
            // we sampled a Discord button. Discord's class on those
            // buttons sets background-color but not text color
            // (their inner <svg fill="currentColor"> inherits from
            // a CSS rule we don't get) — without an explicit color
            // the gear renders black inside an otherwise-styled
            // button. Setting the inline color to `--interactive-normal`
            // with a hover swap to `--interactive-hover` mirrors
            // Discord's panel-icon look.
            btn.style.color = "var(--interactive-normal, #b5bac1)";
            btn.addEventListener("mouseenter", function () {
                btn.style.color = "var(--interactive-hover, #dbdee1)";
            });
            btn.addEventListener("mouseleave", function () {
                btn.style.color = "var(--interactive-normal, #b5bac1)";
            });
            // Only paint our own size/margin when we couldn't sample
            // a sibling button (fallback append). Inline mounts let
            // Discord's icon-row flex layout handle spacing.
            if (!sample) {
                btn.style.width = "32px";
                btn.style.height = "32px";
                btn.style.marginLeft = "4px";
            }
            btn.innerHTML = oslSettingsGearSvg();
            btn.addEventListener("click", function (e) {
                e.stopPropagation();
                // 7d-C: open the trusted local settings window via
                // IPC. Idempotent — Tauri side focuses an existing
                // window if one is already open.
                oslInvoke("osl_open_settings_window", {})
                    .then(function (result) {
                        if (!result.ok) {
                            console.error(
                                "[OSL] open settings window failed: " + result.error
                            );
                            oslToast("Failed to open settings: " + result.error);
                        }
                    });
            });
            const mount = mountInline ? row : panelNow;
            mount.appendChild(btn);
        });
    }

    // ---- Section 7: install ----

    /**
     * Phase 7c: top-level installer. Wires the profile observer,
     * the header observer, and the initial banner load. Called
     * from the DOMContentLoaded gate at the bottom of the IIFE,
     * alongside the existing recv observer + edit-overlay
     * installer.
     *
     * Idempotent: subsequent calls are no-ops.
     */
    let oslPhase7cInstalled = false;
    function oslInstallPhase7c() {
        if (oslPhase7cInstalled) return;
        oslPhase7cInstalled = true;

        // Profile observer: pick up popout/sidebar mounts.
        const profileObs = new MutationObserver(function () {
            const surface = oslFindProfileSurface();
            if (surface) oslInjectProfileButton(surface);
        });
        profileObs.observe(document.body, {
            childList: true,
            subtree: true,
        });

        // Header observer: re-inject buttons when Discord
        // re-mounts the channel header (every navigation).
        // 7d-A: also try to inject the settings gear into the
        // bottom-left panel. The gear injection is idempotent
        // (data-attr guard) so re-running on every observer tick
        // is cheap, and the rAF defer keeps us out of React's way.
        const headerObs = new MutationObserver(function () {
            const header = document.querySelector(
                'section[class*="title_"][class*="container__"]'
            );
            if (header) {
                oslHeaderInjectButtons(header);
                // 7d-D: account burn appears on ALL channel headers,
                // independent of whitelist state. Piggybacks on the
                // same observer to avoid additional mutation watchers.
                oslAccountBurnInject(header);
                oslRefreshBanners(); // banner stack is anchored to header
            }
            oslSettingsGearInject();
            // 7d-PIVOT: composer toggle. Same observer fires on
            // channel switches (the composer remounts) so this
            // catches new-channel mounts naturally. Idempotent.
            try {
                oslComposerToggleInject();
                oslComposerToggleRefreshIfMounted();
            } catch (e) {
                console.warn(
                    "[OSL] composer toggle inject threw:",
                    (e && e.message) || e
                );
            }
        });
        headerObs.observe(document.body, {
            childList: true,
            subtree: true,
        });

        // Initial pass. 7d-FIX2: each injection point is wrapped
        // independently so one fragile site failing (e.g. a Discord
        // class rename breaking `oslAccountBurnInject`) doesn't
        // break the others.
        nativeSetTimeout(function () {
            const header = document.querySelector(
                'section[class*="title_"][class*="container__"]'
            );
            if (header) {
                try {
                    oslHeaderInjectButtons(header);
                } catch (e) {
                    console.warn(
                        "[OSL] initial-pass oslHeaderInjectButtons threw:",
                        (e && e.message) || e
                    );
                }
                try {
                    oslAccountBurnInject(header);
                } catch (e) {
                    console.warn(
                        "[OSL] initial-pass oslAccountBurnInject threw:",
                        (e && e.message) || e
                    );
                }
            }
            try {
                const surface = oslFindProfileSurface();
                if (surface) oslInjectProfileButton(surface);
            } catch (e) {
                console.warn(
                    "[OSL] initial-pass profile button threw:",
                    (e && e.message) || e
                );
            }
            try {
                oslSettingsGearInject();
            } catch (e) {
                console.warn(
                    "[OSL] initial-pass gear inject threw:",
                    (e && e.message) || e
                );
            }
            try {
                oslComposerToggleInject();
            } catch (e) {
                console.warn(
                    "[OSL] initial-pass composer toggle inject threw:",
                    (e && e.message) || e
                );
            }
            try {
                oslRefreshBanners();
            } catch (e) {
                console.warn(
                    "[OSL] initial-pass banner refresh threw:",
                    (e && e.message) || e
                );
            }
        }, 500);

        // 7d-FIX1: hydrate the burned-scopes skip cache once.
        // The recv observer's next sweep then honours it without
        // a per-message Tauri round-trip.
        // 7d-FIX2: wrapped so an init failure can't propagate up
        // and break the rest of Phase 7c install. The .catch on
        // the promise already swallows async rejection; the outer
        // try/catch additionally guards against a synchronous throw
        // before the Promise constructs (e.g. CSP-blocked invoke).
        nativeSetTimeout(function () {
            try {
                oslBurnedScopesInit().catch(function (e) {
                    console.warn(
                        "[OSL][burn] init rejected:",
                        (e && e.message) || e
                    );
                });
            } catch (e) {
                console.warn(
                    "[OSL][burn] init threw synchronously:",
                    (e && e.message) || e
                );
            }
        }, 600);

        // 7d-C: cross-window listeners. The settings window emits
        // these events when it mutates state we cache in-memory
        // here (whitelists, burned-scopes ledger). Tauri 2 routes
        // them through `window.__TAURI__.event.listen` because
        // `withGlobalTauri = true` is set in tauri.conf.json.
        // The main capability grants `core:event:allow-listen`.
        // 7d-FIX2: wrap so a synchronous throw from one of the
        // listen() calls (CSP block) doesn't halt install.
        try {
            oslInstallCrossWindowListeners();
        } catch (e) {
            console.warn(
                "[OSL] oslInstallCrossWindowListeners threw unexpectedly:",
                (e && e.message) || e
            );
        }

        // 7d-FIX3b: snowflake bootstrap. Bootstrap-side
        // verify_peer_map_self_entry handles the common relaunch
        // case (identity has snowflake → repair peer_map). The
        // first-launch case (no snowflake yet) needs boot.js to
        // pull it from Discord runtime + register. Deferred so
        // Discord's React tree is well-populated (the gear +
        // account-burn injections at 500ms already exercise the
        // same surfaces).
        nativeSetTimeout(function () {
            try {
                oslEnsureSelfSnowflakeRegistered();
            } catch (e) {
                console.warn(
                    "[OSL] oslEnsureSelfSnowflakeRegistered threw:",
                    (e && e.message) || e
                );
            }
        }, 1500);

        console.log("[OSL] Phase 7c UI installed");
    }

    /**
     * 7d-FIX3b: extract the local user's Discord snowflake from the
     * page-runtime React tree. Returns null if no strategy succeeds.
     *
     * Strategy A: walk the React fiber on the user-area avatar
     * inside `panels__` (Discord's bottom-left user surface).
     * Strategy B: walk the fiber on the channel-header anchor,
     * which sometimes carries the local user prop further up the
     * parent chain.
     *
     * Both filter on the 17-20 digit numeric format so a stray
     * channel_id / guild_id can't pose as a user id.
     */
    function oslExtractDiscordSnowflakeFromRuntime() {
        function isSnowflake(v) {
            return typeof v === "string" && /^\d{17,20}$/.test(v);
        }

        function fiberFor(el) {
            if (!el) return null;
            try {
                const key = Object.keys(el).find(function (k) {
                    return k.indexOf("__reactFiber") === 0;
                });
                return key ? el[key] : null;
            } catch (_) {
                return null;
            }
        }

        function walk(fiber, maxDepth) {
            let f = fiber;
            const limit = maxDepth || 40;
            for (let d = 0; d < limit && f; d++) {
                try {
                    const p = f.memoizedProps;
                    if (p && typeof p === "object") {
                        if (p.user && typeof p.user === "object") {
                            if (isSnowflake(p.user.id)) return p.user.id;
                        }
                        if (p.currentUser && typeof p.currentUser === "object") {
                            if (isSnowflake(p.currentUser.id))
                                return p.currentUser.id;
                        }
                        if (isSnowflake(p.userId)) return p.userId;
                    }
                    const s = f.memoizedState;
                    if (s && typeof s === "object") {
                        if (s.user && typeof s.user === "object") {
                            if (isSnowflake(s.user.id)) return s.user.id;
                        }
                    }
                } catch (_) {}
                f = f.return;
            }
            return null;
        }

        // Strategy A: panels__ avatar / user area.
        const panel = document.querySelector('section[class*="panels__"]');
        if (panel) {
            const avatarCandidates = panel.querySelectorAll(
                'img[src*="/avatars/"], img[src*="/users/"], ' +
                    '[class*="avatar"], [class*="usernameContainer"], button'
            );
            for (let i = 0; i < avatarCandidates.length; i++) {
                const fiber = fiberFor(avatarCandidates[i]);
                if (!fiber) continue;
                const id = walk(fiber, 30);
                if (id) return id;
            }
            const id = walk(fiberFor(panel), 30);
            if (id) return id;
        }

        // Strategy B: channel-header anchor.
        const header = document.querySelector(
            'section[class*="title_"][class*="container__"]'
        );
        if (header) {
            const id = walk(fiberFor(header), 40);
            if (id) return id;
        }

        return null;
    }

    /**
     * 7d-FIX3b: probe Tauri-side identity for a registered snowflake;
     * if missing, extract from Discord runtime + register via
     * `osl_register_self_snowflake`. Idempotent server-side. Run
     * once per boot. The Rust side validates 17-20 digits and
     * refuses retag to a different snowflake.
     */
    let oslSnowflakeBootstrapDone = false;
    function oslEnsureSelfSnowflakeRegistered() {
        if (oslSnowflakeBootstrapDone) return;
        oslSnowflakeBootstrapDone = true;
        oslInvoke("osl_get_self_user_id", {}).then(function (r) {
            if (r.ok && typeof r.value === "string" && /^\d{17,20}$/.test(r.value)) {
                // Already registered — bootstrap already repaired
                // peer_map. Nothing more to do.
                return;
            }
            const sf = oslExtractDiscordSnowflakeFromRuntime();
            if (!sf) {
                console.warn(
                    "[OSL] could not extract discord snowflake from runtime; " +
                        "some features will be unavailable until next launch"
                );
                return;
            }
            console.log(
                "[OSL] self snowflake extracted from runtime: " + sf
            );
            oslInvoke("osl_register_self_snowflake", { snowflake: sf }).then(
                function (reg) {
                    if (reg.ok) {
                        console.log(
                            "[OSL] self snowflake registered from Discord runtime"
                        );
                        // Reset the cached miss so subsequent
                        // oslSelfDiscordId() calls re-fetch.
                        oslSelfDiscordIdCache = null;
                        oslSelfDiscordIdLastError = null;
                    } else {
                        console.warn(
                            "[OSL] self snowflake registration failed:",
                            reg.error
                        );
                    }
                }
            );
        });
    }

    /**
     * 7d-C Task 5b: register listeners for `osl:*` events emitted
     * by the settings window. Idempotent (guard flag) since this
     * runs from `oslInstallPhase7c` which is itself idempotent.
     *
     * Events handled:
     *   - osl:whitelist_removed   { scope_kind, scope_id, server_id,
     *                               channel_id, peer_discord_id }
     *       Re-evaluate the channel-header lock if the user happens
     *       to be looking at the affected channel.
     *
     *   - osl:scope_burned        { scope_kind, scope_id, server_id,
     *                               channel_id, burn_marker_payload }
     *       Mirror the user's settings-side burn into the local
     *       __oslBurnedScopes cache and trigger receive-observer
     *       repaint via `oslBurnAftermath`. If the settings window
     *       produced a burn-marker payload, ship it via the Discord
     *       send path so other clients in that channel see the
     *       burn marker — settings window can't reach the chat-
     *       input source-rewrite hook directly.
     *
     *   - osl:scope_encryption_toggled  { scope_kind, scope_id, ...,
     *                                     encrypt_toggle }
     *       Re-sync the channel-header lock state.
     *
     *   - osl:password_state_changed   (null)
     *       No-op for now (Discord origin doesn't display password
     *       state). Logged for diagnostics.
     *
     *   - osl:settings_window_closed   (null)
     *       Best-effort: refresh header state in case any of the
     *       above events were missed due to a race.
     */
    let oslCrossWindowListenersInstalled = false;
    function oslInstallCrossWindowListeners() {
        if (oslCrossWindowListenersInstalled) return;
        oslCrossWindowListenersInstalled = true;
        const event =
            window.__TAURI__ && window.__TAURI__.event
                ? window.__TAURI__.event
                : null;
        if (!event || typeof event.listen !== "function") {
            console.warn(
                "[OSL] cross-window listeners: __TAURI__.event.listen unavailable; " +
                    "settings-window state changes will not propagate to Discord origin until reload"
            );
            return;
        }
        // 7d-FIX2: each event.listen() may throw synchronously if
        // Discord's CSP blocks the underlying fetch to Tauri's IPC
        // origin. The earlier version chained `.catch()` AFTER
        // `.listen()`, which never executed because the synchronous
        // throw happens before the Promise is constructed — and the
        // throw propagated up, halting downstream init in
        // `oslInstallPhase7c`. Wrap each call individually so one
        // CSP failure doesn't cascade.
        //
        // Note: event.listen returns a Promise<UnlistenFn> in Tauri 2.
        // We don't store the unlisten fns — these listeners live for
        // the lifetime of the Discord page.
        let registered = 0;
        let failed = 0;
        function safeListen(name, handler) {
            try {
                const p = event.listen(name, handler);
                if (p && typeof p.then === "function") {
                    p.then(
                        function () {
                            // Promise resolved with the unlisten fn —
                            // success is already counted below.
                        },
                        function (err) {
                            // Async rejection (e.g. CSP that throws
                            // via the fetch Promise rather than
                            // synchronously). Log so it surfaces but
                            // don't double-count — the sync return
                            // counted it as registered.
                            console.warn(
                                "[OSL] cross-window listener async rejection: " +
                                    name +
                                    " —",
                                (err && err.message) || err
                            );
                        }
                    );
                }
                registered++;
            } catch (e) {
                failed++;
                console.warn(
                    "[OSL] cross-window listener registration failed: " +
                        name +
                        " —",
                    (e && e.message) || e
                );
            }
        }

        safeListen("osl:whitelist_removed", function (e) {
            try {
                const p = (e && e.payload) || {};
                console.log(
                    "[OSL] event: whitelist_removed " +
                        (p.scope_kind || "?") +
                        ":" +
                        (p.scope_id || "?")
                );
                oslRefreshHeaderState();
            } catch (err) {
                console.error("[OSL] whitelist_removed handler:", err);
            }
        });

        safeListen("osl:scope_burned", function (e) {
            try {
                const p = (e && e.payload) || {};
                console.log(
                    "[OSL] event: scope_burned " +
                        (p.scope_kind || "?") +
                        ":" +
                        (p.scope_id || "?")
                );
                if (p.scope_kind && p.scope_id) {
                    oslBurnedScopesAdd(p.scope_kind, p.scope_id);
                }
                if (p.burn_marker_payload && p.channel_id) {
                    oslSendControlMessage(
                        p.channel_id,
                        p.burn_marker_payload
                    ).catch(function (e2) {
                        console.error(
                            "[OSL] scope_burned send marker:",
                            e2
                        );
                    });
                }
                if (p.channel_id) {
                    try {
                        oslBurnAftermath(p.channel_id);
                    } catch (_) {}
                }
                oslRefreshHeaderState();
            } catch (err) {
                console.error("[OSL] scope_burned handler:", err);
            }
        });

        safeListen("osl:scope_encryption_toggled", function (e) {
            try {
                const p = (e && e.payload) || {};
                console.log(
                    "[OSL] event: scope_encryption_toggled " +
                        (p.scope_kind || "?") +
                        ":" +
                        (p.scope_id || "?") +
                        " → " +
                        (p.encrypt_toggle ? "ON" : "OFF")
                );
                oslRefreshHeaderState({ force: true });
                oslComposerToggleRefreshIfMounted({ force: true });
            } catch (err) {
                console.error(
                    "[OSL] scope_encryption_toggled handler:",
                    err
                );
            }
        });

        // 7d-PIVOT-FIX2 Bug F: post-burn re-engage. Rust un-burns the
        // scope on a successful fresh encrypt and emits this event so
        // every webview drops the scope from its in-memory cache. The
        // receive observer will then resume decrypting incoming DPC0
        // messages in this scope instead of leaving them as ciphertext.
        safeListen("osl:scope_unburned", function (e) {
            try {
                const p = (e && e.payload) || {};
                console.log(
                    "[OSL] event: scope_unburned " +
                        (p.scope_kind || "?") +
                        ":" +
                        (p.scope_id || "?")
                );
                if (p.scope_kind && p.scope_id) {
                    oslBurnedScopesRemove(p.scope_kind, p.scope_id);
                }
            } catch (err) {
                console.error("[OSL] scope_unburned handler:", err);
            }
        });

        // 7d-PIVOT: same event from the new osl_set_scope_encrypt
        // path (composer toggle / settings window). Triggers the
        // same refresh.
        safeListen("osl:scope_encrypt_changed", function (e) {
            try {
                const p = (e && e.payload) || {};
                console.log(
                    "[OSL] event: scope_encrypt_changed " +
                        (p.scope_kind || "?") +
                        ":" +
                        (p.scope_id || "?") +
                        " → " +
                        (p.enabled ? "ON" : "OFF")
                );
                oslRefreshHeaderState({ force: true });
                oslComposerToggleRefreshIfMounted({ force: true });
            } catch (err) {
                console.error("[OSL] scope_encrypt_changed handler:", err);
            }
        });

        safeListen("osl:password_state_changed", function () {
            console.log("[OSL] event: password_state_changed");
        });

        safeListen("osl:settings_window_closed", function () {
            console.log("[OSL] event: settings_window_closed");
            try {
                oslRefreshHeaderState();
            } catch (_) {}
        });

        console.log(
            "[OSL] cross-window listeners: " +
                registered +
                " registered, " +
                failed +
                " failed"
        );
    }

    /**
     * Phase 6a fetch-side edit interception. Top-level entry
     * called from the fetch Proxy when a PATCH /messages/{id}
     * URL is observed. Resolves the body across fetch's three
     * calling conventions (string body, Request object,
     * everything else â†’ passthrough), then hands off to
     * `interceptEditBody` for the encrypt + persist chain.
     *
     * Edit semantics differ from sends in two ways: the URL
     * carries the message_id (so we can scope persist), and a
     * 200/204 response means the edit landed (so the persist
     * call has a hook). Sends use POST and the response carries
     * the assigned id; edits use PATCH and the id was assigned
     * earlier.
     */
    function handleFetchEdit(
        target,
        thisArg,
        args,
        input,
        init,
        channelId,
        messageId
    ) {
        const initBody = init && init.body;
        if (typeof initBody === "string") {
            return interceptEditBody(
                "fetch",
                target,
                thisArg,
                args,
                init,
                null,
                channelId,
                messageId,
                initBody,
                true
            );
        }
        const isRequestObj =
            typeof Request !== "undefined" && input instanceof Request;
        if (isRequestObj) {
            let cloned;
            try {
                cloned = input.clone();
            } catch (e) {
                console.error(
                    "[OSL] failed to clone Request (fetch edit); passthrough",
                    e
                );
                return Reflect.apply(target, thisArg, args);
            }
            return cloned.text().then(
                function (bodyText) {
                    if (!bodyText) {
                        return Reflect.apply(target, thisArg, args);
                    }
                    return interceptEditBody(
                        "fetch",
                        target,
                        thisArg,
                        args,
                        init,
                        input,
                        channelId,
                        messageId,
                        bodyText,
                        false
                    );
                },
                function (err) {
                    console.error(
                        "[OSL] failed to read Request body (fetch edit); passthrough",
                        err
                    );
                    return Reflect.apply(target, thisArg, args);
                }
            );
        }
        if (initBody != null && DEBUG) {
            const bodyKind =
                (initBody.constructor && initBody.constructor.name) ||
                typeof initBody;
            console.log(
                "[OSL] outgoing edit (fetch PATCH): channel=" +
                    channelId +
                    " msg=" +
                    messageId +
                    " non-string init.body (" +
                    bodyKind +
                    "); passthrough"
            );
        }
        return Reflect.apply(target, thisArg, args);
    }

    /**
     * Encrypt-and-persist body of an edit request, parameterised
     * over fetch / XHR. The XHR caller passes its own callbacks
     * for `onMutated` / `onPassthrough` / `onAbort` because the
     * dispatch shape differs (no Promise, has to synthesise a
     * load event via the meta-stash); the fetch caller uses the
     * default Promise-chained path baked into this fn via
     * `viaInit` / `requestInput`.
     *
     * Both flows:
     *  - early-return if `parsed.content` is missing, empty, or
     *    already DPC0::-prefixed (defensive non-double-encrypt)
     *  - capture `parsed.content` as the *original plaintext*
     *  - hand off to `interceptBody` to swap in the cover
     *  - on success log the spec-shaped one-line edit summary
     *    (`[OSL] outgoing edit (<source>): channel=â€¦ msg=â€¦
     *     orig_len=â€¦ wire_len=â€¦`) and queue
     *    `runPersistEdit(messageId, origPlaintext)` for after the
     *    network leg returns 200/204.
     */
    function interceptEditBody(
        source,
        target,
        thisArg,
        args,
        init,
        requestInput,
        channelId,
        messageId,
        bodyText,
        viaInit
    ) {
        let parsed;
        try {
            parsed = JSON.parse(bodyText);
        } catch (e) {
            return Reflect.apply(target, thisArg, args);
        }
        if (typeof parsed.content !== "string" || parsed.content === "") {
            // Non-content edit (flags, mentions toggle, etc.) â€”
            // nothing to encrypt.
            return Reflect.apply(target, thisArg, args);
        }
        if (parsed.content.indexOf("DPC0::") === 0) {
            console.log(
                "[OSL] editTab safetynet HIT (" +
                    source +
                    " PATCH): channel=" +
                    channelId +
                    " msg=" +
                    messageId +
                    " content still DPC0:: at submit time " +
                    "â€” Slate swap failed; edit submitted as no-op"
            );
            return Reflect.apply(target, thisArg, args);
        }
        const origPlaintext = parsed.content;

        return interceptBody(
            source + " edit",
            channelId,
            bodyText,
            function (newBody) {
                let wireLen = -1;
                try {
                    const newParsed = JSON.parse(newBody);
                    if (typeof newParsed.content === "string") {
                        wireLen = newParsed.content.length;
                    }
                } catch (e) {
                    // Log-only; swallow.
                }
                console.log(
                    "[OSL] outgoing edit (" +
                        source +
                        "): channel=" +
                        channelId +
                        " msg=" +
                        messageId +
                        " orig_len=" +
                        origPlaintext.length +
                        " wire_len=" +
                        wireLen
                );
                let newArgs;
                if (viaInit) {
                    const newInit = Object.assign({}, init, {
                        body: newBody,
                    });
                    newArgs = [args[0], newInit];
                } else {
                    newArgs = [
                        new Request(requestInput, { body: newBody }),
                    ];
                }
                const fetchResult = Reflect.apply(target, thisArg, newArgs);
                if (fetchResult && typeof fetchResult.then === "function") {
                    fetchResult.then(
                        function (resp) {
                            try {
                                if (resp && resp.ok) {
                                    runPersistEdit(messageId, origPlaintext);
                                }
                            } catch (e) {
                                console.error(
                                    "[OSL] persist-edit chain (fetch) threw",
                                    e
                                );
                            }
                        },
                        function () {
                            // Fetch rejected by the encrypt path
                            // or network; no persist call.
                        }
                    );
                }
                return fetchResult;
            },
            function () {
                return Reflect.apply(target, thisArg, args);
            },
            function () {
                return Promise.reject(new TypeError("Failed to fetch"));
            }
        );
    }

    /**
     * Phase 6a XHR-side edit interception. Mirrors
     * `handleFetchEdit` but with the XHR dispatch shape:
     * `xhr.send(body)` returns `undefined` synchronously, so
     * the encrypt path runs through `interceptBody` and the
     * mutated `Reflect.apply(target, xhrInst, [newBody])` is
     * fire-and-forget. The persist call rides a passive
     * `addEventListener("load", â€¦)` on the XHR instance,
     * matching the existing send-side selfSentAuthors pattern.
     */
    function handleXhrEdit(target, thisArg, args, body, channelId, messageId) {
        const xhrInst = thisArg;
        if (typeof body !== "string") {
            if (DEBUG && body !== undefined && body !== null) {
                const bodyKind =
                    (body.constructor && body.constructor.name) ||
                    typeof body;
                console.log(
                    "[OSL] outgoing edit (XHR PATCH): channel=" +
                        channelId +
                        " msg=" +
                        messageId +
                        " non-string body (" +
                        bodyKind +
                        "); passthrough"
                );
            }
            return Reflect.apply(target, thisArg, args);
        }
        let parsed;
        try {
            parsed = JSON.parse(body);
        } catch (e) {
            return Reflect.apply(target, thisArg, args);
        }
        if (typeof parsed.content !== "string" || parsed.content === "") {
            return Reflect.apply(target, thisArg, args);
        }
        if (parsed.content.indexOf("DPC0::") === 0) {
            console.log(
                "[OSL] editTab safetynet HIT (XHR PATCH): channel=" +
                    channelId +
                    " msg=" +
                    messageId +
                    " content still DPC0:: at submit time " +
                    "â€” Slate swap failed; edit submitted as no-op"
            );
            return Reflect.apply(target, thisArg, args);
        }
        const origPlaintext = parsed.content;
        const origBody = body;

        // Passive load listener â€” runs persist on a 200/204
        // edit response. Defensively populates selfSentAuthors
        // too, in case the user is editing a message they sent
        // before this session started (no prior /messages POST
        // populated the cache for that id).
        xhrInst.addEventListener("load", function () {
            try {
                if (xhrInst.readyState !== 4) return;
                if (xhrInst.status !== 200 && xhrInst.status !== 204) {
                    return;
                }
                try {
                    const respText = xhrInst.responseText || "";
                    if (respText) {
                        const parsedResp = JSON.parse(respText);
                        if (
                            parsedResp &&
                            typeof parsedResp.id === "string" &&
                            parsedResp.author &&
                            typeof parsedResp.author.id === "string"
                        ) {
                            selfSentAuthors.set(
                                parsedResp.id,
                                parsedResp.author.id
                            );
                            while (
                                selfSentAuthors.size > SELF_SENT_AUTHORS_MAX
                            ) {
                                const oldest = selfSentAuthors
                                    .keys()
                                    .next().value;
                                if (oldest === undefined) break;
                                selfSentAuthors.delete(oldest);
                            }
                        }
                    }
                } catch (e) {
                    // Response missing / non-JSON â€” fine, just
                    // skip the defensive populate.
                }
                runPersistEdit(messageId, origPlaintext);
            } catch (e) {
                // Listener never throws.
            }
        });

        const result = interceptBody(
            "XHR edit",
            channelId,
            body,
            function (newBody) {
                let wireLen = -1;
                try {
                    const newParsed = JSON.parse(newBody);
                    if (typeof newParsed.content === "string") {
                        wireLen = newParsed.content.length;
                    }
                } catch (e) {
                    // Log-only; swallow.
                }
                console.log(
                    "[OSL] outgoing edit (XHR): channel=" +
                        channelId +
                        " msg=" +
                        messageId +
                        " orig_len=" +
                        origPlaintext.length +
                        " wire_len=" +
                        wireLen
                );
                try {
                    Reflect.apply(target, xhrInst, [newBody]);
                } catch (e) {
                    console.error(
                        "[OSL] origSend with mutated body threw (XHR edit)",
                        e
                    );
                }
            },
            function () {
                try {
                    Reflect.apply(target, xhrInst, [origBody]);
                } catch (e) {
                    console.error(
                        "[OSL] origSend (passthrough) threw (XHR edit)",
                        e
                    );
                }
            },
            function () {
                // Fail-closed â€” synthesise the same
                // error/loadend pair the send path uses on a
                // failed encrypt, so Discord shows "Failed to
                // edit" rather than a stuck spinner.
                setTimeout(function () {
                    try {
                        xhrInst.dispatchEvent(new ProgressEvent("error"));
                        xhrInst.dispatchEvent(new ProgressEvent("loadend"));
                    } catch (e) {
                        console.error(
                            "[OSL] XHR edit failure synthesis dispatchEvent threw",
                            e
                        );
                    }
                }, 0);
            }
        );

        if (result && typeof result.catch === "function") {
            result.catch(function (err) {
                console.error(
                    "[OSL] XHR edit intercept tail caught (rare):",
                    err
                );
            });
        }
        return undefined;
    }

    /**
     * Resolve `(input, init)` to a stable {url, method} pair across
     * the three calling conventions Fetch supports.
     */
    function resolveFetchRequest(input, init) {
        let url;
        const isRequestObj =
            typeof Request !== "undefined" && input instanceof Request;
        if (typeof input === "string") {
            url = input;
        } else if (isRequestObj) {
            url = input.url;
        } else if (input && typeof input.toString === "function") {
            url = String(input);
        } else {
            return null;
        }

        let method;
        if (init && typeof init.method === "string") {
            method = init.method;
        } else if (isRequestObj) {
            method = input.method;
        } else {
            method = "GET";
        }
        method = method.toUpperCase();

        return { url: url, method: method, isRequestObj: isRequestObj };
    }

    // ============================================================
    // Capture originals BEFORE any wrapping. These references are
    // closed over by the proxy handlers below; once
    // `window.fetch` / `XMLHttpRequest.prototype.{open,send}` /
    // `Function.prototype.toString` are replaced, callers can no
    // longer reach the originals from outside our IIFE.
    // ============================================================
    const origFetch = window.fetch.bind(window);
    const haveXhr = typeof XMLHttpRequest !== "undefined";
    const origOpen = haveXhr ? XMLHttpRequest.prototype.open : null;
    const origSend = haveXhr ? XMLHttpRequest.prototype.send : null;
    const origSetRequestHeader = haveXhr
        ? XMLHttpRequest.prototype.setRequestHeader
        : null;
    const origFnToString = Function.prototype.toString;

    // Symbol-keyed metadata stash (Symbol over string property to
    // avoid any chance of name collision with Discord properties).
    const OSL_XHR_META = Symbol("OSL_XHR_META");

    // ============================================================
    // Build proxy handlers. The `get` trap is the toString spoof
    // (instance-level); the `apply` trap is the actual hook logic.
    // Other property accesses fall through to the target via
    // Reflect.get to preserve native semantics for descriptor
    // introspection.
    // ============================================================

    /**
     * Build a `get` trap that returns a spoof toString function for
     * `'toString'` access and otherwise forwards to the target.
     */
    function makeToStringGetTrap(spoofString) {
        return function (target, prop, receiver) {
            if (prop === "toString") {
                // Return a spoof function. We bind it freshly each
                // call rather than caching, so the returned function
                // identity matches what a fresh property access
                // would produce on a native â€” defeats simple
                // identity-comparison checks like
                // `fn1.toString === fn2.toString` (would-be true on
                // native, false on cached spoof).
                return function () {
                    return spoofString;
                };
            }
            return Reflect.get(target, prop, receiver);
        };
    }

    function makeFetchHandler() {
        return {
            get: makeToStringGetTrap("function fetch() { [native code] }"),

            apply: function (target, thisArg, args) {
                const input = args[0];
                const init = args[1];

                // Phase 6a edit-overlay: passively capture Authorization
                // header from any outgoing Discord API request so we can
                // authenticate our own PATCHes from the edit overlay.
                try {
                    const sniffInit = args[1];
                    if (sniffInit && sniffInit.headers) {
                        const h = sniffInit.headers;
                        let auth = null;
                        if (typeof h.get === "function") {
                            auth =
                                h.get("Authorization") ||
                                h.get("authorization");
                        } else if (typeof h === "object") {
                            auth = h.Authorization || h.authorization;
                        }
                        if (typeof auth === "string" && auth.length > 0) {
                            editOverlayAuthToken = auth;
                        }
                    }
                    // Also try Request input
                    if (
                        typeof Request !== "undefined" &&
                        args[0] instanceof Request &&
                        args[0].headers
                    ) {
                        const auth =
                            args[0].headers.get("Authorization") ||
                            args[0].headers.get("authorization");
                        if (typeof auth === "string" && auth.length > 0) {
                            editOverlayAuthToken = auth;
                        }
                    }
                } catch (e) {
                    // Token sniff is best-effort; never fail the request.
                }

                const resolved = resolveFetchRequest(input, init);
                if (resolved === null) {
                    return Reflect.apply(target, thisArg, args);
                }
                const url = resolved.url;
                const method = resolved.method;

                const editMatch = EDIT_RE.exec(url);
                if (editMatch && method === "PATCH") {
                    return handleFetchEdit(
                        target,
                        thisArg,
                        args,
                        input,
                        init,
                        editMatch[1],
                        editMatch[2]
                    );
                }

                // 8c: production attachment interception is wired on
                // the XHR side (Discord uses XHR for all three steps
                // of the GCS upload flow), so this fetch path only
                // logs the case Discord migrates a step to fetch in
                // a future build. We do not currently intercept GCS
                // PUTs via fetch — if a future Discord build needs
                // it, mirror oslInterceptStep2GcsPut here.
                if (
                    method === "PUT" &&
                    GCS_UPLOAD_RE.test(url) &&
                    window.__oslPendingUploads.size > 0
                ) {
                    console.warn(
                        "[OSL] GCS PUT via fetch detected; pending uploads exist but fetch-side interception is not wired. Add it if this fires."
                    );
                }

                const sendMatch = SEND_RE.exec(url);
                if (!sendMatch || method !== "POST") {
                    return Reflect.apply(target, thisArg, args);
                }
                const channelId = sendMatch[1];

                const initBody = init && init.body;

                if (typeof initBody === "string") {
                    return interceptBody(
                        "fetch",
                        channelId,
                        initBody,
                        function (newBody) {
                            const newInit = Object.assign({}, init, {
                                body: newBody,
                            });
                            return Reflect.apply(target, thisArg, [
                                input,
                                newInit,
                            ]);
                        },
                        function () {
                            // onPassthrough â€” no plaintext to
                            // encrypt; safe to forward.
                            return Reflect.apply(target, thisArg, args);
                        },
                        function () {
                            // onAbort â€” Phase 4 fail-closed.
                            // Reject the fetch Promise to simulate
                            // a network failure; Discord shows the
                            // message as "Failed to send" rather
                            // than leaking plaintext on the wire.
                            return Promise.reject(
                                new TypeError("Failed to fetch")
                            );
                        }
                    );
                }

                if (initBody != null) {
                    if (DEBUG) {
                        const bodyKind =
                            (initBody.constructor &&
                                initBody.constructor.name) ||
                            typeof initBody;
                        console.log(
                            "[OSL] outgoing /messages (fetch): non-string init.body (" +
                                bodyKind +
                                "); passthrough (Phase 4 will handle multipart)"
                        );
                    }
                    return Reflect.apply(target, thisArg, args);
                }

                if (resolved.isRequestObj) {
                    let cloned;
                    try {
                        cloned = input.clone();
                    } catch (e) {
                        console.error(
                            "[OSL] failed to clone Request (fetch); passthrough",
                            e
                        );
                        return Reflect.apply(target, thisArg, args);
                    }
                    return cloned.text().then(
                        function (bodyText) {
                            if (!bodyText) {
                                return Reflect.apply(target, thisArg, args);
                            }
                            return interceptBody(
                                "fetch",
                                channelId,
                                bodyText,
                                function (newBody) {
                                    return Reflect.apply(target, thisArg, [
                                        new Request(input, { body: newBody }),
                                    ]);
                                },
                                function () {
                                    // onPassthrough â€” see string-
                                    // body branch above.
                                    return Reflect.apply(target, thisArg, args);
                                },
                                function () {
                                    // onAbort â€” Phase 4 fail-closed.
                                    return Promise.reject(
                                        new TypeError("Failed to fetch")
                                    );
                                }
                            );
                        },
                        function (err) {
                            console.error(
                                "[OSL] failed to read Request body (fetch); passthrough",
                                err
                            );
                            return Reflect.apply(target, thisArg, args);
                        }
                    );
                }

                return Reflect.apply(target, thisArg, args);
            },
        };
    }

    function makeOpenHandler() {
        return {
            get: makeToStringGetTrap("function open() { [native code] }"),

            apply: function (target, thisArg, args) {
                // args = [method, url, async?, user?, password?]
                try {
                    thisArg[OSL_XHR_META] = {
                        method:
                            typeof args[0] === "string"
                                ? args[0].toUpperCase()
                                : "GET",
                        url:
                            typeof args[1] === "string"
                                ? args[1]
                                : args[1] == null
                                ? ""
                                : String(args[1]),
                        async: args[2] !== false,
                    };
                } catch (e) {
                    console.error(
                        "[OSL] failed to stash XHR meta on open(); passthrough",
                        e
                    );
                }
                return Reflect.apply(target, thisArg, args);
            },
        };
    }

    /**
     * Phase 6a edit-overlay: passively capture the Authorization
     * header off every outgoing XHR. Discord's API client mostly
     * uses XHR (not fetch) for /messages traffic, so without this
     * hook the overlay would have no token to authenticate its
     * own PATCH with on a long-lived session.
     */
    function makeSetRequestHeaderHandler() {
        return {
            get: makeToStringGetTrap(
                "function setRequestHeader() { [native code] }"
            ),

            apply: function (target, thisArg, args) {
                try {
                    if (
                        typeof args[0] === "string" &&
                        args[0].toLowerCase() === "authorization" &&
                        typeof args[1] === "string" &&
                        args[1].length > 0
                    ) {
                        editOverlayAuthToken = args[1];
                    }
                } catch (e) {
                    // Token sniff is best-effort; never fail the
                    // request.
                }
                return Reflect.apply(target, thisArg, args);
            },
        };
    }

    function makeSendHandler() {
        return {
            get: makeToStringGetTrap("function send() { [native code] }"),

            apply: function (target, thisArg, args) {
                const body = args[0];
                const meta = thisArg[OSL_XHR_META];

                if (!meta || !meta.async) {
                    return Reflect.apply(target, thisArg, args);
                }

                const editMatch = EDIT_RE.exec(meta.url);
                if (editMatch && meta.method === "PATCH") {
                    return handleXhrEdit(
                        target,
                        thisArg,
                        args,
                        body,
                        editMatch[1],
                        editMatch[2]
                    );
                }

                // 8c step 2: PUT to GCS upload URL. Different
                // origin from the discord.com API. Async — read the
                // file bytes, seal, replace body.
                const gcsMatch = GCS_UPLOAD_RE.exec(meta.url);
                if (gcsMatch && meta.method === "PUT") {
                    const uploadId = gcsMatch[1];
                    oslInterceptStep2GcsPut(target, thisArg, args, uploadId).catch(
                        function (err) {
                            console.error(
                                "[OSL] step2 GCS PUT intercept threw:",
                                err
                            );
                            oslAbortXhr(
                                thisArg,
                                "step2 threw: " +
                                    (err && err.message ? err.message : err)
                            );
                        }
                    );
                    return undefined;
                }

                // 8c step 1: POST /channels/{cid}/attachments
                // (pre-upload). Same-origin XHR, JSON body.
                const attMatch = ATTACHMENTS_RE.exec(meta.url);
                if (attMatch && meta.method === "POST") {
                    if (typeof body === "string") {
                        oslInterceptStep1Attachments(
                            target,
                            thisArg,
                            args,
                            attMatch[1],
                            body
                        ).catch(function (err) {
                            console.error(
                                "[OSL] step1 attachments intercept threw:",
                                err
                            );
                            oslAbortXhr(
                                thisArg,
                                "step1 threw: " +
                                    (err && err.message ? err.message : err),
                                attMatch[1]
                            );
                        });
                        return undefined;
                    }
                    // Body isn't a string — Discord might one day
                    // ship FormData here; passthrough so the user
                    // sees plain attachments rather than a silent
                    // break.
                    console.log(
                        "[OSL] step1: non-string body, passthrough"
                    );
                    return Reflect.apply(target, thisArg, args);
                }

                const sendMatch = SEND_RE.exec(meta.url);
                if (!sendMatch || meta.method !== "POST") {
                    return Reflect.apply(target, thisArg, args);
                }
                const channelId = sendMatch[1];

                // Phase 8b: FormData bodies carry multipart
                // attachment uploads (Discord's single-step
                // /messages POST). Dispatch async — XHR.send has
                // no synchronous return contract beyond "queues
                // the send" so deferring the actual Reflect.apply
                // until encryption completes is observably the
                // same to Discord's load/error handlers.
                if (
                    typeof FormData !== "undefined" &&
                    body instanceof FormData
                ) {
                    oslInterceptMultipartXhr(
                        target,
                        thisArg,
                        args,
                        channelId,
                        body
                    ).catch(function (err) {
                        console.error(
                            "[OSL] multipart XHR intercept threw:",
                            err
                        );
                        try {
                            setTimeout(function () {
                                try {
                                    thisArg.dispatchEvent(
                                        new ProgressEvent("error")
                                    );
                                    thisArg.dispatchEvent(
                                        new ProgressEvent("loadend")
                                    );
                                } catch (_) {}
                            }, 0);
                        } catch (_) {}
                    });
                    return undefined;
                }

                if (typeof body !== "string") {
                    if (DEBUG && body !== undefined && body !== null) {
                        const bodyKind =
                            (body.constructor && body.constructor.name) ||
                            typeof body;
                        console.log(
                            "[OSL] outgoing /messages (XHR): non-string non-FormData body (" +
                                bodyKind +
                                "); passthrough"
                        );
                    }
                    return Reflect.apply(target, thisArg, args);
                }

                const xhrInst = thisArg;
                const origBody = body;

                // Outbound-capture: passive `load` listener on
                // this XHR instance. When Discord's response
                // arrives 2xx with a JSON body containing
                // `{id, author: {id}}`, stash the mapping into
                // `selfSentAuthors` so the receive observer can
                // attribute the bounced-back own-send even when
                // Discord's cozy-grouping renders the
                // continuation list-item without
                // `data-author-id`. Wired via
                // `addEventListener` rather than `xhr.onload =`
                // so it's additive â€” we don't displace any
                // handler Discord may have set. The whole body
                // is wrapped in try/catch to ensure the listener
                // never throws regardless of response shape.
                xhrInst.addEventListener("load", function () {
                    try {
                        if (xhrInst.readyState !== 4) return;
                        if (
                            xhrInst.status !== 200 &&
                            xhrInst.status !== 201
                        ) {
                            return;
                        }
                        const parsed = JSON.parse(xhrInst.responseText);
                        if (!parsed || typeof parsed !== "object") return;
                        if (typeof parsed.id !== "string") return;
                        if (
                            !parsed.author ||
                            typeof parsed.author.id !== "string"
                        ) {
                            return;
                        }
                        selfSentAuthors.set(parsed.id, parsed.author.id);
                        // FIFO eviction: Map preserves insertion
                        // order, so the first key is the oldest.
                        while (
                            selfSentAuthors.size > SELF_SENT_AUTHORS_MAX
                        ) {
                            const oldest = selfSentAuthors
                                .keys()
                                .next().value;
                            if (oldest === undefined) break;
                            selfSentAuthors.delete(oldest);
                        }
                        console.log(
                            "[OSL] selfSent capture msg=" +
                                parsed.id +
                                " author=" +
                                parsed.author.id
                        );
                    } catch (e) {
                        // Swallowed â€” listener never throws.
                    }
                });

                const result = interceptBody(
                    "XHR",
                    channelId,
                    body,
                    function (newBody) {
                        try {
                            Reflect.apply(target, xhrInst, [newBody]);
                        } catch (e) {
                            console.error(
                                "[OSL] origSend with mutated body threw (XHR)",
                                e
                            );
                        }
                    },
                    function () {
                        // onPassthrough â€” no plaintext to encrypt.
                        try {
                            Reflect.apply(target, xhrInst, [origBody]);
                        } catch (e) {
                            console.error(
                                "[OSL] origSend (passthrough) threw (XHR)",
                                e
                            );
                        }
                    },
                    function () {
                        // onAbort â€” Phase 4 fail-closed.
                        //
                        // XHR has no Promise to reject (we already
                        // returned `undefined` from the apply trap
                        // synchronously). Synthesize a network-
                        // failure event sequence on a microtask so
                        // Discord's onerror / onloadend handlers
                        // fire and the UI shows "Failed to send"
                        // rather than a stuck-sending spinner.
                        //
                        // Caveat (documented in Phase 4 design
                        // notes Â§13): `xhr.readyState` and `status`
                        // stay at their pre-send values (1 / 0).
                        // Most callers gate on the error event
                        // firing rather than reading those, but
                        // a Phase 4b refinement could synthesize
                        // a more complete failed state.
                        //
                        // Crucially we do NOT call `origSend`, so
                        // no network request leaves the box with
                        // plaintext content.
                        setTimeout(function () {
                            try {
                                xhrInst.dispatchEvent(
                                    new ProgressEvent("error")
                                );
                                xhrInst.dispatchEvent(
                                    new ProgressEvent("loadend")
                                );
                            } catch (e) {
                                console.error(
                                    "[OSL] XHR failure synthesis dispatchEvent threw",
                                    e
                                );
                            }
                        }, 0);
                    }
                );

                if (result && typeof result.catch === "function") {
                    result.catch(function (err) {
                        console.error(
                            "[OSL] XHR intercept tail caught (rare):",
                            err
                        );
                    });
                }

                return undefined;
            },
        };
    }

    // ============================================================
    // Install the proxies. Per-hook idempotency guards so a
    // double-init doesn't chain wrappers.
    // ============================================================

    let fetchInstalled = false;
    let fetchProxy = null;
    if (!window.__OSL_FETCH_HOOK_INSTALLED__) {
        window.__OSL_FETCH_HOOK_INSTALLED__ = true;
        fetchProxy = new Proxy(origFetch, makeFetchHandler());

        // Round 6 diagnostic confirmed: Sentry's instrumentation
        // overwrites `window.fetch` after our init script runs
        // (delayed @+3s probe showed `window.fetch === fetchProxy`
        // â†’ false in one of the two webview contexts; toString
        // returned Sentry's wrapper source). Our Proxy and FPT trap
        // were correct â€” just displaced.
        //
        // Lock the property non-writable + non-configurable so
        // Sentry's later `window.fetch = sentryWrapper` assignment
        // cannot displace us:
        //   - In strict mode: assignment throws TypeError. Sentry
        //     wraps its instrumentation in try/catch (they have to â€”
        //     they can't crash apps), so the throw is swallowed and
        //     Sentry's wrapper simply doesn't install.
        //   - In sloppy mode: assignment silently fails. Same
        //     net effect; Sentry's wrapper doesn't install.
        //   - Sentry attempting `Object.defineProperty` instead of
        //     assignment also fails because configurable: false
        //     forbids further redefinition.
        //
        // `enumerable: true` preserved explicitly so detection
        // checks like `Object.keys(window).includes("fetch")` see
        // the same shape they would on an unmodified client (the
        // default for `Object.defineProperty` is `enumerable: false`,
        // which would otherwise change visibility).
        //
        // Side effect: Sentry's fetch-specific telemetry (request
        // URLs, response codes, error categorisation in Sentry's
        // breadcrumbs) is gone for this client. Sentry's XHR
        // instrumentation is independent of fetch and still
        // functions for paths Discord routes through XHR. Sentry's
        // other observability (unhandled rejections, console
        // capture, etc.) is unaffected. Net: Discord loses one
        // dimension of breadcrumb data on this client; for the OSL
        // threat model â€” Discord shouldn't be able to read message
        // content â€” this is an acceptable trade.
        Object.defineProperty(window, "fetch", {
            value: fetchProxy,
            writable: false,
            configurable: false,
            enumerable: true,
        });
        fetchInstalled = true;

        // ===== Post-fix verification probes =====
        //
        // These were originally added to diagnose the leak (round 6
        // diagnostic round). They now serve as ongoing
        // self-verification: every boot run logs the canary state
        // immediately and again at +3s. If we ever see the delayed
        // canary flip to `false` again, it means Sentry (or some
        // other consumer) found a way around our defineProperty
        // lock and we'd need to investigate further.
        //
        // Prefix `[OSL DIAG]` so probe output is easy to grep
        // separately from normal `[OSL]` operational logs.
        if (DEBUG) {
            console.log(
                "[OSL DIAG] immediate probe 1 (window.fetch.toString()):",
                window.fetch.toString()
            );
            console.log(
                "[OSL DIAG] immediate probe 2 (window.fetch['toString']):",
                window.fetch["toString"]
            );
            console.log(
                "[OSL DIAG] immediate probe 3 (Reflect.get(window.fetch, 'toString')):",
                Reflect.get(window.fetch, "toString")
            );
            console.log(
                "[OSL DIAG] immediate probe 4 (window.fetch === fetchProxy):",
                window.fetch === fetchProxy
            );

            const capturedFetchProxy = fetchProxy;
            setTimeout(function () {
                const stillOurs = window.fetch === capturedFetchProxy;
                console.log(
                    "[OSL DIAG] delayed @+3s (window.fetch === fetchProxy):",
                    stillOurs
                );
                console.log(
                    "[OSL DIAG] delayed @+3s (window.fetch.toString()):",
                    window.fetch.toString()
                );
                console.log(
                    "[OSL DIAG] delayed @+3s (FPT.call(window.fetch)):",
                    Function.prototype.toString.call(window.fetch)
                );
                if (!stillOurs) {
                    // The defineProperty lock was supposed to make
                    // this impossible. If we hit it, Sentry (or
                    // someone) found a path around: prototype
                    // manipulation, frame-based reassignment, or a
                    // browser quirk we didn't anticipate.
                    console.error(
                        "[OSL DIAG] CANARY BROKEN: window.fetch was reassigned " +
                            "despite defineProperty(writable:false, configurable:false). " +
                            "Investigation needed."
                    );
                }
            }, 3000);
        }
    }

    let xhrInstalled = false;
    let openProxy = null;
    let sendProxy = null;
    let setRequestHeaderProxy = null;
    if (!window.__OSL_XHR_HOOK_INSTALLED__ && haveXhr) {
        window.__OSL_XHR_HOOK_INSTALLED__ = true;
        openProxy = new Proxy(origOpen, makeOpenHandler());
        sendProxy = new Proxy(origSend, makeSendHandler());
        setRequestHeaderProxy = new Proxy(
            origSetRequestHeader,
            makeSetRequestHeaderHandler()
        );
        XMLHttpRequest.prototype.open = openProxy;
        XMLHttpRequest.prototype.send = sendProxy;
        XMLHttpRequest.prototype.setRequestHeader = setRequestHeaderProxy;
        xhrInstalled = true;
    }

    // ============================================================
    // Function.prototype.toString trap. Defeats
    //   Function.prototype.toString.call(window.fetch)
    // and
    //   Function.prototype.toString.apply(window.fetch)
    // which bypass instance-level toString overrides and would
    // otherwise still see the wrapper source.
    //
    // Only installed if at least one hook went in; if both fetch
    // and XHR are already wrapped (re-init), we re-use whatever
    // toString trap was installed previously.
    // ============================================================

    if (
        !window.__OSL_FN_TOSTRING_HOOK_INSTALLED__ &&
        (fetchInstalled || xhrInstalled)
    ) {
        window.__OSL_FN_TOSTRING_HOOK_INSTALLED__ = true;

        // WeakMap so the hooked-function references don't
        // accidentally pin them in memory and so the map can't be
        // iterated for fingerprinting.
        const SPOOFED = new WeakMap();
        if (fetchInstalled) {
            SPOOFED.set(fetchProxy, "function fetch() { [native code] }");
        }
        if (xhrInstalled) {
            SPOOFED.set(openProxy, "function open() { [native code] }");
            SPOOFED.set(sendProxy, "function send() { [native code] }");
            SPOOFED.set(
                setRequestHeaderProxy,
                "function setRequestHeader() { [native code] }"
            );
        }

        Function.prototype.toString = new Proxy(origFnToString, {
            apply: function (target, thisArg, args) {
                if (SPOOFED.has(thisArg)) {
                    return SPOOFED.get(thisArg);
                }
                return Reflect.apply(target, thisArg, args);
            },
        });
    }

    // ============================================================
    // Final install log.
    // ============================================================

    if (DEBUG) {
        const installedHooks = [];
        if (fetchInstalled) installedHooks.push("fetch");
        if (xhrInstalled) installedHooks.push("XHR");
        if (installedHooks.length > 0) {
            console.log(
                "[OSL] Boot script installed; hooks: " +
                    installedHooks.join(" + ")
            );
        } else {
            console.log(
                "[OSL] Boot script: all hooks already installed; skipping"
            );
        }
    }


    // ============================================================
    // Phase 5 v1: DOM MutationObserver receive hook.
    //
    // Watches message-content elements as Discord renders them,
    // matches our cover-string prefix ("DPC0::"), pulls the
    // sender's user_id out of the surrounding DOM context, then
    // asks Rust to decrypt. On success we replace `textContent`
    // in-place so the user reads the plaintext rather than the
    // base64 cover.
    //
    // ## Why DOM-layer (vs FluxDispatcher / WebSocket)
    //
    // Layer 10 Â§14 walks through three rounds of internal-hook
    // recon (FluxDispatcher store discovery, WebSocket gateway
    // intercept). Both are reachable but couple the mod tightly
    // to Discord's reducer ordering and obfuscated module IDs;
    // a single bundle refactor breaks them silently. The DOM is
    // the one Discord-facing API that's observable, public, and
    // stable enough to bet on for v1.
    //
    // ## Accepted v1 limitations (documented in Â§14.6 + README)
    //
    //   1. **Brief flash** of the DPC0:: cover before async
    //      decrypt completes â€” typically tens to a few hundred
    //      milliseconds.
    //   2. **DOM-mutation fragility**: any major Discord
    //      message-renderer refactor can break the observer.
    //      Treated as ongoing maintenance.
    //   3. **Sender's own messages flash too** â€” the encoder
    //      auto-includes the sender as a recipient slot so the
    //      flow is symmetrical, but the cover still renders
    //      first and is replaced by the observer.
    //   4. **Best-effort author_id extraction** â€” pulled from
    //      avatar URL or `data-author-id`; if neither is present
    //      we skip rather than guess (safe default).
    //
    // v2 design: separate overlay window with its own message
    // store; receive-decrypt happens at the gateway WebSocket
    // before any rendering, so no flash and no DOM coupling.
    // ============================================================

    const RECV_PREFIX = "DPC0::";
    // Discord wraps each rendered message body in a div with a
    // stable id `message-content-<discord_message_id>`. The
    // **inner** `<span>` that holds the actual text is replaced
    // by React on every re-render, but this outer div persists
    // across re-renders. Anchoring on it (and keying all our
    // state by the message_id extracted from its `id`) survives
    // span replacement â€” when React swaps the inner span back to
    // the cover, our cached plaintext re-applies via
    // `div.textContent = cached`, which removes the new span and
    // installs a single text node. The next React render may
    // re-mount the span; the next observer tick re-applies; the
    // periodic sweep below re-applies once a second as a safety
    // net for renders that don't fire useful mutations.
    const RECV_MESSAGE_DIV_SELECTOR = '[id^="message-content-"]';
    const RECV_MESSAGE_ID_PREFIX = "message-content-";
    // Permanent disposition (decrypt completed â€” success OR
    // unrecoverable Rust-side rejection). Keyed by message_id
    // so the marker survives React replacing the inner span.
    const recvDone = new Set();
    // Decrypt RPC currently in flight. Keyed by message_id.
    // Prevents the observer or periodic sweep from dispatching
    // duplicate decrypt requests for the same message while one
    // is pending. Cleared on resolve / reject / timeout.
    const recvInFlight = new Set();
    // Per-message retry counter for IPC-layer timeouts (Tauri's
    // postMessage fallback on Discord â€” which CSP-blocks the
    // custom protocol â€” has been observed to drop calls after
    // the first roundtrip). Incremented only on rejection /
    // timeout, not pre-call, so a transient hang doesn't burn
    // through the budget while the IPC layer is wedged.
    const recvRetries = new Map();
    const RECV_MAX_RETRIES = 3;
    // IPC timeout. Tuned to be longer than a typical keyserver
    // round-trip (cache-miss fetch_pubkeys is the slowest path,
    // ~1â€“2s on a healthy connection) but short enough that a
    // hung postMessage transport recovers within a few seconds
    // rather than locking the cover string in place forever.
    const RECV_IPC_TIMEOUT_MS = 10000;
    // De-dupe the "no peer mapping for this discord_id" onboarding
    // hint so we don't spam the console once per message from an
    // unmapped sender (every non-peer in a channel triggers it).
    const recvUnmappedLogged = new Set();
    // De-dupe ordinary decrypt rejections under DEBUG. NoMatchingSlot
    // and similar fire once per non-recipient message; in a busy
    // channel that's hundreds of console lines per minute. Cap to
    // 50 unique error strings (subsequent unique errors fall
    // through silently) to bound memory in pathological cases.
    const recvRejectionsLogged = new Set();
    const RECV_REJECTION_LOG_CAP = 50;
    // Plaintext cache, keyed by message_id. Populated on
    // successful decrypt. Survives React replacing the inner
    // span, channel switches, and DOM re-mounts â€” so when the
    // user navigates away and back, we re-apply from cache
    // without re-dispatching IPC.
    const recvPlaintext = new Map();

    // Phase 6a: last-seen DPC0:: cover string per messageId,
    // recorded whenever we successfully apply plaintext to a
    // div. Used by the remote-edit detector in `recvHandleDiv`:
    // if the observer sees a *different* DPC0:: cover for a
    // messageId we've already cached plaintext for, that's a
    // remote edit and the cached plaintext is stale â€” we
    // invalidate the caches and fall through to a fresh
    // decrypt dispatch. Without this map, the existing
    // recvPlaintext short-circuit would re-apply the OLD
    // plaintext indefinitely after an edit.
    const recvCovers = new Map();

    // Phase 5b3: per-channel rehydration cache populated by
    // `osl_load_channel_history` on channel switch. Keyed by
    // discord_message_id. Distinct from `recvPlaintext`:
    //   - `recvPlaintext` is filled by *this session's* successful
    //     decrypts.
    //   - `loadedHistory` is filled from on-disk store rows
    //     decrypted in *prior* sessions, surviving Tauri restart.
    // `recvDispatchDecrypt` consults this map before dispatching
    // a backend call so a lazy-rendered scrollback message
    // resolves instantly without a keyserver round-trip. Entries
    // are not aged out; the v1 alpha workload (two-peer dogfood,
    // bounded channel scrollback) doesn't justify a TTL yet.
    const loadedHistory = new Map();
    // Last channel_id we kicked off `osl_load_channel_history`
    // against. Updated synchronously *before* the invoke so a
    // re-tick during the load doesn't double-fire. Stays set
    // when the user navigates to a non-channel route (settings,
    // friends list) so returning to the same channel doesn't
    // re-load â€” only a switch to a *different* channel does.
    let lastLoadedChannelId = null;
    window.__OSL_LOADED_HISTORY__ = loadedHistory;

    // Phase 6a UX fix: edit-tab textbox observer + Slate-aware
    // plaintext swap.
    //
    // Problem: when the user clicks Edit on a DPC0:: message,
    // Discord's Slate.js editor pulls initial content from
    // React state (the ciphertext), not from our painted-over
    // span. Without intervention the user sees `DPC0::AQFC...`
    // in the edit box.
    //
    // Fix: a MutationObserver on document.body watches for the
    // edit textbox appearing inside an `li[id^='chat-messages-']`
    // (which excludes the main composer at the channel bottom).
    // When found with DPC0:: content, we resolve the plaintext
    // from `loadedHistory` / `recvPlaintext` and replace the
    // textbox content via execCommand("insertText"), which
    // Slate handles as a real edit and updates its internal
    // model. On submit, the existing PATCH interceptor sees
    // plaintext in `parsed.content` and re-encrypts normally.
    //
    // Safety net: `interceptEditBody` already passes-through on
    // DPC0::-prefixed content (lines ~608-622). If the swap
    // silently fails, the edit becomes a no-op rather than
    // corrupting the message.
    let editTabObserver = null;
    const EDIT_TAB_PLACEHOLDER =
        "[message from before persistence â€” cannot edit]";

    // Phase 6a edit-overlay state.
    //
    // The overlay replaces Discord's native edit textbox for DPC0
    // messages. Discord's Slate editor never sees ciphertext for these
    // messages because we kill the pencil-click before React's edit
    // action dispatches.
    //
    // editOverlayActive: messageId -> { overlayEl, errorEl, hiddenSpan }
    // editOverlayTemplate: cached deep-clone of `.channelTextArea__5126c`
    //   from the main composer, with Slate/React attributes stripped.
    //   Lazily initialized on first overlay mount.
    // editOverlayAuthToken: most recent Authorization header observed
    //   on any outgoing Discord API request. Used to authenticate our
    //   PATCH calls. Refreshed on every observed request so it tracks
    //   token rotation.
    const editOverlayActive = new Map();
    let editOverlayTemplate = null;
    let editOverlayAuthToken = null;
    // editOverlayLocallyApplied: message_ids whose plaintext was just
    // written to the DOM directly by editOverlaySave on PATCH 200.
    // Discord's MESSAGE_UPDATE will re-render the message using its
    // cached display content (never showing the new ciphertext to
    // our recv observer), so the recv path can't decrypt-and-apply
    // the edit on its own. We pre-empt by writing the plaintext
    // ourselves and gate recvHandleDiv on this Set so a racing
    // MESSAGE_UPDATE-triggered observer pass doesn't overwrite our
    // local apply with a stale recv decrypt. Entries auto-expire
    // after 5s — long enough to outlast any same-tick race, short
    // enough that a *legitimate* later edit of the same id (remote
    // peer edits the same message minutes later) re-decrypts
    // normally.
    const editOverlayLocallyApplied = new Set();
    // Periodic sweep cadence. 1s feels right: long enough that
    // it's not a CPU sink, short enough that a user who scrolls
    // back to a channel sees plaintext within a beat. Critical
    // for correctness â€” empirically the MutationObserver does
    // NOT reliably fire on newly-rendered messages added to a
    // pre-existing message list (Discord likely renders new
    // messages by mutating an inner span on a pre-mounted
    // wrapper, and our `addedNodes` walk doesn't surface the
    // outer `[id^="message-content-"]` div in those cases). So
    // the sweep is the *primary* mechanism for finding new
    // messages, not a backup.
    const RECV_SWEEP_INTERVAL_MS = 1000;

    // Expose the receive-side state on `window` so the developer
    // can inspect cache health from DevTools without needing a
    // Rust round-trip. These are READ-ONLY references â€” mutating
    // the Map / Set from the console will affect the live state.
    //
    //   window.__OSL_RECV_PLAINTEXT_MAP__.size  // count of cached decrypts
    //   window.__OSL_RECV_DONE_SET__.size       // settled message_ids
    //   window.__OSL_RECV_INFLIGHT_SET__.size   // pending IPCs (should be 0 between bursts)
    //   [...window.__OSL_RECV_PLAINTEXT_MAP__.entries()]  // dump cache
    window.__OSL_RECV_PLAINTEXT_MAP__ = recvPlaintext;
    window.__OSL_RECV_DONE_SET__ = recvDone;
    window.__OSL_RECV_INFLIGHT_SET__ = recvInFlight;

    // Per-message attempt counter for the `recvExtractAuthorId`
    // returns-null path. Discord's freshly-rendered own-sent
    // messages frequently have no avatar block / no
    // `data-author-id` for the first few paints â€” the metadata
    // is wired in during a later React commit. We retry up to
    // RECV_AUTHOR_MAX_RETRIES times (driven by the periodic
    // sweep, ~1s cadence). After the cap we mark the message
    // settled with reason `no_senderDiscordId_after_retries` so
    // we don't poll forever for a message that's structurally
    // unidentifiable (very rare; system messages or non-user
    // authors).
    const recvAuthorRetryCount = new Map();
    const RECV_AUTHOR_MAX_RETRIES = 10;

    // Outbound-capture cache of own-sent (message_id â†’ author_id).
    // Populated by the XHR `load` listener installed in
    // `makeSendHandler` for every /channels/{cid}/messages POST
    // whose response carries `{id, author: {id}}`. Consulted at
    // the top of `recvExtractAuthorId` before any DOM walking,
    // so cozy-grouped own-send continuations (which omit
    // `data-author-id` from the rendered list-item) still
    // resolve to the correct sender.
    //
    // FIFO eviction once size exceeds SELF_SENT_AUTHORS_MAX.
    // Map preserves insertion order, so the oldest entry is
    // always the first key from `.keys()`.
    const selfSentAuthors = new Map();
    const SELF_SENT_AUTHORS_MAX = 500;

    /** Sentinel error returned by the timeout race. */
    const RECV_TIMEOUT_SENTINEL = "__OSL_IPC_TIMEOUT__";

    /**
     * Walk up from `node` until we find an element with an `id`
     * starting with `message-content-`. Returns the element or
     * null. Caps the walk at 16 hops so we don't traverse the
     * whole document for an unrelated text node.
     */
    function recvFindMessageDiv(node) {
        let n = node;
        for (let i = 0; i < 16 && n; i++) {
            if (
                n.nodeType === 1 &&
                typeof n.id === "string" &&
                n.id.indexOf(RECV_MESSAGE_ID_PREFIX) === 0
            ) {
                return n;
            }
            n = n.parentNode;
        }
        return null;
    }

    /** Extract the Discord message_id from a `message-content-â€¦` div. */
    function recvMessageIdOf(div) {
        return div.id.slice(RECV_MESSAGE_ID_PREFIX.length);
    }

    /**
     * Install `plaintext` as the visible content of `div`.
     *
     * Builds a fresh `<span>plaintext</span>` and uses
     * `div.replaceChildren(span)` instead of
     * `div.textContent = plaintext`. Two reasons:
     *
     *  1. **Paint invalidation.** Live diagnostics on Windows
     *     showed `textContent` updates persisting in the DOM
     *     for 5+ seconds without React re-rendering â€” yet the
     *     screen kept showing the prior `DPC0::` cover. The
     *     browser was rendering a stale visual snapshot that
     *     `textContent =` didn't dirty. Constructing a brand-
     *     new `<span>` element with `replaceChildren` gives
     *     the paint engine a fresh DOM node it can't have
     *     cached, forcing a real repaint.
     *
     *  2. **Native shape.** Discord renders message content as
     *     `<div id="message-content-â€¦"><span>â€¦</span></div>`.
     *     Our replacement matches that shape exactly, so any
     *     CSS / styling rules that target `> span` keep
     *     applying. `textContent =` collapses children to a
     *     single bare text node, which works for plaintext but
     *     can lose downstream layout assumptions.
     */
    function recvApplyPlaintext(div, plaintext) {
        // 8d safety: if a sentinel prefix ever reaches this function
        // (legacy or upstream-dispatch bug), strip it to empty so it
        // never renders as visible text.
        const safeText =
            typeof plaintext === "string" &&
            plaintext.indexOf(OSL_RESULT_ATTACHMENT_PREFIX) === 0
                ? ""
                : plaintext;
        const span = document.createElement("span");
        span.textContent = safeText;
        div.replaceChildren(span);
    }

    /**
     * Pull the sender's Discord user_id (== OSL user_id in v1)
     * from the DOM context surrounding `el`. Walks up to the
     * `chat-messages___â€¦` list-item ancestor, then tries:
     *   1. `data-author-id` attribute on item or any descendant.
     *   2. Avatar `<img src="â€¦/avatars/<snowflake>/â€¦">` scan.
     * Returns null on failure â€” caller skips the request.
     */
    function recvExtractAuthorId(el) {
        // Self-sent capture cache hit. The XHR `load` listener
        // populates `selfSentAuthors` with (msg_id â†’ author_id)
        // for every successful own-send. Cozy-grouped
        // continuation list-items drop `data-author-id` from
        // the DOM, so the DOM walks below would return null â€”
        // but the response-side capture has the mapping. Try
        // the cache before any DOM work.
        if (
            el &&
            typeof el.id === "string" &&
            el.id.indexOf(RECV_MESSAGE_ID_PREFIX) === 0
        ) {
            const cached = selfSentAuthors.get(recvMessageIdOf(el));
            if (typeof cached === "string" && /^\d{15,22}$/.test(cached)) {
                return cached;
            }
        }

        let node = el;
        let item = null;
        for (let i = 0; i < 12 && node; i++) {
            const id =
                node.getAttribute && node.getAttribute("data-list-item-id");
            if (id && id.indexOf("chat-messages___") === 0) {
                item = node;
                break;
            }
            node = node.parentElement;
        }
        const root = item || el;
        const directOnSelf =
            root.getAttribute && root.getAttribute("data-author-id");
        if (typeof directOnSelf === "string" && /^\d{15,22}$/.test(directOnSelf)) {
            return directOnSelf;
        }
        const directDescendant =
            root.querySelector && root.querySelector("[data-author-id]");
        if (directDescendant && directDescendant.getAttribute) {
            const v = directDescendant.getAttribute("data-author-id");
            if (typeof v === "string" && /^\d{15,22}$/.test(v)) return v;
        }
        if (root.querySelectorAll) {
            const imgs = root.querySelectorAll(
                "img[src*='cdn.discordapp.com/avatars/']"
            );
            for (const img of imgs) {
                const m = img.src && img.src.match(/\/avatars\/(\d{15,22})\//);
                if (m) return m[1];
            }
        }

        // Cozy-grouping fallback. Discord renders consecutive
        // messages from the same author as a "group" â€” only the
        // first message in the group carries the avatar block and
        // the `data-author-id` attribute; subsequent messages in
        // the group ship as bare list-items with just the message
        // body. Own-sent messages are particularly prone to this
        // because the user's own avatar is often elided from the
        // first paint, then never re-attached if a previous
        // sibling already shows it.
        //
        // Walk `previousElementSibling` from `item` up to 20 hops,
        // looking for the most recent list-item that DOES carry a
        // `data-author-id`. Any such sibling within a cozy group
        // is from the same author by definition, so we attribute
        // this message to that id. Capped at 20 to avoid
        // pathological walks across very long groups (in practice
        // Discord caps a single cozy group at far fewer messages).
        if (item) {
            let sibling = item.previousElementSibling;
            for (let i = 0; i < 20 && sibling; i++) {
                if (sibling.getAttribute) {
                    const sib = sibling.getAttribute("data-author-id");
                    if (typeof sib === "string" && /^\d{15,22}$/.test(sib)) {
                        return sib;
                    }
                }
                if (sibling.querySelector) {
                    const sibDesc = sibling.querySelector("[data-author-id]");
                    if (sibDesc && sibDesc.getAttribute) {
                        const v = sibDesc.getAttribute("data-author-id");
                        if (typeof v === "string" && /^\d{15,22}$/.test(v)) {
                            return v;
                        }
                    }
                }
                sibling = sibling.previousElementSibling;
            }
        }

        return null;
    }

    /**
     * Read channel_id from the current URL. Discord routes are
     * `/channels/<guild_id|@me>/<channel_id>[/<message_id>]`.
     * Returns null when the user isn't on a channel route.
     */
    function recvExtractChannelId() {
        const m = window.location.pathname.match(
            /\/channels\/[^/]+\/(\d{15,22})/
        );
        return m ? m[1] : null;
    }

    /**
     * Phase 6a UX fix: resolve plaintext for an edit-target
     * message_id. Checks the in-memory caches in order:
     *   1. `loadedHistory` â€” populated from on-disk store on
     *      channel switch; survives Tauri restart.
     *   2. `recvPlaintext` â€” populated by *this session's*
     *      successful decrypts.
     *
     * Returns `{ plaintext, source }` on hit or `null` on miss
     * (caller substitutes EDIT_TAB_PLACEHOLDER).
     */
    function editTabResolvePlaintext(messageId) {
        const fromHistory = loadedHistory.get(messageId);
        if (typeof fromHistory === "string") {
            return { plaintext: fromHistory, source: "loadedHistory" };
        }
        const fromSession = recvPlaintext.get(messageId);
        if (typeof fromSession === "string") {
            return { plaintext: fromSession, source: "recvPlaintext" };
        }
        return null;
    }

    /**
     * Phase 6a UX fix: replace the contents of an edit textbox
     * with `plaintext`, using `document.execCommand("insertText")`
     * so Discord's Slate.js editor updates its internal model.
     *
     * `execCommand` is deprecated but remains the only reliable
     * way to drive Slate from outside React: it dispatches the
     * `beforeinput` events Slate listens for and triggers a
     * proper React re-render. A raw `textContent =` assignment
     * paints the DOM but leaves Slate's model holding the
     * ciphertext; on submit Slate would serialise the stale
     * model and Discord would receive the original DPC0:: blob.
     *
     * Fallback path: synthesise an `InputEvent` with
     * `inputType: "insertReplacementText"`. Some Slate versions
     * accept this; others ignore it. The PATCH interceptor's
     * existing DPC0::-passthrough is the final safety net if
     * both paths fail â€” the edit becomes a no-op (Discord is
     * sent the same ciphertext it already has) rather than
     * corrupting the message.
     *
     * Marks `textboxEl.dataset.oslSwapped = messageId` so the
     * observer doesn't re-fire on every mutation tick.
     */
    function editTabSwapTextbox(textboxEl, messageId, plaintext, sourceLabel) {
        try {
            textboxEl.focus();
            const sel = window.getSelection();
            const range = document.createRange();
            range.selectNodeContents(textboxEl);
            sel.removeAllRanges();
            sel.addRange(range);

            let ok = false;
            try {
                ok = document.execCommand("insertText", false, plaintext);
            } catch (e) {
                ok = false;
            }
            if (!ok) {
                const ev = new InputEvent("beforeinput", {
                    bubbles: true,
                    cancelable: true,
                    inputType: "insertReplacementText",
                    data: plaintext,
                });
                textboxEl.dispatchEvent(ev);
                console.log(
                    "[OSL] editTab swap msg=" +
                        messageId +
                        " path=inputEvent (execCommand returned false)"
                );
            } else {
                console.log(
                    "[OSL] editTab swap msg=" +
                        messageId +
                        " path=execCommand source=" +
                        sourceLabel +
                        " plaintext_len=" +
                        plaintext.length
                );
            }
            textboxEl.dataset.oslSwapped = messageId;
        } catch (e) {
            console.error(
                "[OSL] editTab swap threw msg=" + messageId,
                e
            );
        }
    }

    /**
     * Phase 6a UX fix: inspect a candidate edit textbox and, if
     * it carries DPC0::-prefixed content, swap to plaintext.
     *
     * No-op when:
     *   - already swapped (dataset.oslSwapped matches this mid)
     *   - no `li[id^='chat-messages-']` ancestor (means this
     *     isn't an in-message edit textbox â€” most likely the
     *     main composer at the channel bottom; ignore)
     *   - id parse fails
     *   - textContent doesn't start with `DPC0::`
     */
    function editTabHandleTextbox(textboxEl) {
        const li = textboxEl.closest("li[id^='chat-messages-']");
        if (!li) return;
        const m = /chat-messages-\d{15,22}-(\d{15,22})/.exec(li.id);
        if (!m) return;
        const messageId = m[1];
        if (textboxEl.dataset.oslSwapped === messageId) return;
        const text = (textboxEl.textContent || "").trim();
        if (text.indexOf("DPC0::") !== 0) return;

        const resolved = editTabResolvePlaintext(messageId);
        if (resolved) {
            editTabSwapTextbox(
                textboxEl,
                messageId,
                resolved.plaintext,
                resolved.source
            );
        } else {
            console.log(
                "[OSL] editTab no_history msg=" +
                    messageId +
                    " reason=miss_loadedHistory_and_recvPlaintext"
            );
            editTabSwapTextbox(
                textboxEl,
                messageId,
                EDIT_TAB_PLACEHOLDER,
                "placeholder"
            );
        }
    }

    /**
     * Phase 6a UX fix: lazy-init the edit-tab observer.
     * Watches document.body for added textboxes and attribute
     * changes on existing ones (Discord sometimes mounts the
     * textbox first and toggles `contenteditable` after).
     *
     * Idempotent: subsequent calls are no-ops once started.
     */
    function editTabStartObserver() {
        if (editTabObserver) return;
        editTabObserver = new MutationObserver(function (mutations) {
            for (const mut of mutations) {
                if (mut.type === "childList") {
                    for (const node of mut.addedNodes) {
                        if (node.nodeType !== 1) continue;
                        if (
                            node.matches &&
                            node.matches('div[role="textbox"]')
                        ) {
                            editTabHandleTextbox(node);
                        }
                        if (node.querySelectorAll) {
                            const inner = node.querySelectorAll(
                                'div[role="textbox"]'
                            );
                            for (const tb of inner) {
                                editTabHandleTextbox(tb);
                            }
                        }
                    }
                } else if (
                    mut.type === "attributes" &&
                    mut.target &&
                    mut.target.matches &&
                    mut.target.matches('div[role="textbox"]')
                ) {
                    editTabHandleTextbox(mut.target);
                }
            }
        });
        editTabObserver.observe(document.body, {
            childList: true,
            subtree: true,
            attributes: true,
            attributeFilter: ["contenteditable", "role"],
        });
        console.log("[OSL] editTab observer started");
    }

    /**
     * Phase 6a edit-overlay: lazily build (and cache) a Discord-styled
     * template by deep-cloning the main channel composer. The composer
     * lives at `.channelTextArea__5126c` (with hashed suffix that may
     * differ across Discord builds; we use a class-prefix selector).
     *
     * The clone is sanitized:
     *  - all `id` attributes removed (avoid DOM duplicates)
     *  - all `data-slate-*` attributes removed (defang Slate)
     *  - all `data-list-item-id`, `data-can-focus` removed
     *  - inner contenteditable emptied
     *
     * Returns null if no composer is mounted yet (rare — only happens
     * if user triggers edit before the channel has loaded).
     */
    function editOverlayBuildTemplate() {
        if (editOverlayTemplate) return editOverlayTemplate;
        const composer = document.querySelector(
            '[class*="channelTextArea__"]'
        );
        if (!composer) return null;
        const clone = composer.cloneNode(true);
        // Strip identifiers, refs, Slate markers
        const all = clone.querySelectorAll("*");
        for (const el of all) {
            el.removeAttribute("id");
            el.removeAttribute("data-list-item-id");
            el.removeAttribute("data-can-focus");
            for (const attr of Array.from(el.attributes)) {
                if (attr.name.startsWith("data-slate")) {
                    el.removeAttribute(attr.name);
                }
            }
        }
        clone.removeAttribute("id");
        // Find the contenteditable and empty it
        const editable = clone.querySelector('[contenteditable="true"]');
        if (editable) {
            editable.innerHTML = "";
            // Mark our overlay's editable so we can find it again
            editable.setAttribute("data-osl-overlay-editable", "true");
        }

        // Visual cleanup: strip composer chrome that doesn't belong
        // in an edit context — placeholder text, attachment +
        // toolbar buttons, send-button row. Done after the editable
        // is marked so the "exclude editable + descendants" guard
        // below can find it.
        let stripped = 0;
        const stripSelectors = [
            // Placeholders
            '[class*="placeholder"]',
            "[data-slate-placeholder]",
            // Attachment / upload UI
            '[class*="attachWrapper"]',
            '[class*="buttons"]',
            '[aria-label="Upload a File"]',
            '[aria-label*="attach" i]',
            // Bottom toolbar / send button row
            '[class*="buttonContainer"]',
            '[class*="attachedBars"]',
        ];
        const safeRemove = function (el) {
            if (!el || !el.parentNode) return false;
            // Never remove the editable itself or anything inside
            // it (mention spans, emoji elements, etc. — though we
            // emptied it above, defence in depth).
            if (editable && (el === editable || editable.contains(el))) {
                return false;
            }
            el.parentNode.removeChild(el);
            return true;
        };
        for (const sel of stripSelectors) {
            const matches = clone.querySelectorAll(sel);
            for (const el of matches) {
                if (safeRemove(el)) stripped++;
            }
        }
        // Drop any element whose textContent starts with
        // "Message " (Discord's "Message #channel" placeholder
        // sometimes lives outside the [class*="placeholder"]
        // selector). The editable is excluded by safeRemove above.
        const allRemaining = clone.querySelectorAll("*");
        for (const el of allRemaining) {
            const t = el.textContent || "";
            if (t.indexOf("Message ") === 0 && el !== editable) {
                if (safeRemove(el)) stripped++;
            }
        }
        // Drop stray role="button" elements that survived the
        // selector pass and live outside the editable (the
        // editable shouldn't carry one, but we guard anyway).
        const buttons = clone.querySelectorAll('[role="button"]');
        for (const btn of buttons) {
            if (safeRemove(btn)) stripped++;
        }

        // Mark the whole template
        clone.setAttribute("data-osl-overlay", "true");
        editOverlayTemplate = clone;
        console.log(
            "[OSL] editOverlay template built, stripped " +
                stripped +
                " elements"
        );
        return editOverlayTemplate;
    }

    /**
     * Phase 6a edit-overlay: insert overlay over a message and
     * pre-fill plaintext.
     */
    function editOverlayMount(messageId, plaintext, channelId, li) {
        if (editOverlayActive.has(messageId)) {
            console.log(
                "[OSL] editOverlay mount SKIP msg=" +
                    messageId +
                    " reason=already_active"
            );
            return;
        }
        const template = editOverlayBuildTemplate();
        if (!template) {
            console.log(
                "[OSL] editOverlay mount FAIL msg=" +
                    messageId +
                    " reason=no_composer_template"
            );
            return;
        }
        const overlay = template.cloneNode(true);
        overlay.setAttribute("data-osl-overlay-msg", messageId);
        const editable = overlay.querySelector(
            '[data-osl-overlay-editable="true"]'
        );
        if (!editable) {
            console.log(
                "[OSL] editOverlay mount FAIL msg=" +
                    messageId +
                    " reason=no_editable_in_template"
            );
            return;
        }
        editable.textContent = plaintext;

        // Size the cloned containers to fit content rather than
        // the main composer's baked-in dimensions. The composer
        // is laid out for a multi-line bottom-of-channel input;
        // an inline edit overlay should hug its content.
        try {
            // Discord's stylesheet wins on specificity, so every
            // override below is forced via setProperty("...",
            // "important"). Targets:
            //
            //   1. The outermost overlay container (the cloned
            //      [class*="channelTextArea__"]) — kill the
            //      composer's outer padding/margin and let it size
            //      to content.
            //   2. Every wrapper layer between outer + editable —
            //      same deal. Adds `[class*="markup"]` and
            //      `[class*="form__"]` matches because Discord
            //      sometimes places a fixed-aspect inner sizer at
            //      one of those classes.
            //   3. The editable itself — bring padding /
            //      min-height / line-height / font-size in line
            //      with Discord's native edit textbox.
            overlay.style.setProperty("min-height", "unset", "important");
            overlay.style.setProperty("height", "auto", "important");
            overlay.style.setProperty("margin", "0", "important");
            overlay.style.setProperty("padding", "0", "important");

            const innerWrappers = overlay.querySelectorAll(
                '[class*="scrollableContainer"], ' +
                    '[class*="inner__"], ' +
                    '[class*="textArea__"], ' +
                    '[class*="markup"], ' +
                    '[class*="form__"]'
            );
            for (const wrapper of innerWrappers) {
                wrapper.style.setProperty(
                    "min-height",
                    "unset",
                    "important"
                );
                wrapper.style.setProperty("height", "auto", "important");
                wrapper.style.setProperty("padding", "0", "important");
                wrapper.style.setProperty("margin", "0", "important");
            }

            editable.style.setProperty(
                "min-height",
                "20px",
                "important"
            );
            editable.style.setProperty(
                "padding",
                "6px 10px",
                "important"
            );
            editable.style.setProperty(
                "line-height",
                "1.375",
                "important"
            );
            editable.style.setProperty(
                "font-size",
                "0.95rem",
                "important"
            );
        } catch (e) {
            console.error(
                "[OSL] editOverlay size-strip threw msg=" + messageId,
                e
            );
        }

        // Build the (initially empty) error line. Stays inside
        // the overlay container so it visually attaches to the
        // textbox when an error fires; the hint moves OUT (see
        // below) since it's persistently visible.
        const errorEl = document.createElement("div");
        errorEl.setAttribute("data-osl-overlay-error", "true");
        errorEl.style.fontSize = "12px";
        errorEl.style.color = "#fa777c";
        errorEl.style.padding = "4px 0 0 0";
        errorEl.style.display = "none";
        overlay.appendChild(errorEl);

        // Build the hint line as a SIBLING that follows the
        // overlay container, not a child. Inside the cloned
        // channelTextArea the hint sat flush against the bottom
        // edge with no breathing room; reparenting puts proper
        // spacing under the textbox.
        const hint = document.createElement("div");
        hint.setAttribute("data-osl-overlay-hint", "true");
        hint.style.marginTop = "4px";
        hint.style.padding = "0";
        hint.style.fontSize = "12px";
        hint.style.color = "var(--text-muted, #b5bac1)";
        hint.style.lineHeight = "1.4";
        hint.textContent = "escape to cancel · enter to save";

        // Hide the rendered message-content span
        const span = li.querySelector(
            '[id^="message-content-' + messageId + '"]'
        );
        const hiddenSpan = span || null;
        if (hiddenSpan) {
            hiddenSpan.dataset.oslOverlayHidden = "true";
            hiddenSpan.style.display = "none";
        }

        // Insert overlay + hint as siblings of the span. Order:
        // <hidden span> <overlay> <hint>. The hint sits
        // immediately after the overlay.
        if (hiddenSpan && hiddenSpan.parentNode) {
            hiddenSpan.parentNode.insertBefore(
                overlay,
                hiddenSpan.nextSibling
            );
            overlay.parentNode.insertBefore(hint, overlay.nextSibling);
        } else {
            // Fallback: append to the li's contents container
            li.appendChild(overlay);
            li.appendChild(hint);
        }

        // Wire keys
        editable.addEventListener("keydown", function (e) {
            if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                editOverlayUnmount(messageId);
                console.log(
                    "[OSL] editOverlay cancel msg=" + messageId
                );
            } else if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                e.stopPropagation();
                editOverlaySave(messageId, channelId, editable);
            }
        });

        editOverlayActive.set(messageId, {
            overlayEl: overlay,
            hintEl: hint,
            errorEl: errorEl,
            hiddenSpan: hiddenSpan,
        });

        // Focus + place caret at end
        try {
            editable.focus();
            const range = document.createRange();
            range.selectNodeContents(editable);
            range.collapse(false);
            const sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
        } catch (e) {
            console.error(
                "[OSL] editOverlay focus threw msg=" + messageId,
                e
            );
        }

        console.log(
            "[OSL] editOverlay mount msg=" +
                messageId +
                " plaintext_len=" +
                plaintext.length
        );
    }

    /**
     * Phase 6a edit-overlay: tear down overlay, restore message-content
     * span visibility.
     */
    function editOverlayUnmount(messageId) {
        const state = editOverlayActive.get(messageId);
        if (!state) return;
        try {
            if (state.overlayEl && state.overlayEl.parentNode) {
                state.overlayEl.parentNode.removeChild(state.overlayEl);
            }
            // hintEl is a sibling of overlayEl post-Fix 2; remove
            // it independently.
            if (state.hintEl && state.hintEl.parentNode) {
                state.hintEl.parentNode.removeChild(state.hintEl);
            }
            if (state.hiddenSpan) {
                state.hiddenSpan.style.display = "";
                delete state.hiddenSpan.dataset.oslOverlayHidden;
            }
        } catch (e) {
            console.error(
                "[OSL] editOverlay unmount threw msg=" + messageId,
                e
            );
        }
        editOverlayActive.delete(messageId);
    }

    /**
     * Phase 6a edit-overlay: show inline error text without
     * unmounting, so the user can retry.
     */
    function editOverlayShowError(messageId, message) {
        const state = editOverlayActive.get(messageId);
        if (!state || !state.errorEl) return;
        state.errorEl.textContent = message;
        state.errorEl.style.display = "";
    }

    function editOverlayClearError(messageId) {
        const state = editOverlayActive.get(messageId);
        if (!state || !state.errorEl) return;
        state.errorEl.textContent = "";
        state.errorEl.style.display = "none";
    }

    /**
     * Phase 6a edit-overlay: PATCH new plaintext to Discord. Goes
     * through the existing fetch interceptor (interceptEditBody),
     * which sees `content: <plaintext>`, encrypts, ships, and on 200
     * fires runPersistEdit. We just initiate the request.
     */
    function editOverlaySave(messageId, channelId, editable) {
        if (!editOverlayAuthToken) {
            editOverlayShowError(
                messageId,
                "Auth token not yet captured — try again in a few seconds"
            );
            console.log(
                "[OSL] editOverlay save FAIL msg=" +
                    messageId +
                    " reason=no_auth_token"
            );
            return;
        }
        const newPlaintext = editable.textContent || "";
        editOverlayClearError(messageId);

        const url =
            "/api/v9/channels/" + channelId + "/messages/" + messageId;
        const body = JSON.stringify({ content: newPlaintext });

        fetch(url, {
            method: "PATCH",
            headers: {
                "Authorization": editOverlayAuthToken,
                "Content-Type": "application/json",
            },
            body: body,
        })
            .then(function (resp) {
                if (resp && resp.ok) {
                    // Invalidate the receive-side decrypt caches
                    // for this message_id so the next observer
                    // pass re-decrypts the bounced-back NEW
                    // ciphertext rather than re-applying the
                    // OLD plaintext from cache. recvCovers /
                    // recvDone are cleaned up by the existing
                    // recvHandleDiv stale-cover detector once it
                    // sees the new cover; clearing recvPlaintext
                    // + loadedHistory removes the only paths
                    // that would short-circuit before that
                    // detector runs.
                    let invalidated = false;
                    if (
                        recvPlaintext &&
                        typeof recvPlaintext.delete === "function"
                    ) {
                        recvPlaintext.delete(messageId);
                        invalidated = true;
                    }
                    if (
                        loadedHistory &&
                        typeof loadedHistory.delete === "function"
                    ) {
                        loadedHistory.delete(messageId);
                        invalidated = true;
                    }
                    if (invalidated) {
                        console.log(
                            "[OSL] editOverlay invalidated decrypt cache msg=" +
                                messageId
                        );
                    }
                    console.log(
                        "[OSL] editOverlay save OK msg=" +
                            messageId +
                            " plaintext_len=" +
                            newPlaintext.length
                    );

                    // Apply the new plaintext directly to the DOM.
                    // Discord's MESSAGE_UPDATE will re-render with
                    // its cached display content and never expose
                    // the new ciphertext to our recv observer, so
                    // we can't rely on the normal decrypt-and-
                    // apply path for *local* edits — we write the
                    // plaintext ourselves. The Set entry below
                    // tells recvHandleDiv to skip this id during
                    // the 5s window where a racing MESSAGE_UPDATE-
                    // triggered observer pass might otherwise
                    // overwrite our local apply.
                    const contentEl = document.getElementById(
                        "message-content-" + messageId
                    );
                    if (contentEl) {
                        // Discord wraps the message body text in
                        // nested spans alongside an "(edited)"
                        // marker span and (sometimes) a timestamp
                        // span. A naive `textContent =` overwrite
                        // nukes those siblings, so on the second
                        // local edit the (edited) badge would
                        // disappear.
                        //
                        // Strategy: walk childNodes; replace the
                        // first non-marker text-bearing node and
                        // leave everything else alone. Fallback:
                        // if no clean target is found but a marker
                        // exists, drop everything *before* the
                        // marker and prepend a fresh text node.
                        // Last-resort fallback: the original
                        // textContent overwrite.
                        let strategy = "fallback_textcontent";
                        let replaced = false;
                        const children = Array.from(contentEl.childNodes);
                        for (const node of children) {
                            if (node.nodeType === Node.TEXT_NODE) {
                                if (
                                    node.data &&
                                    node.data.trim().length > 0
                                ) {
                                    node.data = newPlaintext;
                                    replaced = true;
                                    strategy = "preserved_marker";
                                    break;
                                }
                            } else if (node.nodeType === Node.ELEMENT_NODE) {
                                const cls = node.className || "";
                                const isItselfMarker =
                                    /edited|timestamp/i.test(cls);
                                const hasEditedMarker =
                                    typeof node.querySelector ===
                                        "function" &&
                                    (node.querySelector(
                                        '[class*="edited"]'
                                    ) ||
                                        node.querySelector(
                                            '[class*="timestamp"]'
                                        ));
                                if (!isItselfMarker && !hasEditedMarker) {
                                    node.textContent = newPlaintext;
                                    replaced = true;
                                    strategy = "preserved_marker";
                                    break;
                                }
                            }
                        }
                        if (!replaced) {
                            const editedMarker = contentEl.querySelector(
                                '[class*="edited"]'
                            );
                            if (editedMarker) {
                                while (
                                    contentEl.firstChild &&
                                    contentEl.firstChild !== editedMarker
                                ) {
                                    contentEl.removeChild(
                                        contentEl.firstChild
                                    );
                                }
                                contentEl.insertBefore(
                                    document.createTextNode(newPlaintext),
                                    editedMarker
                                );
                                replaced = true;
                                strategy = "preserved_marker";
                            } else {
                                contentEl.textContent = newPlaintext;
                                replaced = true;
                                strategy = "fallback_textcontent";
                            }
                        }
                        console.log(
                            "[OSL] editOverlay applied plaintext directly to DOM msg=" +
                                messageId +
                                " len=" +
                                newPlaintext.length +
                                " strategy=" +
                                strategy
                        );
                    }
                    editOverlayLocallyApplied.add(messageId);
                    setTimeout(function () {
                        editOverlayLocallyApplied.delete(messageId);
                    }, 5000);

                    editOverlayUnmount(messageId);
                } else {
                    const status = resp ? resp.status : "?";
                    editOverlayShowError(
                        messageId,
                        "Save failed (HTTP " + status + ")"
                    );
                    console.log(
                        "[OSL] editOverlay save FAIL msg=" +
                            messageId +
                            " status=" +
                            status
                    );
                }
            })
            .catch(function (err) {
                editOverlayShowError(
                    messageId,
                    "Save failed (network): " +
                        (err && err.message ? err.message : String(err))
                );
                console.error(
                    "[OSL] editOverlay save threw msg=" + messageId,
                    err
                );
            });
    }

    /**
     * Phase 6a edit-overlay: capture-phase click handler that
     * intercepts pencil-icon clicks on DPC0 messages.
     */
    function editOverlayHandleClick(e) {
        const target = e.target;
        if (!target || typeof target.closest !== "function") return;
        const btn = target.closest(
            '[role="button"][aria-label="Edit"]'
        );
        if (!btn) return;
        const li = btn.closest("li[id^='chat-messages-']");
        if (!li) return;
        const m = /chat-messages-\d{15,22}-(\d{15,22})/.exec(li.id);
        if (!m) return;
        const messageId = m[1];

        // Resolve plaintext. Three sources, in order:
        //   1. loadedHistory  — fastest, populated on channel switch
        //   2. recvPlaintext  — populated by this session's decrypts
        //   3. live DOM       — self-healing fallback for the
        //      re-edit case: editOverlaySave invalidates both
        //      caches on save, but the new plaintext is sitting
        //      in the message-content textContent because we
        //      wrote it there directly. Read it back rather than
        //      give up and hand the user Discord's native edit
        //      (which would show DPC0:: ciphertext).
        const fromHistory = loadedHistory.get(messageId);
        const fromSession = recvPlaintext.get(messageId);
        let plaintext =
            typeof fromHistory === "string"
                ? fromHistory
                : typeof fromSession === "string"
                ? fromSession
                : null;
        if (typeof plaintext !== "string") {
            const contentEl = document.getElementById(
                "message-content-" + messageId
            );
            if (contentEl) {
                // Clone-and-strip: deep-clone the contentEl, drop
                // any (edited) / timestamp marker descendants,
                // then read textContent off the clone. More robust
                // than a regex against the raw textContent because
                // Discord's marker markup varies (different
                // surrounding whitespace, occasional sibling
                // wrappers); the clone approach trusts the DOM
                // tree shape rather than the stringified output.
                let liveText = "";
                try {
                    const clone = contentEl.cloneNode(true);
                    const markers = clone.querySelectorAll(
                        '[class*="edited"], [class*="timestamp"]'
                    );
                    for (const m of markers) {
                        if (m.parentNode) m.parentNode.removeChild(m);
                    }
                    liveText = (clone.textContent || "").trim();
                } catch (e) {
                    console.error(
                        "[OSL] editOverlay live-DOM clone-strip threw msg=" +
                            messageId,
                        e
                    );
                }
                if (liveText && liveText.indexOf("DPC0::") !== 0) {
                    console.log(
                        "[OSL] editOverlay resolved plaintext from live DOM msg=" +
                            messageId +
                            " len=" +
                            liveText.length
                    );
                    plaintext = liveText;
                }
            }
        }
        if (typeof plaintext !== "string") {
            // Don't have plaintext; let Discord open its native edit.
            // (Will show ciphertext to the user — annoying but safe.)
            console.log(
                "[OSL] editOverlay PASS msg=" +
                    messageId +
                    " reason=no_plaintext (Discord native edit will open)"
            );
            return;
        }

        const channelId = recvExtractChannelId();
        if (!channelId) {
            console.log(
                "[OSL] editOverlay PASS msg=" +
                    messageId +
                    " reason=no_channel_id"
            );
            return;
        }

        // Kill Discord's edit-mode trigger
        e.stopPropagation();
        e.preventDefault();
        console.log(
            "[OSL] editOverlay intercept msg=" +
                messageId +
                " channel=" +
                channelId
        );
        editOverlayMount(messageId, plaintext, channelId, li);
    }

    /**
     * Phase 6a edit-overlay: install the capture-phase click
     * listener. Idempotent.
     */
    let editOverlayInstalled = false;
    function editOverlayInstall() {
        if (editOverlayInstalled) return;
        document.addEventListener(
            "click",
            editOverlayHandleClick,
            true /* capture */
        );
        editOverlayInstalled = true;
        console.log("[OSL] editOverlay click handler installed");
    }

    /**
     * If `el` carries a DPC0:: cover string, request decryption
     * and replace textContent on success. No-op when:
     *   - element already settled (success or permanent failure)
     *   - element has an in-flight decrypt request
     *   - retry counter exhausted
     *   - prefix absent
     *   - author_id / channel_id unavailable
     *   - Tauri IPC bridge missing
     *
     * **Timeout behaviour.** Tauri's IPC over postMessage (used on
     * pages where CSP blocks the `http://ipc.localhost` custom
     * protocol â€” i.e. discord.com) has been observed to silently
     * drop calls after the first successful roundtrip. We wrap
     * the invoke promise in a `Promise.race` against a setTimeout;
     * a timeout does NOT mark `recvDone` â€” the next observer tick
     * gets a fresh chance, bounded by `RECV_MAX_RETRIES`. The
     * retry counter increments only on rejection / timeout, NOT
     * pre-call, so a transient hang doesn't burn through the
     * retry budget while the IPC layer is wedged.
     */
    /**
     * Decide what to do with a `[id^="message-content-"]` div
     * that's been observed (mutation, sweep, or initial scan):
     *
     *   - textContent doesn't start with `DPC0::` â†’ already
     *     plaintext or non-OSL content; do nothing.
     *   - cached plaintext for this message_id â†’ re-apply via
     *     `recvApplyPlaintext` synchronously. No IPC.
     *   - settled with no cache (permanent decrypt failure) â†’
     *     do nothing.
     *   - in-flight decrypt â†’ do nothing; let the resolution
     *     handle the apply.
     *   - otherwise â†’ dispatch a fresh decrypt.
     *
     * No burst cap on re-apply: live diagnostics confirm
     * `textContent` updates persist on Discord's
     * `message-content` div (React reconciliation does not
     * clobber). If a future Discord change reintroduces clobber
     * dynamics, the periodic sweep below provides a 1s
     * safety-net retry.
     */
    function recvHandleDiv(div) {
        // === v0.0.6.4 diagnostic instrumentation ===
        // ENTRY log first, before any guards. If a fresh send is
        // never reaching ENTRY, the bug is upstream in the
        // observer / sweep / scanSubtree path.
        const __dbg_id = (div && div.id) || "NO_ID";
        const __dbg_text =
            div && typeof div.textContent === "string"
                ? div.textContent.substring(0, 20)
                : "NO_TEXT";
        if (OSL_DEBUG_RECV) {
            console.log(
                "[OSL] recvHandleDiv ENTRY id=" +
                    __dbg_id +
                    " text=" +
                    __dbg_text
            );
        }

        if (!div || div.nodeType !== 1) {
            console.log(
                "[OSL] recvHandleDiv SKIP id=" +
                    __dbg_id +
                    " reason=not_an_element"
            );
            return;
        }
        const text = div.textContent;
        if (!text) {
            console.log(
                "[OSL] recvHandleDiv SKIP id=" +
                    __dbg_id +
                    " reason=empty_textContent"
            );
            return;
        }
        if (text.indexOf(RECV_PREFIX) !== 0) {
            if (OSL_DEBUG_RECV) {
                console.log(
                    "[OSL] recvHandleDiv SKIP id=" +
                        __dbg_id +
                        " reason=no_DPC0_prefix" +
                        " (first8=" +
                        text.substring(0, 8) +
                        ")"
                );
            }
            return;
        }
        // 7d-PIVOT-FIX2 Bug G: persist the original ciphertext on the
        // DOM node so `oslBurnAftermath` can restore it after the
        // in-memory `recvCovers` map is cleared. Set unconditionally:
        // on a remote edit the cover changes, and the new ciphertext
        // is what we want to keep on disk after a future burn.
        try {
            div.setAttribute("data-osl-orig-cipher", text);
        } catch (_) {}
        const messageId = recvMessageIdOf(div);

        // Phase 6a edit-overlay: if we *just* wrote this message's
        // new plaintext directly to the DOM after a successful
        // edit PATCH, skip the recv path entirely. A racing
        // MESSAGE_UPDATE-triggered observer pass would otherwise
        // re-enter recvHandleDiv with stale ciphertext snapshot
        // (or worse, hit the cache invalidation path and trigger
        // a redundant decrypt that resolves to the OLD plaintext
        // because the recv ciphertext snapshot can lag behind
        // Discord's display state).
        if (editOverlayLocallyApplied.has(messageId)) {
            console.log(
                "[OSL] recvHandleDiv SKIP id=" +
                    __dbg_id +
                    " reason=locally_applied_edit"
            );
            return;
        }

        // Phase 6a: remote-edit detection. If we've previously
        // applied plaintext to this messageId and recorded the
        // DPC0:: cover that decrypted to it, but the *current*
        // cover differs, the message was edited (remotely or
        // locally â€” either way our cached plaintext is wrong).
        // Invalidate every cache that would short-circuit the
        // dispatch and fall through; the existing
        // recvDispatchDecrypt path will re-decrypt + re-apply
        // + re-persist via cmd_osl_decrypt_message_with_id.
        const lastCover = recvCovers.get(messageId);
        if (lastCover !== undefined && lastCover !== text) {
            console.log(
                "[OSL] recvEdit detected msg=" +
                    messageId +
                    " reason=cache_miss_on_DPC0"
            );
            recvPlaintext.delete(messageId);
            loadedHistory.delete(messageId);
            recvDone.delete(messageId);
            recvCovers.delete(messageId);
        }

        const cached = recvPlaintext.get(messageId);
        if (cached) {
            recvApplyPlaintext(div, cached);
            console.log(
                "[OSL] recvHandleDiv APPLY id=" +
                    __dbg_id +
                    " reason=cached_plaintext (msg=" +
                    messageId +
                    ")"
            );
            if (DEBUG) {
                console.log(
                    "[OSL] msg=" +
                        messageId +
                        " re-applied cached plaintext"
                );
            }
            return;
        }

        if (recvDone.has(messageId)) {
            console.log(
                "[OSL] recvHandleDiv SKIP id=" +
                    __dbg_id +
                    " reason=already_done (msg=" +
                    messageId +
                    ")"
            );
            return;
        }
        if (recvInFlight.has(messageId)) {
            console.log(
                "[OSL] recvHandleDiv SKIP id=" +
                    __dbg_id +
                    " reason=in_flight (msg=" +
                    messageId +
                    ")"
            );
            return;
        }

        console.log(
            "[OSL] recvHandleDiv DISPATCH id=" +
                __dbg_id +
                " (msg=" +
                messageId +
                ")"
        );
        recvDispatchDecrypt(div, messageId, text);
    }

    /**
     * Send a decrypt RPC for the given messageContent div. Caller
     * has already confirmed: textContent starts with `DPC0::`,
     * no cached plaintext, not in-flight, not permanently settled.
     */
    function recvDispatchDecrypt(div, messageId, coverText) {
        // === v0.0.6.4 diagnostic instrumentation ===
        const tries = recvRetries.get(messageId) || 0;
        if (tries >= RECV_MAX_RETRIES) {
            console.log(
                "[OSL] recvDispatchDecrypt SKIP msg=" +
                    messageId +
                    " reason=retries_exhausted (" +
                    tries +
                    "/" +
                    RECV_MAX_RETRIES +
                    ")"
            );
            recvDone.add(messageId);
            return;
        }

        // Phase 5b3 short-circuit: if the channel-switch hook
        // already pulled this message's plaintext out of the
        // on-disk store, skip the IPC entirely. The DOM apply
        // here covers the lazy-render case where the span
        // wasn't yet mounted at history-load time.
        const fromHistory = loadedHistory.get(messageId);
        if (typeof fromHistory === "string") {
            recvApplyPlaintext(div, fromHistory);
            recvPlaintext.set(messageId, fromHistory);
            recvCovers.set(messageId, coverText);
            recvDone.add(messageId);
            console.log(
                "[OSL] decrypt cache hit msg=" +
                    messageId +
                    " (from history)"
            );
            return;
        }

        const channelId = recvExtractChannelId();
        if (!channelId) {
            console.log(
                "[OSL] recvDispatchDecrypt SKIP msg=" +
                    messageId +
                    " reason=no_channelId (path=" +
                    window.location.pathname +
                    ")"
            );
            return;
        }
        // 7d-FIX1: skip dispatch for burned scopes. Applies to
        // EVERY message regardless of sender — 7d-PIVOT-FIX
        // reverted the self-bypass introduced in PIVOT Task 5a
        // because in practice it was leaking plaintext for the
        // local user's own old messages in burned scopes (the
        // wrapped_keys hadn't always been wiped fully, and
        // even when they had, the bypass meant the recv path
        // still attempted decrypt + sometimes found a stale
        // cache to fall back on). User decision: burn means
        // burned for everyone, no special case for self.
        try {
            if (oslBurnedScopesShouldSkip(channelId)) {
                oslBurnedScopesLogOnceForChannel(channelId);
                recvDone.add(messageId);
                return;
            }
        } catch (_) {}
        const senderDiscordId = recvExtractAuthorId(div);
        if (!senderDiscordId) {
            // Bounded retry rather than terminal skip. The author
            // metadata for own-sent and cozy-grouped messages may
            // be wired in by Discord several React commits after
            // the message-content div first mounts; the periodic
            // sweep will re-hit recvDispatchDecrypt for messages
            // not in recvDone/recvInFlight/recvPlaintext, and each
            // retry runs the enhanced (sibling-walk) extractor.
            const attempts =
                (recvAuthorRetryCount.get(messageId) || 0) + 1;
            recvAuthorRetryCount.set(messageId, attempts);
            if (attempts >= RECV_AUTHOR_MAX_RETRIES) {
                console.log(
                    "[OSL] recvDispatchDecrypt SKIP msg=" +
                        messageId +
                        " reason=no_senderDiscordId_after_retries (" +
                        attempts +
                        "/" +
                        RECV_AUTHOR_MAX_RETRIES +
                        ")"
                );
                recvDone.add(messageId);
                return;
            }
            console.log(
                "[OSL] recvDispatchDecrypt RETRY msg=" +
                    messageId +
                    " attempt=" +
                    attempts +
                    "/" +
                    RECV_AUTHOR_MAX_RETRIES +
                    " reason=no_senderDiscordId"
            );
            return;
        }
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") {
            console.log(
                "[OSL] recvDispatchDecrypt SKIP msg=" +
                    messageId +
                    " reason=no_invoke (Tauri IPC bridge missing)"
            );
            return;
        }

        recvInFlight.add(messageId);
        if (DEBUG) {
            console.log(
                "[OSL] dispatching decrypt for msg=" +
                    messageId +
                    " (try " +
                    (tries + 1) +
                    "/" +
                    RECV_MAX_RETRIES +
                    ", sender=" +
                    senderDiscordId +
                    ")"
            );
        }
        // Phase 5b3: surface the persist field on the wire.
        // Always-on (not gated on DEBUG) so the line appears in
        // release logs as evidence the at-rest store is being
        // populated this run.
        console.log(
            "[OSL] decrypt invoke msg=" + messageId + " (with persist)"
        );

        let timeoutHandle;
        const timeoutPromise = new Promise(function (_, reject) {
            timeoutHandle = nativeSetTimeout(function () {
                reject(new Error(RECV_TIMEOUT_SENTINEL));
            }, RECV_IPC_TIMEOUT_MS);
        });
        const ipcPromise = invoke("osl_decrypt_message", {
            channelId: channelId,
            senderDiscordId: senderDiscordId,
            content: coverText,
            // Phase 5b3: opt into at-rest persistence. The
            // backend treats this as `Option<String>` (`None`
            // skips the store, `Some` writes the row).
            discordMessageId: messageId,
        });

        Promise.race([ipcPromise, timeoutPromise])
            .then(function (plaintext) {
                nativeClearTimeout(timeoutHandle);
                recvInFlight.delete(messageId);
                if (DEBUG) {
                    console.log(
                        "[OSL] decrypt result for msg=" +
                            messageId +
                            ": ok"
                    );
                }
                // Cache by message_id so React replacing the
                // inner span (which it does on every re-render)
                // doesn't lose the plaintext. The next observer
                // tick or sweep re-applies from this cache.
                recvPlaintext.set(messageId, plaintext);
                // Phase 6a: bind the cover that produced this
                // plaintext so the remote-edit detector can
                // tell a stale cache from a re-mount.
                recvCovers.set(messageId, coverText);
                recvDone.add(messageId);

                // Apply on the live messageContent div â€” look it
                // up fresh by id, since `div` may have been
                // detached and re-mounted between dispatch and
                // resolve.
                const liveDiv = document.getElementById(
                    RECV_MESSAGE_ID_PREFIX + messageId
                );
                if (!liveDiv) {
                    if (DEBUG) {
                        console.log(
                            "[OSL] msg=" +
                                messageId +
                                " not in DOM at resolve time; sweep will apply"
                        );
                    }
                    return;
                }
                const before = liveDiv.textContent || "";
                if (DEBUG) {
                    console.log(
                        "[OSL] msg=" +
                            messageId +
                            " pre-update: textContent=" +
                            before.slice(0, 64) +
                            " (len=" +
                            before.length +
                            ")"
                    );
                    console.log(
                        "[OSL] msg=" +
                            messageId +
                            " applying plaintext (len=" +
                            plaintext.length +
                            "): " +
                            plaintext.slice(0, 64)
                    );
                }
                recvApplyPlaintext(liveDiv, plaintext);
                if (DEBUG) {
                    const after = liveDiv.textContent;
                    console.log(
                        "[OSL] msg=" +
                            messageId +
                            " post-update: textContent=" +
                            after.slice(0, 64) +
                            " (len=" +
                            after.length +
                            ")"
                    );
                    nativeSetTimeout(function () {
                        const sweepDiv = document.getElementById(
                            RECV_MESSAGE_ID_PREFIX + messageId
                        );
                        if (!sweepDiv) {
                            console.log(
                                "[OSL] msg=" +
                                    messageId +
                                    " delayed-check: detached from DOM"
                            );
                            return;
                        }
                        const delayed = sweepDiv.textContent || "";
                        const reverted =
                            delayed.indexOf(RECV_PREFIX) === 0;
                        console.log(
                            "[OSL] msg=" +
                                messageId +
                                " delayed-check (100ms): textContent=" +
                                delayed.slice(0, 64) +
                                " (len=" +
                                delayed.length +
                                ")" +
                                (reverted
                                    ? " REVERTED â€” sweep will re-apply"
                                    : " STUCK")
                        );
                    }, 100);
                }
            })
            .catch(function (err) {
                nativeClearTimeout(timeoutHandle);
                recvInFlight.delete(messageId);
                const msg = err && err.message ? err.message : String(err);
                const isTimeout = msg === RECV_TIMEOUT_SENTINEL;

                if (isTimeout) {
                    // Hung IPC â€” increment retry counter, leave
                    // recvDone UNSET so the next observer/sweep
                    // tick re-dispatches. Logged unconditionally
                    // since this is a real diagnostic signal.
                    recvRetries.set(messageId, tries + 1);
                    console.log(
                        "[OSL] decrypt result for msg=" +
                            messageId +
                            ": timeout (" +
                            RECV_IPC_TIMEOUT_MS +
                            "ms); will retry (" +
                            (tries + 1) +
                            "/" +
                            RECV_MAX_RETRIES +
                            ")"
                    );
                    return;
                }

                // Rust-side rejection â€” increment retries and
                // mark settled. Most rejections are permanent
                // (UnknownSender, NoMatchingSlot, BadPrefix).
                recvRetries.set(messageId, tries + 1);
                if (DEBUG) {
                    console.log(
                        "[OSL] decrypt result for msg=" +
                            messageId +
                            ": error: " +
                            msg
                    );
                }

                if (msg.indexOf("no peer mapping for discord_id=") !== -1) {
                    if (!recvUnmappedLogged.has(senderDiscordId)) {
                        recvUnmappedLogged.add(senderDiscordId);
                        console.log(
                            "[OSL] no peer mapping for discord_id=" +
                                senderDiscordId +
                                ", skipping decrypt (add an entry to " +
                                "%APPDATA%/osl/peer_map.json to enable)"
                        );
                    }
                } else if (DEBUG) {
                    if (
                        !recvRejectionsLogged.has(msg) &&
                        recvRejectionsLogged.size < RECV_REJECTION_LOG_CAP
                    ) {
                        recvRejectionsLogged.add(msg);
                        console.log(
                            "[OSL] decrypt rejected (expected for non-OSL " +
                                "messages or stale slots, deduped per session): " +
                                msg
                        );
                    }
                }
                recvDone.add(messageId);
            });
    }

    /**
     * Walk `root` for `[id^="message-content-"]` divs and call
     * `recvHandleDiv` on each. Three discovery paths, in order:
     *
     *  1. `root` itself is a messageContent div (the addedNode
     *     was the full div).
     *  2. `root` contains messageContent divs as descendants
     *     (the addedNode was a wrapper higher up â€” initial
     *     channel load, scrollback batches).
     *  3. `root` is INSIDE a pre-existing messageContent div
     *     (the addedNode was an inner span/text added to a
     *     div that was already mounted). Walk up from `root`
     *     to find the ancestor messageContent div. This is the
     *     case Discord uses for live messages dispatched after
     *     initial render: the wrapper is reused, only the inner
     *     span is added.
     *
     * The periodic sweep is the authoritative safety net for
     * cases this misses; this function is a fast-path for the
     * common cases.
     */
    function recvScanSubtree(root) {
        if (!root) return;
        if (root.nodeType === 1) {
            if (root.matches && root.matches(RECV_MESSAGE_DIV_SELECTOR)) {
                recvHandleDiv(root);
            }
            if (root.querySelectorAll) {
                const divs = root.querySelectorAll(RECV_MESSAGE_DIV_SELECTOR);
                for (const div of divs) {
                    recvHandleDiv(div);
                }
            }
            // 8d V2 attachment scan: for every chat-messages li the
            // observer sees, try to open attachments. The Rust side
            // returns MagicNotFound on non-OSL files, so this is
            // cheap on unencrypted channels. Fire-and-forget — DOM
            // swap completes async after the fetch + decrypt.
            try {
                if (
                    root.matches &&
                    root.matches('li[id^="chat-messages-"]')
                ) {
                    oslScanLiAttachmentsV2(root).catch(function () {});
                }
                if (root.querySelectorAll) {
                    const lis = root.querySelectorAll(
                        'li[id^="chat-messages-"]'
                    );
                    for (const li of lis) {
                        oslScanLiAttachmentsV2(li).catch(function () {});
                    }
                }
            } catch (_) {}
        }
        // Path 3: walk up to find an ancestor messageContent
        // div. recvFindMessageDiv handles both Element and Text
        // node inputs (parentNode chain).
        const ancestor = recvFindMessageDiv(root.parentNode);
        if (ancestor) {
            recvHandleDiv(ancestor);
        }
    }

    /**
     * Phase 5b3 channel-history rehydration. Called from the
     * periodic sweep when the URL's channel_id changes; reads
     * up to `RECV_HISTORY_LIMIT` previously decrypted messages
     * for the channel from the at-rest store and:
     *
     *   - Stashes plaintext in `loadedHistory` (keyed by
     *     discord_message_id) so a lazy-rendered scrollback
     *     message can short-circuit through the
     *     `recvDispatchDecrypt` cache check.
     *   - For messages already mounted in the DOM whose
     *     content still shows the `DPC0::` cover, applies
     *     plaintext immediately and adds to `recvDone` so the
     *     receive observer / sweep doesn't re-dispatch.
     *
     * Wrapped in try/catch end-to-end: a broken history load
     * must NOT regress the live decrypt pipeline. The IPC
     * itself is also try/wrapped on the rejection path.
     */
    const RECV_HISTORY_LIMIT = 100;
    function recvLoadHistory(channelId) {
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") {
            console.log(
                "[OSL] history load failed channel=" +
                    channelId +
                    " reason=no_invoke"
            );
            return;
        }
        invoke("osl_load_channel_history", {
            channelId: channelId,
            limit: RECV_HISTORY_LIMIT,
        })
            .then(function (rows) {
                try {
                    const list = Array.isArray(rows) ? rows : [];
                    console.log(
                        "[OSL] history load channel=" +
                            channelId +
                            " count=" +
                            list.length
                    );
                    for (const dto of list) {
                        if (
                            !dto ||
                            typeof dto.discord_message_id !== "string" ||
                            typeof dto.plaintext !== "string"
                        ) {
                            continue;
                        }
                        const mid = dto.discord_message_id;
                        loadedHistory.set(mid, dto.plaintext);
                        const span = document.getElementById(
                            RECV_MESSAGE_ID_PREFIX + mid
                        );
                        let rendered = false;
                        if (span) {
                            const t = span.textContent || "";
                            if (t.indexOf(RECV_PREFIX) === 0) {
                                recvApplyPlaintext(span, dto.plaintext);
                                recvPlaintext.set(mid, dto.plaintext);
                                // Phase 6a: bind cover â†” plaintext.
                                recvCovers.set(mid, t);
                                recvDone.add(mid);
                                rendered = true;
                            }
                        }
                        console.log(
                            "[OSL] history apply msg=" +
                                mid +
                                " rendered=" +
                                String(rendered)
                        );
                    }
                } catch (e) {
                    console.log(
                        "[OSL] history apply threw channel=" +
                            channelId +
                            " reason=" +
                            (e && e.message ? e.message : String(e))
                    );
                }
            })
            .catch(function (err) {
                const msg =
                    err && err.message ? err.message : String(err);
                console.log(
                    "[OSL] history load failed channel=" +
                        channelId +
                        " reason=" +
                        msg
                );
            });
    }

    /**
     * Periodic sweep â€” every `RECV_SWEEP_INTERVAL_MS`, walk
     * every `[id^="message-content-"]` div in the document.
     *
     * **This is the primary mechanism for finding new messages.**
     * The MutationObserver is unreliable for live messages
     * appended to a pre-mounted message list (Discord swaps the
     * inner span without firing addedNodes on the outer div).
     * The sweep doesn't depend on any mutation signal â€” it polls
     * and does the right thing.
     *
     * Per-tick log surfaces what the sweep saw and did so the
     * user can confirm it's actually firing:
     *
     *   `[OSL] periodic sweep tick (msgs=N, cached=M, dispatched=K)`
     *
     * The body runs inside try/catch so a single bad div doesn't
     * kill the interval â€” exceptions are logged and the sweep
     * continues on the next tick.
     */
    function recvPeriodicSweep() {
        try {
            // Phase 5b3 channel-switch detection. Cheap to read
            // each tick; only triggers a load on a real switch
            // to a different valid channel. Wrapped in its own
            // try/catch so a broken history-load path can't
            // poison the rest of the sweep tick.
            try {
                const here = recvExtractChannelId();
                if (here && here !== lastLoadedChannelId) {
                    lastLoadedChannelId = here;
                    recvLoadHistory(here);
                }
            } catch (e) {
                console.log(
                    "[OSL] history channel-switch threw: " +
                        (e && e.message ? e.message : e)
                );
            }

            const divs = document.querySelectorAll(RECV_MESSAGE_DIV_SELECTOR);
            let cachedCount = 0;
            let dispatchedCount = 0;
            for (const div of divs) {
                const text = div.textContent;
                if (!text || text.indexOf(RECV_PREFIX) !== 0) continue;
                const messageId = recvMessageIdOf(div);
                const cached = recvPlaintext.get(messageId);
                if (cached) {
                    recvApplyPlaintext(div, cached);
                    cachedCount++;
                    continue;
                }
                if (recvDone.has(messageId)) continue;
                if (recvInFlight.has(messageId)) continue;
                recvDispatchDecrypt(div, messageId, text);
                dispatchedCount++;
            }
            if (OSL_DEBUG_SWEEP) {
                console.log(
                    "[OSL] periodic sweep tick (msgs=" +
                        divs.length +
                        ", cached=" +
                        cachedCount +
                        ", dispatched=" +
                        dispatchedCount +
                        ")"
                );
            }
        } catch (e) {
            // Don't let a transient DOM exception kill the
            // interval â€” log and let the next tick try again.
            console.log(
                "[OSL] periodic sweep tick threw: " +
                    (e && e.message ? e.message : e)
            );
        }
    }

    function recvInstallObserver() {
        if (window.__OSL_RECV_INSTALLED__) return;
        if (!document.body) return;
        window.__OSL_RECV_INSTALLED__ = true;

        const obs = new MutationObserver(function (records) {
            for (const r of records) {
                if (r.type === "childList") {
                    for (const n of r.addedNodes) {
                        recvScanSubtree(n);
                    }
                } else if (r.type === "characterData") {
                    // Walk up from the mutated text node to the
                    // messageContent div; the cached-vs-dispatch
                    // decision lives in `recvHandleDiv`.
                    const div = recvFindMessageDiv(r.target);
                    if (div) {
                        recvHandleDiv(div);
                    }
                }
            }
        });
        obs.observe(document.body, {
            childList: true,
            subtree: true,
            characterData: true,
        });

        // Initial sweep â€” catches anything Discord rendered
        // before the observer attached.
        recvScanSubtree(document.body);

        // Primary periodic sweep. Uses the captured native
        // `setInterval` so a Discord bundle that overrides the
        // global timer can't disable us. Stored on `window` so
        // the user can inspect / cancel from DevTools:
        //
        //   window.__OSL_SWEEP_INTERVAL__   (timer id)
        //   clearInterval(window.__OSL_SWEEP_INTERVAL__)
        const sweepIntervalId = nativeSetInterval(
            recvPeriodicSweep,
            RECV_SWEEP_INTERVAL_MS
        );
        window.__OSL_SWEEP_INTERVAL__ = sweepIntervalId;

        // Always-on registration log, NOT gated on DEBUG. If the
        // sweep ever stops firing, the user looks for this line
        // and the per-tick lines below to localise the failure.
        console.log(
            "[OSL] periodic sweep registered (interval=" +
                RECV_SWEEP_INTERVAL_MS +
                "ms, id=" +
                String(sweepIntervalId) +
                ")"
        );

        if (DEBUG) {
            console.log(
                "[OSL] Phase 5 receive observer installed on document.body " +
                    "(message-content anchored, " +
                    RECV_SWEEP_INTERVAL_MS +
                    "ms periodic sweep is primary)"
            );
        }
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", recvInstallObserver);
        // document.addEventListener("DOMContentLoaded", editTabStartObserver);  // disabled: broken Slate-model swap, pending overlay rewrite
        document.addEventListener("DOMContentLoaded", editOverlayInstall);
        document.addEventListener("DOMContentLoaded", oslInstallPhase7c);
    } else {
        recvInstallObserver();
        // editTabStartObserver();  // disabled: broken Slate-model swap, pending overlay rewrite
        editOverlayInstall();
        oslInstallPhase7c();
    }
})();

