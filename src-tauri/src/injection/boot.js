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
})();
