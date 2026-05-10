/* OSL boot script — Layer 10 / Phase 3 round 6.
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
 *         bypasses instance-level toString overrides — without this
 *         layer, naive detection would still see the wrapper source.
 *
 *   3. **Compile-time DEBUG strip** (the `DEBUG` const at the top
 *      of the IIFE). All `[OSL]`-prefixed `console.log` /
 *      `console.warn` calls are gated by `if (DEBUG)`. With
 *      `DEBUG = false` in release builds, V8/SpiderMonkey dead-code
 *      eliminate the gated blocks during optimisation — the
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
 *   - `Reflect.getPrototypeOf` / Proxy introspection — Proxies are
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
 *     this isn't a detection vector — but it's worth noting the
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
    //   - decrypt RPCs fire 2× per message, and over a long
    //     session the dispatched count grows unboundedly.
    //
    // The guard is keyed on a single `window` flag — both
    // contexts share `window` because they evaluate in the same
    // frame — so the second evaluation short-circuits before
    // installing anything.
    // ============================================================
    if (window.__OSL_BOOT_INSTALLED__) {
        return;
    }
    window.__OSL_BOOT_INSTALLED__ = true;

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
    // `console.error` calls are intentionally NOT gated — failures
    // are signal we want to surface even in production builds.
    // ============================================================
    const DEBUG = true;

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
                    "[OSL] Tauri IPC bridge not present on window — check " +
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
     * …)` directly — the global is undefined. This wrapper
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
     * `OSL: …` strings the receive observer sees. Includes a
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
     * - `onMutated(newBodyJson)` — encryption succeeded, send the
     *   ciphertext-bearing body.
     * - `onPassthrough()` — there was no plaintext to encrypt
     *   (sticker-only / attachment-only sends with a missing or
     *   empty `content` field). Original body is forwarded as-is.
     *   This is **safe**: nothing was meant to be encrypted.
     * - `onAbort(err)` — Phase 4 fail-closed. We **tried** to
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

        if (typeof parsed.content !== "string") {
            return onPassthrough();
        }
        if (parsed.content === "") {
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
                // would produce on a native — defeats simple
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

                const resolved = resolveFetchRequest(input, init);
                if (resolved === null) {
                    return Reflect.apply(target, thisArg, args);
                }
                const url = resolved.url;
                const method = resolved.method;

                const editMatch = EDIT_RE.exec(url);
                if (editMatch && method === "PATCH") {
                    if (DEBUG)
                        console.log(
                            "[OSL] outgoing edit (fetch PATCH): channel=" +
                                editMatch[1] +
                                " message=" +
                                editMatch[2] +
                                "; passthrough (Phase 4 territory)"
                        );
                    return Reflect.apply(target, thisArg, args);
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
                            // onPassthrough — no plaintext to
                            // encrypt; safe to forward.
                            return Reflect.apply(target, thisArg, args);
                        },
                        function () {
                            // onAbort — Phase 4 fail-closed.
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
                                    // onPassthrough — see string-
                                    // body branch above.
                                    return Reflect.apply(target, thisArg, args);
                                },
                                function () {
                                    // onAbort — Phase 4 fail-closed.
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
                    if (DEBUG)
                        console.log(
                            "[OSL] outgoing edit (XHR PATCH): channel=" +
                                editMatch[1] +
                                " message=" +
                                editMatch[2] +
                                "; passthrough (Phase 4 territory)"
                        );
                    return Reflect.apply(target, thisArg, args);
                }

                const sendMatch = SEND_RE.exec(meta.url);
                if (!sendMatch || meta.method !== "POST") {
                    return Reflect.apply(target, thisArg, args);
                }
                const channelId = sendMatch[1];

                if (typeof body !== "string") {
                    if (DEBUG && body !== undefined && body !== null) {
                        const bodyKind =
                            (body.constructor && body.constructor.name) ||
                            typeof body;
                        console.log(
                            "[OSL] outgoing /messages (XHR): non-string body (" +
                                bodyKind +
                                "); passthrough (Phase 4 will handle multipart)"
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
                // so it's additive — we don't displace any
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
                        // Swallowed — listener never throws.
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
                        // onPassthrough — no plaintext to encrypt.
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
                        // onAbort — Phase 4 fail-closed.
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
                        // notes §13): `xhr.readyState` and `status`
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
        // → false in one of the two webview contexts; toString
        // returned Sentry's wrapper source). Our Proxy and FPT trap
        // were correct — just displaced.
        //
        // Lock the property non-writable + non-configurable so
        // Sentry's later `window.fetch = sentryWrapper` assignment
        // cannot displace us:
        //   - In strict mode: assignment throws TypeError. Sentry
        //     wraps its instrumentation in try/catch (they have to —
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
        // threat model — Discord shouldn't be able to read message
        // content — this is an acceptable trade.
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
    if (!window.__OSL_XHR_HOOK_INSTALLED__ && haveXhr) {
        window.__OSL_XHR_HOOK_INSTALLED__ = true;
        openProxy = new Proxy(origOpen, makeOpenHandler());
        sendProxy = new Proxy(origSend, makeSendHandler());
        XMLHttpRequest.prototype.open = openProxy;
        XMLHttpRequest.prototype.send = sendProxy;
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
    // Layer 10 §14 walks through three rounds of internal-hook
    // recon (FluxDispatcher store discovery, WebSocket gateway
    // intercept). Both are reachable but couple the mod tightly
    // to Discord's reducer ordering and obfuscated module IDs;
    // a single bundle refactor breaks them silently. The DOM is
    // the one Discord-facing API that's observable, public, and
    // stable enough to bet on for v1.
    //
    // ## Accepted v1 limitations (documented in §14.6 + README)
    //
    //   1. **Brief flash** of the DPC0:: cover before async
    //      decrypt completes — typically tens to a few hundred
    //      milliseconds.
    //   2. **DOM-mutation fragility**: any major Discord
    //      message-renderer refactor can break the observer.
    //      Treated as ongoing maintenance.
    //   3. **Sender's own messages flash too** — the encoder
    //      auto-includes the sender as a recipient slot so the
    //      flow is symmetrical, but the cover still renders
    //      first and is replaced by the observer.
    //   4. **Best-effort author_id extraction** — pulled from
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
    // span replacement — when React swaps the inner span back to
    // the cover, our cached plaintext re-applies via
    // `div.textContent = cached`, which removes the new span and
    // installs a single text node. The next React render may
    // re-mount the span; the next observer tick re-applies; the
    // periodic sweep below re-applies once a second as a safety
    // net for renders that don't fire useful mutations.
    const RECV_MESSAGE_DIV_SELECTOR = '[id^="message-content-"]';
    const RECV_MESSAGE_ID_PREFIX = "message-content-";
    // Permanent disposition (decrypt completed — success OR
    // unrecoverable Rust-side rejection). Keyed by message_id
    // so the marker survives React replacing the inner span.
    const recvDone = new Set();
    // Decrypt RPC currently in flight. Keyed by message_id.
    // Prevents the observer or periodic sweep from dispatching
    // duplicate decrypt requests for the same message while one
    // is pending. Cleared on resolve / reject / timeout.
    const recvInFlight = new Set();
    // Per-message retry counter for IPC-layer timeouts (Tauri's
    // postMessage fallback on Discord — which CSP-blocks the
    // custom protocol — has been observed to drop calls after
    // the first roundtrip). Incremented only on rejection /
    // timeout, not pre-call, so a transient hang doesn't burn
    // through the budget while the IPC layer is wedged.
    const recvRetries = new Map();
    const RECV_MAX_RETRIES = 3;
    // IPC timeout. Tuned to be longer than a typical keyserver
    // round-trip (cache-miss fetch_pubkeys is the slowest path,
    // ~1–2s on a healthy connection) but short enough that a
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
    // span, channel switches, and DOM re-mounts — so when the
    // user navigates away and back, we re-apply from cache
    // without re-dispatching IPC.
    const recvPlaintext = new Map();
    // Periodic sweep cadence. 1s feels right: long enough that
    // it's not a CPU sink, short enough that a user who scrolls
    // back to a channel sees plaintext within a beat. Critical
    // for correctness — empirically the MutationObserver does
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
    // Rust round-trip. These are READ-ONLY references — mutating
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
    // `data-author-id` for the first few paints — the metadata
    // is wired in during a later React commit. We retry up to
    // RECV_AUTHOR_MAX_RETRIES times (driven by the periodic
    // sweep, ~1s cadence). After the cap we mark the message
    // settled with reason `no_senderDiscordId_after_retries` so
    // we don't poll forever for a message that's structurally
    // unidentifiable (very rare; system messages or non-user
    // authors).
    const recvAuthorRetryCount = new Map();
    const RECV_AUTHOR_MAX_RETRIES = 10;

    // Outbound-capture cache of own-sent (message_id → author_id).
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

    /** Extract the Discord message_id from a `message-content-…` div. */
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
     *     for 5+ seconds without React re-rendering — yet the
     *     screen kept showing the prior `DPC0::` cover. The
     *     browser was rendering a stale visual snapshot that
     *     `textContent =` didn't dirty. Constructing a brand-
     *     new `<span>` element with `replaceChildren` gives
     *     the paint engine a fresh DOM node it can't have
     *     cached, forcing a real repaint.
     *
     *  2. **Native shape.** Discord renders message content as
     *     `<div id="message-content-…"><span>…</span></div>`.
     *     Our replacement matches that shape exactly, so any
     *     CSS / styling rules that target `> span` keep
     *     applying. `textContent =` collapses children to a
     *     single bare text node, which works for plaintext but
     *     can lose downstream layout assumptions.
     */
    function recvApplyPlaintext(div, plaintext) {
        const span = document.createElement("span");
        span.textContent = plaintext;
        div.replaceChildren(span);
    }

    /**
     * Pull the sender's Discord user_id (== OSL user_id in v1)
     * from the DOM context surrounding `el`. Walks up to the
     * `chat-messages___…` list-item ancestor, then tries:
     *   1. `data-author-id` attribute on item or any descendant.
     *   2. Avatar `<img src="…/avatars/<snowflake>/…">` scan.
     * Returns null on failure — caller skips the request.
     */
    function recvExtractAuthorId(el) {
        // Self-sent capture cache hit. The XHR `load` listener
        // populates `selfSentAuthors` with (msg_id → author_id)
        // for every successful own-send. Cozy-grouped
        // continuation list-items drop `data-author-id` from
        // the DOM, so the DOM walks below would return null —
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
        // messages from the same author as a "group" — only the
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
     * protocol — i.e. discord.com) has been observed to silently
     * drop calls after the first successful roundtrip. We wrap
     * the invoke promise in a `Promise.race` against a setTimeout;
     * a timeout does NOT mark `recvDone` — the next observer tick
     * gets a fresh chance, bounded by `RECV_MAX_RETRIES`. The
     * retry counter increments only on rejection / timeout, NOT
     * pre-call, so a transient hang doesn't burn through the
     * retry budget while the IPC layer is wedged.
     */
    /**
     * Decide what to do with a `[id^="message-content-"]` div
     * that's been observed (mutation, sweep, or initial scan):
     *
     *   - textContent doesn't start with `DPC0::` → already
     *     plaintext or non-OSL content; do nothing.
     *   - cached plaintext for this message_id → re-apply via
     *     `recvApplyPlaintext` synchronously. No IPC.
     *   - settled with no cache (permanent decrypt failure) →
     *     do nothing.
     *   - in-flight decrypt → do nothing; let the resolution
     *     handle the apply.
     *   - otherwise → dispatch a fresh decrypt.
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
        console.log(
            "[OSL] recvHandleDiv ENTRY id=" +
                __dbg_id +
                " text=" +
                __dbg_text
        );

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
            console.log(
                "[OSL] recvHandleDiv SKIP id=" +
                    __dbg_id +
                    " reason=no_DPC0_prefix" +
                    " (first8=" +
                    text.substring(0, 8) +
                    ")"
            );
            return;
        }
        const messageId = recvMessageIdOf(div);

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
                recvDone.add(messageId);

                // Apply on the live messageContent div — look it
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
                                    ? " REVERTED — sweep will re-apply"
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
                    // Hung IPC — increment retry counter, leave
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

                // Rust-side rejection — increment retries and
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
     *     (the addedNode was a wrapper higher up — initial
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
     * Periodic sweep — every `RECV_SWEEP_INTERVAL_MS`, walk
     * every `[id^="message-content-"]` div in the document.
     *
     * **This is the primary mechanism for finding new messages.**
     * The MutationObserver is unreliable for live messages
     * appended to a pre-mounted message list (Discord swaps the
     * inner span without firing addedNodes on the outer div).
     * The sweep doesn't depend on any mutation signal — it polls
     * and does the right thing.
     *
     * Per-tick log surfaces what the sweep saw and did so the
     * user can confirm it's actually firing:
     *
     *   `[OSL] periodic sweep tick (msgs=N, cached=M, dispatched=K)`
     *
     * The body runs inside try/catch so a single bad div doesn't
     * kill the interval — exceptions are logged and the sweep
     * continues on the next tick.
     */
    function recvPeriodicSweep() {
        try {
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
            if (DEBUG) {
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
            // interval — log and let the next tick try again.
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

        // Initial sweep — catches anything Discord rendered
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
    } else {
        recvInstallObserver();
    }
})();
