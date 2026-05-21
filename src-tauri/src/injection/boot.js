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
    // Phase 4b: account-burn zero-presence guard.
    //
    // After osl_burn_engage wipes everything, oslAccountBurnExecute
    // sets localStorage.__OSL_DECOMMISSIONED__ = "1" before
    // navigating to discord.com/app. This sync check fires before
    // any DOM injection, fetch/XHR hook, or observer installs, so
    // the post-burn landing page is vanilla Discord.
    //
    // Different localStorage key than oslPurgeBlankCache's target so
    // it survives the purge that fires alongside the burn.
    //
    // Backup: the Rust side also writes decommissioned.flag to the
    // OSL config dir. If localStorage was cleared independently
    // (DevTools clear, profile reset), the async IPC check below
    // re-syncs localStorage and reloads. Subsequent boots short-
    // circuit synchronously via this check.
    //
    // To bring OSL back: delete decommissioned.flag in the OSL
    // config dir (or reinstall).
    // ============================================================
    try {
        if (
            typeof window !== "undefined" &&
            window.localStorage &&
            localStorage.getItem("__OSL_DECOMMISSIONED__") === "1"
        ) {
            try {
                console.log(
                    "[OSL] decommissioned (localStorage); boot script exiting"
                );
            } catch (_) {}
            return;
        }
    } catch (_) {}

    // Async backup: ask Rust if the file flag is set. If yes,
    // re-sync localStorage and force a reload so this same script
    // exits via the synchronous check on the next page load. Fire-
    // and-forget; if the IPC fails we just behave as before.
    try {
        const _oslDecomInvoke =
            (window.__TAURI_INTERNALS__ &&
                window.__TAURI_INTERNALS__.invoke) ||
            (window.__TAURI__ && window.__TAURI__.invoke);
        if (typeof _oslDecomInvoke === "function") {
            Promise.resolve(_oslDecomInvoke("osl_is_decommissioned", {}))
                .then(function (yes) {
                    if (yes === true) {
                        try {
                            localStorage.setItem(
                                "__OSL_DECOMMISSIONED__",
                                "1"
                            );
                        } catch (_) {}
                        try {
                            window.location.reload();
                        } catch (_) {}
                    }
                })
                .catch(function () {});
        }
    } catch (_) {}

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

    // 9-TD2.2: F0-FIX1's `/login` / `/register` early-bail block
    // was removed from here. F0-FIX3 diagnostics established that
    // Discord ptb/canary/stable serve the login form inline at
    // `/app` — those routes never actually load — so the bail was
    // unreachable code defending against a page Discord doesn't
    // navigate to. The real login gating happens in
    // `oslTourWaitForLoggedIn` and `oslEnsureSelfSnowflakeRegistered`,
    // which detect the logged-in shell from React runtime state
    // rather than URL. (The canary's separate `/login` skip below
    // is left intact as a no-op safety net.)

    // ============================================================
    // 9-PERF1: loading splash on post-unlock navigation.
    //
    // The gate page sets `__osl_post_unlock_nav` in sessionStorage
    // right before `window.location.href` fires. Boot.js sees the
    // flag synchronously here (before Discord's bundle parses) and
    // paints a full-screen splash so the user sees "OSL is loading
    // Discord" rather than 2+ seconds of unstyled WebView2 white.
    // The splash hides on the first gateway READY frame, or 8s of
    // wallclock, or a click — whichever fires first.
    //
    // sessionStorage survives the gate → discord.com navigation
    // because the WebView2 window context persists; only the page
    // unloads. The flag is removed on first read so a manual page
    // refresh (Ctrl+R) inside Discord doesn't re-show the splash.
    // ============================================================
    const __OSL_IS_POST_UNLOCK = (function () {
        try {
            const v = window.sessionStorage.getItem("__osl_post_unlock_nav");
            if (v) {
                window.sessionStorage.removeItem("__osl_post_unlock_nav");
                return true;
            }
        } catch (_) {}
        return false;
    })();

    const __OSL_SPLASH_ID = "__osl_loading_splash";
    let __osl_splash_timeout = null;

    function oslShowLoadingSplash() {
        // Mount on documentElement because document.body may not
        // exist yet at initialization_script time.
        if (document.getElementById(__OSL_SPLASH_ID)) return;
        const css = document.createElement("style");
        css.id = "__osl_loading_splash_css";
        css.textContent =
            "@keyframes __osl_pulse{" +
            "0%,100%{opacity:0.55;transform:scale(0.94);}" +
            "50%{opacity:1;transform:scale(1.06);}}" +
            "#" + __OSL_SPLASH_ID + "{" +
            "position:fixed;inset:0;z-index:100001;" +
            "background:#0a0a0a;color:#dbdee1;" +
            "display:flex;flex-direction:column;align-items:center;justify-content:center;" +
            "font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;" +
            "transition:opacity 300ms ease;opacity:1;" +
            "user-select:none;cursor:default;}" +
            "#" + __OSL_SPLASH_ID + ".__osl_fade{opacity:0;}" +
            "#" + __OSL_SPLASH_ID + " .__osl_splash_lock{" +
            "width:56px;height:56px;color:#5865f2;" +
            "animation:__osl_pulse 1.6s ease-in-out infinite;" +
            "margin-bottom:18px;}" +
            "#" + __OSL_SPLASH_ID + " .__osl_splash_text{" +
            "font-size:15px;font-weight:500;color:#b5bac1;letter-spacing:0.2px;}";
        (document.head || document.documentElement).appendChild(css);

        const overlay = document.createElement("div");
        overlay.id = __OSL_SPLASH_ID;
        // Inline-SVG closed padlock. Self-contained — does not depend
        // on oslLockSvg which lives much later in this IIFE.
        overlay.innerHTML =
            '<svg class="__osl_splash_lock" viewBox="0 0 24 24" fill="none" ' +
            'stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
            '<rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>' +
            '<path d="M7 11V7a5 5 0 0 1 10 0v4"/>' +
            '</svg>' +
            '<div class="__osl_splash_text">Loading Discord…</div>';
        // Escape hatch: any click dismisses. Users who suspect the
        // splash is stuck can recover without restarting.
        overlay.addEventListener("click", function () {
            oslHideLoadingSplash();
        });

        const root = document.body || document.documentElement;
        root.appendChild(overlay);

        // Fallback: never let the splash outlive 8 seconds. Even if
        // gateway READY never fires (offline, Discord outage, hook
        // missed the frame), the user gets to whatever Discord
        // managed to render. Bind to window directly because the
        // later-in-file `nativeSetTimeout` const is in TDZ at this
        // point.
        __osl_splash_timeout = window.setTimeout(function () {
            oslHideLoadingSplash();
        }, 8000);
    }

    function oslHideLoadingSplash() {
        if (__osl_splash_timeout) {
            try { window.clearTimeout(__osl_splash_timeout); } catch (_) {}
            __osl_splash_timeout = null;
        }
        const overlay = document.getElementById(__OSL_SPLASH_ID);
        if (!overlay) return;
        overlay.classList.add("__osl_fade");
        window.setTimeout(function () {
            if (overlay.parentNode) overlay.parentNode.removeChild(overlay);
            const css = document.getElementById("__osl_loading_splash_css");
            if (css && css.parentNode) css.parentNode.removeChild(css);
        }, 320);
    }

    if (__OSL_IS_POST_UNLOCK) {
        oslShowLoadingSplash();
    }

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
    // TD3-1.3: short-circuit fetches to ipc.localhost / tauri.localhost.
    //
    // Tauri 2's @tauri-apps/api invoke() tries fetch(http://ipc.localhost)
    // first, then falls back to window.postMessage if fetch rejects.
    // On Discord's webview the fetch attempt routinely fails (CSP race
    // pre-strip, or the IPC custom protocol not bound on this build),
    // which leaves WebView2's Chromium network stack logging dozens of
    // red "Failed to fetch ipc.localhost" lines per launch — purely
    // cosmetic noise because the postMessage fallback handles every
    // invoke transparently.
    //
    // Reject the fetch synthetically (a JS-level rejection, NOT an
    // actual network dispatch) so WebView2 has nothing to log. Tauri's
    // own catch fires and postMessage takes over with no observable
    // behaviour change. The Proxy form ensures any downstream fetch
    // wrappers (Phase 4 send-side interception further down) still see
    // the same callable shape.
    // ============================================================
    try {
        if (typeof window.fetch === "function" && !window.__OSL_IPC_FETCH_GUARD__) {
            window.__OSL_IPC_FETCH_GUARD__ = true;
            const origFetch = window.fetch;
            window.fetch = new Proxy(origFetch, {
                apply: function (target, thisArg, args) {
                    let urlStr = "";
                    try {
                        const input = args[0];
                        if (typeof input === "string") {
                            urlStr = input;
                        } else if (input && typeof input.url === "string") {
                            urlStr = input.url;
                        }
                    } catch (_) {}
                    if (
                        urlStr.indexOf("//ipc.localhost") !== -1 ||
                        urlStr.indexOf("//tauri.localhost") !== -1
                    ) {
                        return Promise.reject(new TypeError("Failed to fetch"));
                    }
                    return Reflect.apply(target, thisArg, args);
                },
            });
            console.log("[OSL] IPC fetch shortcut installed (postMessage transport)");
        }
    } catch (e) {
        console.warn(
            "[OSL] IPC fetch shortcut install failed:",
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
    // TD3 sweep: every failed invoke used to emit a fresh console.error
    // and the next caller (often the next paint tick) would re-fire,
    // producing a long red ladder in DevTools on a launch where the
    // snowflake isn't yet registered. Keep a fail-until timestamp and
    // a logged-once flag so subsequent callers reuse the cached null
    // for the backoff window without re-spamming. The Rust side hasn't
    // changed; this is purely cosmetic + console-hygiene.
    let oslSelfDiscordIdFailUntil = 0;
    let oslSelfDiscordIdFailLogged = false;
    const OSL_SELF_ID_FAIL_BACKOFF_MS = 5000;
    function oslSelfDiscordId() {
        if (typeof oslSelfDiscordIdCache === "string") {
            return Promise.resolve(oslSelfDiscordIdCache);
        }
        if (oslSelfDiscordIdInFlight) return oslSelfDiscordIdInFlight;
        if (Date.now() < oslSelfDiscordIdFailUntil) {
            // Inside the backoff window after a recent failure.
            return Promise.resolve(null);
        }
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
                    oslSelfDiscordIdFailLogged = false;
                    return id;
                }
                // Log the actual rejected value so we can diagnose
                // shape mismatches (snowflake-as-empty-string, wrong
                // field, etc.).
                oslSelfDiscordIdLastError =
                    "osl_get_self_user_id returned non-snowflake value " +
                    JSON.stringify(id);
                if (!oslSelfDiscordIdFailLogged) {
                    oslSelfDiscordIdFailLogged = true;
                    console.error("[OSL] " + oslSelfDiscordIdLastError);
                }
                oslSelfDiscordIdFailUntil = Date.now() + OSL_SELF_ID_FAIL_BACKOFF_MS;
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
                if (!oslSelfDiscordIdFailLogged) {
                    oslSelfDiscordIdFailLogged = true;
                    console.error("[OSL] osl_get_self_user_id failed: " + msg);
                }
                oslSelfDiscordIdFailUntil = Date.now() + OSL_SELF_ID_FAIL_BACKOFF_MS;
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
     *   non-string result, JSON re-serialisation failure, or the
     *   prose-token send command failing). The caller MUST simulate
     *   a network failure rather than passing the plaintext through.
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
        //
        // 9-MODE1-FIX: same guard for DPC1:: covers. Mode 1 chunks
        // posted via `oslSendCoverMessage` come through this fetch
        // hook; without this branch the interceptor would treat the
        // sentence cover as plaintext and try to re-encrypt it,
        // which is what produced the "Failed to fetch" symptom on
        // every Mode 1 send.
        if (
            parsed.content.indexOf("DPC0::") === 0 ||
            parsed.content.indexOf("DPC1::") === 0
        ) {
            if (DEBUG)
                console.log(
                    "[OSL] outgoing /messages (" +
                        source +
                        "): content already DPC0::/DPC1::; passthrough (pre-encrypted)"
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

        // Phase 7c / 7d-PIVOT send-path gate: the v=1 per-channel-share
        // path (formerly v1Send + window.__OSL_INTERCEPT__) was retired
        // in Phase 5 cleanup. V2 toggle-gated encryption is the only
        // remaining encrypt path. "encrypt off" means plaintext
        // passthrough; "encrypt on" means the v=2 wire below.

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
            // body. (The legacy v=1 per-channel-share path was
            // retired in Phase 5 cleanup.)
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
                // the message plain. (The legacy v=1 path that
                // could turn this into DPC0:: ciphertext anyway was
                // retired in Phase 5 cleanup.)
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
                function (out) {
                    if (
                        !out ||
                        !Array.isArray(out.messages) ||
                        out.messages.length === 0
                    ) {
                        console.error(
                            "[OSL] v=2 send gate (" +
                                source +
                                "): oslEncryptV2 returned malformed; ABORT (fail-closed)"
                        );
                        return onAbort(new Error("v2_encrypt_failed"));
                    }
                    // Mode 0 fast path: single cover, ship through
                    // Discord's own send. Mode 1 always lands on the
                    // mode1SendPipeline branch below.
                    //
                    // PHASE 2 prose-token pivot: the cipher itself
                    // never appears on the wire — we upload it to
                    // the ephemeral cipher-store, get back an 8-byte
                    // blob ID, and encode that ID as ~5 sentences of
                    // chat-style prose. The cover_text is what posts
                    // to Discord. The receive-side hook in
                    // recvHandleDiv recovers the wire from the cover
                    // via `osl_prose_token_recv` before the existing
                    // decrypt path runs.
                    if (out.messages.length === 1) {
                        const __osl_dpc0Wire = out.messages[0];
                        // Phase 3: per-scope TTL (default 72h if no
                        // setting / IPC fails).
                        return oslGetScopeTtl(v7cScope).then(function (__osl_ttl) {
                        return oslInvoke("osl_prose_token_send", {
                            scopeInput: v7cScope,
                            dpc0Wire: __osl_dpc0Wire,
                            ttlSeconds: __osl_ttl,
                        }).then(function (__osl_resp) {
                            if (
                                !__osl_resp ||
                                !__osl_resp.ok ||
                                !__osl_resp.value ||
                                typeof __osl_resp.value.cover_text !==
                                    "string" ||
                                __osl_resp.value.cover_text.length === 0
                            ) {
                                console.error(
                                    "[OSL] v=2 send gate (" +
                                        source +
                                        "): osl_prose_token_send failed " +
                                        "(error=" +
                                        (__osl_resp &&
                                            __osl_resp.error) +
                                        "); ABORT (fail-closed)"
                                );
                                return onAbort(
                                    new Error("prose_token_send_failed")
                                );
                            }
                            const __osl_coverText =
                                __osl_resp.value.cover_text;
                            const __osl_blobId =
                                __osl_resp.value.blob_id;
                            // Stash cover→blob_id for Phase 4 burn.
                            try {
                                if (!window.__oslCoverToBlobId) {
                                    window.__oslCoverToBlobId = new Map();
                                }
                                window.__oslCoverToBlobId.set(
                                    __osl_coverText,
                                    {
                                        blob_id: __osl_blobId,
                                        scope: v7cScope,
                                    }
                                );
                            } catch (_) {}
                        parsed.content = __osl_coverText;
                        // Self-view: key by the COVER, not the wire.
                        // The /messages XHR `load` listener compares
                        // the echoed `data.content` against this map
                        // to derive msg_id→plaintext; Discord echoes
                        // whatever we POSTed, which after Phase 2 is
                        // the cover text. Keying by wire would miss
                        // every self-view lookup.
                        try {
                            oslSentWireToPlaintext.set(
                                __osl_coverText,
                                plaintext
                            );
                            oslFifoEvict(
                                oslSentWireToPlaintext,
                                OSL_SELF_SENT_PLAINTEXT_MAX
                            );
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
                        // SKDM delivery fix: a v=5 group send returns
                        // its per-peer SKDM v=4 wires on
                        // out.control_messages — a field SEPARATE from
                        // `messages` (which Discord's own send posts).
                        // Post each SKDM as its OWN Discord message
                        // BEFORE releasing the content send so the
                        // receiver can install the sender-key chain
                        // around content arrival (1(b)'s revive is the
                        // real safety net; this just shrinks the race).
                        // Fail-closed (locked decision): an SKDM POST
                        // failure — or a Rust-side per-peer build
                        // failure (ok:false in skdm_peer_status, whose
                        // wire is absent from control_messages by
                        // construction) — does NOT abort the content;
                        // proceed and show a user-visible notice
                        // naming the affected peer(s). control_messages
                        // [i] corresponds to the i-th ok:true
                        // skdm_peer_status entry (both pushed together
                        // per peer in encrypt_v5_send).
                        const __oslCtrl =
                            out && Array.isArray(out.control_messages)
                                ? out.control_messages
                                : [];
                        const __oslSk =
                            out && Array.isArray(out.skdm_peer_status)
                                ? out.skdm_peer_status
                                : [];
                        if (
                            __oslCtrl.length === 0 &&
                            __oslSk.length === 0
                        ) {
                            return onMutated(newBody);
                        }
                        // Probe-5 fix: POST CONTENT BEFORE SKDMs.
                        // Previously SKDMs went first, which meant
                        // they got the lower Discord msg_id and
                        // became the GROUP LEADER (the message
                        // Discord renders with avatar+header).
                        // Subsequent same-author messages (the actual
                        // user content) rendered as compact
                        // continuations with no avatar. When we
                        // hid the SKDM bubble the avatar went with
                        // it, and the user's plaintext message looked
                        // authorless -- mis-attributed to whoever
                        // sent the previous visible message.
                        // Posting content first means content is the
                        // group leader (has avatar), SKDMs are
                        // compact continuations (no chrome), and
                        // hiding them collapses cleanly.
                        // Receiver side: SKDM may arrive after
                        // content (msg_id ordering). The v=5 decrypt
                        // returns "no installed sender-key" /
                        // "not a recipient", recvDispatchDecrypt
                        // leaves recvDone unset for the isV5AwaitingSkdm
                        // class, and the SKDM_APPLIED revive loop
                        // re-dispatches when the SKDM finally lands.
                        // Auto-hide keeps the ciphertext invisible
                        // during the brief gap.
                        const __oslSendResult = onMutated(newBody);
                        (async function () {
                            const okStatuses = __oslSk.filter(
                                function (s) {
                                    return s && s.ok;
                                }
                            );
                            const affected = __oslSk
                                .filter(function (s) {
                                    return s && s.ok === false;
                                })
                                .map(function (s) {
                                    return s && s.peer_discord_id;
                                })
                                .filter(Boolean);
                            // Phase 6.4: SKDM bundles ride the
                            // keyserver control-inbox, NOT Discord.
                            // The wire is the same v=3 multi-
                            // recipient bundle (each peer is a slot)
                            // — we fan-out one inbox POST per peer.
                            const skdmRecipients = okStatuses
                                .map(function (s) {
                                    return s && s.peer_discord_id;
                                })
                                .filter(Boolean);
                            for (
                                let i = 0;
                                i < __oslCtrl.length;
                                i++
                            ) {
                                let oobRes = null;
                                try {
                                    oobRes = await oslSendControlOob(
                                        skdmRecipients,
                                        v7cScope,
                                        __oslCtrl[i]
                                    );
                                } catch (_) {
                                    oobRes = null;
                                }
                                if (!oobRes || oobRes.fail > 0 || oobRes.recipients === 0) {
                                    // Treat any per-recipient failure as
                                    // "delivery incomplete" so the user
                                    // gets the same toast as the prior
                                    // Discord-cover failure mode.
                                    const st = okStatuses[i];
                                    affected.push(
                                        (st &&
                                            st.peer_discord_id) ||
                                            "#" + i
                                    );
                                }
                            }
                            if (affected.length > 0) {
                                const uniq = Array.from(
                                    new Set(affected)
                                );
                                oslToast(
                                    "OSL: encrypted-key delivery " +
                                        "failed for " +
                                        uniq.join(", ") +
                                        " — they won't be able to " +
                                        "read this message until you " +
                                        "resend it. (Message was sent.)",
                                    { durationMs: 10000 }
                                );
                                console.log(
                                    "[OSL] SKDM delivery incomplete affected=" +
                                        uniq.join(",")
                                );
                            } else if (__oslCtrl.length > 0) {
                                console.log(
                                    "[OSL] SKDM posted count=" +
                                        __oslCtrl.length +
                                        " channel=" +
                                        channelId
                                );
                            }
                        })();
                        return __oslSendResult;
                        }); // close osl_prose_token_send .then() body
                        }); // close oslGetScopeTtl .then() body
                    }
                    // 9-MODE1-RETIRE: Mode 1 dispatch suppressed for
                    // V2. Rust coerces stego_mode=mode1 → Mode 0 at
                    // encrypt time, so reaching this branch implies
                    // a stale Rust binary that still emits chunked
                    // covers. Fail-closed (abort the send) rather
                    // than ship anything — the alternative is either
                    // an unencrypted plaintext leak or a broken
                    // multi-message post the receive side can't
                    // reassemble. mode1SendPipeline stays defined
                    // (used by V3 revival); just unreachable here.
                    console.warn(
                        "[OSL] mode1 dispatch suppressed: Mode 1 disabled in V2 " +
                            "(unexpected — Rust should have coerced to Mode 0; check binary)"
                    );
                    return onAbort(new Error("mode1_disabled_in_v2"));
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
    function runPersistEdit(messageId, plaintext, channelId) {
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") return;
        // Probe-2 fix: pass channelId so the Rust side can upsert as
        // self when the row is missing (pre-outbound-persistence
        // messages). Older boot-js builds didn't pass it; the Rust
        // command treats `channelId: null` as the legacy no-op-on-miss
        // behaviour, so this is forward/backward compatible.
        const args = {
            discordMessageId: messageId,
            newPlaintext: plaintext,
        };
        if (typeof channelId === "string" && channelId.length > 0) {
            args.channelId = channelId;
        }
        invoke("osl_persist_edit", args)
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
                // Probe-2 Boot Bug 10: don't swallow silently. The
                // edit landed on Discord but our local store still
                // has the OLD plaintext — next session reload would
                // re-decrypt to the old text. Force-invalidate the
                // session caches so a future sweep re-decrypts the
                // bounced-back wire, and toast the user so they
                // know something went sideways.
                try {
                    if (
                        recvPlaintext &&
                        typeof recvPlaintext.delete === "function"
                    ) {
                        recvPlaintext.delete(messageId);
                    }
                    if (
                        loadedHistory &&
                        typeof loadedHistory.delete === "function"
                    ) {
                        loadedHistory.delete(messageId);
                    }
                    if (
                        selfSentPlaintext &&
                        typeof selfSentPlaintext.delete === "function"
                    ) {
                        selfSentPlaintext.delete(messageId);
                    }
                } catch (_) {}
                try {
                    if (typeof oslToast === "function") {
                        oslToast(
                            "OSL: edit reached Discord but local persist failed — reopen the channel if the message reverts to ciphertext."
                        );
                    }
                } catch (_) {}
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
    // 9-C1: the invitation handshake was removed. Old peers running
    // pre-C1 clients can still post 0x02/0x03 control messages; the
    // recv path surfaces the legacy-ignored sentinel and boot.js
    // silently drops the render.
    const OSL_RESULT_LEGACY_HANDSHAKE_IGNORED =
        "__OSL_CONTROL_LEGACY_HANDSHAKE_IGNORED__";
    // Phase 8: recv-side sentinel for an attachment envelope. The
    // full result string is `OSL_RESULT_ATTACHMENT_PREFIX + json`,
    // where the JSON carries the per-attachment AEAD key + filenames
    // + MIME for boot.js to feed into `osl_open_attachment`.
    const OSL_RESULT_ATTACHMENT_PREFIX = "__OSL_CONTROL_ATTACHMENT__|";
    // Phase 9-A3: a v=4 SKDM (Sender Key Distribution Message) ships
    // a rotation_root for a group sender-keys chain. The Rust side
    // applies it to sender_key_state.json and returns this sentinel.
    // boot.js must NOT render this as user-visible text.
    const OSL_RESULT_SKDM_APPLIED = "__OSL_CONTROL_SKDM_APPLIED__";
    // Auto-recovery (4/4) sentinels. SKDM_REREQUEST is a `<prefix>|<wire>`
    // shape (like ATTACHMENT): an inbound SKDM_REQUEST was honored and
    // the trailing DPC0:: wire must be POSTed back to the channel.
    // SESSION_RESET_APPLIED / RECOVERY_IGNORED are exact-string control
    // sentinels — never rendered as user text.
    const OSL_RESULT_SKDM_REREQUEST_PREFIX = "__OSL_CONTROL_SKDM_REREQUEST__|";
    const OSL_RESULT_SESSION_RESET_APPLIED =
        "__OSL_CONTROL_SESSION_RESET_APPLIED__";
    const OSL_RESULT_RECOVERY_IGNORED = "__OSL_CONTROL_RECOVERY_IGNORED__";
    // Phase 9-B1: Mode 1 chunk reassembly sentinels. Boot.js renders
    // a placeholder badge when the receive side is still waiting on
    // chunks, treats a conflict as a dropped session, and surfaces
    // an invalid chunk inline (the cover text stays visible).
    const OSL_RESULT_MODE1_INCOMPLETE_PREFIX =
        "__OSL_CONTROL_MODE1_INCOMPLETE__|";
    const OSL_RESULT_MODE1_CONFLICT = "__OSL_CONTROL_MODE1_CONFLICT__";
    const OSL_RESULT_MODE1_INVALID = "__OSL_CONTROL_MODE1_INVALID__";

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
            // 9-B1: osl_encrypt_message_v2 now returns
            // { messages, session_id, preview_required }. For Mode 0
            // sends, messages.length === 1 and preview_required is
            // false — the caller can keep using the first element as
            // the single wire string, matching pre-B1 behavior.
            const out = await invoke("osl_encrypt_message_v2", {
                plaintext: plaintext,
                scopeInput: scopeInput,
                channelMembers: channelMembers,
                selfDiscordId: selfDiscordId,
            });
            if (out && Array.isArray(out.messages) && out.messages.length > 0) {
                console.log(
                    "[OSL] osl_encrypt_message_v2 returned, messages=" +
                        out.messages.length +
                        " session_id=" +
                        (out.session_id == null ? "none" : out.session_id)
                );
                return out;
            }
            console.log(
                "[OSL] oslEncryptV2 FAIL reason=malformed_encrypt_output"
            );
            return null;
        } catch (err) {
            const msg = err && err.message ? err.message : String(err);
            console.log("[OSL] oslEncryptV2 FAIL reason=" + msg);
            return null;
        }
    }

    // ============================================================
    // Phase 9-B1: Mode 1 multi-message send pipeline.
    //
    // When osl_encrypt_message_v2 returns more than one cover string
    // (Mode 1), we cannot let Discord's own message-create handle the
    // send: we fire each cover via the REST API with a randomized
    // 500-1500ms delay between sends so that the chunks don't all
    // arrive within a single Discord rate-limit window or get
    // reordered by the gateway.
    //
    // The pipeline is fire-and-forget: it returns true if all covers
    // went out, false on first failure. The interceptBody caller
    // treats this as "send accepted" and aborts Discord's own send so
    // Discord doesn't double-post.
    //
    // 9-MODE1-FIX removed the preview-confirm gate; chunks fire
    // immediately without a user-facing modal.
    // ============================================================

    function mode1RandomDelayMs() {
        return 500 + Math.floor(Math.random() * 1000);
    }

    async function mode1SendPipeline(channelId, encryptOutput, scopeKey) {
        const messages = encryptOutput.messages;
        console.log(
            "[OSL] mode1 send: chunks=" + messages.length +
                " session_id=" + (encryptOutput.session_id || "?") +
                " scope=" + scopeKey
        );
        for (let i = 0; i < messages.length; i++) {
            const cover = messages[i];
            // Use the same REST helper as control messages — it
            // POSTs to /api/v9/channels/<id>/messages with the
            // captured auth token.
            const ok = await oslSendCoverMessage(channelId, cover);
            if (!ok) {
                console.log(
                    "[OSL] mode1SendPipeline chunk " + (i + 1) + "/" +
                        messages.length + " FAIL"
                );
                return false;
            }
            if (i + 1 < messages.length) {
                await new Promise(function (r) {
                    setTimeout(r, mode1RandomDelayMs());
                });
            }
        }
        console.log(
            "[OSL] mode1SendPipeline OK channel=" + channelId +
                " messages=" + messages.length
        );
        return true;
    }

    // Generic cover-message send (Mode 0 or Mode 1). Reuses the
    // same REST shape as oslSendControlMessage but without the
    // DPC0:: prefix gate, since Mode 1 covers start with DPC1::.
    async function oslSendCoverMessage(channelId, coverString) {
        if (!editOverlayAuthToken) {
            console.log("[OSL] oslSendCoverMessage FAIL reason=no_auth_token");
            return false;
        }
        if (typeof coverString !== "string" || coverString.length === 0) {
            console.log("[OSL] oslSendCoverMessage FAIL reason=empty_cover");
            return false;
        }
        const url = "/api/v9/channels/" + channelId + "/messages";
        try {
            // 9-B3: retry-on-stale-token wraps the bare fetch so a
            // mid-rotation 401 doesn't tank a Mode 1 multi-chunk
            // send. Any non-401 failure (403, 5xx, network) returns
            // immediately — handled below as before.
            const resp = await oslFetchWithTokenRetry(url, {
                method: "POST",
                headers: {
                    Authorization: editOverlayAuthToken,
                    "Content-Type": "application/json",
                },
                body: JSON.stringify({ content: coverString }),
            });
            if (resp && resp.ok) return true;
            console.log(
                "[OSL] oslSendCoverMessage FAIL channel=" + channelId +
                    " status=" + (resp ? resp.status : "no_response")
            );
            return false;
        } catch (err) {
            console.log(
                "[OSL] oslSendCoverMessage FAIL channel=" + channelId +
                    " err=" + (err && err.message ? err.message : err)
            );
            return false;
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
    async function oslSendControlMessage(channelId, wireString, scopeInput) {
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
        // Phase 2 extension (Mode-1 SKDM/burn-marker leak fix):
        // control wires (SKDMs, burn markers, recovery SKDMs) used
        // to POST raw DPC0:: text, leaving giant ciphertext visible
        // to anyone reading the channel — including server-side
        // observers and non-OSL clients in the GC. Wrap them in the
        // same prose-token cover content messages use. Scope is
        // required so the receiver can HMAC-verify the cover and
        // route to the v=4/v=5 decrypt path.
        if (!scopeInput) {
            try {
                const ctx =
                    typeof oslCurrentChannelContext === "function"
                        ? oslCurrentChannelContext()
                        : null;
                if (
                    ctx &&
                    typeof oslScopeForCurrentContext === "function"
                ) {
                    scopeInput = oslScopeForCurrentContext(ctx);
                }
            } catch (_) {
                scopeInput = null;
            }
        }
        if (!scopeInput) {
            console.error(
                "[OSL] oslSendControlMessage ABORT reason=no_scope " +
                    "channel=" +
                    channelId +
                    " (refusing raw DPC0:: leak)"
            );
            return null;
        }
        let bodyContent;
        try {
            // Phase 3: per-scope TTL (default 72h if no setting).
            const _ttl = await oslGetScopeTtl(scopeInput);
            const proseResp = await oslInvoke("osl_prose_token_send", {
                scopeInput: scopeInput,
                dpc0Wire: wireString,
                ttlSeconds: _ttl,
            });
            if (
                proseResp &&
                proseResp.ok &&
                proseResp.value &&
                typeof proseResp.value.cover_text === "string" &&
                proseResp.value.cover_text.length > 0
            ) {
                bodyContent = proseResp.value.cover_text;
            } else {
                console.error(
                    "[OSL] oslSendControlMessage ABORT reason=prose_token_send_failed " +
                        "channel=" +
                        channelId +
                        " err=" +
                        (proseResp && proseResp.error) +
                        " (refusing raw DPC0:: leak)"
                );
                return null;
            }
        } catch (e) {
            console.error(
                "[OSL] oslSendControlMessage ABORT reason=prose_token_send_threw " +
                    "channel=" +
                    channelId +
                    " (refusing raw DPC0:: leak)",
                e
            );
            return null;
        }
        const url = "/api/v9/channels/" + channelId + "/messages";
        try {
            // 9-B3: retry-on-stale-token wrapper. SKDM dispatch and
            // burn-marker sends rely on this path; a stale-token
            // 401 mid-burn or mid-SKDM would silently drop the
            // control message.
            const resp = await oslFetchWithTokenRetry(url, {
                method: "POST",
                headers: {
                    Authorization: editOverlayAuthToken,
                    "Content-Type": "application/json",
                },
                body: JSON.stringify({ content: bodyContent }),
            });
            if (resp && resp.ok) {
                // Phase 6.2 fix: control messages (SKDM bundles +
                // burn markers) are protocol noise that shouldn't be
                // visible to the SENDER's own UI. The previous
                // approach relied on the receive pipeline observing
                // the message, decoding the prose cover, running
                // Rust decrypt, getting the SKDM_APPLIED sentinel,
                // and only then hiding -- a multi-second async chain
                // during which the sender sees the prose cover sit
                // in chat for every send. Now: parse the POST
                // response (Discord returns { id, ... }) and stash
                // the message_id in oslSkdmHiddenMsgIds immediately,
                // so the next periodic sweep (~1s tick) hides it
                // even before our own recv processes the SKDM. Best-
                // effort: a parse failure leaves the legacy async
                // hide path in charge.
                try {
                    const _respClone = resp.clone();
                    _respClone
                        .json()
                        .then(function (j) {
                            if (j && typeof j.id === "string" && j.id) {
                                oslSkdmHiddenMsgIds.add(j.id);
                            }
                        })
                        .catch(function () {});
                } catch (_) {}
                console.log(
                    "[OSL] oslSendControlMessage OK channel=" +
                        channelId +
                        " wire_len=" +
                        wireString.length +
                        " cover_len=" +
                        bodyContent.length
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
     * Phase 6.4: out-of-band control delivery via the keyserver
     * control-inbox. Replaces the Discord-channel cover POST for
     * SKDMs, burn markers, SKDM_REQUESTs, and recovery SKDMs.
     *
     * `recipientIds` is an array of Discord snowflakes that should
     * each receive the wire. Self is excluded automatically -- the
     * sender doesn't need to receive their own control wire (Rust
     * state was already mutated on the send side).
     *
     * Returns a status object: { ok: number, fail: number,
     * recipients: number } so the caller can surface a partial-
     * delivery toast on any per-recipient failure. Best-effort: a
     * single failed inbox POST does not abort the rest.
     */
    async function oslSendControlOob(recipientIds, scopeInput, wireString) {
        if (
            typeof wireString !== "string" ||
            wireString.indexOf("DPC0::") !== 0
        ) {
            console.log(
                "[OSL] oslSendControlOob FAIL reason=not_dpc0_wire"
            );
            return { ok: 0, fail: 0, recipients: 0 };
        }
        if (!scopeInput) {
            console.log(
                "[OSL] oslSendControlOob FAIL reason=no_scope"
            );
            return { ok: 0, fail: 0, recipients: 0 };
        }
        if (!Array.isArray(recipientIds)) {
            recipientIds = [];
        }
        let self = null;
        try {
            if (typeof oslSelfDiscordId === "function") {
                self = await oslSelfDiscordId();
            }
        } catch (_) {
            self = null;
        }
        const uniq = Array.from(
            new Set(
                recipientIds.filter(function (r) {
                    return (
                        typeof r === "string" &&
                        r.length > 0 &&
                        (!self || r !== self)
                    );
                })
            )
        );
        let okCount = 0;
        let failCount = 0;
        for (let i = 0; i < uniq.length; i++) {
            const recipientId = uniq[i];
            try {
                const resp = await oslInvoke("osl_control_inbox_post", {
                    recipientId: recipientId,
                    scopeInput: scopeInput,
                    wireString: wireString,
                });
                if (resp && resp.ok) {
                    okCount++;
                } else {
                    failCount++;
                    console.log(
                        "[OSL] oslSendControlOob per-recipient FAIL " +
                            "recipient=" +
                            recipientId +
                            " err=" +
                            (resp && resp.error)
                    );
                }
            } catch (e) {
                failCount++;
                console.log(
                    "[OSL] oslSendControlOob per-recipient threw " +
                        "recipient=" +
                        recipientId +
                        " err=" +
                        (e && e.message ? e.message : e)
                );
            }
        }
        console.log(
            "[OSL] oslSendControlOob ok=" +
                okCount +
                " fail=" +
                failCount +
                " recipients=" +
                uniq.length +
                " wire_len=" +
                wireString.length
        );
        return { ok: okCount, fail: failCount, recipients: uniq.length };
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
        // Auto-recovery: an honored inbound SKDM_REQUEST. Classify as
        // control here so no path renders it; the recv `.then` does
        // the actual POST-back (it has channelId in scope).
        if (result.indexOf(OSL_RESULT_SKDM_REREQUEST_PREFIX) === 0) {
            console.log(
                "[OSL] SKDM re-request honored; wire queued msg=" + msgId
            );
            return true;
        }
        switch (result) {
            case OSL_RESULT_SESSION_RESET_APPLIED:
                console.log(
                    "[OSL] v=4 session reset applied (peer-requested) msg=" +
                        msgId
                );
                return true;
            case OSL_RESULT_RECOVERY_IGNORED:
                console.log(
                    "[OSL] recovery request ignored (guarded) msg=" + msgId
                );
                return true;
            case OSL_RESULT_BURN_APPLIED:
                console.log("[OSL] v=2 burn applied msg=" + msgId);
                return true;
            case OSL_RESULT_LEGACY_HANDSHAKE_IGNORED:
                console.log("[OSL] legacy handshake ignored: msg=" + msgId);
                return true;
            case OSL_RESULT_SKDM_APPLIED:
                // Phase 9-A3: SKDM applied silently. Rust already
                // updated sender_key_state.json; the JS layer must
                // NOT render this sentinel as visible text.
                console.log("[OSL] v=4 SKDM applied msg=" + msgId);
                return true;
            case OSL_RESULT_MODE1_CONFLICT:
                console.log(
                    "[OSL] Mode 1 reassembly conflict; session dropped msg=" +
                        msgId
                );
                return true;
            case OSL_RESULT_MODE1_INVALID:
                console.log(
                    "[OSL] Mode 1 chunk rejected (HMAC/header) msg=" + msgId
                );
                return true;
            default:
                // Phase 9-B1: Mode 1 in-progress reassembly. The
                // sentinel carries session_id|received|total. The
                // locked UI policy is "cover-messages-stay-visible"
                // — chunks 1..N-1 keep their sentence covers in the
                // DOM (they read as innocuous English). Only the
                // final chunk's decrypt resolves to plaintext, at
                // which point the recv observer swaps the cover in
                // the usual way.
                if (
                    typeof result === "string" &&
                    result.indexOf(OSL_RESULT_MODE1_INCOMPLETE_PREFIX) === 0
                ) {
                    const tail = result.slice(
                        OSL_RESULT_MODE1_INCOMPLETE_PREFIX.length
                    );
                    const parts = tail.split("|");
                    const received = parts.length > 1 ? parts[1] : "?";
                    const total = parts.length > 2 ? parts[2] : "?";
                    console.log(
                        "[OSL] Mode 1 reassembly incomplete msg=" + msgId +
                            " " + received + "/" + total
                    );
                    return true;
                }
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
        const out = await oslEncryptV2(plaintext, scope, members, selfDiscordId);
        if (!out) return null;
        // 9-B1: encrypt-v2 now returns { messages, ... }. The debug
        // helper just ships every cover via the standard send path —
        // identical to mode1SendPipeline minus the preview gate.
        if (out.messages.length === 1) {
            return await oslSendControlMessage(channelId, out.messages[0]);
        }
        const scopeKey = scope.kind + ":" + scope.id;
        const ok = await mode1SendPipeline(channelId, out, scopeKey);
        return ok ? true : null;
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
        // 8e: video/mp4 so Discord renders a video-card preview
        // surface (not a `.bin` download card) AND doesn't transcode
        // the bytes (unlike image MIMEs). The Rust seal command
        // already wrapped our payload in an MP4 container.
        const sealedBlob = new Blob([sealedBytes], {
            type: "video/mp4",
        });
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

    // ---- 8e-FIX3: attachment URL cache ----
    //
    // Discord's "video failed to load" file card for our zero-sample
    // decoy MP4 (Phase 8e) doesn't render the CDN URL on any DOM
    // attribute — the URL lives only in React state. The scanner's
    // DOM walk (8e-FIX2) can't find it. We pull the URL out of the
    // message API responses Discord fetches and stash it locally,
    // keyed by Discord message_id. The scanner falls back to this
    // cache when the DOM walk returns zero candidates.
    //
    // LRU at ~1000 entries (Map preserves insertion order; we touch-
    // -bump on re-cache so active messages survive eviction). On
    // overflow we drop the oldest 100.
    if (!window.__oslAttachmentUrlCache) {
        window.__oslAttachmentUrlCache = new Map();
    }
    const OSL_ATT_URL_CACHE_CAP = 1000;
    const OSL_ATT_URL_CACHE_EVICT_BATCH = 100;

    function oslCacheAttachmentUrls(msgId, attachments) {
        if (!msgId || !Array.isArray(attachments) || attachments.length === 0) {
            return 0;
        }
        const entries = [];
        for (const a of attachments) {
            if (!a || typeof a !== "object") continue;
            const url = a.url || a.proxy_url || null;
            const filename = a.filename || null;
            if (typeof url !== "string" || typeof filename !== "string") continue;
            entries.push({
                url: url,
                filename: filename,
                contentType: a.content_type || null,
                size: typeof a.size === "number" ? a.size : null,
            });
        }
        if (entries.length === 0) return 0;
        // Touch-bump so re-cached msgs move to the LRU tail.
        if (window.__oslAttachmentUrlCache.has(msgId)) {
            window.__oslAttachmentUrlCache.delete(msgId);
        }
        window.__oslAttachmentUrlCache.set(msgId, entries);
        console.log(
            "[OSL] attachment url cache: msg=" +
                msgId +
                " cached=" +
                entries.length +
                " filenames=" +
                JSON.stringify(entries.map(function (e) {
                    return e.filename;
                }))
        );
        if (window.__oslAttachmentUrlCache.size > OSL_ATT_URL_CACHE_CAP) {
            const before = window.__oslAttachmentUrlCache.size;
            const iter = window.__oslAttachmentUrlCache.keys();
            for (let i = 0; i < OSL_ATT_URL_CACHE_EVICT_BATCH; i++) {
                const k = iter.next();
                if (k.done) break;
                window.__oslAttachmentUrlCache.delete(k.value);
            }
            const after = window.__oslAttachmentUrlCache.size;
            console.log(
                "[OSL] url cache evict: removed " +
                    (before - after) +
                    " oldest entries (was " +
                    before +
                    ", now " +
                    after +
                    ")"
            );
        }
        return entries.length;
    }

    function oslGetCachedAttachmentUrls(msgId) {
        if (!msgId) return null;
        return window.__oslAttachmentUrlCache.get(msgId) || null;
    }

    /**
     * Walk a parsed message-API response payload and cache any
     * `attachments[]` URLs found. Handles both shapes Discord ships:
     *   - single message object (POST send response, PATCH edit response)
     *   - array of message objects (GET /messages history-load)
     */
    function oslMaybeCacheFromApiResponse(method, data, source) {
        const src = source || "fetch";
        let urlsCached = 0;
        let msgs = 0;
        let kind = "unknown";
        if (Array.isArray(data)) {
            kind = "history";
            msgs = data.length;
            for (const m of data) {
                if (
                    m &&
                    typeof m.id === "string" &&
                    Array.isArray(m.attachments) &&
                    oslCacheAttachmentUrls(m.id, m.attachments) > 0
                ) {
                    urlsCached++;
                }
            }
        } else if (data && typeof data === "object" && typeof data.id === "string") {
            kind = method === "PATCH" ? "edit" : "send";
            msgs = 1;
            if (
                Array.isArray(data.attachments) &&
                oslCacheAttachmentUrls(data.id, data.attachments) > 0
            ) {
                urlsCached = 1;
            }
            // Probe-4 fix: own outbound persistence on the FETCH
            // transport. Previously persist_outbound was wired only
            // into the XHR send path's load listener, so any send
            // Discord routed via fetch was delivered to peers fine
            // but the sender's own MessageStore row was never
            // written -- on reopen, recvLoadHistory found nothing,
            // v=4 own-message decrypt correctly failed
            // "not a recipient" (only wrapped to peer's slot), and
            // the DPC0:: cover stayed visible to the sender. This
            // mirror of the XHR persist call closes that gap.
            if (
                method === "POST" &&
                typeof data.content === "string" &&
                typeof data.channel_id === "string" &&
                oslSentWireToPlaintext.has(data.content)
            ) {
                const _pt = oslSentWireToPlaintext.get(data.content);
                try {
                    selfSentPlaintext.set(data.id, _pt);
                    oslFifoEvict(
                        selfSentPlaintext,
                        OSL_SELF_SENT_PLAINTEXT_MAX
                    );
                } catch (_) {}
                try {
                    window.__TAURI__.core
                        .invoke("osl_persist_outbound", {
                            channelId: data.channel_id,
                            discordMessageId: data.id,
                            plaintext: _pt,
                        })
                        .catch(function (e) {
                            console.log(
                                "[OSL] persist_outbound (fetch) failed " +
                                    "msg=" +
                                    data.id +
                                    ": " +
                                    (e && e.message ? e.message : e)
                            );
                        });
                } catch (_) {}
            }
        }
        if (urlsCached > 0) {
            console.log(
                "[OSL] msg api response (" +
                    src +
                    "): type=" +
                    kind +
                    " msgs=" +
                    msgs +
                    " urls_cached=" +
                    urlsCached
            );
        }
    }

    /**
     * Wrap a fetch Promise so the response body is cloned + parsed
     * for `attachments[]` URL caching. The original Response is
     * returned untouched. Body-read failures are swallowed (partial
     * responses, non-JSON shapes) — message decryption falls back
     * to the regular DOM walk when the cache is empty.
     */
    function oslCaptureMessageApiResponse(fetchPromise, method) {
        return fetchPromise.then(function (resp) {
            try {
                if (resp && typeof resp.clone === "function") {
                    const clone = resp.clone();
                    clone.text().then(
                        function (text) {
                            if (!text) return;
                            try {
                                const data = JSON.parse(text);
                                oslMaybeCacheFromApiResponse(
                                    method,
                                    data,
                                    "fetch"
                                );
                            } catch (e) {
                                // Not JSON / partial; skip.
                            }
                        },
                        function () {
                            // Body read rejected; skip.
                        }
                    );
                }
            } catch (e) {
                console.warn(
                    "[OSL] msg api response capture failed:",
                    e
                );
            }
            return resp;
        });
    }

    // 8e-FIX4: shared URL pattern for /messages API calls — POST send,
    // PATCH edit, GET history-load all flow through the same endpoint
    // family. Used by the XHR open() proxy to decide whether to attach
    // a response listener.
    const MSG_API_RE =
        /\/api\/v\d+\/channels\/\d+\/messages(?:\/\d+)?(?:\?|$)/;

    // 9-A1b FIX-CARRY: periodic blob URL cleanup. Walks every
    // [data-osl-injected="1"] element; if its containing <li> is no
    // longer in document.body (Discord re-mounted or scrolled past),
    // revoke the blob URL on the src. Without this, decrypted
    // attachments leak ~1-10 MB per scrollback. The 60s cadence is
    // a tradeoff between memory pressure and CPU churn; tune via
    // OSL_BLOB_CLEANUP_INTERVAL_MS if it surfaces as a problem.
    const OSL_BLOB_CLEANUP_INTERVAL_MS = 60 * 1000;
    if (!window.__oslBlobCleanupInstalled) {
        window.__oslBlobCleanupInstalled = true;
        nativeSetInterval(function () {
            try {
                const injected = document.querySelectorAll(
                    '[data-osl-injected="1"]'
                );
                let revoked = 0;
                for (const el of injected) {
                    const li = el.closest('li[id^="chat-messages-"]');
                    if (li && document.contains(li)) continue;
                    const src = el.getAttribute("src");
                    if (
                        typeof src === "string" &&
                        src.indexOf("blob:") === 0
                    ) {
                        try {
                            URL.revokeObjectURL(src);
                            revoked++;
                        } catch (_) {}
                    }
                    try {
                        el.remove();
                    } catch (_) {}
                }
                if (revoked > 0) {
                    console.log(
                        "[OSL] blob cleanup: revoked " +
                            revoked +
                            " orphaned blob URLs"
                    );
                }
            } catch (e) {
                console.warn("[OSL] blob cleanup threw:", e);
            }
        }, OSL_BLOB_CLEANUP_INTERVAL_MS);
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
        // 9-A1c: auto-unburn removed. Burned scopes are permanent
        // until the user manually re-engages via the composer toggle.

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
            // 8e: 23 MB cap (was 24 MB on .bin). The MP4 decoy +
            // free-box framing add ~12 KB; cutting the input cap by
            // 1 MB keeps borderline uploads safely under Discord's
            // 25 MB free-tier limit with headroom for AEAD framing.
            if (f.file_size > 23 * 1024 * 1024) {
                oslToast("File too large for encrypted upload (max 23 MB)");
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
            // 8e: `.mp4` (was `.bin` post-8d-FIX2, `.png` through 8d).
            // MP4 is a non-image MIME that Discord doesn't transcode,
            // and renders as a video-card preview surface instead of
            // a generic download card — better visual UX for non-OSL
            // viewers. The Rust seal command wraps our payload inside
            // an MP4 `free` box appended to a decoy MP4 container.
            const randomFilename = (function () {
                const a = new Uint8Array(4);
                crypto.getRandomValues(a);
                let s = "";
                for (const x of a)
                    s += x.toString(16).padStart(2, "0");
                return s + ".mp4";
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
            // 8e: declare video/mp4 so Discord renders a video-card
            // preview surface (better UX than the .bin download card)
            // and doesn't transcode our bytes (MP4 isn't re-encoded
            // the way image MIMEs are). The `content_type` here is
            // honoured by Discord's pre-upload flow and propagated as
            // Content-Type on the GCS presigned PUT URL.
            f.content_type = "video/mp4";
            console.log(
                "[OSL] " +
                    oslLogTime() +
                    " step1 reserved: original=" +
                    reservation.originalFilename +
                    " random=" +
                    randomFilename +
                    " reserved_size=" +
                    reservedSize +
                    " content_type=video/mp4"
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
        // 8e: V3 seal — MP4-wrapped wire. Cover envelope still lives
        // inside the file; outer wrapper is now an MP4 container so
        // the upload MIME is video/mp4 (video-card preview, no
        // transcode) instead of the .bin download card the V2 wire
        // produced post-8d-FIX2.
        const sealRes = await oslInvoke("osl_seal_attachment_with_cover_v3", {
            scopeInput: reservation.scope,
            channelMembers: reservation.channelMembers,
            selfDiscordId: selfId,
            originalBytesB64: b64,
            originalFilename: reservation.originalFilename,
            randomFilename: reservation.randomFilename,
        });
        if (!sealRes.ok) {
            // F3.6 / F-FIX1: free users hit the tier gate at the v3
            // seal entry and get back `OSL-TIER-BLOCKED:{json}`.
            // Render the upgrade modal instead of the generic
            // "encryption failed" toast. The in-flight upload is
            // aborted unconditionally below — Discord's send
            // pipeline can't strip the file mid-flight, so it
            // cannot proceed whichever modal button the user picks.
            // The "Cancel and send text only" path shows a toast
            // telling the user to remove the file and resend; the
            // text then goes through the normal (ungated) encrypt
            // path.
            const handled = oslMaybeHandleTierBlocked(
                sealRes.error,
                /* onCancelTextOnly */ function () {
                    oslToast(
                        "Attached file is a paid feature. " +
                            "Click the × on the file in your message to remove it, " +
                            "then click Send again — your text will go through encrypted.",
                        { durationMs: 8000 }
                    );
                }
            );
            window.__oslPendingUploads.delete(uploadId);
            if (!handled) {
                // Not a tier block — preserve the existing
                // generic-error UX.
                oslToast("Encryption failed, attachment not sent");
            }
            return oslAbortXhr(
                xhr,
                handled
                    ? "step2: tier-gate blocked (free user, paid feature: encrypted attachments)"
                    : "step2: seal_with_cover_v3 failed: " + sealRes.error
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

        // 8e: serve as video/mp4 so Discord's CDN doesn't transcode
        // (MP4 isn't re-encoded the way image MIMEs are) and the
        // file renders as a video-card preview on non-OSL clients.
        // The Rust seal command already wrapped our payload in an
        // MP4 container.
        const newBody = new Blob([sealedBytes], {
            type: "video/mp4",
        });
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

    // ============================================================
    // 9-D: oslBanner — persistent bottom-right notification with
    // optional action buttons. Sibling to #__osl_toast_stack so a
    // pinned banner doesn't block transient toasts above it. Used
    // for the VPN warning ("Dismiss" / "Don't show again").
    //
    // Idempotent on `#__osl_banner`: a second oslBanner call replaces
    // any existing banner content. Returns the banner element so
    // callers can dismiss early via `el.remove()`.
    // ============================================================
    function oslBanner(opts) {
        opts = opts || {};
        const existing = document.getElementById("__osl_banner");
        if (existing) existing.remove();
        const banner = document.createElement("div");
        banner.id = "__osl_banner";
        banner.style.position = "fixed";
        banner.style.bottom = "16px";
        banner.style.right = "16px";
        banner.style.zIndex = "99998";
        banner.style.maxWidth = "380px";
        banner.style.background = "var(--background-floating, #18191c)";
        banner.style.color = "var(--text-normal, #dbdee1)";
        banner.style.padding = "14px 16px 14px 18px";
        banner.style.borderRadius = "8px";
        banner.style.borderLeft = "4px solid var(--status-warning, #f0b132)";
        banner.style.fontSize = "13px";
        banner.style.lineHeight = "1.5";
        banner.style.boxShadow = "0 6px 18px rgba(0, 0, 0, 0.45)";
        banner.style.display = "flex";
        banner.style.flexDirection = "column";
        banner.style.gap = "10px";
        banner.style.pointerEvents = "auto";

        const close = document.createElement("button");
        close.textContent = "✕";
        close.setAttribute("aria-label", "Close");
        close.style.position = "absolute";
        close.style.top = "6px";
        close.style.right = "8px";
        close.style.background = "transparent";
        close.style.border = "none";
        close.style.color = "var(--interactive-normal, #b5bac1)";
        close.style.cursor = "pointer";
        close.style.fontSize = "14px";
        close.style.padding = "2px 6px";
        close.addEventListener("click", function () {
            if (banner.parentNode) banner.parentNode.removeChild(banner);
            if (typeof opts.onClose === "function") {
                try {
                    opts.onClose();
                } catch (_) {}
            }
        });
        banner.appendChild(close);

        const msg = document.createElement("div");
        msg.textContent = String(opts.message || "");
        msg.style.paddingRight = "16px";
        msg.style.whiteSpace = "pre-line";
        banner.appendChild(msg);

        if (Array.isArray(opts.actions) && opts.actions.length > 0) {
            const row = document.createElement("div");
            row.style.display = "flex";
            row.style.gap = "8px";
            row.style.justifyContent = "flex-end";
            for (const act of opts.actions) {
                const btn = document.createElement("button");
                btn.textContent = String(act.label || "");
                btn.style.padding = "6px 12px";
                btn.style.borderRadius = "4px";
                btn.style.fontSize = "13px";
                btn.style.cursor = "pointer";
                btn.style.border = "none";
                if (act.secondary) {
                    btn.style.background = "transparent";
                    btn.style.color = "var(--text-muted, #949ba4)";
                    btn.style.border = "1px solid var(--background-modifier-accent, #3f4147)";
                } else {
                    btn.style.background = "var(--brand-experiment, #5865f2)";
                    btn.style.color = "white";
                }
                btn.addEventListener("click", function () {
                    if (banner.parentNode) banner.parentNode.removeChild(banner);
                    if (typeof act.onClick === "function") {
                        try {
                            act.onClick();
                        } catch (_) {}
                    }
                });
                row.appendChild(btn);
            }
            banner.appendChild(row);
        }

        document.body.appendChild(banner);
        return banner;
    }

    // ============================================================
    // 9-D: oslSpotlight — the onboarding tour's primary visual.
    //
    // Renders a full-screen dim backdrop with an optional rectangular
    // cutout around `opts.target`, a tooltip card positioned relative
    // to the target (or centered when target is null), and the
    // action button + optional skip link.
    //
    // Keyboard contract:
    //   Enter / ArrowRight → advance (calls opts.onAdvance)
    //   Esc → skip if opts.onSkip is set, else no-op
    //   Backdrop click → soft-pause (no advance — explicit choice
    //                    per the D spec's locked design)
    //
    // Edge case: `opts.target` selector may not yet exist (e.g. tour
    // fires before Discord's loaded enough). Caller should resolve
    // the element ahead of time; for the slide driver, see the
    // `waitForElement` helper.
    //
    // Z-index 99999 (below oslConfirm modals at 100000, above the
    // VPN banner at 99998 — keeps the priority ordering sane).
    //
    // Returns `{ el, close }`. `close()` removes the overlay and
    // unbinds listeners; callers must close before constructing the
    // next slide.
    // ============================================================
    function oslSpotlight(opts) {
        opts = opts || {};
        const overlay = document.createElement("div");
        overlay.setAttribute("data-osl-modal", "1");
        overlay.setAttribute("data-osl-tour", "1");
        overlay.style.position = "fixed";
        overlay.style.inset = "0";
        overlay.style.zIndex = "99999";
        overlay.style.pointerEvents = "auto";

        const dimBg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
        dimBg.setAttribute("width", "100%");
        dimBg.setAttribute("height", "100%");
        dimBg.style.position = "absolute";
        dimBg.style.inset = "0";
        const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
        const mask = document.createElementNS("http://www.w3.org/2000/svg", "mask");
        const maskId = "__osl_tour_mask_" + Math.floor(Math.random() * 1e9);
        mask.setAttribute("id", maskId);
        const maskBg = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        maskBg.setAttribute("width", "100%");
        maskBg.setAttribute("height", "100%");
        maskBg.setAttribute("fill", "white");
        mask.appendChild(maskBg);
        const cutout = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        cutout.setAttribute("fill", "black");
        cutout.setAttribute("rx", "6");
        cutout.setAttribute("ry", "6");
        mask.appendChild(cutout);
        defs.appendChild(mask);
        dimBg.appendChild(defs);
        const dimRect = document.createElementNS("http://www.w3.org/2000/svg", "rect");
        dimRect.setAttribute("width", "100%");
        dimRect.setAttribute("height", "100%");
        dimRect.setAttribute("fill", "rgba(0, 0, 0, 0.65)");
        dimRect.setAttribute("mask", "url(#" + maskId + ")");
        dimBg.appendChild(dimRect);
        overlay.appendChild(dimBg);

        const card = document.createElement("div");
        card.style.position = "absolute";
        card.style.background = "var(--background-floating, #18191c)";
        card.style.color = "var(--text-normal, #dbdee1)";
        card.style.padding = "20px 22px";
        card.style.borderRadius = "10px";
        card.style.boxShadow = "0 12px 28px rgba(0, 0, 0, 0.55)";
        card.style.maxWidth = "420px";
        card.style.minWidth = "320px";
        card.style.fontSize = "14px";
        card.style.lineHeight = "1.55";

        if (typeof opts.onSkip === "function") {
            const skipLink = document.createElement("button");
            skipLink.textContent = "Skip tour";
            skipLink.style.position = "absolute";
            skipLink.style.top = "10px";
            skipLink.style.right = "12px";
            skipLink.style.background = "transparent";
            skipLink.style.border = "none";
            skipLink.style.color = "var(--text-muted, #949ba4)";
            skipLink.style.fontSize = "12px";
            skipLink.style.cursor = "pointer";
            skipLink.style.padding = "2px 4px";
            skipLink.addEventListener("click", function (e) {
                e.stopPropagation();
                doSkip();
            });
            card.appendChild(skipLink);
        }

        const titleEl = document.createElement("h3");
        titleEl.style.margin = "0 0 10px 0";
        titleEl.style.fontSize = "18px";
        titleEl.style.fontWeight = "600";
        titleEl.style.paddingRight = "60px";
        titleEl.textContent = opts.title || "";
        card.appendChild(titleEl);

        const bodyEl = document.createElement("div");
        bodyEl.style.whiteSpace = "pre-line";
        bodyEl.style.marginBottom = "16px";
        bodyEl.textContent = opts.body || "";
        card.appendChild(bodyEl);

        if (opts.formContent instanceof Node) {
            card.appendChild(opts.formContent);
        }

        const advanceBtn = document.createElement("button");
        advanceBtn.textContent = opts.buttonLabel || "Next →";
        advanceBtn.style.padding = "8px 16px";
        advanceBtn.style.borderRadius = "5px";
        advanceBtn.style.fontSize = "14px";
        advanceBtn.style.fontWeight = "500";
        advanceBtn.style.cursor = "pointer";
        advanceBtn.style.border = "none";
        advanceBtn.style.background = "var(--brand-experiment, #5865f2)";
        advanceBtn.style.color = "white";
        advanceBtn.style.float = "right";
        advanceBtn.addEventListener("click", function (e) {
            e.stopPropagation();
            doAdvance();
        });
        const actionRow = document.createElement("div");
        actionRow.style.display = "flex";
        actionRow.style.justifyContent = "flex-end";
        actionRow.appendChild(advanceBtn);
        card.appendChild(actionRow);

        overlay.appendChild(card);

        function positionCard() {
            const target = opts.target;
            if (!target || !document.body.contains(target)) {
                cutout.setAttribute("x", "-100");
                cutout.setAttribute("y", "-100");
                cutout.setAttribute("width", "0");
                cutout.setAttribute("height", "0");
                card.style.top = "50%";
                card.style.left = "50%";
                card.style.transform = "translate(-50%, -50%)";
                return;
            }
            const r = target.getBoundingClientRect();
            const pad = 8;
            cutout.setAttribute("x", String(Math.max(0, r.left - pad)));
            cutout.setAttribute("y", String(Math.max(0, r.top - pad)));
            cutout.setAttribute("width", String(r.width + 2 * pad));
            cutout.setAttribute("height", String(r.height + 2 * pad));
            card.style.transform = "none";
            const vh = window.innerHeight;
            const vw = window.innerWidth;
            const cardH = 220;
            const cardW = 360;
            let top, left;
            if (r.top > vh / 2) {
                top = Math.max(20, r.top - cardH - 20);
            } else {
                top = Math.min(vh - cardH - 20, r.bottom + 20);
            }
            left = Math.max(20, Math.min(vw - cardW - 20, r.left + r.width / 2 - cardW / 2));
            card.style.top = top + "px";
            card.style.left = left + "px";
        }
        positionCard();

        function onResize() {
            positionCard();
        }
        window.addEventListener("resize", onResize);

        let mutObs = null;
        if (opts.target && opts.target.parentNode) {
            try {
                mutObs = new MutationObserver(function () {
                    positionCard();
                });
                mutObs.observe(opts.target.parentNode, {
                    childList: true,
                    subtree: true,
                    attributes: true,
                });
            } catch (_) {}
        }

        function onKey(e) {
            if (e.key === "Enter" || e.key === "ArrowRight") {
                e.preventDefault();
                e.stopPropagation();
                doAdvance();
            } else if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                doSkip();
            }
        }
        window.addEventListener("keydown", onKey, true);

        overlay.addEventListener("click", function (e) {
            if (e.target === overlay || e.target === dimBg || e.target === dimRect) {
                // Soft-pause: do nothing (locked design).
                e.stopPropagation();
            }
        });

        function close() {
            window.removeEventListener("resize", onResize);
            window.removeEventListener("keydown", onKey, true);
            if (mutObs) mutObs.disconnect();
            if (overlay.parentNode) overlay.parentNode.removeChild(overlay);
        }

        function doAdvance() {
            close();
            if (typeof opts.onAdvance === "function") {
                try {
                    opts.onAdvance();
                } catch (e) {
                    console.error("[OSL] tour: onAdvance threw", e);
                }
            }
        }
        function doSkip() {
            if (typeof opts.onSkip !== "function") return;
            close();
            try {
                opts.onSkip();
            } catch (e) {
                console.error("[OSL] tour: onSkip threw", e);
            }
        }

        document.body.appendChild(overlay);
        nativeSetTimeout(function () {
            try {
                advanceBtn.focus();
            } catch (_) {}
        }, 30);

        return { el: overlay, close: close };
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
            // 9-B4: tag every OSL modal backdrop so the global keybind
            // dispatcher can suppress firing while a modal is up.
            backdrop.setAttribute("data-osl-modal", "1");
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

    // ============================================================
    // F3.6 / F-FIX1: oslTierGateModal — paid-feature gate.
    //
    // Surfaced when the Rust attachment-seal command returns
    // `OSL-TIER-BLOCKED:{json}` (free user tried to send an
    // encrypted attachment). The in-flight GCS upload is already
    // aborted by the caller before this modal renders — Discord's
    // send pipeline can't cleanly strip a file mid-flight, so the
    // file cannot proceed regardless of which button is chosen.
    // F-FIX1 collapsed the F3.6 three-button layout (where "Send
    // without attachment" and "Cancel" both just aborted, making
    // the middle button misleading) to two honest actions:
    //
    //   1. "Upgrade" (primary blue)        — opens OSL_UPGRADE_URL
    //   2. "Cancel and send text only"
    //      (muted outline)                 — onCancelTextOnly
    //
    // Click-outside is absorbed (no dismiss). ESC unbound. The user
    // MUST pick a button. Idempotent on `#__osl_tier_modal` — a
    // second call while the modal is up focuses the existing one
    // instead of stacking.
    //
    // `opts`:
    //   - feature           : string (diagnostic label, e.g.
    //                          "encrypted attachments"; not rendered,
    //                          surfaced as a data attribute)
    //   - onCancelTextOnly() : called after the "Cancel and send
    //                          text only" button closes the modal
    //                          (wired to the "remove the file and
    //                          resend" toast). Optional.
    //
    // Upgrade is handled fully internally (opens the pricing URL).
    // The send-side abort is unconditional and happens in the
    // caller before the modal is even shown — no per-button
    // abort callback is needed.
    //
    // Returns the modal element so callers can dismiss
    // programmatically via `el.remove()` if needed.
    // ============================================================
    const OSL_UPGRADE_URL = "https://oslprivacy.com/pricing";
    function oslTierGateModal(opts) {
        opts = opts || {};
        const feature = String(opts.feature || "this feature");

        // Idempotency: if a modal is already up, focus it and
        // bail. (A second tier-blocked event during the same
        // user click shouldn't stack two modals on top of each
        // other.)
        const existing = document.getElementById("__osl_tier_modal");
        if (existing) {
            const focusable = existing.querySelector("button");
            if (focusable) {
                try {
                    focusable.focus();
                } catch (_) {}
            }
            return existing;
        }

        const backdrop = document.createElement("div");
        backdrop.id = "__osl_tier_modal";
        backdrop.setAttribute("data-osl-modal", "1");
        backdrop.style.position = "fixed";
        backdrop.style.inset = "0";
        backdrop.style.background = "rgba(0, 0, 0, 0.72)";
        backdrop.style.zIndex = "100001";
        backdrop.style.display = "flex";
        backdrop.style.alignItems = "center";
        backdrop.style.justifyContent = "center";

        const modal = document.createElement("div");
        modal.style.background = "var(--background-floating, #18191c)";
        modal.style.color = "var(--text-normal, #dbdee1)";
        modal.style.padding = "28px 28px 20px 28px";
        modal.style.borderRadius = "10px";
        modal.style.maxWidth = "460px";
        modal.style.boxShadow = "0 12px 36px rgba(0, 0, 0, 0.65)";
        modal.style.fontSize = "14px";
        modal.style.lineHeight = "1.5";

        const title = document.createElement("h2");
        title.style.margin = "0 0 12px 0";
        title.style.fontSize = "20px";
        title.style.fontWeight = "600";
        title.style.letterSpacing = "-0.01em";
        title.textContent = "Encrypted attachments are a paid feature";
        modal.appendChild(title);

        const body = document.createElement("p");
        body.style.margin = "0 0 22px 0";
        body.style.color = "var(--text-normal, #dbdee1)";
        body.textContent =
            "Paid users can send fully encrypted images, files, and other attachments. " +
            "They also get early access to new features.";
        modal.appendChild(body);
        // The `feature` field is surfaced as a data attribute on
        // the modal so DevTools / a future test can verify the
        // gate fired with the expected label. Not rendered.
        modal.setAttribute("data-osl-feature", feature);

        const row = document.createElement("div");
        row.style.display = "flex";
        row.style.flexDirection = "column";
        row.style.gap = "10px";
        row.style.marginBottom = "14px";

        function makeBtn(label, primary, onClick) {
            const btn = document.createElement("button");
            btn.textContent = label;
            btn.style.padding = "10px 14px";
            btn.style.borderRadius = "6px";
            btn.style.fontSize = "14px";
            btn.style.fontWeight = primary ? "500" : "400";
            btn.style.cursor = "pointer";
            btn.style.border = "none";
            btn.style.textAlign = "center";
            if (primary) {
                btn.style.background = "var(--brand-560, #5865f2)";
                btn.style.color = "white";
            } else {
                btn.style.background = "transparent";
                btn.style.color = "var(--text-muted, #949ba4)";
                btn.style.border =
                    "1px solid var(--background-modifier-accent, #3f4147)";
            }
            btn.addEventListener("click", function () {
                close();
                try {
                    onClick();
                } catch (e) {
                    console.error("[OSL] tier-modal action threw:", e);
                }
            });
            return btn;
        }

        const upgradeBtn = makeBtn("Upgrade", true, function () {
            try {
                window.open(OSL_UPGRADE_URL, "_blank", "noopener,noreferrer");
            } catch (_) {}
        });
        const cancelBtn = makeBtn(
            "Cancel and send text only",
            false,
            function () {
                if (typeof opts.onCancelTextOnly === "function") {
                    opts.onCancelTextOnly();
                }
            }
        );
        row.appendChild(upgradeBtn);
        row.appendChild(cancelBtn);
        modal.appendChild(row);

        const footer = document.createElement("p");
        footer.style.margin = "8px 0 0 0";
        footer.style.fontSize = "12px";
        footer.style.color = "var(--text-muted, #949ba4)";
        footer.style.lineHeight = "1.45";
        footer.textContent =
            "\"Cancel and send text only\" cancels the attached file upload. " +
            "You can remove the file in the composer and send your text on " +
            "its own — it'll still be encrypted. Discord itself stays fully " +
            "usable; only encrypted attachment sending requires a paid license.";
        modal.appendChild(footer);

        function close() {
            if (backdrop.parentNode) backdrop.parentNode.removeChild(backdrop);
        }

        // Absorb click-outside (no dismiss): a stray composer click
        // shouldn't ship the user past the choice point.
        backdrop.addEventListener("click", function (e) {
            if (e.target === backdrop) {
                e.preventDefault();
                e.stopPropagation();
            }
        });

        backdrop.appendChild(modal);
        document.body.appendChild(backdrop);

        // Focus the primary action on render so keyboard users
        // can immediately Enter to upgrade.
        try {
            upgradeBtn.focus();
        } catch (_) {}

        return backdrop;
    }

    /**
     * F3.6 / F-FIX1: detect + handle the OSL-TIER-BLOCKED reply
     * from a Rust attachment-seal command. Returns true if the gate
     * fired and the modal was shown; the caller MUST stop the
     * in-flight send (abort the XHR) regardless of which button the
     * user later picks — the file cannot proceed either way.
     * Returns false if the error doesn't carry the prefix — caller
     * handles as a normal error.
     *
     * `onCancelTextOnly` is invoked AFTER the modal's "Cancel and
     * send text only" button closes the modal. Wire it to the
     * "remove the file and resend" toast. Upgrade is handled
     * internally by the modal (opens the pricing URL); it needs no
     * caller callback because the send-side abort is unconditional.
     */
    function oslMaybeHandleTierBlocked(errMsg, onCancelTextOnly) {
        const msg = typeof errMsg === "string" ? errMsg : "";
        const PREFIX = "OSL-TIER-BLOCKED:";
        if (!msg.startsWith(PREFIX)) return false;
        let parsed = null;
        try {
            parsed = JSON.parse(msg.slice(PREFIX.length));
        } catch (e) {
            console.warn("[OSL] tier-blocked JSON tail unparseable:", e, msg);
            // Fall through to true so the caller still aborts — the
            // gate fired even if we can't render the right copy.
            parsed = { feature: "this feature" };
        }
        if (parsed.kind && parsed.kind !== "paid_feature_required") {
            console.warn("[OSL] tier-blocked kind unexpected:", parsed.kind);
        }
        oslTierGateModal({
            feature: parsed.feature || "encrypted attachments",
            onCancelTextOnly: function () {
                if (typeof onCancelTextOnly === "function") {
                    onCancelTextOnly();
                }
            },
        });
        return true;
    }

    // ---- Section 2: current-channel-context helper ----

    /**
     * Private to oslCurrentChannelContext: resolve Discord's
     * ChannelStore + SelectedChannelStore via the webpack chunk.
     * Cached only on SUCCESS (a null result is not cached, so a
     * call made before webpack is ready retries on the next click).
     * Returns `{ ChannelStore, SelectedChannelStore }` or `null`.
     */
    function oslDiscordStores() {
        if (oslDiscordStores._c) return oslDiscordStores._c;
        let result = null;
        try {
            const chunk = window.webpackChunkdiscord_app;
            if (Array.isArray(chunk) && typeof chunk.push === "function") {
                let req;
                const id = "osl_ctx_" + Math.random().toString(36).slice(2);
                chunk.push([
                    [id],
                    {},
                    function (r) {
                        req = r;
                    },
                ]);
                if (req && req.c) {
                    let CS = null;
                    let SCS = null;
                    for (const k in req.c) {
                        const m = req.c[k] && req.c[k].exports;
                        if (!m) continue;
                        const cands = [m, m.default, m.Z, m.ZP];
                        for (const e of cands) {
                            if (!e || typeof e !== "object") continue;
                            if (
                                !CS &&
                                typeof e.getChannel === "function" &&
                                typeof e.hasChannel === "function"
                            ) {
                                CS = e;
                            }
                            if (
                                !SCS &&
                                typeof e.getChannelId === "function" &&
                                (typeof e.getCurrentlySelectedChannelId ===
                                    "function" ||
                                    typeof e.getLastSelectedChannelId ===
                                        "function")
                            ) {
                                SCS = e;
                            }
                        }
                        if (CS && SCS) break;
                    }
                    if (CS && SCS) {
                        result = {
                            ChannelStore: CS,
                            SelectedChannelStore: SCS,
                        };
                    }
                }
            }
        } catch (_) {
            result = null;
        }
        if (result) oslDiscordStores._c = result;
        return result;
    }

    /** Private to oslCurrentChannelContext: best-effort selfId from
     *  the anchor fiber (unchanged source — orthogonal to the
     *  channel facts; callers that truly need self use the Rust
     *  osl_get_self_user_id path). */
    function oslWalkSelfId(fiber) {
        let f = fiber;
        for (let depth = 0; depth < 30 && f; depth++) {
            try {
                const p = f.memoizedProps;
                if (
                    p &&
                    typeof p === "object" &&
                    p.currentUser &&
                    typeof p.currentUser.id === "string"
                ) {
                    return p.currentUser.id;
                }
            } catch (_) {
                // keep walking
            }
            f = f.return;
        }
        return null;
    }

    /** Normalize a recipients array to snowflake strings. */
    function oslNormalizeRecipients(rec) {
        if (!Array.isArray(rec)) return [];
        return rec
            .map(function (r) {
                return typeof r === "string"
                    ? r
                    : (r && (r.id || r.user_id)) || null;
            })
            .filter(function (s) {
                return typeof s === "string" && s.length > 0;
            });
    }

    /**
     * Private to oslCurrentChannelContext: walk the React fiber
     * from a focusable anchor up to the first `memoizedProps.channel`
     * that carries an `.id` + numeric `.type`. VERIFIED LIVE on
     * Discord build 545032: from the composer `[role="textbox"]`
     * this yields the full channel object — for a DM:
     *   { type:1, id:"<dm id>", recipients:["<peer snowflake>"],
     *     rawRecipients:[{id,username,…}] }.
     *
     * Anchors tried in order (first whose fiber yields a channel
     * wins): composer textbox (verified), the channel-header
     * section, a rendered message element. Defensive: never throws;
     * returns the raw channel object or `null`.
     */
    /** The open channel's id from Discord's URL routing contract:
     *  `/channels/@me/<id>` (DM/GC) or `/channels/<guild>/<id>`
     *  (server). This is stable across class re-hashes and fiber
     *  re-shapes — it is what we bind every resolver to so a nearby
     *  stale `memoizedProps.channel` (a DM list row, recents, a call
     *  tile) can never be mistaken for the open channel (the GC→DM
     *  misclassification that made GCs send as v=4 single-peer). */
    function oslSelectedChannelIdFromUrl() {
        try {
            var m = /^\/channels\/(?:@me|\d{15,21})\/(\d{15,21})/.exec(
                location.pathname || ""
            );
            return m ? m[1] : null;
        } catch (_) {
            return null;
        }
    }

    function oslResolveChannelViaFiber() {
        const want = oslSelectedChannelIdFromUrl();
        const anchors = [];
        try {
            anchors.push(oslAnchorResolve("composer"));
        } catch (_) {}
        try {
            anchors.push(document.querySelector('[role="textbox"]'));
        } catch (_) {}
        try {
            anchors.push(oslAnchorResolve("channelHeader"));
        } catch (_) {}
        try {
            anchors.push(
                document.querySelector('[id^="message-content-"]')
            );
        } catch (_) {}
        // Best id-matched channel found; only used as a fallback when
        // the URL id is unknown (e.g. odd route) so we never regress
        // to "no channel" — but an id match always wins immediately.
        let loose = null;
        for (let i = 0; i < anchors.length; i++) {
            const el = anchors[i];
            if (!el) continue;
            let key = null;
            try {
                key = Object.keys(el).find(function (k) {
                    return k.indexOf("__reactFiber") === 0;
                });
            } catch (_) {
                key = null;
            }
            if (!key) continue;
            let f = el[key];
            for (let depth = 0; f && depth < 60; depth++) {
                try {
                    const p = f.memoizedProps;
                    if (
                        p &&
                        p.channel &&
                        typeof p.channel === "object" &&
                        typeof p.channel.id === "string" &&
                        typeof p.channel.type === "number"
                    ) {
                        if (want == null) {
                            if (!loose) loose = p.channel;
                        } else if (p.channel.id === want) {
                            // Bound to the open channel — authoritative.
                            return p.channel;
                        }
                        // id mismatch → a nearby/stale channel prop;
                        // keep walking instead of mis-resolving.
                    }
                } catch (_) {
                    // keep walking
                }
                f = f.return;
            }
        }
        return want == null ? loose : null;
    }

    /** Private: FALLBACK (b) — Discord's webpack ChannelStore /
     *  SelectedChannelStore. Kept in case a future build restores
     *  the module cache. Returns the raw channel object or null. */
    function oslResolveChannelViaStore() {
        const stores = oslDiscordStores();
        if (!stores) return null;
        try {
            const cid = stores.SelectedChannelStore.getChannelId();
            if (typeof cid === "string" && cid) {
                const ch = stores.ChannelStore.getChannel(cid);
                if (
                    ch &&
                    typeof ch === "object" &&
                    typeof ch.type === "number" &&
                    typeof ch.id === "string"
                ) {
                    return ch;
                }
            }
        } catch (_) {
            // fall through
        }
        return null;
    }

    /** Private: FALLBACK (c) — the legacy title-section DOM anchor
     *  + atomic per-channel-object fiber bind. Returns the raw
     *  channel object (the ONE whose id matches the rendered
     *  channel) or null. No cross-fiber merge, no p.guildId leak. */
    function oslResolveChannelViaDom() {
        let anchor = null;
        try {
            anchor =
                document.querySelector(
                    'section[class*="title_"][class*="container__"]'
                ) ||
                document.querySelector(
                    'section[class*="title_"][class*="container_"]'
                ) ||
                document.querySelector('[id^="message-content-"]');
        } catch (_) {
            anchor = null;
        }
        if (!anchor) return null;
        let fiber = null;
        try {
            const key = Object.keys(anchor).find(function (k) {
                return k.indexOf("__reactFiber") === 0;
            });
            fiber = key ? anchor[key] : null;
        } catch (_) {
            fiber = null;
        }
        if (!fiber) return null;
        let channelId = null;
        let f = fiber;
        for (let depth = 0; f && depth < 30; depth++) {
            try {
                const p = f.memoizedProps;
                if (p && typeof p === "object") {
                    if (
                        channelId == null &&
                        typeof p.channelId === "string"
                    ) {
                        channelId = p.channelId;
                    }
                    const c =
                        p.channel && typeof p.channel === "object"
                            ? p.channel
                            : null;
                    if (
                        c &&
                        typeof c.id === "string" &&
                        typeof c.type === "number" &&
                        (channelId == null || c.id === channelId)
                    ) {
                        return c;
                    }
                }
            } catch (_) {
                // keep walking
            }
            f = f.return;
        }
        return null;
    }

    /** Private: normalize ANY resolver's raw channel object into the
     *  ctx-facts shape. guildId is ONLY set for server text (type 0)
     *  — DM (1) / Group DM (3) are ALWAYS null so a DM can never take
     *  oslScopeForCurrentContext's server-channel branch. */
    function oslChannelFacts(ch, via) {
        if (!ch || typeof ch !== "object") return null;
        const type =
            typeof ch.type === "number" ? ch.type : null;
        const id = typeof ch.id === "string" ? ch.id : null;
        if (id == null || type == null) return null;
        const isPrivate = type === 1 || type === 3;
        return {
            resolvedVia: via,
            channelId: id,
            channelType: type,
            guildId:
                type === 0 && typeof ch.guild_id === "string"
                    ? ch.guild_id
                    : null,
            members: isPrivate
                ? oslNormalizeRecipients(
                      Array.isArray(ch.recipients)
                          ? ch.recipients
                          : ch.recipientIds
                  )
                : [],
        };
    }

    /**
     * Recover the open channel's context:
     *   - channelId   : the Discord channel snowflake
     *   - channelType : 0 server text, 1 DM, 3 GC
     *   - guildId     : the guild snowflake for server channels;
     *                   ALWAYS null for DMs/GCs (type 1/3)
     *   - members     : DM/GC → recipient snowflake strings;
     *                   server (type 0) → [] (the gateway-roster
     *                   path in oslCtxMemberIds fills this)
     *   - selfId      : best-effort local user id (anchor fiber)
     *
     * Returns `null` when nothing resolves (no channel UI mounted,
     * e.g. settings open) — preserving the prior lifecycle contract.
     *
     * Resolver order, first that yields a channel wins:
     *   (a) React-fiber walk  — VERIFIED on build 545032 (primary)
     *   (b) webpack ChannelStore / SelectedChannelStore (fallback)
     *   (c) legacy title-section DOM anchor + atomic fiber bind
     * Resilient: if Discord breaks one shape the others may survive.
     */
    function oslCurrentChannelContext() {
        // Best-effort selfId — light walk from a focusable anchor;
        // orthogonal to the channel facts (callers that truly need
        // self use the Rust osl_get_self_user_id path).
        let selfId = null;
        try {
            const a =
                document.querySelector('[role="textbox"]') ||
                document.querySelector('section[class*="title_"]') ||
                document.querySelector('[id^="message-content-"]');
            if (a) {
                const k = Object.keys(a).find(function (x) {
                    return x.indexOf("__reactFiber") === 0;
                });
                if (k) selfId = oslWalkSelfId(a[k]);
            }
        } catch (_) {
            selfId = null;
        }

        // Prefer whichever resolver yields the channel whose id ==
        // the URL's open-channel id (Discord's stable routing
        // contract). Only if none match (odd route / URL parse miss)
        // do we fall back to first-non-null, preserving the old
        // lifecycle contract without ever mis-resolving a GC as a DM.
        const want = oslSelectedChannelIdFromUrl();
        const candidates = [
            oslChannelFacts(oslResolveChannelViaFiber(), "fiber"),
            oslChannelFacts(oslResolveChannelViaStore(), "store"),
            oslChannelFacts(oslResolveChannelViaDom(), "dom"),
        ];
        let facts = null;
        if (want != null) {
            for (let ci = 0; ci < candidates.length; ci++) {
                if (candidates[ci] && candidates[ci].channelId === want) {
                    facts = candidates[ci];
                    break;
                }
            }
        }
        if (!facts) {
            facts = candidates[0] || candidates[1] || candidates[2];
        }

        if (!facts) {
            oslCurrentChannelContext._dbg = {
                resolvedVia: null,
                channelId: null,
                channelType: null,
                members: [],
                guildId: null,
            };
            return null;
        }

        const ctx = {
            channelId: facts.channelId,
            channelType: facts.channelType,
            guildId: facts.guildId, // null unless server text (0)
            members: facts.members,
            selfId: selfId,
        };

        // Debug hook payload (read by window.__oslDebugCtx).
        oslCurrentChannelContext._dbg = {
            resolvedVia: facts.resolvedVia,
            channelId: ctx.channelId,
            channelType: ctx.channelType,
            members: ctx.members,
            guildId: ctx.guildId,
        };
        // One readable line per channel switch (dedupe by id so a
        // busy sweep loop doesn't spam the console).
        if (oslCurrentChannelContext._loggedCid !== ctx.channelId) {
            oslCurrentChannelContext._loggedCid = ctx.channelId;
            try {
                console.log(
                    "[OSL] ctx resolved via " +
                        facts.resolvedVia +
                        ": type=" +
                        ctx.channelType +
                        " members=" +
                        (ctx.members ? ctx.members.length : 0)
                );
            } catch (_) {}
        }
        return ctx;
    }

    // DevTools instrumentation: window.__oslDebugCtx() forces a
    // fresh resolve and returns JSON of what the resolver produced
    // + which path won — call it in the console after switching
    // channels, no rebuild needed.
    try {
        window.__oslDebugCtx = function () {
            oslCurrentChannelContext();
            return JSON.stringify(
                oslCurrentChannelContext._dbg || null
            );
        };
    } catch (_) {}

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
     * 9-TD1.4: after any state-mutating invoke, poll the persist-
     * error slot once and surface a toast if a disk write failed
     * silently. Idempotent — the take-and-clear semantics mean
     * over-polling is harmless; the cost of NOT polling is the user
     * thinking a change saved when it didn't.
     *
     * Fire-and-forget — callers don't await this; it runs after
     * their primary invoke completes. A failing poll itself is
     * silently swallowed (no double-toast).
     */
    function oslCheckPersistError() {
        try {
            const invoke = getTauriInvoke();
            if (typeof invoke !== "function") return;
            invoke("osl_take_last_persist_error", {})
                .then(function (msg) {
                    if (typeof msg === "string" && msg.length > 0) {
                        try {
                            if (typeof oslToast === "function") {
                                oslToast(
                                    "Couldn't save change to disk — please try again. (" + msg + ")",
                                    { durationMs: 6000 }
                                );
                            } else {
                                console.warn("[OSL] persist error: " + msg);
                            }
                        } catch (_) {}
                    }
                })
                .catch(function () {});
        } catch (_) {}
    }

    /**
     * REGISTER-FIX: surface the two security signals that must NOT
     * be warn-swallowed:
     *   1. registration conflict (our user_id is held by a DIFFERENT
     *      key — squat or lost key): one-shot, read+cleared server-
     *      slot; shown as a strong banner.
     *   2. peer TOFU key-change: a peer's identity key differs from
     *      the trusted first-seen baseline. Shown as a banner with
     *      the new safety number + Accept / Dismiss actions
     *      (Accept adopts the new key as baseline; Dismiss keeps the
     *      old one and the alert re-raises next fetch). Decryption is
     *      never blocked — the user decides.
     *
     * Fire-and-forget, fully defensive (mirrors oslCheckPersistError).
     * `oslShownKeyChange` de-dupes so a still-changed key doesn't
     * re-banner on every opportunistic poll within a session.
     */
    var oslShownKeyChange = oslShownKeyChange || {};
    function oslCheckSecurityAlerts() {
        try {
            const invoke = getTauriInvoke();
            if (typeof invoke !== "function") return;

            invoke("osl_take_registration_alert", {})
                .then(function (msg) {
                    if (typeof msg === "string" && msg.length > 0 &&
                        typeof oslBanner === "function") {
                        oslBanner({
                            message:
                                "⚠ OSL SECURITY: " + msg +
                                "\n\nUntil resolved, peers may be unable to " +
                                "message you securely. Open OSL Settings for details.",
                        });
                    }
                })
                .catch(function () {});

            invoke("osl_list_key_change_alerts", {})
                .then(function (list) {
                    if (!Array.isArray(list)) return;
                    for (const a of list) {
                        if (!a || !a.discord_id) continue;
                        const seen = oslShownKeyChange[a.discord_id];
                        if (seen === a.new_ed25519_pub) continue;
                        oslShownKeyChange[a.discord_id] = a.new_ed25519_pub;
                        const who = a.osl_user_id || a.discord_id;
                        if (typeof oslBanner !== "function") continue;
                        oslBanner({
                            message:
                                "⚠ OSL SECURITY: " + who +
                                "'s security key CHANGED. This can be a new " +
                                "device — or someone intercepting your messages. " +
                                "Verify this safety number with them out-of-band " +
                                "before continuing:\n\n" + a.new_safety_number,
                            actions: [
                                {
                                    label: "I verified — Accept",
                                    onClick: function () {
                                        invoke("osl_accept_key_change", {
                                            discordId: a.discord_id,
                                        }).catch(function () {});
                                    },
                                },
                                {
                                    label: "Dismiss",
                                    secondary: true,
                                    onClick: function () {
                                        invoke("osl_decline_key_change", {
                                            discordId: a.discord_id,
                                        }).catch(function () {});
                                        // allow re-alert later if still changed
                                        delete oslShownKeyChange[a.discord_id];
                                    },
                                },
                            ],
                        });
                    }
                })
                .catch(function () {});
        } catch (_) {}
    }

    /**
     * Phase 7c: invoke a Tauri command with a uniform error
     * shape. Returns `{ ok: true, value }` or `{ ok: false, error }`.
     *
     * 9-TD1.4: after every successful non-self invoke, opportunistically
     * poll the persist-error slot. If a recent Rust-side persist
     * failed silently (whitelist write, burn flush, settings save,
     * gateway-tap state push, etc.), the user sees a toast. Skips the
     * poll if we're invoking the take-error command itself (avoids
     * infinite recursion) and if the command failed (don't double-
     * toast a single failure).
     */
    async function oslInvoke(name, args) {
        const invoke = getTauriInvoke();
        if (typeof invoke !== "function") {
            return { ok: false, error: "no_invoke" };
        }
        try {
            const value = await invoke(name, args || {});
            if (name !== "osl_take_last_persist_error") {
                oslCheckPersistError();
            }
            // REGISTER-FIX: opportunistically surface security
            // alerts too. Exclude the security commands themselves
            // (avoid recursion / self-clear races).
            if (name !== "osl_take_registration_alert" &&
                name !== "osl_list_key_change_alerts" &&
                name !== "osl_accept_key_change" &&
                name !== "osl_decline_key_change") {
                oslCheckSecurityAlerts();
            }
            return { ok: true, value: value };
        } catch (err) {
            const msg = err && err.message ? err.message : String(err);
            return { ok: false, error: msg };
        }
    }

    // Phase 3: per-scope cipher-store TTL. Replaces the hardcoded
    // 259200 (72h) every `osl_prose_token_send` callsite used to
    // pass. Falls back to the same 72h default if the IPC fails or
    // returns something unexpected — never blocks a send on a
    // settings read.
    const OSL_DEFAULT_TTL_SECONDS = 259200;
    async function oslGetScopeTtl(scopeInput) {
        if (!scopeInput) return OSL_DEFAULT_TTL_SECONDS;
        try {
            const resp = await oslInvoke("osl_get_scope_ttl", {
                scopeInput: scopeInput,
            });
            if (
                resp &&
                resp.ok &&
                typeof resp.value === "number" &&
                resp.value > 0
            ) {
                return resp.value | 0;
            }
        } catch (_) {}
        return OSL_DEFAULT_TTL_SECONDS;
    }

    /**
     * Phase 3 debug handle. Sets the per-scope TTL until the
     * settings UI slider is wired in. Returns the effective
     * (post-clamp) value so the caller can confirm.
     *
     * Example:
     *   await window.__oslSetScopeTtl("gc", "1502771310428819569", 86400)
     */
    window.__oslSetScopeTtl = async function (scopeKind, scopeId, seconds) {
        const resp = await oslInvoke("osl_set_scope_ttl", {
            scopeInput: { kind: scopeKind, id: scopeId },
            ttlSeconds: seconds | 0,
        });
        console.log(
            "[OSL] __oslSetScopeTtl",
            scopeKind + ":" + scopeId,
            "requested=" + (seconds | 0),
            "result=",
            resp
        );
        return resp;
    };
    window.__oslGetScopeTtl = async function (scopeKind, scopeId) {
        const r = await oslGetScopeTtl({ kind: scopeKind, id: scopeId });
        console.log("[OSL] __oslGetScopeTtl", scopeKind + ":" + scopeId, "=>", r);
        return r;
    };

    // ---- Section 3: profile popout/sidebar Whitelist button ----

    const PROFILE_BUTTON_DATA_ATTR = "data-osl-whitelist-btn";

    /**
     * Tri-state lock colors. Baked directly into the SVG stroke (see
     * oslLockSvg) rather than relying on `currentColor` inheritance.
     * Discord ships CSS rules on header buttons whose specificity beat
     * our inline `style.color`, so a currentColor SVG could render
     * blurple (#5865f2) instead of our intended color. Hardcoding the
     * stroke removes that whole class of "lock is blue" bugs.
     */
    const OSL_LOCK_COLORS = {
        closed: "#23a559", // green  — fully encrypted
        partial: "#f0b132", // yellow — some recipients set up
        open: "#b5bac1", // light grey — no encryption (clearly not blue)
        unknown: "#80848e", // dim grey — roster not loaded yet
    };

    /**
     * SVG lock icon used by the Whitelist button + encrypt toggle.
     * `state` is "open" | "closed" | "partial" | "unknown". The stroke
     * color is baked in per state so the icon can never inherit
     * Discord's button accent (blurple). Renders at 16x16.
     */
    function oslLockSvg(state) {
        // 9-C1 Stage 4: tri-state lock — "closed" (all whitelisted),
        // "partial" (some), "open" (none), "unknown" (roster unknown).
        // The shackle path varies per state; the body is constant.
        const color = OSL_LOCK_COLORS[state] || OSL_LOCK_COLORS.open;
        let shackle;
        switch (state) {
            case "closed":
                shackle = '<path d="M8 11V7a4 4 0 0 1 8 0v4"/>';
                break;
            case "partial":
                // Half-open shackle — closes from the left, dashes
                // on the right.
                shackle =
                    '<path d="M8 11V7a4 4 0 0 1 4-4"/>' +
                    '<path d="M12 3a4 4 0 0 1 4 4" stroke-dasharray="2 2"/>';
                break;
            case "unknown":
                // Question-mark inside an upside-down shackle.
                return (
                    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" ' +
                    'stroke="' +
                    color +
                    '" stroke-width="2" stroke-linecap="round" ' +
                    'stroke-linejoin="round" aria-hidden="true">' +
                    '<rect x="4" y="11" width="16" height="10" rx="2"/>' +
                    '<path d="M8 11V7a4 4 0 0 1 8 0"/>' +
                    '<text x="12" y="19" text-anchor="middle" font-size="9" ' +
                    'font-weight="bold" stroke="none" fill="' +
                    color +
                    '">?</text>' +
                    "</svg>"
                );
            case "open":
            default:
                shackle = '<path d="M8 11V7a4 4 0 0 1 8 0"/>';
                break;
        }
        return (
            '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" ' +
            'stroke="' +
            color +
            '" stroke-width="2" stroke-linecap="round" ' +
            'stroke-linejoin="round" aria-hidden="true">' +
            '<rect x="4" y="11" width="16" height="10" rx="2"/>' +
            shackle +
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

    // === W3: central anchor resolver ===========================
    // One chokepoint for every fragile Discord-DOM lookup. Each
    // anchor lists ordered strategies; the FIRST strategies are the
    // exact selectors used before this refactor (strictly behavior-
    // preserving), followed by resilient ARIA/structure fallbacks
    // that are only reached when ALL prior strategies already failed
    // — i.e. they can only ADD coverage, never regress. A strategy
    // is a CSS string (querySelector on the root) or a
    // `(root) => Element|null` function. Resolver-by-name keeps the
    // ~40 hashed-class call sites fixable in one place and is the
    // seam a future signed remote selector-map plugs into.
    function oslAnchorRegistry() {
        if (window.__oslAnchorReg) return window.__oslAnchorReg;
        var reg = {
            // Strategy order is semantics-first: Discord re-hashes
            // CSS class names frequently but its ARIA labels, role
            // attributes, `data-list-id`s and the `message-content-`
            // / `chat-messages-` id prefixes have been stable for
            // years. Lead with those so a class re-hash does NOT
            // break us; keep the old `[class*=]` selectors only as
            // last-ditch hints (harmless if they never match).
            channelHeader: [
                // The channel header is the bar carrying the toolbar
                // buttons. These aria-labels are user-facing and
                // stable across class re-hashes; the bar is their
                // nearest section/header ancestor.
                function () {
                    var lbls = [
                        "Pinned Messages",
                        "Threads",
                        "Notification Settings",
                        "Hide Member List",
                        "Show Member List",
                        "Start Voice Call",
                    ];
                    for (var i = 0; i < lbls.length; i++) {
                        var b = document.querySelector(
                            '[aria-label="' + lbls[i] + '" i]'
                        );
                        if (b) {
                            var sec = b.closest("section, header");
                            if (sec) return sec;
                        }
                    }
                    return null;
                },
                // Structural: the first labelled region inside the
                // chat content main (the messages list is below it).
                function () {
                    var m = document.querySelector(
                        'main[class*="chatContent"], main'
                    );
                    if (!m) return null;
                    return (
                        m.querySelector(
                            'section[aria-label], header[aria-label]'
                        ) || null
                    );
                },
                'section[class*="title_"]',
                'header[class*="title_"]',
            ],
            userPanel: [
                // The bottom-left account panel: a control with one
                // of these stable aria-labels, climbed to the small
                // container that also holds the avatar.
                function () {
                    var ctl = document.querySelector(
                        '[aria-label="Mute" i],' +
                            '[aria-label="Deafen" i],' +
                            '[aria-label="User Settings" i],' +
                            '[aria-label="Settings" i]'
                    );
                    if (!ctl) return null;
                    var n = ctl;
                    for (var i = 0; i < 6 && n; i++) {
                        n = n.parentElement;
                        if (
                            n &&
                            n.querySelector(
                                'img[class*="avatar" i], [class*="avatar" i]'
                            )
                        ) {
                            return n;
                        }
                    }
                    return ctl.closest("section") || ctl.parentElement;
                },
                'section[class*="panels_"]',
            ],
            composer: [
                // Stable: the message box is the editable textbox;
                // its enclosing form is the composer region.
                function () {
                    var box = document.querySelector(
                        '[role="textbox"][contenteditable="true"]'
                    );
                    if (!box) return null;
                    return box.closest("form") || box.parentElement || null;
                },
                '[class*="channelTextArea_"]',
            ],
            guildsRail: [
                // Discord tags the server rail list with a stable
                // data-list-id; this survives every class re-hash.
                '[data-list-id="guildsnav"]',
                // The nav element that actually contains server /
                // @me channel links.
                function () {
                    var navs = document.querySelectorAll("nav");
                    for (var i = 0; i < navs.length; i++) {
                        if (
                            navs[i].querySelector(
                                'a[href^="/channels/@me"], a[href^="/channels/"]'
                            )
                        ) {
                            return navs[i];
                        }
                    }
                    return null;
                },
                'nav[class*="guilds_"]',
                '[class*="guildsList_"]',
            ],
            profileSurface: [
                '[class*="user-profile-sidebar"], [class*="user-profile-popout"]',
                '[class*="userProfileOuter"], [class*="userProfile_"]',
            ],
        };
        window.__oslAnchorReg = reg;
        return reg;
    }

    /**
     * Resolve a registered anchor to its first matching Element, or
     * null. `root` defaults to document. Never throws (each strategy
     * is guarded). Exposed as `window.__oslAnchorResolve` for live
     * diagnosis after a Discord update.
     */
    function oslAnchorResolve(name, root) {
        var scope = root || document;
        var strategies = oslAnchorRegistry()[name];
        if (!strategies) return null;
        for (var i = 0; i < strategies.length; i++) {
            var s = strategies[i];
            try {
                var el =
                    typeof s === "function" ? s(scope) : scope.querySelector(s);
                if (el) return el;
            } catch (_) {}
        }
        return null;
    }
    window.__oslAnchorResolve = oslAnchorResolve;
    // === end W3 anchor resolver ================================

    function oslFindProfileSurface() {
        return oslAnchorResolve("profileSurface");
    }

    /**
     * Resolve the PROFILE SUBJECT (the peer whose profile this is) —
     * NOT the first `user` fiber found. On a DM `user-profile-sidebar`
     * the nearest user-object ancestor is the SELF user; returning
     * that is the Symptom-2 bug (whitelisting stored self).
     *
     * Resolution order:
     *   1. If we're in a DM, the conversation peer IS the subject —
     *      take it from the ONE canonical resolver (recipients[0]),
     *      so the sidebar agrees with the header/scope paths.
     *   2. Otherwise walk the fiber for the subject id Discord passes
     *      to profile cards: `memoizedProps.userId` first (the
     *      explicit subject id), then `memoizedProps.user.id`.
     *
     * HARD SELF-GUARD: never return the local user's snowflake. Any
     * candidate equal to the cached self id is skipped; if the only
     * resolvable id is self, return null so the caller surfaces
     * "could not resolve user id from profile" instead of silently
     * whitelisting self. (`oslSelfDiscordIdCache` is the synchronous
     * session cache; null only before the first self-id resolve.)
     */
    function oslExtractUserFromProfile(surfaceEl) {
        const selfId =
            typeof oslSelfDiscordIdCache === "string"
                ? oslSelfDiscordIdCache
                : null;
        const isSnowflake = function (s) {
            return typeof s === "string" && /^\d{17,20}$/.test(s);
        };

        // 1. DM context → the peer is the subject (canonical resolver).
        try {
            const ctx = oslCurrentChannelContext();
            if (ctx && ctx.channelType === 1) {
                const ids = oslCtxMemberIds(ctx).filter(function (m) {
                    return m !== selfId;
                });
                if (ids.length > 0 && isSnowflake(ids[0])) {
                    return { id: ids[0], username: ids[0] };
                }
            }
        } catch (_) {
            // fall through to the fiber walk
        }

        // 2. Fiber walk for the profile subject id, self-guarded.
        try {
            const key = Object.keys(surfaceEl).find(function (k) {
                return k.indexOf("__reactFiber") === 0;
            });
            let fiber = key ? surfaceEl[key] : null;
            for (let d = 0; d < 30 && fiber; d++) {
                const p = fiber.memoizedProps;
                if (p && typeof p === "object") {
                    const cand =
                        (typeof p.userId === "string" && p.userId) ||
                        (p.user &&
                            typeof p.user.id === "string" &&
                            p.user.id) ||
                        null;
                    if (cand && cand !== selfId && isSnowflake(cand)) {
                        const uname =
                            (p.user &&
                                (typeof p.user.username === "string"
                                    ? p.user.username
                                    : p.user.global_name)) ||
                            cand;
                        return { id: cand, username: uname };
                    }
                }
                fiber = fiber.return;
            }
        } catch (e) {
            // fall through
        }
        // Nothing resolvable that isn't self → caller toasts.
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
        // 9-B4: dropdown intercepts Escape and traps user focus, so
        // treat it as a modal for the global keybind suppression gate.
        dd.setAttribute("data-osl-modal", "1");
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
                await oslAddWhitelist(
                    user,
                    opt.scopeInput,
                    opt.kind === "dm" ? broadenChecked : false
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
     * Add a local whitelist for `user` in `scopeInput`. This is a
     * LOCAL action only: `osl_set_whitelist` mutates client state
     * and returns nothing. There is NO invitation and NO wire — the
     * 9-C1 handshake was removed and decrypt is permissive, so the
     * peer needs no acceptance; once they have our keys their recv
     * path simply decrypts our (new) messages in this scope.
     */
    async function oslAddWhitelist(user, scopeInput, broadened) {
        const setResult = await oslInvoke("osl_set_whitelist", {
            peerDiscordId: user.id,
            scopeInput: scopeInput,
            broadened: broadened,
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
        // The Rust set_whitelist call auto-removed the scope from the
        // burned-scopes ledger; mirror that to the in-memory JS cache
        // so the recv observer immediately resumes decrypting (new)
        // messages in this scope on the next sweep tick. Old
        // ciphertext stays unreadable (wrapped_keys gone). This is a
        // legitimate local cache sync, not invitation cruft.
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
        oslToast("Whitelisted " + (user.username || user.id));
        // Refresh the channel header (encrypt toggle may have
        // become available).
        oslRefreshHeaderState();
    }

    // ---- Section 4: channel header encrypt toggle + burn ----

    const HEADER_ENCRYPT_DATA_ATTR = "data-osl-encrypt-toggle";
    const HEADER_BURN_DATA_ATTR = "data-osl-burn-btn";

    // Sidebar channel-lock: replace the # icon on the
    // currently-selected server channel with the same tri-state OSL
    // lock the channel header uses. Pure JS, NO global CSS -- if
    // anything ever breaks we just remove the injection and the #
    // icon snaps right back. Click goes through the existing
    // oslOnWhitelistIconClick handler.
    const SIDEBAR_LOCK_DATA_ATTR = "data-osl-sidebar-channel-lock";
    const SIDEBAR_HIDDEN_ICON_ATTR = "data-osl-sidebar-hashtag-hidden";
    // Last definitive lock state per channel id ("closed"|"partial"|
    // "open"). Lets a freshly-injected sidebar lock paint its real
    // color INSTANTLY from cache instead of showing the grey-blue
    // "unknown" glyph while the async whitelist-state IPC is in flight
    // — that flash (worse under IPC congestion) is the "still blue,
    // not instant" report.
    const oslSidebarLockCache = new Map();

    function oslSweepSidebarChannelLock() {
        try {
            const selected = document.querySelector(
                "li[class*='modeSelected__']"
            );
            const existingLock = document.querySelector(
                "[" + SIDEBAR_LOCK_DATA_ATTR + "='1']"
            );
            const existingHiddenIcon = document.querySelector(
                "[" + SIDEBAR_HIDDEN_ICON_ATTR + "='1']"
            );
            // If selection moved (our lock is in a non-current <li>),
            // restore the previous # icon + remove the orphan lock.
            if (existingLock) {
                const lockLi = existingLock.closest(
                    "li[class*='modeSelected__']"
                );
                if (!lockLi || (selected && lockLi !== selected)) {
                    try {
                        existingLock.remove();
                    } catch (_) {}
                    if (existingHiddenIcon) {
                        try {
                            existingHiddenIcon.style.removeProperty(
                                "display"
                            );
                            existingHiddenIcon.removeAttribute(
                                SIDEBAR_HIDDEN_ICON_ATTR
                            );
                        } catch (_) {}
                    }
                }
            }
            if (!selected) return;
            const inSelectedLock = selected.querySelector(
                "[" + SIDEBAR_LOCK_DATA_ATTR + "='1']"
            );
            if (inSelectedLock) {
                try {
                    oslRefreshSidebarLockState(inSelectedLock);
                } catch (_) {}
                return;
            }
            // Discord's structure for a text channel:
            //   <li class="modeSelected__... wrapper__...">
            //     ...
            //     <div class="iconContainer__..." role="img"
            //          aria-label="Text icon">
            //       <svg class="icon__..."> ... </svg>
            //     </div>
            //     <div class="channelName__...">general</div>
            //     ...action buttons (mute, settings, etc.)
            //   </li>
            // Target the iconContainer directly -- it's the slot
            // Discord reserves for the channel-type icon. We hide
            // the whole container and inject our lock in its place.
            // Falls back to the svg if no iconContainer wrapper
            // exists (older Discord builds may differ).
            const iconEl =
                selected.querySelector("[class*='iconContainer__']") ||
                selected.querySelector("svg[class*='icon__']");
            if (!iconEl || !iconEl.parentElement) return;
            const lock = document.createElement("div");
            lock.setAttribute(SIDEBAR_LOCK_DATA_ATTR, "1");
            lock.setAttribute("role", "button");
            lock.setAttribute("tabindex", "0");
            lock.style.display = "inline-flex";
            lock.style.alignItems = "center";
            lock.style.justifyContent = "center";
            lock.style.cursor = "pointer";
            lock.style.color = "var(--text-muted, #87898c)";
            lock.style.width = (iconEl.offsetWidth || 16) + "px";
            lock.style.height = (iconEl.offsetHeight || 16) + "px";
            // Paint the last-known state for this channel immediately so
            // there's no grey "unknown" flash; the async refresh below
            // overwrites it once the real state lands.
            let __initState = "unknown";
            try {
                const __c = oslCurrentChannelContext();
                if (__c && __c.channelId && oslSidebarLockCache.has(__c.channelId)) {
                    __initState = oslSidebarLockCache.get(__c.channelId);
                }
            } catch (_) {}
            lock.innerHTML = oslLockSvg(__initState);
            lock.title = "Server-channel whitelist (loading…)";
            lock.addEventListener("click", function (e) {
                e.preventDefault();
                e.stopPropagation();
                try {
                    oslOnWhitelistIconClick(e);
                } catch (err) {
                    console.error(
                        "[OSL] sidebar lock click handler threw",
                        err
                    );
                }
                nativeSetTimeout(function () {
                    try {
                        oslSweepSidebarChannelLock();
                    } catch (_) {}
                }, 50);
            });
            // Hide the original # via inline style only (NOT global
            // CSS -- guarantees easy reversal if anything breaks).
            try {
                iconEl.style.display = "none";
                iconEl.setAttribute(SIDEBAR_HIDDEN_ICON_ATTR, "1");
            } catch (_) {}
            iconEl.parentElement.insertBefore(lock, iconEl);
            try {
                oslRefreshSidebarLockState(lock);
            } catch (_) {}
        } catch (_) {}
    }

    async function oslRefreshSidebarLockState(lock) {
        if (!lock) return;
        const ctx = oslCurrentChannelContext();
        if (!ctx || !ctx.channelId) return;
        const scopeInput = oslScopeForCurrentContext(ctx);
        if (!scopeInput || scopeInput.kind !== "server_channel") return;
        try {
            const sw = await oslInvoke("osl_get_server_whitelist_state", {
                serverId: scopeInput.server_id,
                channelScopeInput: scopeInput,
            });
            if (!sw.ok) {
                lock.innerHTML = oslLockSvg("unknown");
                lock.style.color = "var(--text-muted, #87898c)";
                lock.title =
                    "Server channel whitelist: unknown (" +
                    (sw.error || "?") +
                    ")";
                return;
            }
            const st = sw.value || {};
            let state, color, title;
            if (st.server_header) {
                state = "closed";
                color = "var(--status-positive, #23a559)";
                title = "Server-wide whitelist ON. Click to turn OFF.";
            } else if (st.channel) {
                state = "partial";
                color = "#f0b132";
                title =
                    "This channel is whitelisted. Click to whitelist the WHOLE server.";
            } else {
                state = "open";
                color = "var(--text-muted, #87898c)";
                title =
                    "Not whitelisted. Click to whitelist the whole server.";
            }
            lock.innerHTML = oslLockSvg(state);
            lock.style.color = color;
            lock.title = title;
            // Cache the definitive state so the next inject for this
            // channel paints instantly (no unknown/blue flash).
            if (ctx.channelId) {
                oslSidebarLockCache.set(ctx.channelId, state);
            }
        } catch (_) {}
    }

    /** Last-known scope state for the header (so toggle clicks have
     *  current values without round-tripping every time). */
    let oslHeaderState = {
        scopeKey: null,
        encryptToggle: false,
        hasWhitelist: false,
    };

    // State-based blank-row sweep with a local cache + viewport
    // filter, so the per-tick cost is O(visible-uncached) instead of
    // O(all-visible-li). Cache lookup is O(1); cached entries skip
    // the slow textContent read entirely. Hidden rows just get
    // their display:none re-applied (no textContent read) so
    // Discord re-mounting can't strand a blank.
    //
    // Decision is invalidated by recvApplyPlaintext when plaintext
    // lands -- the next sweep tick re-evaluates that one row and
    // shows it.
    const oslBlankDecisions = new Map();

    // Persist the hidden-row decisions across program restarts so
    // re-opening a previously-viewed channel hides cipher rows on
    // mount (mutation observer applies display:none synchronously
    // before paint) instead of after the next sweep tick. Storage:
    // localStorage (per-origin discord.com, survives close). Format:
    // JSON array of msgId strings. Only "hidden" decisions persist
    // -- "shown" is the cheap recomputed default.
    const __OSL_BLANK_CACHE_KEY = "osl_blank_hidden_v1";
    // Soft cap. msgId is ~18B; 50k entries ≈ 1MB serialised, well
    // under the 5-10MB localStorage budget. Map insertion order is
    // preservation-FIFO, so capping by tail keeps the most recent.
    const __OSL_BLANK_CACHE_MAX = 50000;
    let __oslBlankCacheDirty = false;
    let __oslBlankCacheFlushTimer = null;

    function oslLoadBlankCacheFromStorage() {
        try {
            const raw = localStorage.getItem(__OSL_BLANK_CACHE_KEY);
            if (!raw) return;
            const arr = JSON.parse(raw);
            if (!Array.isArray(arr)) return;
            for (const msgId of arr) {
                if (typeof msgId === "string" && msgId.length > 0) {
                    oslBlankDecisions.set(msgId, "marked");
                }
            }
        } catch (_) {}
    }

    function oslMarkBlankCacheDirty() {
        __oslBlankCacheDirty = true;
        if (__oslBlankCacheFlushTimer !== null) return;
        __oslBlankCacheFlushTimer = nativeSetTimeout(function () {
            __oslBlankCacheFlushTimer = null;
            oslFlushBlankCacheToStorage();
        }, 5000);
    }

    let __oslBlankCachePurged = false;

    function oslFlushBlankCacheToStorage() {
        // After account burn, the cache is purged and writes are
        // suppressed until next page navigation — otherwise the
        // beforeunload flush would persist the in-memory Map back
        // to localStorage immediately after we just wiped it.
        if (__oslBlankCachePurged) return;
        if (!__oslBlankCacheDirty) return;
        try {
            const hidden = [];
            for (const [msgId, decision] of oslBlankDecisions) {
                if (decision === "marked") hidden.push(msgId);
            }
            const capped =
                hidden.length > __OSL_BLANK_CACHE_MAX
                    ? hidden.slice(-__OSL_BLANK_CACHE_MAX)
                    : hidden;
            localStorage.setItem(
                __OSL_BLANK_CACHE_KEY,
                JSON.stringify(capped)
            );
            __oslBlankCacheDirty = false;
        } catch (_) {}
    }

    // Called by the account-burn path. Clears the in-memory cache,
    // removes the localStorage entry, and suppresses any pending
    // flushes (including the beforeunload one) so post-burn state
    // is genuinely vanilla.
    function oslPurgeBlankCache() {
        try {
            oslBlankDecisions.clear();
        } catch (_) {}
        __oslBlankCachePurged = true;
        __oslBlankCacheDirty = false;
        if (__oslBlankCacheFlushTimer !== null) {
            try {
                clearTimeout(__oslBlankCacheFlushTimer);
            } catch (_) {}
            __oslBlankCacheFlushTimer = null;
        }
        try {
            localStorage.removeItem(__OSL_BLANK_CACHE_KEY);
        } catch (_) {}
    }

    // Pre-fill the in-memory cache from disk BEFORE the first
    // sweep tick or mutation observer fires.
    oslLoadBlankCacheFromStorage();

    // Flush on unload so a rapid program close doesn't strand
    // pending writes inside the 5s debounce window.
    try {
        window.addEventListener("beforeunload", function () {
            oslFlushBlankCacheToStorage();
        });
        // pagehide fires even when beforeunload is suppressed
        // (Discord sometimes blocks it). Belt-and-suspenders.
        window.addEventListener("pagehide", function () {
            oslFlushBlankCacheToStorage();
        });
    } catch (_) {}

    // Synchronous mount-time marker. Called for each new <li> the
    // mutation observer sees; if the cache says this msgId is
    // cipher, find the message-content div and apply the
    // [ENCRYPTED] marker IMMEDIATELY (mutation observer fires
    // before paint, so the user never sees the cipher flash on
    // channel re-open).
    function oslApplyCachedHideToLi(li) {
        const id = (li && li.id) || "";
        if (!id.startsWith("chat-messages-")) return;
        const dash = id.lastIndexOf("-");
        const msgId = dash >= 0 ? id.slice(dash + 1) : id;
        if (oslBlankDecisions.get(msgId) !== "marked") return;
        const div = li.querySelector("[id^='message-content-']");
        if (div) oslAutoHideCiphertext(div);
    }

    // Scroll-preservation wrapper. SKDM hide still uses display:none
    // on the row, which shrinks layout above the viewport and
    // (without compensation) makes the visible content jump.
    // Snapshot a visible-anchor row's viewport-relative offset before
    // `fn`, then nudge scrollTop after so that anchor stays put.
    // No-op when there's no scroller or no anchor.
    function oslFindChatScroller() {
        const list = document.querySelector(
            "ol[data-list-id='chat-messages']"
        );
        if (!list) return null;
        let n = list.parentElement;
        while (n && n !== document.body) {
            const cs = getComputedStyle(n);
            if (
                (cs.overflowY === "auto" ||
                    cs.overflowY === "scroll" ||
                    cs.overflow === "auto" ||
                    cs.overflow === "scroll") &&
                n.scrollHeight > n.clientHeight
            ) {
                return n;
            }
            n = n.parentElement;
        }
        return null;
    }

    // Phase 2 made this a no-op. The prose-token path doesn't hide
    // <li>s anymore (only the inner content div gets marker text),
    // so there's no layout shrink to compensate for. Earlier full
    // implementation did querySelectorAll + getBoundingClientRect
    // per <li> per mutation — fine on quiet channels, catastrophic
    // during composer typing where Discord fires many mutations per
    // keystroke and a busy channel might have 50+ rendered <li>s.
    // The browser's default `overflow-anchor: auto` handles SKDM
    // display:none shrinks without our help.
    function oslWithScrollPreservation(fn) {
        fn();
    }

    function oslDecideBlank(li) {
        const content = li.querySelector("[id^='message-content-']");
        if (!content) return null; // not a regular message; leave alone
        if (content.hasAttribute("data-osl-cipher-hidden")) {
            return "marked"; // already marked, just record the decision
        }
        const text = (content.textContent || "").trim();
        const looksLikeCipher =
            text.indexOf("DPC0::") !== -1 ||
            text.indexOf("DPC1::") !== -1;
        return looksLikeCipher ? "marked" : "shown";
    }

    // Walk all <li>s, mark any cipher rows with [ENCRYPTED],
    // cache the decision. Replaces the old hide-the-<li> sweep --
    // no more layout shrink, so no scroll preservation needed.
    // Cache is checked first (O(1) Map.get) so steady-state cost
    // is just the querySelectorAll + per-row Map lookup.
    function oslSweepBlankRows() {
        const lis = document.querySelectorAll(
            "li[id^='chat-messages-']"
        );
        for (const li of lis) {
            const id = li.id || "";
            const dash = id.lastIndexOf("-");
            const msgId = dash >= 0 ? id.slice(dash + 1) : id;
            const cached = oslBlankDecisions.get(msgId);
            if (cached === "marked") {
                // Re-apply the marker in case Discord re-mounted the
                // <li> with the original cipher text. oslAutoHideCiphertext
                // is idempotent (checks data-osl-cipher-hidden), so
                // already-marked divs early-return.
                const div = li.querySelector(
                    "[id^='message-content-']"
                );
                if (div) oslAutoHideCiphertext(div);
                continue;
            }
            if (cached === "shown") continue;
            const decision = oslDecideBlank(li);
            if (decision === null) continue;
            oslBlankDecisions.set(msgId, decision);
            if (decision === "marked") {
                const div = li.querySelector(
                    "[id^='message-content-']"
                );
                if (div) oslAutoHideCiphertext(div);
                oslMarkBlankCacheDirty();
            }
        }
    }

    // Invalidate the cached decision for a msg id so the next sweep
    // tick re-evaluates it. Called by recvApplyPlaintext when a
    // plaintext is applied (the row should now be "shown").
    function oslInvalidateBlankDecision(msgId) {
        if (typeof msgId === "string" && msgId.length > 0) {
            if (oslBlankDecisions.delete(msgId)) {
                oslMarkBlankCacheDirty();
            }
        }
    }

    /**
     * 9-C1-FIX1: locate the channel header across DM / GC / server-channel
     * contexts. The pre-FIX1 selector
     * `section[class*="title_"][class*="container__"]` had a double-
     * underscore matcher on `container__` that only matched some
     * Discord builds; on builds using single-underscore class hashes
     * (`container_<hash>`) the selector returned null in GC + server
     * channel headers and the icon row was never installed.
     *
     * New strategy: try the strictest selector first (preserve any
     * lucky-match behaviour from older builds), then progressively
     * broaden. Each candidate is validated by checking it actually
     * contains *some* icon-row affordance via
     * `oslFindHeaderIconContainer`.
     */
    function oslFindChannelHeader() {
        // W3: delegate to the central resolver. The "channelHeader"
        // registry entry leads with the exact selector sequence that
        // lived here (behavior-preserving) and appends a resilient
        // labelled-region fallback.
        return oslAnchorResolve("channelHeader");
    }

    /**
     * 9-C1-FIX1: locate the icon-row container inside a channel
     * header. Discord uses several class shapes across DM / GC /
     * server contexts:
     *   - DM 1-on-1:    `[class*="iconWrapper_"]` per icon
     *   - GC:           same
     *   - Server text:  `[class*="toolbar_"]` wrapping the icon row,
     *                   `iconWrapper_*` per child icon
     *
     * We try iconWrapper-style anchors first (pre-FIX1 behaviour for
     * back-compat), then `toolbar_*`, then fall back to "any element
     * inside header that has ≥2 button-like children."
     */
    function oslFindHeaderIconContainer(header) {
        if (!header) return null;

        // 9-C1-FIX2: prefer `toolbar_*` first. Discord uses
        // `toolbar_<hash>` as the wrapping element for the right-hand
        // icon row across DM, GC, AND server-channel headers.
        // Pre-FIX2 the resolver took the parent of the FIRST
        // `iconWrapper_*` in document order; in server channels the
        // first match is often the leftmost channel-mute / topic icon
        // whose parent is a left-side container, not the right-hand
        // icon row. Toolbar-first lands every injection in the same
        // visual spot regardless of which context we're in.
        const toolbar = header.querySelector('[class*="toolbar_"]');
        if (toolbar) return toolbar;

        // Fallback chain: iconWrapper parent. Try double-underscore
        // first (older builds), then single-underscore. The "use
        // LAST iconWrapper" variant guards against the same wrong-
        // sibling-container problem the toolbar branch was added to
        // fix, but is only reached when no toolbar exists.
        const anchorSelectors = [
            '[class*="iconWrapper__"]',
            '[class*="iconWrapper_"]',
        ];
        for (const sel of anchorSelectors) {
            const matches = header.querySelectorAll(sel);
            if (matches.length === 0) continue;
            const last = matches[matches.length - 1];
            if (last && last.parentElement) {
                return last.parentElement;
            }
        }

        // Final fallback: scan the header subtree for a container
        // holding ≥2 button-like children. Avoids hardcoding Discord
        // class names entirely.
        const nodes = header.querySelectorAll("div, nav, section");
        for (const n of nodes) {
            let buttonish = 0;
            for (const c of n.children) {
                if (
                    (c.tagName === "DIV" || c.tagName === "BUTTON") &&
                    (c.getAttribute("role") === "button" ||
                        c.getAttribute("aria-label") ||
                        (c.className &&
                            typeof c.className === "string" &&
                            c.className.indexOf("icon") !== -1))
                ) {
                    buttonish++;
                    if (buttonish >= 2) return n;
                }
            }
        }
        return null;
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
        // Wipe any client-side caches that live OUTSIDE the OSL
        // config dir (Rust-side wipes already cleared identity.json,
        // peer_map, channels, whitelist, sqlite store). localStorage
        // lives in the WebView2 user-data dir, so the Rust burn
        // didn't touch it — purge here.
        oslPurgeBlankCache();
        // Phase 4b: set the decommission flag so the post-burn page
        // load exits at the very top of boot.js (sync check). The
        // Rust side already wrote decommissioned.flag during
        // osl_burn_engage as the durable source of truth; this is
        // the fast-path. Different localStorage key than
        // oslPurgeBlankCache's target so the line above doesn't
        // wipe it.
        try {
            localStorage.setItem("__OSL_DECOMMISSIONED__", "1");
        } catch (_) {}
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
        if (!header) {
            console.log(
                "[OSL] account burn: no header container in context=" +
                    oslDetectChannelContext()
            );
            return;
        }
        const container = oslFindHeaderIconContainer(header);
        if (!container) {
            console.log(
                "[OSL] account burn: no icon-row container in context=" +
                    oslDetectChannelContext()
            );
            return;
        }
        // Idempotent: bail if the account burn already exists.
        if (
            container.querySelector(
                "[" + HEADER_ACCOUNT_BURN_DATA_ATTR + "='1']"
            )
        ) {
            return;
        }
        // 9-C1-FIX1: broadened sample anchor — match the same
        // single-or-double-underscore variants the header detector
        // accepts, so the button inherits Discord's per-icon styling
        // on every build.
        const sample =
            header.querySelector('[class*="iconWrapper__"]') ||
            header.querySelector('[class*="iconWrapper_"]');
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
        console.log(
            "[OSL] account burn button injected: context=" +
                oslDetectChannelContext()
        );
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
        // 9-A1c: toggling encryption ON in a burned scope is the
        // explicit manual re-engage path. Unburn synchronously so
        // the next inbound DPC0 is decrypted instead of skipped, and
        // persist via osl_unburn_scope (idempotent on both sides).
        if (!currentOn) {
            const scopeKey = oslBurnedScopesKey(scope.kind, scope.id);
            const wasBurned =
                window.__oslBurnedScopes &&
                window.__oslBurnedScopes.has(scopeKey);
            if (wasBurned) {
                console.log(
                    "[OSL] manual unburn: scope=" + scopeKey + " via UI toggle"
                );
                oslBurnedScopesRemove(scope.kind, scope.id);
                try {
                    oslInvoke("osl_unburn_scope", {
                        scopeKind: scope.kind,
                        scopeId: scope.id,
                    }).catch(function (err) {
                        console.error(
                            "[OSL] manual unburn: osl_unburn_scope failed",
                            err
                        );
                    });
                } catch (err) {
                    console.error(
                        "[OSL] manual unburn: invoke threw",
                        err
                    );
                }
            }
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

    /**
     * 9-C1-FIX1: cheap classifier for log lines so we can tell which
     * header context the injector is firing in (helps debug Discord
     * class renames later).
     */
    function oslDetectChannelContext() {
        try {
            const path = (window.location && window.location.pathname) || "";
            const parts = path.split("/").filter((x) => x);
            // /channels/@me/<id>      → dm or gc (disambiguated below)
            // /channels/<guildId>/<id> → server
            if (parts[0] === "channels") {
                if (parts[1] === "@me") {
                    const ctx = oslCurrentChannelContext();
                    if (ctx && ctx.channelType === 3) return "gc";
                    return "dm";
                }
                if (parts.length >= 3) return "server";
            }
        } catch (_) {}
        return "unknown";
    }

    function oslHeaderInjectButtons(header) {
        if (!header) {
            console.log(
                "[OSL] lock icon: no header container found in context=" +
                    oslDetectChannelContext()
            );
            return;
        }
        const container = oslFindHeaderIconContainer(header);
        if (!container) {
            console.log(
                "[OSL] lock icon: no icon-row container found in context=" +
                    oslDetectChannelContext()
            );
            return;
        }
        // Sample className: try the same-broadened anchor selectors
        // we use in oslFindHeaderIconContainer so the new buttons
        // inherit Discord's per-icon styling regardless of build.
        const sample =
            header.querySelector('[class*="iconWrapper__"]') ||
            header.querySelector('[class*="iconWrapper_"]');
        const sampleClass = sample ? sample.className : "";

        // Don't double-inject; if the buttons exist already we just
        // refresh their state.
        let encryptBtn = container.querySelector(
            "[" + HEADER_ENCRYPT_DATA_ATTR + "='1']"
        );
        let burnBtn = container.querySelector(
            "[" + HEADER_BURN_DATA_ATTR + "='1']"
        );

        if (encryptBtn && burnBtn) {
            console.log(
                "[OSL] lock icon already present: skipping context=" +
                    oslDetectChannelContext()
            );
        }

        let injectedAny = false;
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
            // 9-C1-FIX2: paint a default "unknown" glyph immediately
            // so the icon is visible BEFORE the first summary fetch
            // resolves (and stays visible if the fetch fails). The
            // refresh path overwrites this once the summary lands.
            encryptBtn.style.color = "var(--text-muted, #87898c)";
            encryptBtn.innerHTML = oslLockSvg("unknown");
            encryptBtn.title = "Whitelist state loading…";
            encryptBtn.addEventListener("click", oslOnWhitelistIconClick);
            // Press-and-hold resets a SERVER lock to grey (nobody). A
            // plain click cycles yellow/green; only a hold returns to
            // grey. No-op for DM/GC (their handlers are unchanged).
            let __oslHoldTimer = null;
            let __oslHoldFired = false;
            const __oslClearHold = function () {
                if (__oslHoldTimer) {
                    nativeClearTimeout(__oslHoldTimer);
                    __oslHoldTimer = null;
                }
            };
            encryptBtn.addEventListener("pointerdown", function () {
                __oslHoldFired = false;
                __oslClearHold();
                __oslHoldTimer = nativeSetTimeout(function () {
                    __oslHoldFired = true;
                    try {
                        oslServerLockSetGrey();
                    } catch (_) {}
                }, 550);
            });
            encryptBtn.addEventListener("pointerup", __oslClearHold);
            encryptBtn.addEventListener("pointerleave", __oslClearHold);
            // Capture-phase guard: swallow the click that fires after a
            // hold so the cycle handler doesn't also run.
            encryptBtn.addEventListener(
                "click",
                function (e) {
                    if (__oslHoldFired) {
                        __oslHoldFired = false;
                        e.preventDefault();
                        e.stopImmediatePropagation();
                    }
                },
                true
            );
            container.insertBefore(encryptBtn, container.firstChild);
            injectedAny = true;
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
            injectedAny = true;
        }

        if (injectedAny) {
            const ctx = oslCurrentChannelContext();
            console.log(
                "[OSL] lock icon injected: context=" +
                    oslDetectChannelContext() +
                    " channel_id=" +
                    ((ctx && ctx.channelId) || "?")
            );
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
    // True when the most recent header render for the cached scope key
    // resolved to "unknown" (state fetch failed/incomplete). Lets the
    // throttle retry instead of latching a grey lock forever.
    let oslHeaderStateLastWasUnknown = false;
    // Rate-limits the unknown-retry bypass so it can't spam IPCs during
    // the send-time header-observer storm.
    let oslHeaderStateLastUnknownRetryAt = 0;
    // 9-C1 Stage 4: cache the most recent summary so the click
    // handler doesn't have to round-trip again.
    let oslHeaderLastSummary = null;

    /**
     * THE canonical channel-member / peer resolver. Every whitelist
     * entry point (header control, bulk path, DM-sidebar profile
     * button, scope derivation) routes through this so they can
     * never again disagree on the element shape.
     *
     * CONTRACT (docs/phase-7c-selectors.md:414): in current Discord
     * builds `memoizedProps.channel.recipients` — the source of
     * `ctx.members` — is an array of Discord snowflake STRINGS.
     * Older/other shapes (user objects `{id}` / `{user_id}`) are
     * normalized defensively here using the EXACT pattern already
     * proven in the encrypt/attachment send paths, so there is one
     * normalization, used everywhere.
     *
     * Returns: snowflake `string[]` (possibly empty = roster unknown
     * → tri-state renders "unknown"). For server channels with no
     * fiber recipients, falls back to the gateway-tap per-channel
     * cache (already snowflake strings).
     */
    function oslCtxMemberIds(ctx) {
        if (!ctx) return [];
        if (Array.isArray(ctx.members) && ctx.members.length > 0) {
            return ctx.members
                .map(function (m) {
                    return typeof m === "string"
                        ? m
                        : (m && (m.id || m.user_id)) || null;
                })
                .filter(function (s) {
                    return typeof s === "string" && s.length > 0;
                });
        }
        try {
            const cache = window.__OSL_CHANNEL_MEMBERS__;
            if (cache && ctx.channelId && cache.get) {
                const arr = cache.get(ctx.channelId);
                if (Array.isArray(arr)) return arr.slice();
            }
        } catch (_) {}
        return [];
    }

    function oslHeaderChannelMembers(ctx) {
        // Single source of truth — see oslCtxMemberIds. (Previously
        // this did `m.id` over what is actually a string array, so
        // it returned [] for every DM/GC → the header control
        // silently no-opped. That object-shaped read is deleted.)
        return oslCtxMemberIds(ctx);
    }

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
        // Throttle on scope key UNLESS the last render for this scope
        // landed on "unknown" (a failed/incomplete state fetch). Without
        // the unknown bypass, a single failed fetch cached the key and
        // the lock stayed grey ("blue"/unknown) forever until a channel
        // switch — which is exactly the "lock does not change" report.
        //
        // The unknown bypass is itself rate-limited to once per 1.5s:
        // the header MutationObserver fires hundreds of times/sec during
        // a send, and an unbounded bypass would spam osl_get_server_
        // whitelist_state IPCs and reintroduce the "multi-second slow
        // send" the scope-key throttle exists to prevent.
        if (!force && key === oslHeaderStateLastScopeKey) {
            if (!oslHeaderStateLastWasUnknown) return;
            const _now = Date.now();
            if (_now - oslHeaderStateLastUnknownRetryAt < 1500) return;
            oslHeaderStateLastUnknownRetryAt = _now;
        }
        oslHeaderStateLastScopeKey = key;
        // Assume unknown until a branch below renders a definitive
        // state; that branch flips this back to false.
        oslHeaderStateLastWasUnknown = true;

        // W2b: server text channels use the scope-flag model, NOT the
        // roster-summary path (Discord never loads a server roster —
        // that was the "Channel roster not loaded" dead-end). Render
        // from the server-header / channel flags instead. DM / GC fall
        // through to the unchanged summary path below.
        if (scopeInput.kind === "server_channel") {
            const sw = await oslInvoke("osl_get_server_whitelist_state", {
                serverId: scopeInput.server_id,
                channelScopeInput: scopeInput,
            });
            if (!sw.ok) {
                encryptBtn.innerHTML = oslLockSvg("unknown");
                encryptBtn.style.color = "var(--text-muted, #87898c)";
                encryptBtn.setAttribute(
                    "aria-label",
                    "OSL whitelist: unknown (server state fetch failed)"
                );
                encryptBtn.title =
                    "Server whitelist state unknown: " + (sw.error || "?");
                oslHeaderStateLastScopeKey = null;
                return;
            }
            const st = sw.value || {};
            // Server-lock TRI-STATE (the header button now reflects the
            // SERVER tier only; per-channel lives on the sidebar lock):
            //   GREEN  (server_header) = every OSL member of the server
            //   YELLOW (server_dm)     = your DM-whitelisted peers here
            //   GREY                   = nobody (self-only)
            let lockState, color, label, aria;
            if (st.server_header) {
                lockState = "closed";
                color = "var(--status-positive, #23a559)";
                aria = "green";
                label =
                    "Server lock: GREEN — every OSL member of this server " +
                    "can read your messages. Click → yellow. Press and hold " +
                    "→ grey (nobody).";
            } else if (st.server_dm) {
                lockState = "partial";
                color = "#f0b132";
                aria = "yellow";
                label =
                    "Server lock: YELLOW — only your DM-whitelisted peers " +
                    "who are in this server can read. Click → green (all OSL " +
                    "members). Press and hold → grey (nobody).";
            } else {
                lockState = "open";
                color = "var(--text-muted, #87898c)";
                aria = "grey";
                label =
                    "Server lock: GREY — nobody in this server can read your " +
                    "messages. Click → yellow (your DM peers).";
            }
            encryptBtn.innerHTML = oslLockSvg(lockState);
            // Definitive state rendered — clear the unknown flag so the
            // throttle can cache this scope.
            oslHeaderStateLastWasUnknown = false;
            encryptBtn.setAttribute("aria-label", "OSL server lock: " + aria);
            encryptBtn.style.opacity = "1";
            encryptBtn.style.pointerEvents = "auto";
            encryptBtn.style.color = color;
            encryptBtn.title = label;
            if (burnBtn) {
                burnBtn.title =
                    "Burn your messages in " + oslScopeLabel(scopeInput);
            }
            return;
        }

        // GC follow-up: GCs use the same scope-flag + dynamic-
        // membership model as server channels (the GC header
        // whitelists all OSL members, current + future). Reuse the
        // server-state command with an empty serverId so server_header
        // is false and `channel` carries the gc:<id> flag. Legacy
        // per-peer GC whitelists still decrypt (handled in Rust); this
        // only changes what the header button controls. DM still falls
        // through to the unchanged summary path below.
        if (scopeInput.kind === "gc") {
            const gw = await oslInvoke("osl_get_server_whitelist_state", {
                serverId: "",
                channelScopeInput: scopeInput,
            });
            if (!gw.ok) {
                encryptBtn.innerHTML = oslLockSvg("unknown");
                encryptBtn.style.color = "var(--text-muted, #87898c)";
                encryptBtn.setAttribute(
                    "aria-label",
                    "OSL whitelist: unknown (gc state fetch failed)"
                );
                encryptBtn.title =
                    "GC whitelist state unknown: " + (gw.error || "?");
                oslHeaderStateLastScopeKey = null;
                return;
            }
            const on = !!(gw.value && gw.value.channel);
            encryptBtn.innerHTML = oslLockSvg(on ? "closed" : "open");
            oslHeaderStateLastWasUnknown = false;
            encryptBtn.setAttribute(
                "aria-label",
                "OSL GC whitelist: " + (on ? "on" : "off")
            );
            encryptBtn.style.opacity = "1";
            encryptBtn.style.pointerEvents = "auto";
            encryptBtn.style.color = on
                ? "var(--status-positive, #23a559)"
                : "var(--text-muted, #87898c)";
            encryptBtn.title = on
                ? "GC whitelist ON — encrypting to every OSL member of " +
                  "this group chat (current + future). Click to turn OFF."
                : "Not whitelisted. Click to whitelist this group chat " +
                  "(all OSL members, current + future).";
            if (burnBtn) {
                burnBtn.title =
                    "Burn your messages in " + oslScopeLabel(scopeInput);
            }
            return;
        }

        const selfId = await oslSelfDiscordId();
        const members = oslHeaderChannelMembers(ctx);
        const result = await oslInvoke("osl_get_scope_whitelist_summary", {
            scopeInput: scopeInput,
            channelMembers: members,
            selfDiscordId: selfId || "",
        });
        if (!result.ok) {
            console.log(
                "[OSL] header summary refresh failed: " + result.error
            );
            // 9-C1-FIX2: paint a muted "unknown" lock so the user
            // sees SOMETHING even when the summary command isn't
            // available (capability gap, transient error, etc.). Do
            // NOT bail with the button blank — that was the FIX1-era
            // failure mode the live console traced.
            encryptBtn.innerHTML = oslLockSvg("unknown");
            encryptBtn.style.color = "var(--text-muted, #87898c)";
            encryptBtn.setAttribute(
                "aria-label",
                "OSL whitelist: unknown (summary fetch failed)"
            );
            encryptBtn.title =
                "Whitelist state unknown — " +
                "summary fetch failed: " +
                (result.error || "?");
            // Allow next refresh attempt to retry — clear the cache
            // sentinel so a follow-up channel switch (or force refresh)
            // re-runs.
            oslHeaderStateLastScopeKey = null;
            return;
        }
        const summary = result.value;
        oslHeaderLastSummary = {
            scopeInput: scopeInput,
            members: members,
            selfId: selfId,
            summary: summary,
        };
        oslHeaderState.scopeKey = key;
        oslHeaderState.encryptToggle = !!summary.encrypt_toggle;
        oslHeaderState.hasWhitelist = summary.state !== "none";

        // 9-C1 Stage 4: tri-state icon.
        //   "all"     → closed/green
        //   "some"    → partial/yellow
        //   "none"    → open/gray
        //   "unknown" → question-marked lock, muted
        let lockState;
        let color;
        let label;
        switch (summary.state) {
            case "all":
                lockState = "closed";
                color = "var(--status-positive, #23a559)";
                label = "Encrypting with everyone in " + oslScopeLabel(scopeInput);
                break;
            case "some":
                lockState = "partial";
                color = "#f0b132";
                label =
                    "Encrypting with " +
                    summary.whitelisted_count +
                    "/" +
                    summary.total_members +
                    " in " +
                    oslScopeLabel(scopeInput);
                break;
            case "none":
                lockState = "open";
                color = "var(--text-muted, #87898c)";
                label = "No one whitelisted in " + oslScopeLabel(scopeInput);
                break;
            case "unknown":
            default:
                lockState = "unknown";
                color = "var(--text-muted, #87898c)";
                label =
                    "Channel roster not loaded yet — open & scroll the " +
                    "member list, then click to refresh";
                break;
        }
        encryptBtn.innerHTML = oslLockSvg(lockState);
        // Clear the unknown-retry flag only on a definitive state; a
        // genuine "unknown" (roster not loaded) keeps retrying so the
        // lock self-heals once the roster lands.
        oslHeaderStateLastWasUnknown = lockState === "unknown";
        encryptBtn.setAttribute("aria-label", "OSL whitelist: " + summary.state);
        encryptBtn.style.opacity = "1";
        encryptBtn.style.pointerEvents = "auto";
        encryptBtn.style.color = color;
        encryptBtn.title = label;
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
            // DM: peer is the (single) recipient — resolved through
            // the ONE canonical resolver so the shape contract is
            // identical to the header / sidebar paths.
            const ids = oslCtxMemberIds(ctx);
            const peer = ids.length > 0 ? ids[0] : null;
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

    // 9-C1 Stage 4: tri-state header lock click. The action depends
    // on the current `summary.state`:
    //   "none"    → bulk-set-whitelist for every non-self channel member
    //   "all"     → bulk-unwhitelist every non-self channel member
    //   "some"    → ask the user (modal): promote or demote?
    //   "unknown" → refresh the cache + nudge the user to open the
    //               member list to populate the roster
    //
    // When the action would affect more than 25 peers we surface a
    // confirm modal first — a single click that whitelists 80+
    // members would be impossible to undo by memory alone.
    const BULK_CONFIRM_THRESHOLD = 25;

    // Press-and-hold target for the server header lock: reset to GREY
    // (nobody in the server can read). Server channels only.
    async function oslServerLockSetGrey() {
        const ctx = oslCurrentChannelContext();
        const scopeInput = oslScopeForCurrentContext(ctx);
        if (!scopeInput || scopeInput.kind !== "server_channel") return;
        const r = await oslInvoke("osl_set_server_lock", {
            serverId: scopeInput.server_id,
            lockState: "grey",
        });
        if (!r.ok) {
            oslToast("OSL: " + r.error);
            return;
        }
        oslToast("Server lock GREY — nobody in this server can read your messages.");
        try {
            await oslRefreshHeaderState({ force: true });
        } catch (_) {}
        try {
            oslChanWlRefreshAll();
        } catch (_) {}
    }

    async function oslOnWhitelistIconClick(e) {
        e.preventDefault();
        e.stopPropagation();
        const ctx = oslCurrentChannelContext();
        const scopeInput = oslScopeForCurrentContext(ctx);
        if (!scopeInput) {
            oslToast("OSL: cannot determine current scope");
            return;
        }

        // W2b: server text channel — the header button toggles the
        // SERVER-WIDE whitelist (all OSL members, current + future).
        // No roster needed; recipients resolve dynamically from the
        // accrued membership. Turning ON also enables encryption for
        // this channel now (decision #2). Returns before the DM/GC
        // roster flow, which is left entirely unchanged.
        if (scopeInput.kind === "server_channel") {
            // REWORK: the header button is now the SERVER-LOCK tri-state
            // (grey/yellow/green). A plain CLICK cycles grey→yellow→
            // green→yellow; it never returns to grey (press-and-hold
            // does that — see oslServerLockSetGrey, wired at the button).
            //   yellow = DM-whitelisted peers who are in this server
            //   green  = every OSL member of this server
            // Per-channel "everyone in this channel" lives on the
            // sidebar lock now, not here.
            const cur = await oslInvoke("osl_get_server_whitelist_state", {
                serverId: scopeInput.server_id,
                channelScopeInput: scopeInput,
            });
            if (!cur.ok) {
                oslToast("OSL: " + cur.error);
                return;
            }
            const green = !!(cur.value && cur.value.server_header);
            const yellow = !!(cur.value && cur.value.server_dm);
            const target = green ? "yellow" : yellow ? "green" : "yellow";
            const r = await oslInvoke("osl_set_server_lock", {
                serverId: scopeInput.server_id,
                lockState: target,
            });
            if (!r.ok) {
                oslToast("OSL: " + r.error);
                try {
                    await oslRefreshHeaderState({ force: true });
                } catch (_) {}
                return;
            }
            // Picking a sharing tier turns encryption on for this channel.
            await oslInvoke("osl_set_scope_encrypt", {
                scopeInput: scopeInput,
                enabled: true,
            });
            oslToast(
                target === "green"
                    ? "Server lock GREEN — every OSL member of this server can read."
                    : "Server lock YELLOW — your DM-whitelisted peers in this server can read."
            );
            try {
                await oslRefreshHeaderState({ force: true });
            } catch (_) {}
            try {
                oslChanWlRefreshAll();
            } catch (_) {}
            return;
        }

        // GC follow-up: the GC header toggles the GC-wide whitelist
        // flag (all OSL members, current + future) — same scope-flag
        // model as a server channel. osl_set_channel_whitelist now
        // accepts a gc scope and flips encrypt_toggle when ON.
        // Returns before the legacy per-peer GC roster flow (kept for
        // DM); legacy per-peer GC whitelists still decrypt in Rust.
        if (scopeInput.kind === "gc") {
            const cur = await oslInvoke("osl_get_server_whitelist_state", {
                serverId: "",
                channelScopeInput: scopeInput,
            });
            if (!cur.ok) {
                oslToast("OSL: " + cur.error);
                return;
            }
            const wasOn = !!(cur.value && cur.value.channel);
            const next = !wasOn;
            const setRes = await oslInvoke("osl_set_channel_whitelist", {
                scopeInput: scopeInput,
                on: next,
            });
            if (!setRes.ok) {
                oslToast("OSL: " + setRes.error);
                return;
            }
            oslToast(
                next
                    ? "GC whitelist ON — encrypting to all OSL members of " +
                          "this group chat."
                    : "GC whitelist OFF."
            );
            try {
                await oslRefreshHeaderState({ force: true });
            } catch (_) {}
            return;
        }

        const selfId = await oslSelfDiscordId();
        const members = oslHeaderChannelMembers(ctx).filter(
            (m) => m !== selfId
        );

        // Re-poll the summary right before acting — the user could
        // have changed whitelists elsewhere since the last refresh.
        const sumRes = await oslInvoke("osl_get_scope_whitelist_summary", {
            scopeInput: scopeInput,
            channelMembers: members.concat(selfId ? [selfId] : []),
            selfDiscordId: selfId || "",
        });
        if (!sumRes.ok) {
            oslToast("OSL: " + sumRes.error);
            return;
        }
        const summary = sumRes.value;

        if (summary.state === "unknown" || members.length === 0) {
            oslToast(
                "Channel roster not loaded yet — open the member list " +
                    "and scroll it so Discord sends the full roster, " +
                    "then click again."
            );
            return;
        }

        const scopeLabel = oslScopeLabel(scopeInput);
        let actionKind; // "set" | "unset"
        if (summary.state === "none") {
            actionKind = "set";
        } else if (summary.state === "all") {
            actionKind = "unset";
        } else {
            // "some" — ask which way to go.
            const promote = await oslConfirm({
                title: "Encrypt with everyone in " + scopeLabel + "?",
                body:
                    summary.whitelisted_count +
                    " of " +
                    summary.total_members +
                    " are whitelisted. Click Confirm to add the remaining " +
                    (summary.total_members - summary.whitelisted_count) +
                    " peers; click Cancel to stop encrypting with the " +
                    summary.whitelisted_count +
                    " who are.",
                confirmText: "Add the rest",
                cancelText: "Stop with all",
            });
            actionKind = promote ? "set" : "unset";
        }

        if (members.length > BULK_CONFIRM_THRESHOLD) {
            const verb = actionKind === "set" ? "whitelist" : "remove from whitelist";
            const ok = await oslConfirm({
                title: verb + " " + members.length + " peers?",
                body:
                    "This will " +
                    verb +
                    " " +
                    members.length +
                    " people at once in " +
                    scopeLabel +
                    ". The change applies locally only — peers aren't " +
                    "notified beyond the normal scope-burn signal.",
                confirmText: "Confirm",
                cancelText: "Cancel",
            });
            if (!ok) return;
        }

        const cmd =
            actionKind === "set"
                ? "osl_bulk_set_whitelist"
                : "osl_bulk_unwhitelist_scope";
        const res = await oslInvoke(cmd, {
            scopeInput: scopeInput,
            memberDids: members,
        });
        if (!res.ok) {
            oslToast("OSL: bulk action failed: " + res.error);
            return;
        }
        oslToast(
            (actionKind === "set" ? "Whitelisted " : "Removed ") +
                res.value +
                " peer" +
                (res.value === 1 ? "" : "s") +
                " in " +
                scopeLabel
        );
        oslRefreshHeaderState({ force: true });
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
            // Phase 6.4: burn markers ride the keyserver control-
            // inbox; recipients = ctx.members minus self.
            const burnRecipients = (Array.isArray(ctx.members) ? ctx.members : [])
                .filter(function (m) {
                    return typeof m === "string" && m !== selfId;
                });
            await oslSendControlOob(
                burnRecipients,
                scopeInput,
                sendResult.value
            );
        } else if (sendResult.error !== "no_whitelisted_recipients") {
            // Real error — burn marker not shipped. Still proceed
            // with local apply so the user's own state is wiped.
            console.log(
                "[OSL] burn marker send failed: " + sendResult.error
            );
        }
        // Phase 4: delete every cipher-store blob this client
        // recorded under the scope BEFORE the local wipe. After this
        // the covers are unrecoverable for anyone (other peers,
        // forensic backup, the burner themselves). Best-effort —
        // single-blob failures don't block the local burn since the
        // worst case is "blob lingers until 72h TTL".
        try {
            const burnBlobsRes = await oslInvoke("osl_scope_burn_blobs", {
                scopeInput: scopeInput,
            });
            if (burnBlobsRes && burnBlobsRes.ok && burnBlobsRes.value) {
                console.log(
                    "[OSL] scope_burn_blobs deleted=" +
                        burnBlobsRes.value.deleted +
                        " failed=" +
                        burnBlobsRes.value.failed
                );
            } else if (burnBlobsRes && !burnBlobsRes.ok) {
                console.warn(
                    "[OSL] scope_burn_blobs failed: " + burnBlobsRes.error
                );
            }
        } catch (e) {
            console.warn("[OSL] scope_burn_blobs threw:", e);
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
        // 9-A1c: collect message IDs visible in the channel so the
        // server can record them in the burn kill list. The decrypt
        // entry-points (cmd_osl_decrypt_message_v2 /
        // cmd_osl_open_attachment_v2) refuse to surface plaintext
        // for any message_id in this list — defense-in-depth for
        // the case where the scope-level skip cache is cleared by a
        // later manual re-engage.
        const burnedMessageIds = [];
        try {
            const burnChannelId =
                scopeInput.channel_id || ctx.channelId || null;
            if (burnChannelId) {
                const items = document.querySelectorAll(
                    'li[id^="chat-messages-' + burnChannelId + '-"]'
                );
                const re = /chat-messages-\d{15,22}-(\d{15,22})/;
                items.forEach(function (li) {
                    const m = re.exec(li.id);
                    if (m && m[1]) burnedMessageIds.push(m[1]);
                });
            }
        } catch (e) {
            console.log(
                "[OSL][burn] collecting kill list message IDs failed: " + e
            );
        }
        const markResult = await oslInvoke("osl_mark_scope_burned", {
            scopeKind: scopeInput.kind,
            scopeId: scopeInput.id,
            serverId: scopeInput.server_id || null,
            channelId: scopeInput.channel_id || ctx.channelId || null,
            burnedMessageIds: burnedMessageIds,
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

    // ============================================================
    // Phase 9-B4: Global keybinds.
    //
    //   Ctrl+Shift+O           → open settings window
    //   Ctrl+Alt+E             → flip the composer encrypt pill
    //   Ctrl+Alt+B             → burn the current scope (confirms)
    //   Ctrl+Shift+Backspace   → account-burn chord (arm/execute,
    //                            reuses oslAccountBurnOnActivate)
    //
    // W5: encrypt/burn are Ctrl+Alt chords (was bare E/B, which
    // required a fragile text-entry bypass and fired mid-typing).
    // All OSL keybinds are now modifier chords: none collide with
    // Discord's editor (Ctrl+B bold / Ctrl+E) and all are safe to
    // fire even while the composer is focused, so the text-entry
    // gate no longer guards any OSL accelerator.
    //
    // Dispatch gates (in this order; any tripping gate exits):
    //   a. an OSL modal is up (any element with [data-osl-modal='1']) →
    //      don't compete with the modal's own input handling
    //   b. the text-entry gate (focused INPUT / TEXTAREA /
    //      contenteditable) now only affects future bare-key binds, if
    //      any are ever re-added; the chord binds run before it.
    // Key matching is case-insensitive (capslock-E == e).
    // ============================================================

    function oslKeybindActiveIsTextEntry() {
        const el = document.activeElement;
        if (!el) return false;
        const tag = el.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA") return true;
        // contenteditable can be inherited from any ancestor. Walk up.
        let node = el;
        while (node && node.nodeType === 1) {
            const ce = node.getAttribute && node.getAttribute("contenteditable");
            if (ce === "true" || ce === "") return true;
            node = node.parentElement;
        }
        return false;
    }

    function oslKeybindAnyModalOpen() {
        return !!document.querySelector("[data-osl-modal='1']");
    }

    async function oslKeybindOpenSettings() {
        console.log("[OSL] keybind: open settings");
        const result = await oslInvoke("osl_open_settings_window", {});
        if (!result.ok) {
            console.error(
                "[OSL] keybind: open settings failed: " + result.error
            );
            oslToast("Failed to open settings: " + result.error);
        }
    }

    async function oslKeybindEncryptToggle() {
        const btn = document.querySelector(
            "[" + COMPOSER_TOGGLE_DATA_ATTR + "='1']"
        );
        if (!btn) {
            console.log("[OSL] keybind: encrypt toggle no-op (no pill mounted)");
            return;
        }
        await oslComposerToggleOnClick(btn);
        // After the async call, data-osl-encrypt-state reflects new state.
        const state = btn.getAttribute("data-osl-encrypt-state") || "?";
        console.log("[OSL] keybind: encrypt toggle (state=" + state + ")");
    }

    async function oslKeybindBurnScope() {
        const ctx = oslCurrentChannelContext();
        const scopeInput = ctx ? oslScopeForCurrentContext(ctx) : null;
        if (!scopeInput) {
            console.log("[OSL] keybind: burn no-op (no scope)");
            return;
        }
        const scopeKey = scopeInput.kind + ":" + scopeInput.id;
        console.log("[OSL] keybind: burn scope=" + scopeKey);
        // Synthesize an event-shaped arg for oslOnBurnClick — it only
        // touches preventDefault/stopPropagation.
        oslOnBurnClick({
            preventDefault: function () {},
            stopPropagation: function () {},
        });
    }

    function oslKeybindAccountBurnChord() {
        const btn = document.querySelector(
            "[" + HEADER_ACCOUNT_BURN_DATA_ATTR + "='1']"
        );
        if (!btn) {
            console.log("[OSL] keybind: account burn no-op (no icon mounted)");
            return;
        }
        const wasArmed = oslAccountBurnArmed;
        oslAccountBurnOnActivate(btn);
        console.log(
            "[OSL] keybind: account burn chord (armed=" + (!wasArmed) + ")"
        );
    }

    function oslGlobalKeydownDispatcher(event) {
        // Modal gate runs uniformly: if an OSL modal is up, no keybind
        // fires (the user can dismiss the modal first).
        if (oslKeybindAnyModalOpen()) return;

        const k = event.key;
        const kLower = typeof k === "string" ? k.toLowerCase() : "";
        const ctrlShift = event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey;
        // W5: encrypt/burn moved off bare E/B (which fired mid-typing
        // via the text-entry bypass) to Ctrl+Alt+E / Ctrl+Alt+B —
        // intentional chords that don't collide with Discord's Ctrl+B
        // (bold) / Ctrl+E and are safe to fire even in the composer.
        const ctrlAlt = event.ctrlKey && event.altKey && !event.shiftKey && !event.metaKey;
        // W5: every OSL keybind is now a modifier chord (Ctrl+Shift+O,
        // Ctrl+Alt+E, Ctrl+Alt+B, Ctrl+Shift+Backspace) — all
        // intentional accelerators that fire even while the composer
        // is focused, and none claimed by Discord's text fields. The
        // text-entry gate is computed but no longer guards any active
        // bind (kept for any future bare-key bind).
        const inTextEntry = oslKeybindActiveIsTextEntry();

        if (ctrlShift && kLower === "o") {
            event.preventDefault();
            oslKeybindOpenSettings();
            return;
        }
        if (ctrlShift && k === "Backspace") {
            event.preventDefault();
            oslKeybindAccountBurnChord();
            return;
        }
        // W5: Ctrl+Alt+E / Ctrl+Alt+B — chords, fire even in the
        // composer (intentional accelerator), not gated by text entry.
        if (ctrlAlt && kLower === "e") {
            event.preventDefault();
            oslKeybindEncryptToggle();
            return;
        }
        if (ctrlAlt && kLower === "b") {
            event.preventDefault();
            oslKeybindBurnScope();
            return;
        }
        if (inTextEntry) return;
    }

    function oslInstallKeybinds() {
        if (window.__oslKeybindsInstalled) return;
        window.__oslKeybindsInstalled = true;
        document.addEventListener("keydown", oslGlobalKeydownDispatcher, true);
        console.log("[OSL] keybinds installed");
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
        // Phase 9-A1b FIX10: also wipe decrypted attachment state so
        // burned scopes hide their images, not just their text.
        let injectedRemoved = 0;
        let blobUrlsRevoked = 0;
        // 9-A1c: split aftermath unhide counter into hidden vs swapped.
        // Wrapper-path swaps stamp data-osl-swapped instead of
        // data-osl-hidden; both need to be reverted post-burn so the
        // native broken-video placeholder returns.
        let unhidHidden = 0;
        let unhidSwapped = 0;
        items.forEach(function (li) {
            const div = li.querySelector(
                '[id^="' + RECV_MESSAGE_ID_PREFIX + '"]'
            );
            if (!div) return;
            const messageId = recvMessageIdOf(div);
            loadedHistory.delete(messageId);
            recvPlaintext.delete(messageId);
            recvDone.delete(messageId);
            // Probe-2 Boot Bug 5: aftermath used to leave selfSentPlaintext
            // intact, so on any DOM re-mount the self-view short-circuit
            // re-rendered the sender's own plaintext over a freshly-burned
            // message — contradicting the locked "burn means burned for
            // everyone, no special case for self" policy. Also clear the
            // retry/inflight maps so a re-dispatch doesn't loop on a
            // burned id.
            selfSentPlaintext.delete(messageId);
            recvCovers.delete(messageId);
            recvRetries.delete(messageId);
            recvInFlight.delete(messageId);
            if (typeof recvAuthorRetryCount !== "undefined") {
                recvAuthorRetryCount.delete(messageId);
            }
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

            // 9-A1b FIX10: remove decrypted-attachment injections.
            const injected = li.querySelectorAll('[data-osl-injected="1"]');
            for (const el of injected) {
                const src = el.getAttribute("src");
                if (
                    typeof src === "string" &&
                    src.indexOf("blob:") === 0
                ) {
                    try {
                        URL.revokeObjectURL(src);
                        blobUrlsRevoked++;
                    } catch (_) {}
                }
                try {
                    el.remove();
                    injectedRemoved++;
                } catch (_) {}
            }

            // Un-hide any wrappers we hid OR swapped earlier so
            // Discord's native "broken video" placeholder returns —
            // that's a visible signal to the user that this slot
            // held encrypted attachment data that's now burned.
            const restorable = li.querySelectorAll(
                '[data-osl-hidden="1"], [data-osl-swapped="1"]'
            );
            for (const el of restorable) {
                const wasHidden = el.getAttribute("data-osl-hidden") === "1";
                const wasSwapped = el.getAttribute("data-osl-swapped") === "1";
                try {
                    el.style.display = "";
                    if (wasHidden) {
                        el.removeAttribute("data-osl-hidden");
                        unhidHidden++;
                    }
                    if (wasSwapped) {
                        el.removeAttribute("data-osl-swapped");
                        unhidSwapped++;
                    }
                } catch (_) {}
            }

            // Drop attachment URL cache entry for this message.
            try {
                if (window.__oslAttachmentUrlCache) {
                    window.__oslAttachmentUrlCache.delete(messageId);
                }
            } catch (_) {}

            // Drop decrypted-blob cache + revoke its URLs.
            try {
                if (window.__oslAttachmentDecrypted) {
                    const cached =
                        window.__oslAttachmentDecrypted.get(messageId);
                    if (cached && Array.isArray(cached.blobUrls)) {
                        for (const url of cached.blobUrls) {
                            try {
                                URL.revokeObjectURL(url);
                                blobUrlsRevoked++;
                            } catch (_) {}
                        }
                    }
                    window.__oslAttachmentDecrypted.delete(messageId);
                }
            } catch (_) {}
        });
        console.log(
            "[OSL] burn aftermath: channel=" +
                channelId +
                " items=" +
                items.length +
                " repainted=" +
                repainted +
                " blanked=" +
                blanked +
                " injected_removed=" +
                injectedRemoved +
                " blob_urls_revoked=" +
                blobUrlsRevoked +
                " unhid_hidden=" +
                unhidHidden +
                " unhid_swapped=" +
                unhidSwapped
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
    // Raised from 50: at 50 entries, scrolling an image-heavy channel
    // evicted decrypted blobs fast, so re-entry / scroll-back re-fetched
    // + re-decrypted (and looked like "images don't cache"). 250 keeps a
    // session's worth of images resident; cross-restart disk persistence
    // is a separate follow-up.
    const OSL_ATT_CACHE_CAP = 250;

    // Negative cache: message ids that scanned with NO attachment
    // candidates. The periodic sweep skips these instead of re-walking
    // their DOM subtree every tick — a quiet channel was re-scanning
    // every text message once a second, pegging the main thread and
    // making sends feel sluggish. oslScanLiAttachmentsV2 clears a msg
    // id from this set on entry (so a mutation-observer-triggered
    // re-scan always re-evaluates, catching lazy-rendered media) and
    // re-adds it when the scan finds nothing.
    const oslAttScannedEmpty = new Set();

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
     * 8e-FIX5: build an inline media element for swap injection.
     * Returns a <video controls> for video MIMEs, <img> otherwise.
     * Styling approximates Discord's normal embed image rendering
     * so the swapped element fits the message flow without bespoke
     * CSS. `data-osl-injected="1"` marks the replacement so the
     * idempotency check in swap can skip a re-injection on the
     * next observer pass.
     */
    /**
     * Beta 1.0: build a blob from decrypted bytes, register it in the
     * per-message cache, and swap it into the DOM. Shared by the live
     * CDN-decrypt path and the local-cache-hit path so both render
     * identically. `bytes` is a Uint8Array of the decrypted file.
     */
    function oslInjectAttachmentBytes(li, msgId, cand, bytes, mime, newCache) {
        const blob = new Blob([bytes], { type: mime });
        const blobUrl = URL.createObjectURL(blob);
        newCache.byRandomName[cand.name] = {
            blobUrl: blobUrl,
            mime: mime,
            url: cand.url,
        };
        newCache.blobUrls.push(blobUrl);
        const targets = oslFindAttachmentTargets(li, cand.name);
        if (targets.length === 0) {
            oslSwapAttachmentElement(
                { el: null, url: cand.url, name: cand.name, li: li, msgId: msgId },
                blobUrl,
                mime
            );
        } else {
            targets.forEach(function (t) {
                oslSwapAttachmentElement(t, blobUrl, mime);
            });
        }
        return blobUrl;
    }

    /** Decode a base64 string to a Uint8Array. */
    function oslB64ToBytes(b64) {
        const binary = atob(b64);
        const bytes = new Uint8Array(binary.length);
        for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
        }
        return bytes;
    }

    function oslMakeInlineMedia(blobUrl, mime) {
        if (typeof mime === "string" && mime.indexOf("video/") === 0) {
            const video = document.createElement("video");
            video.setAttribute("src", blobUrl);
            video.setAttribute("controls", "controls");
            video.setAttribute("preload", "metadata");
            video.setAttribute("data-osl-injected", "1");
            video.style.maxWidth = "550px";
            video.style.maxHeight = "350px";
            video.style.borderRadius = "8px";
            return video;
        }
        const img = document.createElement("img");
        img.setAttribute("src", blobUrl);
        img.setAttribute("data-osl-injected", "1");
        img.style.maxWidth = "550px";
        img.style.maxHeight = "350px";
        img.style.borderRadius = "8px";
        img.style.objectFit = "contain";
        return img;
    }

    /**
     * 8e-FIX5: walk up from a URL-carrying element to find the
     * Discord attachment-card wrapper. Discord's classnames are
     * hash-suffixed (`messageAttachment-2L3vk7`, `attachment-3F8sJ`,
     * etc.) so we substring-match the de-suffixed root tokens.
     * Stops at the message <li> boundary so we never hide anything
     * outside the message bubble. Returns null if no wrapper found
     * within 8 ancestor levels — caller falls back to hiding `el`
     * itself.
     */
    function oslFindAttachmentCardWrapper(el) {
        let node = el;
        for (let i = 0; i < 8 && node; i++) {
            if (node.nodeType !== 1) {
                node = node.parentNode;
                continue;
            }
            if (node.tagName === "LI") return null;
            const cls =
                node.className && typeof node.className === "string"
                    ? node.className.toLowerCase()
                    : "";
            if (
                cls.indexOf("attachment") >= 0 ||
                cls.indexOf("messageattachment") >= 0 ||
                cls.indexOf("embed") >= 0 ||
                cls.indexOf("mediaplaceholder") >= 0
            ) {
                return node;
            }
            node = node.parentNode;
        }
        return null;
    }

    /**
     * Inside `li`, find every element whose attribute values carry
     * a CDN URL whose last path segment matches `randomFilename`.
     * Returns targets ordered by preferred swap strategy:
     *   1. <img>  (just swap src)
     *   2. <video> (just swap src)
     *   3. <a>     (replace anchor with inline media)
     *   4. wrapper (any other tag — hide its card ancestor, inject
     *               inline media as a sibling)
     *
     * Phase 8e-FIX5 broadened path 4: Discord's "video failed to
     * load" file card for our zero-sample decoy MP4 carries the
     * CDN URL on a `<div data-*>` or `<span aria-*>` rather than
     * any standard media tag — same pattern the FIX2 scanner walk
     * targets.
     */
    function oslFindAttachmentTargets(li, randomFilename) {
        if (!li || !randomFilename) return [];
        const out = [];
        const seen = new Set();

        const ATTACHMENT_URL_RE =
            /https:\/\/(?:cdn|media)\.discordapp\.(?:com|net)\/attachments\/[^\s"'<>]+/g;

        function matchesLastSegment(url) {
            const path = url.split("?")[0];
            const last = path.split("/").pop() || "";
            return last === randomFilename;
        }

        // Paths 1 + 2: direct media tags.
        li.querySelectorAll("img, video").forEach(function (el) {
            if (seen.has(el)) return;
            const url = el.getAttribute("src");
            if (typeof url !== "string" || !matchesLastSegment(url)) return;
            seen.add(el);
            out.push({
                el: el,
                url: url,
                kind: el.tagName === "VIDEO" ? "video" : "img",
            });
        });

        // Path 3: <a href>.
        li.querySelectorAll("a[href]").forEach(function (el) {
            if (seen.has(el)) return;
            const url = el.getAttribute("href");
            if (typeof url !== "string" || !matchesLastSegment(url)) return;
            seen.add(el);
            out.push({ el: el, url: url, kind: "anchor" });
        });

        // Path 4: full-subtree attribute walk. Same broadening as
        // the FIX2 scanner. Skip the inline-injected <img>/<video>
        // markers so we don't pick our own injected media as a
        // candidate for re-swap.
        const allEls = li.querySelectorAll("*");
        for (const el of allEls) {
            if (seen.has(el)) continue;
            if (el.getAttribute && el.getAttribute("data-osl-injected") === "1") {
                continue;
            }
            if (!el.attributes || el.attributes.length === 0) continue;
            let matchedUrl = null;
            for (let i = 0; i < el.attributes.length; i++) {
                const val = el.attributes[i].value;
                if (typeof val !== "string") continue;
                if (val.indexOf("discordapp") < 0) continue;
                const matches = val.match(ATTACHMENT_URL_RE);
                if (!matches) continue;
                for (const url of matches) {
                    if (matchesLastSegment(url)) {
                        matchedUrl = url;
                        break;
                    }
                }
                if (matchedUrl) break;
            }
            if (matchedUrl) {
                seen.add(el);
                out.push({ el: el, url: matchedUrl, kind: "wrapper" });
            }
        }

        return out;
    }

    /**
     * 8d-FIX5: fetch the attachment URL via Rust. Discord's CSP
     * (connect-src) blocks browser fetch from discord.com to
     * cdn.discordapp.com, so we route through the Rust HTTP client
     * via `osl_fetch_attachment_bytes`. Rust returns standard
     * base64 (same shape as the old `btoa(arrayBuffer)` path) and
     * enforces a URL allowlist server-side.
     */
    /**
     * Item 2 fix — encrypted-attachment receive was broken because
     * the fetched URL was Discord's media.discordapp.net transcoding
     * PROXY (the `<img>/<video>` src + `proxy_url`), often with
     * `?...&format=webp&width=&height=`. That proxy re-encodes the
     * file: it strips everything past the container's logical end
     * (our OSL-ATT3 magic + cover + AEAD payload), and refuses
     * unknown transcodes with `415 Unsupported Media Type`
     * (`...688d0739.mp4?...format=webp 415`). The ~351KB blob then
     * "decrypts" to a 520-byte garbage/placeholder (or fails),
     * never the real image.
     *
     * Only the cdn.discordapp.com ORIGIN serves the bytes verbatim.
     * Canonicalize before fetching: rewrite the host
     * media.discordapp.net -> cdn.discordapp.com and drop the
     * transcode-only query params, while PRESERVING Discord's signed
     * attachment params (`ex`,`is`,`hm`) which cdn.discordapp.com
     * now requires. Both hosts are already in the Rust
     * `osl_fetch_attachment_bytes` allowlist, so the rewritten URL
     * still passes. Defensive: any parse failure returns the URL
     * unchanged.
     */
    function oslCanonicalAttachmentUrl(url) {
        try {
            const u = new URL(url);
            if (u.hostname === "media.discordapp.net") {
                u.hostname = "cdn.discordapp.com";
            }
            // Transcode-only params understood by the media proxy;
            // cdn.discordapp.com ignores them and they are what
            // trigger the re-encode/415. ex/is/hm (signed auth) and
            // anything else are kept.
            const drop = [
                "format",
                "width",
                "height",
                "quality",
                "passthrough",
                "animated",
            ];
            for (const p of drop) u.searchParams.delete(p);
            return u.toString();
        } catch (_) {
            return url;
        }
    }

    async function oslFetchAttachmentBase64(rawUrl) {
        const url = oslCanonicalAttachmentUrl(rawUrl);
        if (url !== rawUrl) {
            console.log(
                "[OSL] attachment fetch canonicalized: " +
                    rawUrl.substring(0, 80) +
                    " -> " +
                    url.substring(0, 80)
            );
        }
        const result = await oslInvoke("osl_fetch_attachment_bytes", { url });
        if (!result.ok) {
            const errMsg = result.error || "unknown";
            console.log(
                "[OSL] fetch via IPC: url=" +
                    url.substring(0, 100) +
                    " error=" +
                    errMsg
            );
            throw new Error("osl_fetch_attachment_bytes: " + errMsg);
        }
        console.log(
            "[OSL] fetch via IPC: url=" +
                url.substring(0, 100) +
                " result_len=" +
                result.value.length
        );
        return result.value;
    }

    /**
     * 9-A1b FIX9: schedule a delayed retry of the broken-video-card
     * hide. Discord renders the "Image failed to load." text AFTER
     * our swap completes — the synchronous FIX7 walk runs before
     * the text exists in the DOM and silently gives up. We attach a
     * MutationObserver on the <li> watching for the text to appear
     * (and a 1500ms setTimeout as a backstop), then re-run the
     * hide. The observer self-disconnects on first successful hide
     * or after a 5-second cap, whichever comes first.
     */
    function oslScheduleBrokenCardRetry(li, msgIdHint, injectedMedia) {
        if (!li) return;
        let done = false;
        const attempt = function (delayed) {
            if (done) return;
            const before = li.querySelectorAll(
                '[data-osl-hidden="1"]'
            ).length;
            try {
                console.log(
                    "[OSL] cube hide attempt: msg=" +
                        msgIdHint +
                        " delayed=" +
                        (delayed ? "yes" : "no")
                );
                oslHideBrokenVideoCard(li, msgIdHint, injectedMedia);
            } catch (e) {
                console.warn("[OSL] cube hide retry threw:", e);
            }
            const after = li.querySelectorAll(
                '[data-osl-hidden="1"]'
            ).length;
            if (after > before) {
                done = true;
            }
        };

        // Immediate-delay backstop in case Discord renders the cube
        // text during this microtask flush.
        nativeSetTimeout(function () {
            attempt(true);
        }, 1500);

        // MutationObserver — fires on any subtree change. Throttled
        // to avoid re-attempt thrashing during React's render churn.
        let lastAttempt = 0;
        const observer = new MutationObserver(function () {
            if (done) return;
            const now = Date.now();
            if (now - lastAttempt < 200) return;
            lastAttempt = now;
            attempt(true);
        });
        try {
            observer.observe(li, {
                subtree: true,
                childList: true,
                characterData: true,
            });
        } catch (_) {}

        // Hard cap: disconnect after 5 seconds regardless. Avoids
        // observers piling up on scrollback-mounted messages.
        nativeSetTimeout(function () {
            try {
                observer.disconnect();
            } catch (_) {}
            if (!done) {
                console.log(
                    "[OSL] cube hide gave up: msg=" +
                        msgIdHint +
                        " reason=no_text_after_5s"
                );
            }
        }, 5000);
    }

    /**
     * 8e-FIX7: locate and hide Discord's broken-video / file-card UI
     * after a cache-hit fallback injection. Called only on the
     * `fallback_content` path (the `found_url` path already hides
     * its wrapper directly).
     *
     * Detection (try in order, take first hit):
     *   1. visible descendant whose `textContent` matches
     *      /video failed|image failed|failed to load/i;
     *   2. descendant whose class contains attachment/embed/placeholder
     *      tokens (case-insensitive) and isn't on the exclude list;
     *   3. descendant with `aria-label` matching attachment/video/image.
     *
     * From the matched element, walk up at most 6 ancestor levels,
     * stopping at: the `<li>` root, the message-content div, any
     * ancestor that contains the injected media (would over-hide),
     * or any ancestor with an excluded class (username/avatar/
     * timestamp/header/reactions/toolbar). The final element is the
     * card root we hide.
     *
     * Safety: never hides an ancestor of the injected media. Skips
     * already-hidden elements (`data-osl-hidden="1"`).
     */
    function oslHideBrokenVideoCard(li, msgIdHint, injectedMedia) {
        if (!li) return;
        const TEXT_FAIL_RE = /(video failed|image failed|failed to load)/i;
        const CARD_CLASS_RE =
            /(attachment|mediaplaceholder|embed|messageattachment|loadingplaceholder|placeholder|noimage|errorimage)/i;
        const ARIA_RE = /(attachment|video|image)/i;
        const EXCLUDE_CLASS_RE =
            /(username|avatar|timestamp|reactions|buttons|header|toolbar)/i;

        function isVisible(el) {
            try {
                const cs = window.getComputedStyle(el);
                if (cs.display === "none") return false;
                if (cs.visibility === "hidden") return false;
                return true;
            } catch (_) {
                return true;
            }
        }
        function skipEl(el) {
            if (!el || el.nodeType !== 1) return true;
            if (el === injectedMedia) return true;
            if (injectedMedia && injectedMedia.contains(el)) return true;
            if (el.getAttribute) {
                if (el.getAttribute("data-osl-hidden") === "1") return true;
                if (el.getAttribute("data-osl-injected") === "1") return true;
                if (el.getAttribute("data-osl-swapped") === "1") return true;
            }
            return false;
        }

        const all = li.querySelectorAll("*");
        let matched = null;
        let matchedBy = null;
        let matchedSample = "";

        // 9-A1c FIX11 Pass 0 (structural primary). Discord renders the
        // failed-image cube via div.imageErrorWrapper_<hash> inside a
        // div.loadingOverlay_<hash> wrapper inside the imageWrapper.
        // The hash suffix changes between Discord builds; prefix-match
        // via [class*="..."] survives that churn. Prefer hiding the
        // loadingOverlay parent so the reserved aspect-ratio layout
        // slot collapses too, not just the error icon. Pass 1-3 below
        // remain as defensive fallbacks for future DOM shapes.
        const structural = li.querySelector(
            'div[class*="imageErrorWrapper"], div[class*="loadingOverlay"]'
        );
        if (structural && !skipEl(structural) && isVisible(structural)) {
            // Prefer the loadingOverlay ancestor when the match was
            // an imageErrorWrapper child; otherwise use the match itself.
            const cls =
                structural.className &&
                typeof structural.className === "string"
                    ? structural.className
                    : "";
            let toHide = structural;
            if (
                /imageErrorWrapper/.test(cls) &&
                !/loadingOverlay/.test(cls)
            ) {
                const overlay = structural.closest(
                    '[class*="loadingOverlay"]'
                );
                if (overlay && !skipEl(overlay) && isVisible(overlay)) {
                    toHide = overlay;
                }
            }
            // Defensive: never hide an ancestor of the injected media.
            if (!injectedMedia || !toHide.contains(injectedMedia)) {
                toHide.style.display = "none";
                toHide.setAttribute("data-osl-hidden", "1");
                const toHideCls =
                    toHide.className &&
                    typeof toHide.className === "string"
                        ? toHide.className.substring(0, 80)
                        : "";
                console.log(
                    "[OSL] cube hidden: msg=" +
                        msgIdHint +
                        " method=pass0_structural target_tag=" +
                        toHide.tagName.toLowerCase() +
                        " target_class=" +
                        toHideCls
                );
                return;
            }
        }

        // Bug 2 — Pass 0a (attachment-card frame). Pass 0 only
        // collapses the error cube / loadingOverlay slot; Passes
        // 1-3 are failure-text/placeholder-class oriented. But the
        // common case is the img/video src-replace fast path on a
        // card Discord DID render: there is NO error chrome, yet
        // Discord still draws the attachment-card FOOTER strip
        // (filename + download + the "+"/add affordance) as a
        // sibling of the media slot inside the attachment box. No
        // pass targets it, and its class carries buttons/toolbar
        // tokens that EXCLUDE_CLASS_RE actively skips — so the
        // trailing bar survives below the swapped image. Fix
        // structurally (no hash-class guessing): climb from the
        // injected media to the OUTERMOST attachment-ish ancestor
        // (matching the stable attachment|messageattachment|embed|
        // mediaplaceholder|mosaic|nonmediaattachment prefixes), then
        // hide every child subtree of that box that is neither our
        // media nor an ancestor of it — i.e. the original media slot
        // and the footer/+ strip — leaving the image visible. The
        // climb stops at the <li>, the message-content div, or an
        // excluded ancestor, so username / reactions / timestamp /
        // message text (all outside the attachment box) are never
        // touched; the injected-media containment guard keeps the
        // image itself safe.
        if (injectedMedia) {
            try {
                const CARD_PREFIX_RE =
                    /(attachment|messageattachment|embed|mediaplaceholder|mosaic|nonmediaattachment)/i;
                // Climb to the OUTERMOST attachment-ish container
                // (the footer/+ strip is typically a sibling of the
                // inner media wrapper but still inside this outer
                // box). Bounded; stop at the <li>, the message-
                // content div, or an excluded ancestor.
                let outer = null;
                let node = injectedMedia.parentNode;
                for (let i = 0; i < 8 && node && node.nodeType === 1; i++) {
                    if (node.tagName === "LI") break;
                    if (
                        node.id &&
                        node.id.indexOf("message-content-") === 0
                    ) {
                        break;
                    }
                    const ncls =
                        node.className &&
                        typeof node.className === "string"
                            ? node.className
                            : "";
                    if (ncls && EXCLUDE_CLASS_RE.test(ncls)) break;
                    if (ncls && CARD_PREFIX_RE.test(ncls)) {
                        outer = node;
                    }
                    node = node.parentNode;
                }
                if (
                    outer &&
                    outer !== injectedMedia &&
                    outer.contains(injectedMedia) &&
                    isVisible(outer)
                ) {
                    // `outer` contains our media. Collapse every
                    // child subtree that is neither our injected
                    // media nor an ancestor of it — that is the
                    // original media slot + the trailing footer/+
                    // controls bar. The image (inside the kept
                    // ancestor child) stays visible.
                    let hidAny = false;
                    const kids = Array.prototype.slice.call(
                        outer.children
                    );
                    for (const kid of kids) {
                        if (kid.nodeType !== 1) continue;
                        if (
                            kid === injectedMedia ||
                            kid.contains(injectedMedia)
                        ) {
                            continue;
                        }
                        if (
                            kid.getAttribute("data-osl-hidden") ===
                                "1" ||
                            kid.getAttribute("data-osl-injected") ===
                                "1"
                        ) {
                            continue;
                        }
                        kid.style.display = "none";
                        kid.setAttribute("data-osl-hidden", "1");
                        hidAny = true;
                    }
                    if (hidAny) {
                        console.log(
                            "[OSL] cube hidden: msg=" +
                                msgIdHint +
                                " method=pass0a_card_frame_children"
                        );
                        return;
                    }
                }
            } catch (e) {
                console.warn(
                    "[OSL] pass0a card-frame strip threw:",
                    e
                );
            }
        }

        // Pass 1: text match.
        for (const el of all) {
            if (skipEl(el)) continue;
            if (!isVisible(el)) continue;
            const cls =
                el.className && typeof el.className === "string"
                    ? el.className
                    : "";
            if (cls && EXCLUDE_CLASS_RE.test(cls)) continue;
            const text = (el.textContent || "").trim();
            if (
                text.length > 0 &&
                text.length < 200 &&
                TEXT_FAIL_RE.test(text)
            ) {
                matched = el;
                matchedBy = "text";
                matchedSample = text.substring(0, 50);
                break;
            }
        }
        // Pass 2: card-class match.
        if (!matched) {
            for (const el of all) {
                if (skipEl(el)) continue;
                if (!isVisible(el)) continue;
                const cls =
                    el.className && typeof el.className === "string"
                        ? el.className
                        : "";
                if (!cls) continue;
                if (EXCLUDE_CLASS_RE.test(cls)) continue;
                if (CARD_CLASS_RE.test(cls)) {
                    matched = el;
                    matchedBy = "class";
                    matchedSample = cls.substring(0, 50);
                    break;
                }
            }
        }
        // Pass 3: aria-label match.
        if (!matched) {
            for (const el of all) {
                if (skipEl(el)) continue;
                if (!isVisible(el)) continue;
                const aria =
                    (el.getAttribute && el.getAttribute("aria-label")) || "";
                if (aria && ARIA_RE.test(aria)) {
                    matched = el;
                    matchedBy = "aria";
                    matchedSample = aria.substring(0, 50);
                    break;
                }
            }
        }

        if (!matched) {
            console.log(
                "[OSL] no broken-video card found to hide: msg=" +
                    msgIdHint +
                    " li_descendants=" +
                    all.length
            );
            return;
        }

        // Walk up to the card root.
        let toHide = matched;
        for (let i = 0; i < 6; i++) {
            const parent = toHide.parentNode;
            if (!parent || parent === li) break;
            if (parent.id && parent.id.indexOf("message-content-") === 0) {
                break;
            }
            if (injectedMedia && parent.contains(injectedMedia)) break;
            const pcls =
                parent.className && typeof parent.className === "string"
                    ? parent.className
                    : "";
            if (pcls && EXCLUDE_CLASS_RE.test(pcls)) break;
            toHide = parent;
        }
        if (injectedMedia && toHide.contains(injectedMedia)) {
            console.log(
                "[OSL] no broken-video card found to hide: msg=" +
                    msgIdHint +
                    " reason=would_hide_injected_media"
            );
            return;
        }

        toHide.style.display = "none";
        toHide.setAttribute("data-osl-hidden", "1");
        const toHideCls =
            toHide.className && typeof toHide.className === "string"
                ? toHide.className.substring(0, 80)
                : "";
        console.log(
            "[OSL] hid broken-video card: msg=" +
                msgIdHint +
                " tag=" +
                toHide.tagName.toLowerCase() +
                " class=" +
                toHideCls +
                " matched_by=" +
                matchedBy +
                " text_sample=" +
                matchedSample
        );
    }

    /**
     * 8e-FIX6: cache-hit swap fallback. Called when the swap target
     * has `el: null` — happens for the FIX3 url-cache-fallback path,
     * where the scanner pushed a candidate based on the message-API
     * URL cache rather than a DOM walk. The DOM has no element
     * exposing the URL (Discord's broken-MP4 file card keeps it
     * in React state only), so we:
     *
     *   1. Re-scan the `<li>` once more in case Discord mounted the
     *      URL after the scanner ran (race), and if found use the
     *      regular wrapper-hide-and-inject path.
     *   2. Otherwise inject inline media as a sibling of the
     *      message-content div. The original file card stays
     *      visible alongside — acceptable since we can't reliably
     *      identify which `<div>` ancestor IS the card.
     *
     * Idempotent via a per-attachment `data-osl-injected-for="<name>"`
     * marker on the injected media; re-scans walk past it.
     */
    function oslSwapCacheHitFallback(target, blobUrl, mime) {
        const msgIdLog = target.msgId || "?";
        const li =
            target.li ||
            (target.msgId
                ? document.querySelector("li[id$='-" + target.msgId + "']")
                : null);
        if (!li) {
            console.log(
                "[OSL] attachment swap: msg=" +
                    msgIdLog +
                    " reason=li_not_found_for_cache_hit"
            );
            return;
        }
        // Idempotency: skip if we already injected media for this
        // filename in this <li>. The marker survives observer reruns
        // because we only swap-in our own elements.
        if (target.name) {
            const injected = li.querySelectorAll("[data-osl-injected-for]");
            for (const e of injected) {
                if (e.getAttribute("data-osl-injected-for") === target.name) {
                    return;
                }
            }
        }
        // Second-pass URL search — Discord may have mounted the URL
        // between the scanner's walk and now. Skip elements we
        // already injected so we don't bind to our own media.
        let foundEl = null;
        if (target.url) {
            const all = li.querySelectorAll("*");
            for (const el of all) {
                if (
                    el.getAttribute &&
                    el.getAttribute("data-osl-injected") === "1"
                ) {
                    continue;
                }
                if (!el.attributes || el.attributes.length === 0) continue;
                for (let i = 0; i < el.attributes.length; i++) {
                    const v = el.attributes[i].value;
                    if (typeof v === "string" && v.indexOf(target.url) >= 0) {
                        foundEl = el;
                        break;
                    }
                }
                if (foundEl) break;
            }
        }

        const media = oslMakeInlineMedia(blobUrl, mime);
        if (target.name) {
            media.setAttribute("data-osl-injected-for", target.name);
        }

        let method;
        if (foundEl) {
            const wrapper = oslFindAttachmentCardWrapper(foundEl) || foundEl;
            if (wrapper.getAttribute("data-osl-swapped") !== "1") {
                wrapper.style.display = "none";
                wrapper.setAttribute("data-osl-swapped", "1");
                const wrapperCls =
                    wrapper.className &&
                    typeof wrapper.className === "string"
                        ? wrapper.className.substring(0, 80)
                        : "";
                console.log(
                    "[OSL] hid original attachment wrapper: tag=" +
                        wrapper.tagName.toLowerCase() +
                        " class=" +
                        wrapperCls
                );
            }
            method = "found_url";
            const parent = wrapper.parentNode;
            if (parent) {
                parent.insertBefore(media, wrapper.nextSibling);
            } else {
                li.appendChild(media);
            }
        } else {
            method = "fallback_content";
            const content = li.querySelector('[id^="message-content-"]');
            if (content && content.parentNode) {
                content.parentNode.insertBefore(media, content.nextSibling);
            } else {
                li.appendChild(media);
            }
            // 8e-FIX7 / 9-A1b FIX9: locate + hide Discord's
            // broken-video / file card (incl. the late-rendered
            // "Image failed to load." cube text) so it doesn't sit
            // alongside our decrypted image. Same cleanup the
            // DOM-target swap branches now use (Item 1).
            oslStripBrokenCardChrome(li, media, msgIdLog);
        }
        console.log(
            "[OSL] attachment swap: msg=" +
                msgIdLog +
                " target=cache method=" +
                method +
                " injected=" +
                media.tagName.toLowerCase()
        );
    }

    /**
     * Item 1: strip Discord's broken-.mp4 card chrome (the error
     * cube / loadingOverlay and the trailing filename+download "+"
     * bar) that otherwise sits around/below a swapped-in decrypted
     * image. Previously this cleanup ran ONLY on the cache
     * `fallback_content` path; the DOM-target swap branches
     * (img/video/anchor/wrapper) swapped the media but left the card
     * shell behind. Reuses the proven detector (Pass 0 structural
     * imageErrorWrapper/loadingOverlay + text/class/aria + ancestor
     * walk) and the FIX9 late-text retry; both are defensive,
     * idempotent (data-osl-hidden), and never hide the injected
     * media or an ancestor of it, so calling them on every swap
     * strictly improves and cannot regress a clean swap.
     */
    function oslStripBrokenCardChrome(li, media, msgIdHint) {
        if (!li) return;
        try {
            oslHideBrokenVideoCard(li, msgIdHint || "?", media);
        } catch (e) {
            console.warn("[OSL] strip broken-card chrome threw:", e);
        }
        try {
            oslScheduleBrokenCardRetry(li, msgIdHint || "?", media);
        } catch (e) {
            console.warn("[OSL] schedule broken-card retry threw:", e);
        }
    }

    /**
     * Swap a single rendered element to point at `blobUrl`.
     *
     * 8e-FIX5: handles four target kinds (set by oslFindAttachmentTargets):
     *   - `img` / `video`: just swap `src` (existing-tag fast path).
     *   - `anchor`: replace `<a>` with inline media via replaceWith.
     *   - `wrapper`: any other tag. Find the closest attachment-card
     *               ancestor, hide it with display:none, insert
     *               inline media as a sibling immediately after.
     *               Falls back to hiding the target itself if no
     *               wrapper ancestor matches.
     *
     * Idempotent: tags swapped elements with `data-osl-swapped="1"`
     * and skips on a second call. The injected media carries
     * `data-osl-injected="1"` so the scanner's target-discovery
     * doesn't pick it up as a candidate.
     */
    function oslSwapAttachmentElement(target, blobUrl, mime) {
        try {
            const el = target.el;
            if (!el) {
                // 8e-FIX6: cache-hit candidate path. The scanner's
                // url-cache fallback (8e-FIX3) creates targets with
                // `el: null` when Discord's file card doesn't expose
                // the CDN URL anywhere we can locate in the DOM.
                oslSwapCacheHitFallback(target, blobUrl, mime);
                return;
            }
            const kind =
                target.kind ||
                (el.tagName === "IMG"
                    ? "img"
                    : el.tagName === "VIDEO"
                      ? "video"
                      : el.tagName === "A"
                        ? "anchor"
                        : "wrapper");

            if (el.getAttribute && el.getAttribute("data-osl-swapped") === "1") {
                // Already swapped in a previous observer tick.
                return;
            }

            // Item 1: the enclosing message <li> + a msg-id hint so
            // every swap branch can strip the broken-.mp4 card chrome
            // that otherwise remains around the swapped-in image.
            const li =
                (el.closest &&
                    el.closest('li[id^="chat-messages-"]')) ||
                target.li ||
                null;
            const msgIdHint =
                target.msgId || (li && li.id) || "?";

            if (kind === "img") {
                el.setAttribute("src", blobUrl);
                el.removeAttribute("srcset");
                el.setAttribute("data-osl-swapped", "1");
                console.log(
                    "[OSL] attachment swap: target=img method=src-replace"
                );
                oslStripBrokenCardChrome(li, el, msgIdHint);
                return;
            }
            if (kind === "video") {
                el.setAttribute("src", blobUrl);
                el.setAttribute("type", mime);
                el.setAttribute("data-osl-swapped", "1");
                console.log(
                    "[OSL] attachment swap: target=video method=src-replace"
                );
                oslStripBrokenCardChrome(li, el, msgIdHint);
                return;
            }
            if (kind === "anchor") {
                const replacement = oslMakeInlineMedia(blobUrl, mime);
                el.replaceWith(replacement);
                // The original <a> is detached; mark the replacement
                // so a re-scan via observer doesn't re-swap.
                console.log(
                    "[OSL] attachment swap: target=a method=replace-with-" +
                        replacement.tagName.toLowerCase()
                );
                oslStripBrokenCardChrome(li, replacement, msgIdHint);
                return;
            }

            // wrapper: hide closest attachment-card ancestor + inject.
            const wrapper = oslFindAttachmentCardWrapper(el) || el;
            const wrapperCls =
                wrapper.className && typeof wrapper.className === "string"
                    ? wrapper.className.substring(0, 80)
                    : "";
            wrapper.style.display = "none";
            wrapper.setAttribute("data-osl-swapped", "1");
            console.log(
                "[OSL] hid original attachment wrapper: tag=" +
                    wrapper.tagName.toLowerCase() +
                    " class=" +
                    wrapperCls
            );
            const replacement = oslMakeInlineMedia(blobUrl, mime);
            if (wrapper.parentNode) {
                wrapper.parentNode.insertBefore(
                    replacement,
                    wrapper.nextSibling
                );
                console.log(
                    "[OSL] attachment swap: target=" +
                        el.tagName.toLowerCase() +
                        " method=wrapper-hide-and-sibling-append" +
                        " injected=" +
                        replacement.tagName.toLowerCase()
                );
            } else {
                // Detached wrapper (rare). Re-show + inject as child.
                wrapper.style.display = "";
                wrapper.appendChild(replacement);
                console.log(
                    "[OSL] attachment swap: target=" +
                        el.tagName.toLowerCase() +
                        " method=wrapper-child-append (parent missing)"
                );
            }
            // wrapper hides its own card, but Discord's failed-.mp4
            // accessory frequently has a sibling footer bar / "+"
            // OUTSIDE that wrapper; strip it too.
            oslStripBrokenCardChrome(li, replacement, msgIdHint);
            return;
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
        const __dbg_li_id = li.id;
        if (OSL_DEBUG_SWEEP) {
            console.log("[OSL] scan entry: li_id=" + __dbg_li_id);
        }
        const m = /chat-messages-(?:\d{15,22})-(\d{15,22})/.exec(__dbg_li_id);
        if (!m) {
            return;
        }
        const msgId = m[1];
        // Clear the negative-cache mark on entry: any direct call
        // (mutation observer) re-evaluates, so lazy-rendered media is
        // caught. The sweep, by contrast, skips set members without
        // calling us at all.
        oslAttScannedEmpty.delete(msgId);

        // 8d-FIX4: wrap remaining body so async rejections / unexpected
        // throws are surfaced. The caller (.catch(()=>{}) at the two
        // observer call sites) was swallowing them silently.
        try {

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
                console.log(
                    "[OSL] scan skip: msg=" +
                        msgId +
                        " reason=burned_scope"
                );
                return;
            }
        } catch (_) {}

        // Cache hit: replay swap.
        const cacheEntry = window.__oslAttachmentDecrypted.get(msgId);
        if (cacheEntry && cacheEntry.byRandomName) {
            for (const randomName of Object.keys(cacheEntry.byRandomName)) {
                const cached = cacheEntry.byRandomName[randomName];
                const targets = oslFindAttachmentTargets(li, randomName);
                if (targets.length === 0) {
                    // 8e-FIX6: same fallback as the new-decrypt path
                    // when the DOM doesn't carry the URL.
                    oslSwapAttachmentElement(
                        {
                            el: null,
                            url: cached.url || null,
                            name: randomName,
                            li: li,
                            msgId: msgId,
                        },
                        cached.blobUrl,
                        cached.mime
                    );
                } else {
                    targets.forEach(function (t) {
                        oslSwapAttachmentElement(
                            t,
                            cached.blobUrl,
                            cached.mime
                        );
                    });
                }
            }
            console.log(
                "[OSL] scan skip: msg=" + msgId + " reason=cache_replay"
            );
            return;
        }

        // 8e-FIX2: full-subtree walk. Previous (img/video/a) selector
        // missed Discord's "video failed to load" card for our
        // zero-sample decoy MP4 — that card probably renders the CDN
        // URL on a <div data-*> or similar non-standard surface. Walk
        // every descendant, scan every attribute value for a CDN
        // attachments URL, then filter by the OSL filename regex.
        // ~50 descendants × ~5 attrs per Discord message <li> ≈ 250
        // string-indexOf calls; cheap.
        const CDN_URL_RE =
            /https:\/\/(?:cdn|media)\.discordapp\.(?:com|net)\/attachments\/[^\s"'<>]+/g;
        const NAME_RE = /^[0-9a-f]{8}\.(mp4|bin|png)$/i;
        const candidates = [];
        const seenUrls = new Set();
        const filenamesSeen = [];
        let elementsWithCdn = 0;
        const allEls = li.querySelectorAll("*");
        for (const el of allEls) {
            if (!el.attributes || el.attributes.length === 0) continue;
            let elHadCdn = false;
            for (let i = 0; i < el.attributes.length; i++) {
                const attr = el.attributes[i];
                const val = attr.value;
                if (typeof val !== "string") continue;
                if (val.indexOf("discordapp") < 0) continue;
                const matches = val.match(CDN_URL_RE);
                if (!matches) continue;
                for (const url of matches) {
                    if (seenUrls.has(url)) continue;
                    seenUrls.add(url);
                    if (!elHadCdn) {
                        elementsWithCdn++;
                        elHadCdn = true;
                    }
                    const path = url.split("?")[0];
                    const last = path.split("/").pop() || "";
                    if (NAME_RE.test(last)) {
                        candidates.push({
                            el: el,
                            url: url,
                            name: last,
                        });
                        console.log(
                            "[OSL] scan candidate found: tag=" +
                                el.tagName +
                                " attribute=" +
                                attr.name +
                                " url=" +
                                url.substring(0, 100)
                        );
                    } else if (last) {
                        filenamesSeen.push(last);
                    }
                }
            }
        }
        const domCount = candidates.length;
        let cacheCount = 0;
        const cacheFilenamesSeen = [];
        // 8e-FIX3: if the DOM walk turned up nothing, fall back to
        // the attachment URL cache (populated by intercepting
        // Discord's /messages API responses). Discord's zero-sample
        // decoy MP4 renders as a "video failed to load" card whose
        // DOM doesn't expose the CDN URL anywhere — the URL only
        // exists in React state we can't reach. The cache is our
        // out-of-band path to that URL.
        if (domCount === 0) {
            const cached = oslGetCachedAttachmentUrls(msgId);
            if (cached) {
                for (const entry of cached) {
                    const path = entry.url.split("?")[0];
                    const last = path.split("/").pop() || "";
                    if (!NAME_RE.test(last)) {
                        if (last) cacheFilenamesSeen.push(last);
                        continue;
                    }
                    if (seenUrls.has(entry.url)) continue;
                    seenUrls.add(entry.url);
                    candidates.push({ el: null, url: entry.url, name: last });
                    cacheCount++;
                    console.log(
                        "[OSL] scan url cache hit: msg=" +
                            msgId +
                            " url=" +
                            entry.url.substring(0, 100) +
                            " filename=" +
                            last
                    );
                }
            }
        }

        if (OSL_DEBUG_SWEEP) {
            console.log(
                "[OSL] scan candidates: count=" +
                    candidates.length +
                    " for li_id=" +
                    __dbg_li_id +
                    " (dom=" +
                    domCount +
                    ", cache=" +
                    cacheCount +
                    ", descendants=" +
                    allEls.length +
                    ", elements_with_cdn=" +
                    elementsWithCdn +
                    ")"
            );
        }
        if (candidates.length === 0) {
            // Fix B: last resort before giving up — the receive-side
            // failed-media card may hold the CDN URL only in React
            // state. Walk the <li> fiber for message.attachments.
            try {
                const fiberAtts = oslAttachmentUrlsViaFiber(li);
                for (const fa of fiberAtts) {
                    const path = fa.url.split("?")[0];
                    const last = path.split("/").pop() || "";
                    if (!NAME_RE.test(last)) continue;
                    if (seenUrls.has(fa.url)) continue;
                    seenUrls.add(fa.url);
                    candidates.push({
                        el: null,
                        url: fa.url,
                        name: last,
                    });
                    console.log(
                        "[OSL] scan fiber-url fallback: msg=" +
                            msgId +
                            " url=" +
                            fa.url.substring(0, 100) +
                            " filename=" +
                            last
                    );
                }
            } catch (_) {}
        }
        if (candidates.length === 0) {
            // Three distinct "nothing to scan" reasons now:
            // - no_cdn_url_in_subtree: DOM had no CDN URLs anywhere
            //   AND the cache had no entry (or empty) for this msg.
            // - cdn_urls_found_but_filename_unmatched: DOM had CDN
            //   URLs but none matched the random-filename regex
            //   (emoji/avatar URLs).
            // - url_cache_filename_unmatched: cache had URLs for
            //   this msg but their filenames didn't match the regex
            //   (unencrypted attachment).
            const cached = oslGetCachedAttachmentUrls(msgId);
            let reason;
            if (cacheFilenamesSeen.length > 0) {
                reason =
                    "url_cache_filename_unmatched cached_filenames=[" +
                    cacheFilenamesSeen.join(", ") +
                    "]";
            } else if (elementsWithCdn > 0) {
                reason =
                    "cdn_urls_found_but_filename_unmatched filenames=[" +
                    filenamesSeen.join(", ") +
                    "]";
            } else if (cached && cached.length > 0) {
                // Defensive: cached entries existed but had no usable
                // filename (shouldn't happen with the cache helper's
                // null filter, but keep the branch for clarity).
                reason = "url_cache_entries_have_no_usable_filename";
            } else {
                reason = "no_cdn_url_in_subtree";
            }
            // Negative-cache this message so the periodic sweep stops
            // re-walking it every tick. A real DOM change to the <li>
            // fires the mutation observer, which calls this function
            // directly and clears the mark on entry, so lazily-rendered
            // media is still picked up.
            oslAttScannedEmpty.add(msgId);
            if (OSL_DEBUG_SWEEP) {
                console.log(
                    "[OSL] scan no candidates: li_id=" +
                        __dbg_li_id +
                        " reason=" +
                        reason
                );
            }
            return;
        }

        // Sender id is required for the cover decrypt (v=2 wrap is
        // bound to sender's pubkey on the recv side). Pull from
        // li's data-author-id, walking up if needed.
        //
        // 8e: self-sent .mp4 messages mount the <li> before React
        // hydrates the data-author-id attribute, so recvExtractAuthorId
        // returns null on the first observer pass. Retry briefly,
        // then fall back to oslSelfDiscordId() — peer messages won't
        // hit the fallback (their author id resolves immediately or
        // their <li> dispatches a second observer event once hydrated;
        // a wrong-author scope-mismatch on the fallback is caught
        // downstream by the v=2 scope-acceptance gate).
        let senderId = null;
        let resolutionMethod = "recv";
        try {
            senderId = recvExtractAuthorId(li);
        } catch (_) {}
        for (let attempt = 1; attempt <= 3 && !senderId; attempt++) {
            await new Promise(function (res) {
                nativeSetTimeout(res, 100);
            });
            try {
                senderId = recvExtractAuthorId(li);
            } catch (_) {}
            console.log(
                "[OSL] scan retry author id: msg=" +
                    msgId +
                    " attempt=" +
                    attempt +
                    "/3 resolved=" +
                    (senderId ? "yes" : "no")
            );
            if (senderId) {
                resolutionMethod = "retry";
            }
        }
        if (!senderId) {
            const selfId = await oslSelfDiscordId();
            if (selfId) {
                senderId = selfId;
                resolutionMethod = "self_fallback";
                console.log(
                    "[OSL] scan: author id fallback to self (" +
                        selfId +
                        ") for msg=" +
                        msgId
                );
            } else {
                console.log(
                    "[OSL] scan skip: msg=" +
                        msgId +
                        " reason=no_sender_id_no_self_fallback"
                );
                return;
            }
        }
        console.log(
            "[OSL] scan author resolution: msg=" +
                msgId +
                " method=" +
                resolutionMethod +
                " id=" +
                senderId
        );

        const newCache = { byRandomName: {}, blobUrls: [] };
        for (const cand of candidates) {
            try {
                // Beta 1.0: local sealed-store cache hit. If we
                // decrypted this attachment in a prior session, skip
                // the CDN fetch + decrypt entirely and render from the
                // cached bytes. Keyed by (msgId, random filename).
                try {
                    if (msgId) {
                        const cacheRes = await oslInvoke(
                            "osl_attachment_cache_get",
                            {
                                discordMessageId: msgId,
                                randomFilename: cand.name,
                            }
                        );
                        if (
                            cacheRes &&
                            cacheRes.ok &&
                            cacheRes.value &&
                            typeof cacheRes.value.bytes_b64 === "string"
                        ) {
                            const cachedBytes = oslB64ToBytes(
                                cacheRes.value.bytes_b64
                            );
                            oslInjectAttachmentBytes(
                                li,
                                msgId,
                                cand,
                                cachedBytes,
                                cacheRes.value.mime,
                                newCache
                            );
                            console.log(
                                "[OSL] attachment cache HIT msg=" +
                                    msgId +
                                    " random=" +
                                    cand.name +
                                    " bytes=" +
                                    cachedBytes.length
                            );
                            continue;
                        }
                    }
                } catch (_) {}
                // 8d-FIX2: with `.bin` wrappers Discord doesn't
                // transcode, so the DOM-rendered URL on the
                // attachment card (typically the actual
                // cdn.discordapp.com/attachments/{cid}/{aid}/{fn}
                // pointing at the right attachment_id) is now
                // authoritative. FIX1 tried to reconstruct the URL
                // from message_id but Discord uses a separate
                // attachment_id, so the reconstructed URL 404'd
                // and we always fell back to the DOM URL anyway.
                console.log(
                    "[OSL] attachment scan: msg=" +
                        msgId +
                        " random=" +
                        cand.name +
                        " fetch_url=" +
                        cand.url.substring(0, 100)
                );
                const fileB64 = await oslFetchAttachmentBase64(cand.url);
                // Quick magic-presence diagnostic — base64-decoded
                // length + look for OSL-ATT[12] in the first 64KB
                // so a future Discord-side regression that
                // transcodes octet-stream uploads shows up clearly.
                try {
                    const sniff = atob(fileB64.substring(0, 1024 * 96));
                    const hasV3 = sniff.indexOf("OSL-ATT3") >= 0;
                    const hasV2 = sniff.indexOf("OSL-ATT2") >= 0;
                    const hasV1 = sniff.indexOf("OSL-ATT1") >= 0;
                    console.log(
                        "[OSL] attachment fetch: msg=" +
                            msgId +
                            " b64_len=" +
                            fileB64.length +
                            " magic_present=" +
                            (hasV3
                                ? "OSL-ATT3"
                                : hasV2
                                  ? "OSL-ATT2"
                                  : hasV1
                                    ? "OSL-ATT1"
                                    : "no")
                    );
                } catch (_) {}
                // 8e-FIX1: self-bypass on the scope-acceptance gate.
                // Rust's `should_decrypt_from` checks
                // `peer_map[sender].incoming_decrypt_accepted[scope]`,
                // which is never populated for self (you never "accept"
                // your own invitation). Passing `scope_input=None` for
                // self-sent attachments skips the gate — Rust guards
                // the check on `if let Some(scope) = scope_opt`. Peer
                // messages still gate normally.
                let isSelfSent = false;
                try {
                    const sId = await oslSelfDiscordId();
                    isSelfSent = !!sId && senderId === sId;
                } catch (_) {}
                console.log(
                    "[OSL] attachment decrypt invoke: msg=" +
                        msgId +
                        " sender=" +
                        senderId +
                        " isSelf=" +
                        isSelfSent
                );
                const decRes = await oslInvoke("osl_open_attachment_v2", {
                    senderDiscordId: senderId,
                    scopeInput: isSelfSent ? null : scope || null,
                    fileBytesB64: fileB64,
                    legacyAttKeyB64: null,
                    discordMessageId: msgId || null,
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
                const bytes = oslB64ToBytes(plain.plaintextB64);
                oslInjectAttachmentBytes(
                    li,
                    msgId,
                    cand,
                    bytes,
                    plain.mimeType,
                    newCache
                );
                // Beta 1.0: persist the decrypted bytes to the local
                // sealed store so the next re-entry / restart hits the
                // cache-get path above instead of re-fetching +
                // re-decrypting. Fire-and-forget; size-capped in Rust.
                if (msgId) {
                    oslInvoke("osl_attachment_cache_put", {
                        discordMessageId: msgId,
                        randomFilename: cand.name,
                        mime: plain.mimeType,
                        bytesB64: plain.plaintextB64,
                    }).catch(function () {});
                }
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
        } catch (err) {
            console.warn(
                "[OSL] scan error: li_id=" + __dbg_li_id + " err=",
                err
            );
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
     * @deprecated Use osl_unburn_scope directly via UI flow.
     *
     * Auto-unburn on encrypt-send was removed in 9-A1c. Burned scopes
     * are permanent until the user manually re-engages via the
     * composer encrypt toggle, which calls osl_unburn_scope explicitly.
     * Function body is kept here in case a manual re-engage flow wants
     * the synchronous local-cache update for future use; current call
     * sites have all been removed.
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
        // Beta 1.0 burn simplification.
        //
        // This gate USED to skip decrypt for the ENTIRE channel,
        // permanently, the moment a scope was burned — which made a
        // burned conversation look broken forever: every NEW message
        // after the burn also got skipped, and the only escape was a
        // manual unburn. That was the single biggest source of "burn
        // breaks things."
        //
        // Burn is meant to be simple: destroy the PAST (this scope's
        // keys + the messages that existed at burn time) and let the
        // conversation carry on. Correctness for the destroyed
        // messages is already enforced two ways in Rust, both still
        // active:
        //   1. burn wipes the wrapped keys + deletes the rows, so old
        //      ciphertext has no key and renders as cover.
        //   2. the per-message burn kill-list (is_message_in_burn_kill
        //      _list) refuses the specific message IDs that were live
        //      at burn time, even if a key somehow survived.
        // New messages aren't in the kill-list and get fresh keys, so
        // they decrypt normally. We therefore no longer skip the whole
        // channel here.
        void channelId;
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
    //
    // 9-C1: the invitation-banner subsystem was removed alongside
    // the handshake. `oslRefreshBanners` survives as a no-op stub
    // so the few remaining call sites (refreshHeaderState chain,
    // observer hooks) don't need surgery.

    async function oslRefreshBanners() {
        // 9-C1: stub. No pending invitations exist post-handshake.
    }

    // 9-C1: oslEnsureBannerStack / second oslRefreshBanners /
    // oslRenderBanner / oslBannerScopeLabel / oslOnInvitationDecision
    // all removed alongside the invitation handshake.

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
                //
                // Probe-2 Boot Bug 6: previously this used
                // oslCurrentChannelContext() (= the channel the user
                // is VIEWING). If the burn marker arrives via gateway
                // while the user is in a different channel, that
                // repainted the wrong channel and left the actually-
                // burned one stale. Derive the burned channel from
                // the marker's DOM position instead — the chat-
                // messages <li> id encodes the channel: `chat-
                // messages-<channelId>-<msgId>`.
                try {
                    let burnedChannelId = null;
                    const contentEl = document.getElementById(
                        "message-content-" + msgId
                    );
                    if (contentEl && typeof contentEl.closest === "function") {
                        const li = contentEl.closest(
                            "li[id^='chat-messages-']"
                        );
                        if (li && li.id) {
                            const mm = /chat-messages-(\d{15,22})-\d{15,22}/.exec(
                                li.id
                            );
                            if (mm) burnedChannelId = mm[1];
                        }
                    }
                    // Fall back to the viewed-channel context only
                    // when the marker's <li> isn't in the DOM (the
                    // user has scrolled past it / it's not rendered).
                    if (!burnedChannelId) {
                        const burnCtx = oslCurrentChannelContext();
                        burnedChannelId =
                            burnCtx && burnCtx.channelId
                                ? burnCtx.channelId
                                : null;
                    }
                    if (burnedChannelId) {
                        oslBurnAftermath(burnedChannelId);
                    }
                } catch (e) {
                    console.log(
                        "[OSL] recv-side burn aftermath threw: " +
                            (e && e.message ? e.message : e)
                    );
                }
                oslRefreshHeaderState();
            } else if (result === OSL_RESULT_LEGACY_HANDSHAKE_IGNORED) {
                // 9-C1: no side effect; the recv pipeline already
                // logged the suppress.
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
            const header = oslFindChannelHeader();
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
            const header = oslFindChannelHeader();
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
        // 9-F0-FIX3: the function is now async (it awaits the
        // logged-in shell internally). Drop the synchronous
        // try/catch — sync throws can't escape an async function;
        // any rejection lands in the returned Promise and is logged
        // there. Catch the Promise tail to keep the legacy warn
        // semantics for fingerprint-detectable failures.
        nativeSetTimeout(function () {
            const p = oslEnsureSelfSnowflakeRegistered();
            if (p && typeof p.catch === "function") {
                p.catch(function (e) {
                    console.warn(
                        "[OSL] oslEnsureSelfSnowflakeRegistered threw:",
                        (e && e.message) || e
                    );
                });
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
     * `osl_register_self_snowflake`. Idempotent server-side. The
     * Rust side validates 17-20 digits and refuses retag to a
     * different snowflake.
     *
     * 9-F0-FIX3: gate the whole flow on the logged-in shell being
     * mounted. Pre-fix this fired 1500ms after DOMContentLoaded —
     * on a fresh install that's while the user is still typing
     * their Discord login. Extraction (`oslExtractDiscordSnowflakeFromRuntime`)
     * structurally fails on the login page (no panels__ avatar, no
     * title_ header to walk the React fiber from), and the
     * single-shot `oslSnowflakeBootstrapDone` guard then prevents
     * any retry once Discord finishes login.
     *
     * Fix: await `oslTourWaitForLoggedIn` (the same gate F0-FIX2 Bug B
     * used to fix the tour) BEFORE tripping the single-shot guard,
     * and retry extraction inside a 10s poll window since Discord's
     * React fiber may need a moment to populate the user prop after
     * the shell mounts.
     */
    let oslSnowflakeBootstrapDone = false;

    // 9-TD2.3: F0-FIX3 trace helper. Default off; set
    // `window.__OSL_TRACE__ = true` in DevTools (or pre-paint in a
    // userscript) to re-enable the snowflake-registration / login-
    // gate breadcrumbs. Kept narrow to F0-FIX3 sites.
    function oslTrace(msg) {
        if (window.__OSL_TRACE__) {
            console.log(msg);
        }
    }

    /**
     * 9-F0-FIX3: poll `oslExtractDiscordSnowflakeFromRuntime` for up
     * to `maxWaitMs` (default 10s). Discord's React tree may have
     * mounted the panels__ avatar before the user prop on that
     * fiber is populated, so the first extract call can miss even
     * on a logged-in shell. Polling closes that race.
     */
    async function oslExtractSnowflakeRetry(maxWaitMs) {
        const deadline = Date.now() + (maxWaitMs || 10000);
        while (Date.now() < deadline) {
            const sf = oslExtractDiscordSnowflakeFromRuntime();
            if (sf) return sf;
            await new Promise(function (r) {
                window.setTimeout(r, 500);
            });
        }
        return null;
    }

    async function oslEnsureSelfSnowflakeRegistered() {
        if (oslSnowflakeBootstrapDone) return;

        oslTrace("[F0-FIX3-TRACE] snowflake bootstrap entered");

        // 9-F0-FIX3: wait for the logged-in shell BEFORE tripping
        // the single-shot guard. Reuses the same selector pair F0-FIX2
        // Bug B locked the tour on: `nav[class*="guilds_"]` AND
        // `section[class*="panels__"]`, both of which only exist
        // post-login.
        const loggedIn = await oslTourWaitForLoggedIn(30 * 60 * 1000);
        if (!loggedIn) {
            console.warn(
                "[OSL][F0-FIX3] snowflake bootstrap: logged-in shell " +
                    "never detected; deferring to next launch"
            );
            return;
        }

        // Trip the guard now — the wait above could in theory run
        // for ~30min, during which a second caller might enter. The
        // guard prevents racing post-wait invocations.
        oslSnowflakeBootstrapDone = true;
        oslTrace("[F0-FIX3-TRACE] snowflake bootstrap: shell ready, proceeding");

        const r = await oslInvoke("osl_get_self_user_id", {});
        const storedSnowflake =
            r.ok && typeof r.value === "string" && /^\d{17,20}$/.test(r.value)
                ? r.value
                : null;

        // Discord-account-switch detection. Even if the Rust side
        // ALREADY has an identity registered, we must compare it
        // against what Discord runtime is currently reporting — if
        // the user switched Discord accounts on this machine, the
        // stored identity is now wrong for the new account. The
        // mismatch triggers an auto-burn + re-register on the Rust
        // side inside `osl_register_self_snowflake`.
        const sf = await oslExtractSnowflakeRetry(10000);
        oslTrace(
            "[F0-FIX3-TRACE] extracted snowflake=" +
                (sf || "null") +
                " stored=" +
                (storedSnowflake || "null")
        );
        if (storedSnowflake && sf && storedSnowflake !== sf) {
            console.warn(
                "[OSL] Discord account switch detected on client side: " +
                    "stored=" +
                    storedSnowflake +
                    " runtime=" +
                    sf +
                    " — Rust-side osl_register_self_snowflake will " +
                    "auto-burn + re-register under the new snowflake."
            );
        }
        if (!sf) {
            console.warn(
                "[OSL][F0-FIX3] snowflake extraction failed after 10s " +
                    "poll on logged-in shell; identity not generated. " +
                    "User will need to reload Discord to retry."
            );
            return;
        }
        console.log("[OSL] self snowflake extracted from runtime: " + sf);

        const reg = await oslInvoke("osl_register_self_snowflake", {
            snowflake: sf,
        });
        if (reg.ok) {
            oslTrace("[F0-FIX3-TRACE] registration succeeded");
            console.log("[OSL] self snowflake registered from Discord runtime");
            // Reset the cached miss so subsequent
            // oslSelfDiscordId() calls re-fetch.
            oslSelfDiscordIdCache = null;
            oslSelfDiscordIdLastError = null;
        } else {
            oslTrace(
                "[F0-FIX3-TRACE] registration failed: " + (reg.error || "?")
            );
            console.warn("[OSL] self snowflake registration failed:", reg.error);
        }
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
                // force: the scope key is unchanged, so a throttled
                // refresh would skip — but the whitelist/encrypt flags
                // just changed, so the lock must re-read and (likely)
                // go grey.
                oslRefreshHeaderState({ force: true });
                // Repaint the sidebar channel locks too: removing a
                // server's whitelist must drop their green/yellow to
                // grey (they're skipped by inject's dedup, so they need
                // an explicit re-resolve).
                oslChanWlRefreshAll();
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
                    const _burnScope =
                        p.scope_kind && p.scope_id
                            ? {
                                  kind: p.scope_kind,
                                  id: p.scope_id,
                                  server_id: p.server_id || null,
                                  channel_id: p.channel_id,
                              }
                            : null;
                    // Phase 6.4: burn markers ride the keyserver
                    // control-inbox; recipients = either the payload's
                    // own `recipients` field (if the future Rust
                    // emitter populates it) or the current channel
                    // ctx members minus self.
                    let burnRecipients = Array.isArray(p.recipients)
                        ? p.recipients.slice()
                        : null;
                    if (!burnRecipients) {
                        try {
                            const _ctx =
                                typeof oslCurrentChannelContext === "function"
                                    ? oslCurrentChannelContext()
                                    : null;
                            burnRecipients =
                                _ctx && Array.isArray(_ctx.members)
                                    ? _ctx.members
                                    : [];
                        } catch (_) {
                            burnRecipients = [];
                        }
                    }
                    oslSendControlOob(
                        burnRecipients,
                        _burnScope,
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
                oslChanWlRefreshAll();
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
                oslChanWlRefreshAll();
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
        // Beta 1.0 edit safety net: if the submitted content is still
        // the PROSE COVER we recorded for this message (i.e. the
        // plaintext swap into Discord's Slate editor never took, so the
        // user is unknowingly submitting the cover unchanged), pass it
        // through untouched. Re-encrypting the cover-as-plaintext would
        // post garbage and corrupt the message. A no-op edit is the
        // safe failure mode.
        try {
            const knownCover =
                typeof recvCovers !== "undefined" &&
                recvCovers &&
                typeof recvCovers.get === "function"
                    ? recvCovers.get(messageId)
                    : null;
            const knownWire =
                window.__oslProseWireByMsgId &&
                typeof window.__oslProseWireByMsgId.get === "function"
                    ? window.__oslProseWireByMsgId.get(messageId)
                    : null;
            const submitted = parsed.content.trim();
            if (
                (typeof knownCover === "string" &&
                    submitted === knownCover.trim()) ||
                (typeof knownWire === "string" &&
                    submitted === knownWire.trim())
            ) {
                console.log(
                    "[OSL] edit no-op (" +
                        source +
                        "): submitted content equals the cover for msg=" +
                        messageId +
                        " (plaintext swap didn't take); passthrough"
                );
                return Reflect.apply(target, thisArg, args);
            }
        } catch (_) {}
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
                                    runPersistEdit(messageId, origPlaintext, channelId);
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
                runPersistEdit(messageId, origPlaintext, channelId);
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
    // 9-C1 Stage 3: gateway WebSocket tap for channel-roster cache.
    //
    // The tri-state header icon needs the live channel roster so it
    // can compute how many of the channel's members are whitelisted.
    // Discord's gateway dispatches roster changes via JSON frames on
    // the WebSocket; we install a thin proxy on `window.WebSocket`
    // that decodes `op:0` dispatch frames and pushes the resulting
    // member set to Rust via `osl_membership_update`, throttled to
    // one update per channel per 2 s.
    //
    // The proxy is install-once at IIFE head; the message-decoded
    // `osl_membership_update` invoke uses `oslInvoke` which is
    // defined further down — that's fine because the invoke is async
    // and only fires after Tauri is ready (gateway frames arrive
    // many ms after boot.js runs).
    //
    // Throttling: a per-channel `Map<channelId, lastSentEpochMs>`
    // suppresses duplicate pushes within a 2 s window. A trailing
    // update is scheduled via setTimeout so the final state lands.
    //
    // Anti-detect: the proxy is a `new Proxy(origWS, ...)` over the
    // constructor, mirroring the fetch wrapper's shape. `addEvent-
    // Listener('message', h)` intercepts handler registration and
    // wraps the handler to peek at the frame before forwarding.
    // ============================================================
    const origWebSocket = typeof window.WebSocket === "function" ? window.WebSocket : null;
    if (origWebSocket) {
        const MEMBER_UPDATE_THROTTLE_MS = 2000;
        const channelMemberCache = new Map();
        const lastSentEpochMs = new Map();
        const pendingTrail = new Map();

        function oslChannelMembersUpdate(channelId, members) {
            if (!channelId || !Array.isArray(members)) {
                return;
            }
            const uniq = Array.from(new Set(members.filter((m) => typeof m === "string")));
            channelMemberCache.set(channelId, uniq);
            const now = Date.now();
            const last = lastSentEpochMs.get(channelId) || 0;
            const gap = now - last;
            const dispatch = () => {
                lastSentEpochMs.set(channelId, Date.now());
                pendingTrail.delete(channelId);
                try {
                    if (typeof oslInvoke === "function") {
                        oslInvoke("osl_membership_update", {
                            channelId: channelId,
                            memberIds: uniq,
                        });
                    }
                } catch (_) {}
            };
            if (gap >= MEMBER_UPDATE_THROTTLE_MS) {
                dispatch();
            } else if (!pendingTrail.has(channelId)) {
                const delay = MEMBER_UPDATE_THROTTLE_MS - gap;
                const handle = setTimeout(dispatch, delay);
                pendingTrail.set(channelId, handle);
            }
        }

        // Expose the cache so the tri-state icon UX (Stage 4) can
        // read it synchronously for first-paint state before any
        // gateway event fires.
        window.__OSL_CHANNEL_MEMBERS__ = channelMemberCache;
        window.__OSL_MEMBERS_UPDATE__ = oslChannelMembersUpdate;

        // W2b: durable scope-membership feed. Additive — independent
        // of the osl_membership_update path above. Per-scope throttle
        // (same window) so chunk bursts don't hammer the persister.
        // Best-effort: never throws, fire-and-forget; Rust accrues +
        // persists, decision #3 (lurkers heal via auto-recovery).
        const noteScopeLastMs = new Map();
        function oslNoteScope(scopeInput, members) {
            if (!scopeInput || !Array.isArray(members) || members.length === 0) {
                return;
            }
            const key = scopeInput.kind + ":" + scopeInput.id;
            const now = Date.now();
            if (now - (noteScopeLastMs.get(key) || 0) < MEMBER_UPDATE_THROTTLE_MS) {
                return;
            }
            noteScopeLastMs.set(key, now);
            const uniq = Array.from(
                new Set(members.filter((m) => typeof m === "string"))
            );
            if (uniq.length === 0) return;
            try {
                if (typeof oslInvoke === "function") {
                    oslInvoke("osl_note_scope_membership", {
                        scopeInput: scopeInput,
                        memberIds: uniq,
                    });
                }
            } catch (_) {}
        }
        window.__OSL_NOTE_SCOPE__ = oslNoteScope;

        function ingestPrivateChannel(ch) {
            if (!ch || typeof ch.id !== "string" || !Array.isArray(ch.recipients)) {
                return;
            }
            const members = ch.recipients
                .map((r) => (r && typeof r.id === "string" ? r.id : null))
                .filter((s) => !!s);
            oslChannelMembersUpdate(ch.id, members);
            // W2b: a group DM (type 3) is a gc: scope — accrue its
            // members durably. type 1 (1:1 DM) needs no accrual.
            if (ch.type === 3) {
                oslNoteScope(
                    { kind: "gc", id: ch.id, channel_id: ch.id },
                    members
                );
            }
        }

        // Bug B (whitelist repair): caps so __OSL_CHANNEL_MEMBERS__
        // cannot grow unbounded under chunk bursts.
        const OSL_ROSTER_CAPS = {
            maxPerChannel: 2000,
            maxChannels: 500,
            // A single frame carrying more ids than this is treated
            // as pathological and skipped entirely (defensive — never
            // block the passthrough on parsing a giant frame).
            maxIngest: 10000,
        };

        // __OSL_TEST_EXTRACT_START oslIngestRosterFrame
        // Pure, OBSERVE-ONLY roster accumulator. Given an already-
        // parsed gateway frame (t, d), union any member ids it
        // carries into `cache` (channelId -> string[]), keyed to the
        // channels of d.guild_id via `guildCache`. Merges (never
        // clobbers — chunks are partial), caps per-channel + total
        // channels (oldest-channel eviction), skips malformed /
        // oversized frames silently, NEVER throws, and NEVER mutates
        // `d` or anything that is forwarded to Discord. Returns the
        // list of channel ids it touched (for throttled propagation
        // by the caller); returns [] for non-roster / skipped frames.
        function oslIngestRosterFrame(cache, guildCache, t, d, caps) {
            try {
                if (
                    t !== "GUILD_MEMBERS_CHUNK" &&
                    t !== "GUILD_MEMBER_LIST_UPDATE"
                ) {
                    return [];
                }
                if (!d || typeof d.guild_id !== "string") {
                    return [];
                }
                const guildId = d.guild_id;
                const ids = [];
                if (t === "GUILD_MEMBERS_CHUNK") {
                    if (Array.isArray(d.members)) {
                        for (const m of d.members) {
                            const uid =
                                m && m.user && typeof m.user.id === "string"
                                    ? m.user.id
                                    : null;
                            if (uid) ids.push(uid);
                        }
                    }
                } else {
                    // GUILD_MEMBER_LIST_UPDATE: ops[].items[].member
                    // (SYNC/INSERT/UPDATE). `group` items carry no
                    // user — skipped. Defensive at every hop.
                    if (Array.isArray(d.ops)) {
                        for (const op of d.ops) {
                            if (!op) continue;
                            const items = Array.isArray(op.items)
                                ? op.items
                                : op.item
                                  ? [op.item]
                                  : [];
                            for (const it of items) {
                                const uid =
                                    it &&
                                    it.member &&
                                    it.member.user &&
                                    typeof it.member.user.id === "string"
                                        ? it.member.user.id
                                        : null;
                                if (uid) ids.push(uid);
                            }
                        }
                    }
                }
                if (ids.length === 0) return [];
                // Pathological frame — skip whole thing, leave cache
                // intact rather than risk jank.
                if (ids.length > caps.maxIngest) return [];

                const g =
                    guildCache && guildCache[guildId] ? guildCache[guildId] : null;
                const channelIds =
                    g && Array.isArray(g.channel_ids) ? g.channel_ids : [];
                if (channelIds.length === 0) {
                    // We can't map guild members to channels until
                    // GUILD_CREATE has cached this guild's channel
                    // inventory. Drop silently — a later frame (or the
                    // GUILD_CREATE permissive over-set) covers it.
                    return [];
                }

                const touched = [];
                for (const cid of channelIds) {
                    if (typeof cid !== "string") continue;
                    const prior = cache.get(cid);
                    const set = new Set(Array.isArray(prior) ? prior : []);
                    for (const id of ids) set.add(id);
                    let merged = Array.from(set);
                    if (merged.length > caps.maxPerChannel) {
                        merged = merged.slice(0, caps.maxPerChannel);
                    }
                    // Oldest-channel eviction on a NEW key over cap.
                    if (!cache.has(cid) && cache.size >= caps.maxChannels) {
                        const oldest = cache.keys().next().value;
                        if (oldest !== undefined) cache.delete(oldest);
                    }
                    cache.set(cid, merged);
                    touched.push(cid);
                }
                return touched;
            } catch (_) {
                // Never throw out of the observe path.
                return [];
            }
        }
        // __OSL_TEST_EXTRACT_END oslIngestRosterFrame

        function ingestFrame(data) {
            let payload;
            try {
                payload = JSON.parse(data);
            } catch (_) {
                return;
            }
            if (!payload || typeof payload !== "object" || payload.op !== 0) {
                return;
            }
            const t = payload.t;
            const d = payload.d;
            if (!t || !d) {
                return;
            }
            switch (t) {
                case "READY": {
                    // 9-PERF1: gateway READY = Discord is interactive.
                    // Fade the loading splash. No-op if splash wasn't
                    // shown (e.g. user didn't go through unlock this
                    // navigation) or already hidden by timeout/click.
                    try {
                        if (typeof oslHideLoadingSplash === "function") {
                            oslHideLoadingSplash();
                        }
                    } catch (_) {}
                    if (Array.isArray(d.private_channels)) {
                        for (const ch of d.private_channels) {
                            ingestPrivateChannel(ch);
                        }
                    }
                    // 9-C2: capture the friend-id list for the Bulk
                    // Whitelist modal. relationships[*].type === 1
                    // → friend (2=blocked, 3=incoming req, 4=outgoing).
                    if (Array.isArray(d.relationships)) {
                        const friendIds = d.relationships
                            .filter(
                                (r) =>
                                    r &&
                                    r.type === 1 &&
                                    r.user &&
                                    typeof r.user.id === "string"
                            )
                            .map((r) => r.user.id);
                        try {
                            if (typeof oslInvoke === "function") {
                                oslInvoke("osl_set_friend_ids", { ids: friendIds });
                                console.log(
                                    "[OSL] friend list seeded: count=" +
                                        friendIds.length
                                );
                            }
                        } catch (_) {}
                    }
                    break;
                }
                case "CHANNEL_CREATE": {
                    ingestPrivateChannel(d);
                    // 9-C3: server-channel auto-apply. CREATE only —
                    // don't re-apply on UPDATE because the user may
                    // have explicitly toggled the channel off since
                    // it was created.
                    if (
                        d &&
                        typeof d.guild_id === "string" &&
                        typeof d.id === "string"
                    ) {
                        oslC3MaybeAutoApply(d.guild_id, d.id).catch((_) => {});
                    }
                    break;
                }
                case "CHANNEL_UPDATE": {
                    ingestPrivateChannel(d);
                    break;
                }
                case "GUILD_CREATE": {
                    // Seed each guild channel with the full member
                    // roster — a permissive over-set for the icon
                    // (channel overwrites would narrow it but we
                    // don't track those here).
                    const memberIds = Array.isArray(d.members)
                        ? d.members
                              .map((m) =>
                                  m && m.user && typeof m.user.id === "string"
                                      ? m.user.id
                                      : null
                              )
                              .filter((s) => !!s)
                        : [];
                    if (Array.isArray(d.channels)) {
                        for (const ch of d.channels) {
                            if (ch && typeof ch.id === "string") {
                                oslChannelMembersUpdate(ch.id, memberIds);
                                // W2b: text channels (type 0) are
                                // server_channel scopes — durably
                                // accrue the guild roster into each so
                                // the header-whitelist recipient
                                // resolution has a member set.
                                if (ch.type === 0 && typeof d.id === "string") {
                                    oslNoteScope(
                                        {
                                            kind: "server_channel",
                                            id: d.id + ":" + ch.id,
                                            server_id: d.id,
                                            channel_id: ch.id,
                                        },
                                        memberIds
                                    );
                                }
                            }
                        }
                    }
                    // 9-C2: stash guild metadata for the Bulk Whitelist
                    // modal's Server picker. Keep a per-instance cache
                    // keyed by guild id; push the full snapshot to Rust
                    // on each update (simpler than incremental).
                    // 9-C3 added channel_ids to the GuildDto so the
                    // settings "apply to existing channels" flow can
                    // iterate the channel inventory in one round-trip.
                    if (d && typeof d.id === "string") {
                        const channelIds = Array.isArray(d.channels)
                            ? d.channels
                                  .map((c) =>
                                      c && typeof c.id === "string" ? c.id : null
                                  )
                                  .filter((s) => !!s)
                            : [];
                        window.__OSL_GUILD_CACHE__ =
                            window.__OSL_GUILD_CACHE__ || {};
                        window.__OSL_GUILD_CACHE__[d.id] = {
                            id: d.id,
                            name:
                                typeof d.name === "string"
                                    ? d.name
                                    : "(guild " + d.id + ")",
                            member_ids: memberIds,
                            channel_ids: channelIds,
                        };
                        try {
                            if (typeof oslInvoke === "function") {
                                oslInvoke("osl_set_guild_list", {
                                    guilds: Object.values(
                                        window.__OSL_GUILD_CACHE__
                                    ),
                                });
                            }
                        } catch (_) {}
                    }
                    break;
                }
                case "GUILD_MEMBERS_CHUNK":
                case "GUILD_MEMBER_LIST_UPDATE": {
                    // Bug B (whitelist repair): OBSERVE-ONLY guild
                    // roster capture. These fire when the user opens
                    // / scrolls the member list (and on explicit
                    // guild-members requests). Merge the carried ids
                    // into the per-channel cache (union, capped) and
                    // propagate each touched channel through the
                    // existing throttled `oslChannelMembersUpdate`
                    // (which also pushes to Rust). The frame itself
                    // is NOT touched — passthrough is unchanged.
                    const touched = oslIngestRosterFrame(
                        channelMemberCache,
                        window.__OSL_GUILD_CACHE__ || {},
                        t,
                        d,
                        OSL_ROSTER_CAPS
                    );
                    for (const cid of touched) {
                        const ms = channelMemberCache.get(cid) || [];
                        oslChannelMembersUpdate(cid, ms);
                        // W1 fix: GUILD_CREATE.d.members is near-empty
                        // (Discord lazy-loads the roster), so the
                        // durable ScopeMembership store — which
                        // server-header recipient resolution depends
                        // on — was only ever seeded with self. The
                        // REAL roster arrives here, via the lazy
                        // CHUNK / LIST_UPDATE frames; durably accrue it
                        // as a server_channel scope so a header
                        // whitelist actually encrypts to the server's
                        // OSL members (else: sender keys to {self} →
                        // every other user gets "not a recipient").
                        if (d && typeof d.guild_id === "string") {
                            oslNoteScope(
                                {
                                    kind: "server_channel",
                                    id: d.guild_id + ":" + cid,
                                    server_id: d.guild_id,
                                    channel_id: cid,
                                },
                                ms
                            );
                        }
                    }
                    break;
                }
                case "GUILD_MEMBER_ADD":
                case "GUILD_MEMBER_UPDATE": {
                    // Single-member ADD / UPDATE. The CHUNK /
                    // LIST_UPDATE cases above carry the bulk roster;
                    // these fine-grained deltas stay no-ops for adds
                    // (the permissive over-set tolerates minor
                    // staleness until the next list scroll / chunk).
                    break;
                }
                case "GUILD_MEMBER_REMOVE": {
                    // Probe-2 Boot Bug 3: previously no-op'd, so a
                    // user who left a server (or was kicked) stayed
                    // in the durable scope membership forever and
                    // server-header sends kept wrapping SKDMs to a
                    // peer who could no longer receive them. Walk
                    // the guild's channels and re-emit the scope
                    // with the leaver removed; the in-memory roster
                    // cache is also pruned so the icon UI catches up.
                    try {
                        const guildId =
                            d && typeof d.guild_id === "string" ? d.guild_id : null;
                        const leaver =
                            d && d.user && typeof d.user.id === "string"
                                ? d.user.id
                                : null;
                        if (guildId && leaver) {
                            const guildCache =
                                window.__OSL_GUILD_CACHE__ &&
                                window.__OSL_GUILD_CACHE__[guildId];
                            const channelIds =
                                guildCache && Array.isArray(guildCache.channel_ids)
                                    ? guildCache.channel_ids
                                    : [];
                            for (const cid of channelIds) {
                                const prev = channelMemberCache.get(cid);
                                if (Array.isArray(prev)) {
                                    const next = prev.filter((u) => u !== leaver);
                                    if (next.length !== prev.length) {
                                        channelMemberCache.set(cid, next);
                                        oslChannelMembersUpdate(cid, next);
                                        oslNoteScope(
                                            {
                                                kind: "server_channel",
                                                id: guildId + ":" + cid,
                                                server_id: guildId,
                                                channel_id: cid,
                                            },
                                            next
                                        );
                                    }
                                }
                            }
                        }
                    } catch (_) {}
                    break;
                }
                case "CHANNEL_RECIPIENT_ADD":
                case "CHANNEL_RECIPIENT_REMOVE": {
                    // Probe-2 Boot Bug 3: GC roster changes mid-session
                    // were previously invisible — adders/leavers stayed
                    // in the durable membership forever (or never made
                    // it in), so SKDMs were wrapped to the wrong set
                    // and joiners hit "not a recipient" until restart.
                    // We don't get a full updated recipients[] in this
                    // event, only the delta — patch the in-memory
                    // cache and re-emit the gc: scope.
                    try {
                        const channelId =
                            d && typeof d.channel_id === "string"
                                ? d.channel_id
                                : null;
                        const userId =
                            d && d.user && typeof d.user.id === "string"
                                ? d.user.id
                                : null;
                        if (channelId && userId) {
                            let members =
                                channelMemberCache.get(channelId) || [];
                            if (t === "CHANNEL_RECIPIENT_ADD") {
                                if (members.indexOf(userId) === -1) {
                                    members = members.concat([userId]);
                                }
                            } else {
                                members = members.filter((u) => u !== userId);
                            }
                            channelMemberCache.set(channelId, members);
                            oslChannelMembersUpdate(channelId, members);
                            oslNoteScope(
                                {
                                    kind: "gc",
                                    id: channelId,
                                    channel_id: channelId,
                                },
                                members
                            );
                        }
                    } catch (_) {}
                    break;
                }
                case "MESSAGE_CREATE":
                case "MESSAGE_UPDATE": {
                    // Slow-image fix: live (gateway-delivered)
                    // messages never pass through the fetch
                    // interceptor that fills __oslAttachmentUrlCache
                    // (only POST-send / PATCH-edit / GET-history
                    // do). Without a cache entry the receiver's
                    // attachment scan can't find the CDN URL for
                    // Discord's unrenderable encrypted-.mp4 card
                    // (URL lives in React state, not the DOM), so
                    // live decrypt stalled until a restart loaded
                    // the channel via history. Feed the same
                    // canonical attachments[] the history path uses
                    // into the existing cache helper here, so the
                    // first sweep re-scan hits the cache instead of
                    // a futile DOM walk. (The 8eacb87 fiber fallback
                    // stays as the in-DOM safety net.)
                    try {
                        if (
                            d &&
                            typeof d.id === "string" &&
                            Array.isArray(d.attachments) &&
                            d.attachments.length > 0
                        ) {
                            oslCacheAttachmentUrls(d.id, d.attachments);
                        }
                    } catch (_) {}
                    break;
                }
                default:
                    break;
            }
        }

        function wrapMessageHandler(handler) {
            if (typeof handler !== "function") {
                return handler;
            }
            return function (ev) {
                try {
                    if (ev && typeof ev.data === "string") {
                        ingestFrame(ev.data);
                    }
                } catch (_) {}
                return handler.call(this, ev);
            };
        }

        const wsHandler = {
            construct: function (target, args) {
                const inst = Reflect.construct(target, args);
                const origAdd = inst.addEventListener.bind(inst);
                inst.addEventListener = function (type, listener, options) {
                    if (type === "message") {
                        return origAdd(type, wrapMessageHandler(listener), options);
                    }
                    return origAdd(type, listener, options);
                };
                // onmessage setter property: route through the
                // instance's prototype-level setter while wrapping.
                try {
                    let _onmsg = null;
                    Object.defineProperty(inst, "onmessage", {
                        configurable: true,
                        enumerable: true,
                        get: function () {
                            return _onmsg;
                        },
                        set: function (fn) {
                            _onmsg = fn;
                            origAdd("message", wrapMessageHandler(fn));
                        },
                    });
                } catch (_) {}
                return inst;
            },
        };
        try {
            window.WebSocket = new Proxy(origWebSocket, wsHandler);
        } catch (_) {
            // Defensive: if the host blocks Proxy on WebSocket, leave
            // the original untouched — the tri-state icon will render
            // as "unknown" for guild channels until manual refresh.
        }
    }

    // ============================================================
    // 9-C3: per-server "encrypt new channels by default" helpers.
    //
    // The CHANNEL_CREATE hook (above) calls `oslC3MaybeAutoApply` to
    // check whether `server_defaults[guild_id].encrypt_by_default` is
    // on; if so, it flips the new ServerChannel scope's encrypt_toggle.
    // A small TTL cache avoids an IPC round-trip on every CHANNEL_CREATE
    // burst (channel-rename storms during admin bulk edits, etc.).
    // The cache is invalidated by the cross-window `osl:server_default_changed`
    // event emitted from the settings Server Defaults modal.
    // ============================================================
    const OSL_C3_CACHE_TTL_MS = 30_000;
    let oslC3Cache = null; // { ts: number, map: { [server_id]: bool } }

    async function oslC3GetDefaults() {
        const now = Date.now();
        if (oslC3Cache && now - oslC3Cache.ts < OSL_C3_CACHE_TTL_MS) {
            return oslC3Cache.map;
        }
        try {
            const r = await oslInvoke("osl_get_server_defaults", {});
            if (!r.ok) {
                return oslC3Cache ? oslC3Cache.map : {};
            }
            const map = {};
            for (const row of r.value || []) {
                if (row && row.server_id) {
                    map[row.server_id] = !!row.encrypt_by_default;
                }
            }
            oslC3Cache = { ts: now, map: map };
            return map;
        } catch (_) {
            return oslC3Cache ? oslC3Cache.map : {};
        }
    }

    function oslC3InvalidateCache() {
        oslC3Cache = null;
    }

    async function oslC3MaybeAutoApply(guildId, channelId) {
        const map = await oslC3GetDefaults();
        if (!map[guildId]) return;
        try {
            await oslInvoke("osl_set_scope_encrypt", {
                scopeInput: {
                    kind: "server_channel",
                    id: guildId + ":" + channelId,
                    server_id: guildId,
                    channel_id: channelId,
                },
                enabled: true,
            });
            console.log(
                "[OSL] C3 auto-encrypt: new channel " +
                    channelId +
                    " in guild " +
                    guildId
            );
        } catch (e) {
            console.log(
                "[OSL] C3 auto-apply failed for channel=" +
                    channelId +
                    ": " +
                    ((e && e.message) || e)
            );
        }
    }

    // 9-C3: sidebar overlay icon — paints a small lock badge on each
    // guild icon in the left rail, reflecting the server's encrypt-
    // by-default state. Click toggles + (when turning on) retro-
    // applies to existing channels.
    const OSL_C3_SIDEBAR_DATA_ATTR = "data-osl-server-default-icon";

    function oslC3FindGuildSidebar() {
        const candidates = [
            'nav[class*="guilds_"] [class*="scroller_"]',
            'nav[class*="guilds_"]',
            '[class*="guildsList_"]',
            '[class*="guildsList__"]',
        ];
        for (const sel of candidates) {
            try {
                const el = document.querySelector(sel);
                if (el) return el;
            } catch (_) {}
        }
        return null;
    }

    function oslC3GuildIdFromListItem(li) {
        // Strategy A: data-list-item-id="guildsnav___<guild_id>"
        try {
            const v = li.getAttribute && li.getAttribute("data-list-item-id");
            if (typeof v === "string" && v.indexOf("___") !== -1) {
                const parts = v.split("___");
                const id = parts[parts.length - 1];
                if (/^\d{15,21}$/.test(id)) return id;
            }
        } catch (_) {}
        // Strategy B: nested <a href="/channels/<guild_id>/...">
        try {
            const a = li.querySelector('a[href^="/channels/"]');
            if (a) {
                const href = a.getAttribute("href") || "";
                const m = /^\/channels\/(\d{15,21})(?:\/|$)/.exec(href);
                if (m) return m[1];
            }
        } catch (_) {}
        return null;
    }

    function oslC3LockSvg(state) {
        // Small corner-badge SVG. closed = encrypt-by-default ON.
        const closed = state === "closed";
        return (
            '<svg width="14" height="14" viewBox="0 0 24 24" fill="' +
            (closed ? "#23a559" : "#87898c") +
            '" aria-hidden="true">' +
            '<rect x="4" y="11" width="16" height="10" rx="2"/>' +
            (closed
                ? '<path d="M8 11V7a4 4 0 0 1 8 0v4" stroke="' +
                  (closed ? "#23a559" : "#87898c") +
                  '" stroke-width="2" fill="none" stroke-linecap="round"/>'
                : '<path d="M8 11V7a4 4 0 0 1 8 0" stroke="#87898c" stroke-width="2" fill="none" stroke-linecap="round"/>') +
            "</svg>"
        );
    }

    function oslC3PaintOverlay(badge, on) {
        badge.innerHTML = oslC3LockSvg(on ? "closed" : "open");
        badge.title = on
            ? "OSL: encrypt new channels by default — ON (click to disable)"
            : "OSL: encrypt new channels by default — OFF (click to enable)";
        badge.setAttribute("data-osl-c3-state", on ? "on" : "off");
    }

    async function oslC3OnOverlayClick(e, guildId, badge) {
        e.preventDefault();
        e.stopPropagation();
        const currentlyOn =
            badge.getAttribute("data-osl-c3-state") === "on";
        const next = !currentlyOn;
        // Optimistically repaint.
        oslC3PaintOverlay(badge, next);
        const setR = await oslInvoke("osl_set_server_default", {
            serverId: guildId,
            encryptByDefault: next,
        });
        if (!setR.ok) {
            oslC3PaintOverlay(badge, currentlyOn);
            return;
        }
        oslC3InvalidateCache();
    }

    async function oslC3SidebarOverlayInject() {
        const sidebar = oslC3FindGuildSidebar();
        if (!sidebar) return;
        const items = sidebar.querySelectorAll('[class*="listItem_"]');
        if (items.length === 0) return;
        const map = await oslC3GetDefaults();
        for (const li of items) {
            const guildId = oslC3GuildIdFromListItem(li);
            if (!guildId) continue;
            if (li.querySelector("[" + OSL_C3_SIDEBAR_DATA_ATTR + "='1']")) {
                // Already injected — just repaint the state.
                const existing = li.querySelector(
                    "[" + OSL_C3_SIDEBAR_DATA_ATTR + "='1']"
                );
                if (existing) oslC3PaintOverlay(existing, !!map[guildId]);
                continue;
            }
            // Anchor: try to find the guild icon's clickable wrapper
            // so we can place the badge at its bottom-right corner.
            const anchor =
                li.querySelector('[class*="wrapper_"]') ||
                li.querySelector('[class*="blobContainer_"]') ||
                li;
            // Ensure the anchor is positioned so our absolute badge
            // overlays correctly.
            const cs = window.getComputedStyle(anchor);
            if (cs.position === "static") {
                anchor.style.position = "relative";
            }
            const badge = document.createElement("div");
            badge.setAttribute(OSL_C3_SIDEBAR_DATA_ATTR, "1");
            badge.style.cssText =
                "position:absolute;bottom:-2px;right:-2px;width:16px;height:16px;" +
                "background:var(--background-floating, #18191c);border-radius:50%;" +
                "display:flex;align-items:center;justify-content:center;" +
                "cursor:pointer;z-index:2;pointer-events:auto;" +
                "box-shadow:0 0 0 2px var(--background-tertiary, #1e1f22);";
            oslC3PaintOverlay(badge, !!map[guildId]);
            badge.addEventListener("click", (e) =>
                oslC3OnOverlayClick(e, guildId, badge)
            );
            anchor.appendChild(badge);
        }
    }

    // Sidebar observer: re-inject on DOM mutation. Periodic sweep as
    // backup since Discord's React rendering sometimes re-mounts the
    // guild list without triggering childList mutations on the
    // outer scroller.
    try {
        const sidebar0 = oslC3FindGuildSidebar();
        if (sidebar0) {
            const obs = new MutationObserver(() => {
                try {
                    oslC3SidebarOverlayInject();
                } catch (_) {}
            });
            obs.observe(sidebar0, { childList: true, subtree: true });
        }
        // Periodic backstop every 5s.
        nativeSetInterval(() => {
            try {
                oslC3SidebarOverlayInject();
            } catch (_) {}
        }, 5000);
        // Initial pass after Tauri is ready.
        nativeSetTimeout(() => {
            try {
                oslC3SidebarOverlayInject();
            } catch (_) {}
        }, 800);
    } catch (_) {}

    // Cross-window event from settings → invalidate cache + repaint.
    try {
        if (window.__TAURI__ && window.__TAURI__.event) {
            window.__TAURI__.event
                .listen("osl:server_default_changed", () => {
                    oslC3InvalidateCache();
                    oslC3SidebarOverlayInject().catch((_) => {});
                })
                .catch((_) => {});
        }
    } catch (_) {}

    // ============================================================
    // W2b-ii: per-text-channel whitelist button in the channel
    // sidebar. PURELY ADDITIVE — a new isolated injected affordance
    // (its own data attr), mirroring the C3 guild-rail badge system
    // above. Nothing existing is modified. The server-header button
    // (W2b-i) whitelists the whole server and OVERRIDES per-channel;
    // when it's on, this button is shown inert with an explanatory
    // tooltip (matches the locked precedence).
    // ============================================================
    const OSL_CHANWL_ATTR = "data-osl-channel-wl";

    function oslChanWlParseHref(href) {
        if (typeof href !== "string") return null;
        const m = /^\/channels\/(\d{15,21})\/(\d{15,21})/.exec(href);
        if (!m) return null; // excludes /channels/@me/... (non-numeric)
        return { guildId: m[1], channelId: m[2] };
    }

    function oslChanWlScope(ids) {
        return {
            kind: "server_channel",
            id: ids.guildId + ":" + ids.channelId,
            server_id: ids.guildId,
            channel_id: ids.channelId,
        };
    }

    // Lock glyph for the per-channel button. Colors are BAKED per
    // state (no more currentColor): "off" inherited Discord's
    // interactive color, which on an unread/selected channel is blue —
    // that was the "still blue" report. Now:
    //   off / server-overridden-off → grey   (#80848e)
    //   partial (some members WL'd)  → yellow (#f0b132)
    //   on / server-wide (all)       → green  (#23a559)
    function oslChanWlColor(state) {
        if (state === "on" || state === "server") return "#23a559";
        if (state === "partial") return "#f0b132";
        return "#80848e";
    }
    function oslChanWlSvg(state) {
        const c = oslChanWlColor(state);
        return (
            '<svg width="16" height="16" viewBox="0 0 24 24" ' +
            'fill="' + c + '" aria-hidden="true">' +
            '<rect x="4" y="11" width="16" height="10" rx="2"/>' +
            '<path d="M8 11V7a4 4 0 0 1 8 0' + (state === "on" || state === "server" || state === "partial" ? "v4" : "") + '" stroke="' + c + '" ' +
            'stroke-width="2" fill="none" stroke-linecap="round"/>' +
            "</svg>"
        );
    }

    // state: "on" (channel whitelisted, all), "partial" (some members
    // whitelisted), "off" (none), "server" (server-wide whitelist on →
    // per-channel overridden / inert, shown green).
    function oslChanWlPaint(btn, state) {
        btn.innerHTML = oslChanWlSvg(state);
        btn.setAttribute("data-osl-chanwl-state", state);
        if (state === "server") {
            btn.style.opacity = "0.7";
            btn.style.cursor = "not-allowed";
            btn.title =
                "Server-wide whitelist is ON (overrides channels). " +
                "Turn it off from the channel header to whitelist " +
                "individual channels.";
        } else {
            btn.style.opacity = "1";
            btn.style.cursor = "pointer";
            btn.title =
                state === "on"
                    ? "OSL: this channel is whitelisted (click to remove)"
                    : state === "partial"
                      ? "OSL: some members whitelisted (click to whitelist the channel)"
                      : "OSL: whitelist this channel (encrypt to its OSL members)";
        }
    }

    async function oslChanWlRefresh(btn, scopeInput) {
        try {
            const sw = await oslInvoke("osl_get_server_whitelist_state", {
                serverId: scopeInput.server_id,
                channelScopeInput: scopeInput,
            });
            // Server-wide whitelist covers this channel → green/inert.
            if (sw.ok && sw.value && sw.value.server_header) {
                oslChanWlPaint(btn, "server");
                return;
            }
            // Channel-level "whitelist everyone here" flag → green.
            if (sw.ok && sw.value && sw.value.channel) {
                oslChanWlPaint(btn, "on");
                return;
            }
            // Otherwise distinguish some-vs-none from the per-member
            // whitelist summary so a partially-whitelisted channel reads
            // yellow instead of grey.
            const sum = await oslInvoke("osl_get_scope_whitelist_summary", {
                scopeInput,
            });
            if (sum.ok && sum.value) {
                if (sum.value.state === "all") {
                    oslChanWlPaint(btn, "on");
                    return;
                }
                if (sum.value.state === "some") {
                    oslChanWlPaint(btn, "partial");
                    return;
                }
            }
            oslChanWlPaint(btn, "off");
        } catch (_) {
            oslChanWlPaint(btn, "off");
        }
    }

    // Re-resolve every already-injected channel lock (e.g. after a
    // whitelist change from settings or the header). Inject() skips
    // existing buttons via dedup, so without this they'd keep their
    // stale color until the user navigated away and back.
    function oslChanWlRefreshAll() {
        let btns;
        try {
            btns = document.querySelectorAll("[" + OSL_CHANWL_ATTR + "='1']");
        } catch (_) {
            return;
        }
        for (const btn of btns) {
            try {
                const a =
                    (btn.closest && btn.closest('a[href^="/channels/"]')) ||
                    (btn.closest && btn.closest("li") &&
                        btn.closest("li").querySelector('a[href^="/channels/"]'));
                const ids = a && oslChanWlParseHref(a.getAttribute("href") || "");
                if (ids) oslChanWlRefresh(btn, oslChanWlScope(ids));
            } catch (_) {}
        }
    }

    async function oslChanWlOnClick(e, scopeInput, btn) {
        e.preventDefault();
        e.stopPropagation();
        const state = btn.getAttribute("data-osl-chanwl-state");
        if (state === "server") {
            oslToast(
                "Server-wide whitelist is ON — it overrides per-channel. " +
                    "Turn it off (channel header lock) to control " +
                    "channels individually."
            );
            return;
        }
        const next = state !== "on";
        oslChanWlPaint(btn, next ? "on" : "off"); // optimistic
        try {
            const r = await oslInvoke("osl_set_channel_whitelist", {
                scopeInput: scopeInput,
                on: next,
            });
            if (!r.ok) {
                oslToast("OSL: " + r.error);
                await oslChanWlRefresh(btn, scopeInput);
                return;
            }
            oslToast(
                next
                    ? "Channel whitelisted — encrypting to its OSL members."
                    : "Channel whitelist removed."
            );
        } catch (err) {
            oslToast(
                "OSL: " + (err && err.message ? err.message : String(err))
            );
            await oslChanWlRefresh(btn, scopeInput);
        }
    }

    // Locate the channel name slot inside the channel link and
    // return the parent + insert-before reference so our button
    // lands to the LEFT of the channel name "general", visually
    // tucked between the `#` icon and the name. The button lives
    // INSIDE the <a>, so the click handler MUST preventDefault to
    // suppress the SPA router; that guard is wired in
    // `oslChanWlInject` below.
    //
    // Falls back to the legacy "next to Create Invite" placement
    // when the iconContainer isn't found (e.g. forum/voice channels
    // with a different layout). Last resort: as a sibling of <a>,
    // which is the current "underneath the row" position -- only
    // hit when neither anchor works.
    function oslChanWlFindInvitePlacement(a) {
        // REPLACE the `#` channel-type icon with our button so the
        // button sits in the exact slot Discord already laid out
        // for the icon (correct flex sizing, no vertical stacking).
        // We:
        //   1. Find the <svg class="icon__…"> inside the <a>.
        //   2. Walk up to its wrapper div (whatever Discord uses
        //      as the icon container — modern Discord uses a
        //      generic wrapper, older builds use class iconContainer__).
        //   3. Hide that wrapper via display:none + a marker attr
        //      so we can identify-and-skip on the dedupe pass.
        //   4. Insert our button as the wrapper's nextSibling so it
        //      gets the same flex slot.
        // The button lives INSIDE the <a>, so the click handler MUST
        // preventDefault to suppress the SPA router (wired below).
        const svg =
            a.querySelector && a.querySelector("svg[class*='icon__']");
        if (svg && svg.parentElement) {
            // Hide ONLY the svg element itself, and inject our
            // button as its immediate sibling. Earlier versions
            // climbed up to "the first wrapper that is a direct
            // child of <a>", but on modern Discord that ancestor
            // is `iconContainer__` which ALSO wraps the channel
            // name — hiding it killed "general" and let the button
            // expand into the freed flex slot, taking the whole
            // textbox. Hiding just the svg keeps every wrapper
            // intact; the button slots into the icon's exact spot.
            return {
                container: svg.parentElement,
                before: svg.nextSibling,
                styleRef: null,
                hideAfterInsert: svg,
            };
        }
        // Fallback: no SVG icon visible (forum/voice channel, custom
        // layout). Land just before the name/icon-container wrapper.
        const iconContainer =
            a.querySelector && a.querySelector("[class*='iconContainer__']");
        if (iconContainer && iconContainer.parentElement) {
            return {
                container: iconContainer.parentElement,
                before: iconContainer,
                styleRef: null,
            };
        }
        const row =
            (a.closest && a.closest("li")) ||
            (a.closest && a.closest('[class*="containerDefault"]')) ||
            a.parentElement;
        if (!row) return null;
        let invite = null;
        let labelled;
        try {
            labelled = row.querySelectorAll("[aria-label]");
        } catch (_) {
            return null;
        }
        for (const c of labelled) {
            const lbl = (c.getAttribute("aria-label") || "").toLowerCase();
            if (lbl.indexOf("invite") !== -1 && !a.contains(c)) {
                invite = c;
                break;
            }
        }
        if (!invite) {
            if (!a.parentElement) return null;
            return {
                container: a.parentElement,
                before: a.nextSibling,
                styleRef: null,
            };
        }
        let node = invite;
        while (
            node.parentElement &&
            !node.parentElement.contains(a) &&
            node.parentElement !== row
        ) {
            node = node.parentElement;
        }
        const container = node.parentElement;
        if (!container || container.contains(a) === false) {
            return {
                container: invite.parentElement,
                before: invite,
                styleRef: invite,
            };
        }
        return { container: container, before: node, styleRef: node };
    }

    // Document-level capture handler. Capture phase walks
    // document → ancestors → target, so a listener attached HERE
    // fires before any Discord listener attached to <a> or row li.
    // That ordering is what lets our `stopImmediatePropagation`
    // suppress the SPA router entirely when the button is nested
    // inside the channel link. Per-button listeners alone aren't
    // sufficient because Discord's capture listeners on <a> fire
    // BEFORE per-button capture listeners (closer-to-root wins
    // in capture phase).
    if (!window.__oslChanWlGlobalGuardInstalled) {
        window.__oslChanWlGlobalGuardInstalled = true;
        const globalGuard = function (ev) {
            const t = ev.target;
            const btn =
                t && t.closest
                    ? t.closest("[" + OSL_CHANWL_ATTR + "='1']")
                    : null;
            if (!btn) return;
            try { ev.preventDefault(); } catch (_) {}
            try { ev.stopImmediatePropagation(); } catch (_) {}
            if (ev.type !== "click") return;
            const a = btn.closest && btn.closest("a[href^='/channels/']");
            if (!a) return;
            const ids = oslChanWlParseHref(
                a.getAttribute("href") || ""
            );
            if (!ids) return;
            const scopeInput = oslChanWlScope(ids);
            try {
                oslChanWlOnClick(ev, scopeInput, btn);
            } catch (err) {
                console.error("[OSL] chanwl click handler threw", err);
            }
        };
        for (const evt of [
            "click",
            "pointerdown",
            "mousedown",
            "mouseup",
            "auxclick",
            "contextmenu",
        ]) {
            document.addEventListener(evt, globalGuard, true);
        }
    }

    // Remove OSL channel-WL buttons that ended up OUTSIDE any
    // channel <a> (legacy placement). Idempotent. Called on every
    // inject pass so a stale orphan from a prior build can't sit
    // around eating clicks or visual space.
    function oslChanWlCleanupOrphans() {
        try {
            const all = document.querySelectorAll(
                "[" + OSL_CHANWL_ATTR + "='1']"
            );
            for (const btn of all) {
                const a =
                    btn.closest &&
                    btn.closest("a[href^='/channels/']");
                if (!a) {
                    try { btn.remove(); } catch (_) {}
                }
            }
        } catch (_) {}
    }

    function oslChanWlInject() {
        oslChanWlCleanupOrphans();
        let links;
        try {
            links = document.querySelectorAll('a[href^="/channels/"]');
        } catch (_) {
            return;
        }
        for (const a of links) {
            const ids = oslChanWlParseHref(a.getAttribute("href") || "");
            if (!ids) continue;
            // Dedup BEFORE the placement work so we don't repeatedly
            // hide-and-show the # icon on every scan tick. Check the
            // entire <a> + its row (covers historical placements
            // OUTSIDE the <a> from earlier builds — would otherwise
            // double-inject).
            const row =
                (a.closest && a.closest("li")) || a.parentElement || a;
            if (
                row.querySelector("[" + OSL_CHANWL_ATTR + "='1']")
            ) {
                continue;
            }
            const place = oslChanWlFindInvitePlacement(a);
            if (!place || !place.container) continue;
            const scopeInput = oslChanWlScope(ids);
            const btn = document.createElement("div");
            btn.setAttribute(OSL_CHANWL_ATTR, "1");
            btn.setAttribute("role", "button");
            btn.setAttribute("aria-label", "OSL channel whitelist");
            // Mimic the invite action item exactly (sizing, layout,
            // color, hover transition) by reusing its Discord class.
            // styleRef is null in the no-invite fallback — then our
            // own inline styles + currentColor lock carry it.
            try {
                if (place.styleRef && place.styleRef.className) {
                    btn.className = place.styleRef.className;
                }
            } catch (_) {}
            btn.style.display = "inline-flex";
            btn.style.alignItems = "center";
            btn.style.justifyContent = "center";
            btn.style.cursor = "pointer";
            // Size the button. When we're replacing the # icon
            // slot, mirror its measured width/height EXACTLY so the
            // channel name doesn't shift. When there's no styleRef
            // class AND no measurable icon (fallback paths), use
            // 16px as a sane default.
            if (place.hideAfterInsert) {
                const w = place.hideAfterInsert.offsetWidth || 16;
                const h = place.hideAfterInsert.offsetHeight || 16;
                btn.style.width = w + "px";
                btn.style.height = h + "px";
                btn.style.flexShrink = "0";
                // Nudge the lock glyph down a touch so it sits on
                // the channel-name baseline instead of riding above it.
                btn.style.transform = "translateY(3px)";
            } else if (!place.styleRef) {
                btn.style.width = "16px";
                btn.style.height = "16px";
                btn.style.flexShrink = "0";
                btn.style.marginRight = "4px";
            }
            oslChanWlPaint(btn, "off");
            // Inside-<a> click guard. Discord wires both
            // `pointerdown` (focus/navigation prep) and the standard
            // `click` (router transition); Chromium fires a synthetic
            // `mouseup` between them. Suppressing all of those, in
            // capture phase, plus preventDefault on every step is
            // what reliably stops the row from navigating when the
            // button lives inside the link.
            //
            // `oslChanWlOnClick` does NOT prevent/stop again — that
            // would no-op here but matter if the function is called
            // from a non-button context (none currently).
            const swallow = function (ev) {
                try { ev.preventDefault(); } catch (_) {}
                try { ev.stopImmediatePropagation(); } catch (_) {}
            };
            btn.addEventListener(
                "click",
                (ev) => {
                    swallow(ev);
                    oslChanWlOnClick(ev, scopeInput, btn);
                },
                true
            );
            btn.addEventListener("pointerdown", swallow, true);
            btn.addEventListener("mousedown", swallow, true);
            btn.addEventListener("mouseup", swallow, true);
            btn.addEventListener("auxclick", swallow, true);
            // Right-click / middle-click on Discord channel rows
            // sometimes also pops the channel context menu; absorb.
            btn.addEventListener("contextmenu", swallow, true);
            try {
                place.container.insertBefore(btn, place.before);
            } catch (_) {
                continue;
            }
            // Hide the # icon AFTER inserting so the measurement
            // above could read the natural dimensions. Mark with an
            // attr in case future code wants to identify our hide.
            if (place.hideAfterInsert) {
                try {
                    place.hideAfterInsert.style.display = "none";
                    place.hideAfterInsert.setAttribute(
                        "data-osl-chanwl-hid-icon",
                        "1"
                    );
                } catch (_) {}
            }
            // Resolve real state asynchronously (fire-and-forget).
            oslChanWlRefresh(btn, scopeInput);
        }
    }

    try {
        // Periodic backstop + a body observer best-effort. Lowered the
        // backstop 5000 -> 1200ms: at 5s, switching channels left the
        // new row showing Discord's bare # for up to five seconds (the
        // "gotta click 3-4 times before the hashtags switch" report).
        // 1.2s is a snappy backstop; the observer still catches most
        // renders sooner.
        nativeSetInterval(() => {
            try {
                oslChanWlInject();
            } catch (_) {}
        }, 1200);
        nativeSetTimeout(() => {
            try {
                oslChanWlInject();
            } catch (_) {}
        }, 1200);
        try {
            const obs = new MutationObserver(() => {
                try {
                    oslChanWlInject();
                } catch (_) {}
            });
            obs.observe(document.body, { childList: true, subtree: true });
        } catch (_) {}
    } catch (_) {}

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
                            oslMaybeLogTokenChange(auth);
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
                            oslMaybeLogTokenChange(auth);
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
                    return oslCaptureMessageApiResponse(
                        handleFetchEdit(
                            target,
                            thisArg,
                            args,
                            input,
                            init,
                            editMatch[1],
                            editMatch[2]
                        ),
                        "PATCH"
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
                // 8e-FIX3: GET /messages (history load) is read-only
                // from our side. We clone the response to populate
                // the attachment URL cache so the scanner can find
                // .mp4 URLs that Discord's DOM doesn't expose.
                if (sendMatch && method === "GET") {
                    return oslCaptureMessageApiResponse(
                        Reflect.apply(target, thisArg, args),
                        "GET"
                    );
                }
                if (!sendMatch || method !== "POST") {
                    return Reflect.apply(target, thisArg, args);
                }
                const channelId = sendMatch[1];

                const initBody = init && init.body;

                if (typeof initBody === "string") {
                    return oslCaptureMessageApiResponse(
                        interceptBody(
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
                        ),
                        "POST"
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
                    return oslCaptureMessageApiResponse(
                        Reflect.apply(target, thisArg, args),
                        "POST"
                    );
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
                    return oslCaptureMessageApiResponse(
                        cloned.text().then(
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
                        ),
                        "POST"
                    );
                }

                return oslCaptureMessageApiResponse(
                    Reflect.apply(target, thisArg, args),
                    "POST"
                );
            },
        };
    }

    function makeOpenHandler() {
        return {
            get: makeToStringGetTrap("function open() { [native code] }"),

            apply: function (target, thisArg, args) {
                // args = [method, url, async?, user?, password?]
                let method = "GET";
                let url = "";
                try {
                    method =
                        typeof args[0] === "string"
                            ? args[0].toUpperCase()
                            : "GET";
                    url =
                        typeof args[1] === "string"
                            ? args[1]
                            : args[1] == null
                              ? ""
                              : String(args[1]);
                    thisArg[OSL_XHR_META] = {
                        method: method,
                        url: url,
                        async: args[2] !== false,
                    };
                } catch (e) {
                    console.error(
                        "[OSL] failed to stash XHR meta on open(); passthrough",
                        e
                    );
                }

                // 8e-FIX4: Discord uses XHR (not fetch) for outgoing
                // /messages traffic. Without this hook the fetch-side
                // capture (8e-FIX3) never fires for sends. Attach a
                // load listener now in open() so the URL/method are
                // already known by the time send() runs.
                try {
                    if (
                        MSG_API_RE.test(url) &&
                        (method === "POST" ||
                            method === "PATCH" ||
                            method === "GET")
                    ) {
                        thisArg.addEventListener("load", function () {
                            try {
                                let data = null;
                                // responseType "json" exposes a
                                // pre-parsed object on .response.
                                if (
                                    thisArg.responseType === "json" &&
                                    thisArg.response != null
                                ) {
                                    data = thisArg.response;
                                } else {
                                    const text = thisArg.responseText;
                                    if (
                                        typeof text === "string" &&
                                        text.length > 0
                                    ) {
                                        data = JSON.parse(text);
                                    }
                                }
                                if (data != null) {
                                    oslMaybeCacheFromApiResponse(
                                        method,
                                        data,
                                        "XHR"
                                    );
                                }
                            } catch (err) {
                                console.log(
                                    "[OSL] msg api xhr response parse failed: " +
                                        (err && err.message
                                            ? err.message
                                            : String(err))
                                );
                            }
                        });
                    }
                } catch (e) {
                    // addEventListener failure on a non-standard XHR
                    // shim — skip silently, the fetch path covers most
                    // history loads as a backstop.
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
                        oslMaybeLogTokenChange(args[1]);
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
                        // Item 1: Discord echoes the created
                        // message's `content` (the DPC0:: wire we
                        // sent). Map the now-known message id to the
                        // send-time plaintext so recvDispatchDecrypt
                        // renders it instead of failing decrypt.
                        if (
                            typeof parsed.content === "string" &&
                            oslSentWireToPlaintext.has(parsed.content)
                        ) {
                            const pt = oslSentWireToPlaintext.get(
                                parsed.content
                            );
                            selfSentPlaintext.set(parsed.id, pt);
                            oslFifoEvict(
                                selfSentPlaintext,
                                OSL_SELF_SENT_PLAINTEXT_MAX
                            );
                            console.log(
                                "[OSL] selfSent plaintext mapped msg=" +
                                    parsed.id +
                                    " len=" +
                                    pt.length
                            );
                            // Probe-2 fix: persist outbound to disk so
                            // own messages survive close+reopen. Without
                            // this the recv-side decrypt on next session
                            // sees v=4 wire and returns "not a recipient"
                            // (correct — own outbound is encrypted only
                            // to peer's key) → ciphertext renders. The
                            // companion `osl_persist_outbound` IPC is
                            // best-effort and never blocks the send.
                            try {
                                window.__TAURI__.core
                                    .invoke("osl_persist_outbound", {
                                        channelId: channelId,
                                        discordMessageId: parsed.id,
                                        plaintext: pt,
                                    })
                                    .catch(function (e) {
                                        console.log(
                                            "[OSL] persist_outbound failed (non-fatal) msg=" +
                                                parsed.id +
                                                ": " +
                                                (e && e.message ? e.message : e)
                                        );
                                    });
                            } catch (_) {}
                            // Fix A: the bounced-back own message
                            // almost always reaches recvDispatchDecrypt
                            // BEFORE this REST `load` populates
                            // selfSentPlaintext, so the first decrypt
                            // fails (v=4 "not a recipient") and the
                            // .catch marks it recvDone — terminal. The
                            // self-view short-circuit then never runs
                            // again (every re-dispatch path is gated on
                            // !recvDone). Now that the plaintext is
                            // known, clear the terminal/retry gates and
                            // render immediately if the div is mounted;
                            // otherwise the cleared gates let the next
                            // sweep/observer tick re-dispatch and hit
                            // the short-circuit.
                            try {
                                recvDone.delete(parsed.id);
                                recvInFlight.delete(parsed.id);
                                recvRetries.delete(parsed.id);
                                recvAuthorRetryCount.delete(parsed.id);
                                const liveDiv = document.getElementById(
                                    RECV_MESSAGE_ID_PREFIX + parsed.id
                                );
                                if (liveDiv) {
                                    recvApplyPlaintext(liveDiv, pt);
                                    recvPlaintext.set(parsed.id, pt);
                                    recvDone.add(parsed.id);
                                    console.log(
                                        "[OSL] self-view render msg=" +
                                            parsed.id +
                                            " (post-load, send-time plaintext)"
                                    );
                                }
                            } catch (_) {}
                        }
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
    // Phase 9-B1: Mode 1 cover messages start with DPC1::. Receive
    // observer checks both prefixes via oslMessageIsStego().
    const RECV_PREFIX_MODE1 = "DPC1::";
    function oslMessageIsStego(text) {
        if (typeof text !== "string") return false;
        return (
            text.indexOf(RECV_PREFIX) === 0 ||
            text.indexOf(RECV_PREFIX_MODE1) === 0
        );
    }
    // Probe-5 v5: permissive "looks-like cipher anywhere in text"
    // check used by the auto-hide path. Discord can prepend
    // textContent with reply-quote or accessibility chrome, pushing
    // the DPC0:: marker away from index 0; those messages still
    // need to be hidden visually. The dispatch path uses the strict
    // prefix check above because decrypt requires a clean cipher
    // wire string at index 0.
    function oslMessageContainsStego(text) {
        if (typeof text !== "string") return false;
        return (
            text.indexOf(RECV_PREFIX) !== -1 ||
            text.indexOf(RECV_PREFIX_MODE1) !== -1
        );
    }
    // Single source of truth for peeking the wire-format version
    // byte out of a DPC0:: cover. The version is the first decoded
    // byte of the base64 payload that follows the 6-char "DPC0::"
    // prefix (slice(6,10) is one base64 quantum -> 3 bytes; byte 0
    // is the version). Returns -1 for non-DPC0:: text, DPC1:: Mode 1
    // covers, or any decode failure. The wire format must be decoded
    // in exactly ONE place — both the recv-result logger and the
    // SKDM-revive v=5 filter call this rather than re-rolling atob().
    function oslCoverWireVersion(text) {
        if (typeof text !== "string") return -1;
        if (text.indexOf(RECV_PREFIX) !== 0) return -1;
        try {
            return atob(text.slice(6, 10)).charCodeAt(0);
        } catch (_) {
            return -1;
        }
    }
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
    // Probe-3 fix: persistent set of message ids known to be v=3
    // SKDM bundle wires. The bundle is a control message with no
    // user-visible content; once apply_skdm_recv runs we hide it,
    // but Discord re-mounts the <li>/divs fresh when you leave +
    // re-enter a channel, so we have to re-hide on every sweep
    // tick. This set persists for the session; entries are added
    // in the SKDM_APPLIED handler.
    const oslSkdmHiddenMsgIds = new Set();

    // Probe-5 fix: hide the message-content div AND walk up the DOM
    // collapsing each ancestor that contains NO avatar/header
    // (i.e., is purely a wrapper around this message body), stopping
    // at the <li> or at the first ancestor that DOES hold an avatar
    // or header. Result: empty "bar" from prior fix disappears
    // (Discord's per-message padding/margin wrapper collapses too)
    // without nuking the avatar/header for following same-author
    // messages in the group.
    //
    // Defensively undo any leftover `<li>` display:none from earlier
    // fix passes so users carrying stale style state recover their
    // avatars.
    function oslHideSkdmDom(messageId) {
        const skdmDivs = document.querySelectorAll(
            "[id='" + RECV_MESSAGE_ID_PREFIX + messageId + "']"
        );
        for (const d of skdmDivs) {
            d.style.display = "none";
            d.setAttribute("data-osl-skdm-hidden", "1");
            // Walk up: collapse each ancestor that has no avatar/
            // header inside (i.e., it's a pure body wrapper for this
            // specific message). Stop at the <li> (don't touch it --
            // it may be the group leader holding the avatar) or at
            // the first ancestor that DOES contain an avatar/header
            // (those are the group-leader chrome and must stay).
            try {
                let p = d.parentElement;
                while (p && p !== document.body) {
                    // Probe-5 fix: same chrome-check tightening as
                    // oslAutoHideCiphertext (avatar img only, not
                    // h3/header/username which false-positive on
                    // Discord's compact-message wrapper classes).
                    // Also walk through the <li> itself when it
                    // contains no avatar, so the SKDM row collapses
                    // completely instead of leaving a bar.
                    const hasGroupChrome =
                        p.querySelector("img[class*='avatar']") ||
                        p.querySelector(
                            "[id^='" + RECV_MESSAGE_ID_PREFIX + "']:not([data-osl-skdm-hidden='1'])"
                        );
                    if (hasGroupChrome) break;
                    p.style.display = "none";
                    p.setAttribute("data-osl-skdm-hidden-wrap", "1");
                    if (p.tagName === "LI") break;
                    p = p.parentElement;
                }
            } catch (_) {}
            // Defensively undo any leftover <li> display:none.
            const li =
                typeof d.closest === "function"
                    ? d.closest("li[id^='chat-messages-']")
                    : null;
            if (
                li &&
                li.getAttribute("data-osl-skdm-hidden") === "1"
            ) {
                li.style.removeProperty("display");
                li.removeAttribute("data-osl-skdm-hidden");
            }
        }
    }
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
    // Auto-recovery (4/4): JS-side cooldown so a stuck message can't
    // re-fire a recovery request on every 1s sweep. This is only an
    // invoke-spam damper — the ipc layer's RecoveryGuard is the hard
    // throttle/replay/act-on-symptom guarantee. Keyed
    // `kind|peer|scopeKey` → last-attempt epoch ms.
    const recvRecoveryCooldown = new Map();
    // Lowered 120000 -> 30000 to match the Rust recovery throttle so a
    // desynced DM re-syncs within ~30s instead of being stuck for two
    // minutes. Armed only on a CONFIRMED successful send, so failed
    // delivery doesn't waste the window.
    const RECV_RECOVERY_COOLDOWN_MS = 30000;
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
    // 9-B3: throttled visibility log for token rotations so we can
    // observe Discord's refresh cadence in real use without flooding
    // the console (Discord re-sends the Authorization header on
    // every heartbeat / presence / typing call, which is constant).
    let _lastTokenLogAt = 0;
    let _lastTokenLogged = null;
    function oslMaybeLogTokenChange(newToken) {
        if (typeof newToken !== "string" || newToken.length === 0) return;
        if (newToken === _lastTokenLogged) return;
        _lastTokenLogged = newToken;
        const now = Date.now();
        if (now - _lastTokenLogAt < 60_000) return;
        _lastTokenLogAt = now;
        // Log only the first 8 chars; the full token is a session
        // credential and should never reach a log line in full.
        console.log(
            "[OSL] token refreshed: prefix=" + newToken.slice(0, 8)
        );
    }
    // ============================================================
    // Phase 9-B3: retry-on-stale-token wrapper around fetch().
    //
    // The fetch + XHR proxies (~lines 7497-7539 and 7807-7830) sniff
    // every outgoing Authorization header from Discord's own client
    // and keep `editOverlayAuthToken` current — but there's a race:
    // when Discord rotates the token, an OSL-issued fetch already in
    // flight (or composed against the prior value) returns 401. The
    // B1 Mode 1 multi-message pipeline makes this race materially
    // more likely because one rotation during a 12-16-chunk send
    // aborts the entire pipeline.
    //
    // This wrapper handles exactly the 401-stale-token case: one
    // retry after a 500ms wait (long enough for Discord's next
    // heartbeat to refresh our sniffed cache), rebuilding the
    // Authorization header from whatever editOverlayAuthToken now
    // holds. Any other failure (403, 404, 5xx, network) returns
    // immediately — those aren't stale-token problems.
    //
    // Token-staleness is the only failure mode we retry. We do NOT
    // retry on network errors, transient 5xx, or Discord rate
    // limits — those each have their own characteristics and need
    // separate handling if they ever become a problem.
    // ============================================================
    async function oslFetchWithTokenRetry(url, init) {
        const firstResp = await fetch(url, init);
        if (!firstResp || firstResp.status !== 401) {
            return firstResp;
        }
        console.log(
            "[OSL] token retry: url=" + url +
                " status=401, awaiting refresh"
        );
        // 500ms is one Discord heartbeat cycle in the typical case.
        // The fetch + XHR proxies will sniff the next outbound
        // Authorization header from Discord's own client and update
        // editOverlayAuthToken inside this window.
        await new Promise(function (resolve) { setTimeout(resolve, 500); });

        // Rebuild headers with the (hopefully) refreshed token.
        // init.headers may be a plain object or a Headers instance.
        const retryInit = Object.assign({}, init || {});
        const freshHeaders = {};
        const src = init && init.headers ? init.headers : {};
        if (typeof src.forEach === "function") {
            // Headers instance.
            src.forEach(function (value, key) { freshHeaders[key] = value; });
        } else {
            for (const k in src) {
                if (Object.prototype.hasOwnProperty.call(src, k)) {
                    freshHeaders[k] = src[k];
                }
            }
        }
        // Overwrite Authorization specifically — leave Content-Type
        // and any other caller-supplied headers untouched.
        if (editOverlayAuthToken) {
            freshHeaders["Authorization"] = editOverlayAuthToken;
        }
        retryInit.headers = freshHeaders;

        const secondResp = await fetch(url, retryInit);
        if (secondResp && secondResp.status === 401) {
            console.log(
                "[OSL] token retry failed: url=" + url +
                    " still 401 after refresh"
            );
        } else {
            console.log(
                "[OSL] token retry: url=" + url +
                    " recovered status=" +
                    (secondResp ? secondResp.status : "?")
            );
        }
        return secondResp;
    }

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

    // Phase 6.4: control-inbox drain cadence. SKDMs, burn markers,
    // and recovery wires ride the keyserver inbox instead of
    // Discord channels; this is how often we poll for new items
    // addressed to this user. 10s is the locked default: low enough
    // that a typical send→install→re-decrypt round-trip feels
    // instant, high enough to keep keyserver load bounded across
    // the user base.
    const CONTROL_INBOX_POLL_INTERVAL_MS = 10000;

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

    // Item 1 (self-view): the local user's own encrypted v=4 DM
    // bounces back as a DPC0:: wire string that the decrypt path
    // cannot open (v=4 wraps ONLY to the peer's slot — "not a
    // recipient" is expected for self). Carry the send-time
    // plaintext to the render path instead of decrypting.
    //
    //   oslSentWireToPlaintext: wire string -> plaintext, populated
    //     in interceptBody at the moment the cover is produced (we
    //     have both there). Keyed by wire because the Discord
    //     message id doesn't exist until the POST response.
    //   selfSentPlaintext: Discord message id -> plaintext,
    //     populated in the /messages XHR `load` listener by matching
    //     the echoed `content` against oslSentWireToPlaintext.
    //
    // Both FIFO-evicted like selfSentAuthors (insertion order).
    const oslSentWireToPlaintext = new Map();
    const selfSentPlaintext = new Map();
    const OSL_SELF_SENT_PLAINTEXT_MAX = 500;
    function oslFifoEvict(map, max) {
        while (map.size > max) {
            const oldest = map.keys().next().value;
            if (oldest === undefined) break;
            map.delete(oldest);
        }
    }

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
        // Drop the cipher-hidden marker + the stashed cipher text so
        // a subsequent observer firing doesn't re-mark this div as
        // encrypted. The [ENCRYPTED] span we previously inserted is
        // already gone (replaceChildren above overwrote it).
        try {
            div.removeAttribute("data-osl-cipher-hidden");
            div.removeAttribute("data-osl-cipher-text");
        } catch (_) {}
        // Invalidate cached "marked" decision so the next sweep
        // tick treats this msgId as shown. Same persistence path
        // as the cache itself.
        try {
            let msgId = null;
            if (typeof div.id === "string") {
                msgId = div.id.replace(RECV_MESSAGE_ID_PREFIX, "");
            }
            if (typeof msgId === "string" && msgId.length > 0) {
                oslInvalidateBlankDecision(msgId);
            }
        } catch (_) {}
    }

    /**
     * Probe-5 fix per user request: hide DPC0:: ciphertext visually
     * the moment we observe it in a message-content div, BEFORE the
     * IPC decrypt roundtrip resolves. The textContent stays intact
     * (so oslMessageIsStego / the sweep / recvExtractChannelId logic
     * keeps working), only the visual rendering collapses to zero.
     *
     * On successful decrypt -> recvApplyPlaintext undoes the CSS.
     * On SKDM bundle -> oslHideSkdmDom collapses the whole bubble.
     * On decryption failure -> div stays visually empty (better than
     *   leaving a DPC0:: blob; the user sees an empty message bubble
     *   they can identify by its empty footprint and surrounding
     *   author/timestamp chrome).
     */
    // Cipher rows are no longer hidden -- they're text-replaced
    // with a `[ENCRYPTED]` marker by oslAutoHideCiphertext, which
    // keeps row layout stable and avoids the virtualised-scroller
    // snap-back. SKDM keysharing payloads are still hidden the old
    // way (they're not user-visible content; full hide is correct
    // for them).
    if (!window.__oslAutoHideStyleInstalled) {
        try {
            const _s = document.createElement("style");
            _s.textContent =
                "li[id^='chat-messages-']:has([data-osl-skdm-hidden='1']){display:none !important;}";
            (document.head || document.documentElement).appendChild(_s);
            window.__oslAutoHideStyleInstalled = true;
        } catch (_) {}
    }
    // Replace DPC0::/DPC1:: cipher content with a blank placeholder.
    // Critical property: the row keeps a normal text-line height --
    // layout never shrinks, which means Discord's virtualised
    // scroller doesn't get confused (no scroll snap-back, no
    // "view creeps up" symptom). The original cipher is stashed
    // on `data-osl-cipher-text` so any code that needs to re-read
    // the wire (re-dispatch, debug) can still find it.
    //
    // Phase 6.4 cleanup: marker text was removed entirely. The
    // post-6.4 transport keeps control wires off Discord, so the
    // only thing that hits this path is content waiting on a
    // sender-key — which usually resolves within ~1 tick. A
    // visible "[Encryption - Ignore]" string was just noise.
    // ` ` (non-breaking space) preserves the row's line
    // height without rendering any visible glyph.
    //
    // Idempotent via `data-osl-cipher-hidden=1`. recvApplyPlaintext
    // clears that attribute (along with `data-osl-cipher-text` and
    // the inline marker styling) when real plaintext lands.
    function oslAutoHideCiphertext(div) {
        // Beta 1.0: no longer blanks undecrypted messages. We keep
        // stashing the original wire on `data-osl-cipher-text` (re-
        // dispatch / burn-restore / debug read it) but leave the
        // visible children alone, so an undecrypted message shows its
        // cover prose instead of a blank that hid the user's own sends.
        if (!div) return;
        try {
            if (!div.hasAttribute("data-osl-cipher-text")) {
                div.setAttribute(
                    "data-osl-cipher-text",
                    div.textContent || ""
                );
            }
        } catch (_) {}
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

        // React-fiber fallback. On Discord build 545032 every DOM
        // source above can return null (data-author-id is no longer
        // emitted, the avatar img is virtualized away, and the cozy
        // group has no id-bearing sibling). The message row's React
        // fiber still carries the author, so walk the fiber `.return`
        // chain the same build-resilient way oslCurrentChannelContext
        // does (commit 29249df). Used ONLY after every DOM source
        // failed, before the dispatch retries exhaust.
        const viaFiber = recvAuthorIdViaFiber(el, root);
        if (viaFiber) return viaFiber;

        return null;
    }

    /**
     * Build-resilient last-resort author-id resolver: walk the React
     * fiber tree upward from the message element looking for the
     * author/user id Discord threads through message-row props.
     * Mirrors the fiber approach in oslResolveChannelViaFiber /
     * oslExtractUserFromProfile (verified live on build 545032).
     *
     * Anchors tried in order: the message-content element passed in
     * (closest to the message model), then the resolved list-item.
     * Prop shapes checked per fiber, in order of specificity:
     *   message.author.id  →  author.id  →  user.id  →  userId
     *
     * No self-guard: parity with the DOM sources above, which return
     * whatever author id is present (own-sent is already handled by
     * the selfSentAuthors cache at the top of recvExtractAuthorId).
     * Defensive: never throws; returns a snowflake string or null.
     */
    function recvAuthorIdViaFiber(el, root) {
        const isSnowflake = function (s) {
            return typeof s === "string" && /^\d{15,22}$/.test(s);
        };
        const anchors = [el, root];
        for (let a = 0; a < anchors.length; a++) {
            const node = anchors[a];
            if (!node) continue;
            let key = null;
            try {
                key = Object.keys(node).find(function (k) {
                    return k.indexOf("__reactFiber") === 0;
                });
            } catch (_) {
                key = null;
            }
            if (!key) continue;
            let f = node[key];
            for (let depth = 0; f && depth < 40; depth++) {
                try {
                    const p = f.memoizedProps;
                    if (p && typeof p === "object") {
                        const cand =
                            (p.message &&
                                p.message.author &&
                                typeof p.message.author.id === "string" &&
                                p.message.author.id) ||
                            (p.author &&
                                typeof p.author.id === "string" &&
                                p.author.id) ||
                            (p.user &&
                                typeof p.user.id === "string" &&
                                p.user.id) ||
                            (typeof p.userId === "string" && p.userId) ||
                            null;
                        if (isSnowflake(cand)) return cand;
                    }
                } catch (_) {
                    // keep walking
                }
                f = f.return;
            }
        }
        return null;
    }

    /**
     * Fix B: React-fiber fallback for attachment URLs. On the
     * receive side an encrypted upload is an unrenderable .mp4;
     * Discord's failed-media/file card often keeps the CDN URL only
     * in React state and never emits it to a DOM attribute, so the
     * scanner's attribute walk finds nothing (the sender only worked
     * because its URL cache was pre-populated from its own POST
     * response — a gateway-delivered receiver has neither). Walk the
     * <li>'s fiber `.return` chain (same proven shape as
     * recvAuthorIdViaFiber) for `message.attachments` and return
     * [{url, filename}]. Defensive: never throws; [] on miss.
     */
    function oslAttachmentUrlsViaFiber(li) {
        const out = [];
        if (!li) return out;
        let key = null;
        try {
            key = Object.keys(li).find(function (k) {
                return k.indexOf("__reactFiber") === 0;
            });
        } catch (_) {
            key = null;
        }
        if (!key) return out;
        let f = li[key];
        const pushAtts = function (atts) {
            if (!Array.isArray(atts)) return;
            for (const a of atts) {
                if (!a || typeof a !== "object") continue;
                const url =
                    (typeof a.url === "string" && a.url) ||
                    (typeof a.proxy_url === "string" && a.proxy_url) ||
                    null;
                if (!url) continue;
                const filename =
                    (typeof a.filename === "string" && a.filename) || "";
                out.push({ url: url, filename: filename });
            }
        };
        for (let depth = 0; f && depth < 40; depth++) {
            try {
                const p = f.memoizedProps;
                if (p && typeof p === "object") {
                    if (p.message && Array.isArray(p.message.attachments)) {
                        pushAtts(p.message.attachments);
                    }
                    if (Array.isArray(p.attachments)) {
                        pushAtts(p.attachments);
                    }
                    if (out.length > 0) return out;
                }
            } catch (_) {
                // keep walking
            }
            f = f.return;
        }
        return out;
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
        // Probe-2 fix: also consult the send-time cache so editing
        // an own outbound message that was sent THIS session (before
        // loadedHistory caught up via a channel switch) doesn't fall
        // through to the EDIT_TAB_PLACEHOLDER.
        const fromSelfSent = selfSentPlaintext.get(messageId);
        if (typeof fromSelfSent === "string") {
            return { plaintext: fromSelfSent, source: "selfSentPlaintext" };
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

        // Beta 1.0 edit fix: the old gate only fired when the edit box
        // showed `DPC0::` ciphertext. After the prose-token cutover the
        // box shows innocuous PROSE cover instead, so the swap never
        // ran and the user ended up editing the cover text. Gate on
        // POSITIVE OSL signals instead, so we swap the plaintext in for
        // prose covers AND legacy DPC0::, while never touching a plain
        // Discord message someone is editing.
        const resolved = editTabResolvePlaintext(messageId);
        const looksLikeRawWire =
            text.indexOf("DPC0::") === 0 || text.indexOf("DPC1::") === 0;
        const knownOslMsg =
            !!resolved ||
            looksLikeRawWire ||
            (window.__oslProseWireByMsgId &&
                window.__oslProseWireByMsgId.has(messageId)) ||
            (typeof recvCovers !== "undefined" &&
                recvCovers &&
                typeof recvCovers.has === "function" &&
                recvCovers.has(messageId));
        if (!knownOslMsg) return; // plain Discord message; leave alone

        if (resolved) {
            // Already showing the plaintext (Discord re-mounted the box
            // after our swap)? Mark and bail so we don't clobber the
            // user's in-progress keystrokes or loop the observer.
            if (text === (resolved.plaintext || "").trim()) {
                textboxEl.dataset.oslSwapped = messageId;
                return;
            }
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

        // 9-B3: retry-on-stale-token wrapper. The "no token at all"
        // guard above (editOverlayAuthToken === null) stays — that's
        // a different failure shape than a stale 401.
        oslFetchWithTokenRetry(url, {
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
                    // Probe-2 Boot Bug 1: this path used to leave the
                    // PRE-edit plaintext in selfSentPlaintext, so the
                    // self-view short-circuit at recvDispatchDecrypt
                    // rendered the OLD text over the NEW ciphertext on
                    // any DOM re-mount after the 5s editOverlayLocallyApplied
                    // window. Updating the map (instead of just clearing
                    // it) preserves the short-circuit's purpose — the
                    // sender never needs to v=4-decrypt their own wire.
                    try {
                        selfSentPlaintext.set(messageId, newPlaintext);
                        oslFifoEvict(
                            selfSentPlaintext,
                            OSL_SELF_SENT_PLAINTEXT_MAX
                        );
                    } catch (_) {}
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
        // Probe-2 Boot Bug 8: aria-label="Edit" is the English locale's
        // literal; non-English clients use "Modifier" / "Editar" / etc.
        // Match the OSL pencil button (we install our own `data-osl-edit`
        // marker in the message-actions row) first, then fall back to
        // Discord's native English-only label so existing flows are
        // preserved on en-US.
        const btn =
            target.closest("[data-osl-edit='1']") ||
            target.closest('[role="button"][aria-label="Edit"]') ||
            target.closest('button[class*="messageActionsButton"]');
        if (!btn) return;
        const li = btn.closest("li[id^='chat-messages-']");
        if (!li) return;
        const m = /chat-messages-\d{15,22}-(\d{15,22})/.exec(li.id);
        if (!m) return;
        const messageId = m[1];

        // Resolve plaintext. Four sources, in order:
        //   1. loadedHistory     — fastest, populated on channel switch
        //   2. recvPlaintext     — populated by this session's decrypts
        //   3. selfSentPlaintext — populated by this session's sends
        //                          (Probe-2 fix; covers own outbound
        //                          messages that haven't yet flowed
        //                          through loadedHistory)
        //   4. live DOM          — self-healing fallback for the
        //      re-edit case: editOverlaySave invalidates the caches
        //      on save, but the new plaintext is sitting in the
        //      message-content textContent because we wrote it
        //      there directly. Read it back rather than give up
        //      and hand the user Discord's native edit (which would
        //      show DPC0:: ciphertext).
        const fromHistory = loadedHistory.get(messageId);
        const fromSession = recvPlaintext.get(messageId);
        const fromSelfSent = selfSentPlaintext.get(messageId);
        let plaintext =
            typeof fromHistory === "string"
                ? fromHistory
                : typeof fromSession === "string"
                ? fromSession
                : typeof fromSelfSent === "string"
                ? fromSelfSent
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
        let text = div.textContent;
        if (!text) {
            // 8d-FIX3: an encrypted-attachment message has empty
            // text by design (cover envelope lives in the file
            // post-FIX2). If the enclosing <li> carries Discord-
            // hosted media, route to the attachment scanner
            // instead of skipping. The scanner does its own
            // filename-shape filter + MagicNotFound rejection, so
            // a stray emoji/avatar match here is harmless and the
            // hot path on plain empty messages remains the skip.
            try {
                const li = div.closest("li[id^='chat-messages-']");
                if (
                    li &&
                    li.querySelector(
                        "img[src*='discord'], video[src*='discord'], a[href*='discord']"
                    )
                ) {
                    console.log(
                        "[OSL] recvHandleDiv id=" +
                            __dbg_id +
                            " empty textContent but has attachments, routing to attachment scan"
                    );
                    oslScanLiAttachmentsV2(li).catch(function () {});
                    return;
                }
            } catch (_) {}
            console.log(
                "[OSL] recvHandleDiv SKIP id=" +
                    __dbg_id +
                    " reason=empty_textContent"
            );
            return;
        }
        // PHASE 2 prose-token pivot: every incoming message in an
        // OSL-enabled scope first runs through `osl_prose_token_recv`.
        // It returns null fast for plain chat (HMAC mismatch) and
        // returns a `DPC0::<base64>` wire string for real OSL
        // tokens. Cached by msg_id so the IPC + cipher-store fetch
        // run at most once per message.
        if (
            text.indexOf("DPC0::") !== 0 &&
            text.indexOf("DPC1::") !== 0
        ) {
            const __osl_pre_msgId = recvMessageIdOf(div);
            if (__osl_pre_msgId) {
                // Loading-time win (cross-restart): if this message's
                // plaintext is already known — rehydrated from the
                // local sealed store by recvLoadHistory, or decrypted
                // earlier this session — render it directly and SKIP
                // the osl_prose_token_recv IPC + cipher-store network
                // fetch. On a relaunch this turns "N network
                // round-trips to repaint a channel" into "N cheap DOM
                // applies." We seed recvCovers with the current cover
                // so a later ONLINE edit (cover changes) is still
                // detected and re-decrypted; the only thing skipped is
                // catching an edit that happened while the app was
                // closed, which is rare and self-heals on the next
                // edit.
                const __osl_cachedPt =
                    loadedHistory.get(__osl_pre_msgId) ||
                    recvPlaintext.get(__osl_pre_msgId) ||
                    selfSentPlaintext.get(__osl_pre_msgId);
                if (typeof __osl_cachedPt === "string") {
                    const __osl_coverNow = (div.textContent || "").trim();
                    if (__osl_coverNow !== __osl_cachedPt.trim()) {
                        recvCovers.set(__osl_pre_msgId, __osl_coverNow);
                        recvApplyPlaintext(div, __osl_cachedPt);
                    }
                    recvPlaintext.set(__osl_pre_msgId, __osl_cachedPt);
                    recvDone.add(__osl_pre_msgId);
                    return;
                }
                if (!window.__oslProseWireByMsgId) {
                    window.__oslProseWireByMsgId = new Map();
                }
                const __osl_cachedWire =
                    window.__oslProseWireByMsgId.get(__osl_pre_msgId);
                if (typeof __osl_cachedWire === "string") {
                    // If the message has already been decrypted +
                    // applied, bail. The existing cached-plaintext
                    // branch below would otherwise re-apply plaintext
                    // on every Discord mutation, which itself mutates
                    // the div, which re-triggers the observer — an
                    // infinite loop that buried the console in 50k
                    // "scan no candidates" lines until the client died.
                    if (recvDone.has(__osl_pre_msgId)) {
                        // Channel-re-entry cache fix: Discord may have
                        // re-mounted this <div> with the original prose
                        // cover textContent instead of our previously-
                        // applied plaintext span. If we have cached
                        // plaintext (in-memory or MessageStore-loaded),
                        // re-apply it before bailing — otherwise the row
                        // stays as cover text forever and the user sees
                        // "messages don't decrypt that previously did".
                        //
                        // Stop signal against the observer loop: only
                        // re-apply when textContent currently differs
                        // from the cached plaintext. After apply, the
                        // observer fires once more, re-enters here, the
                        // textContent now matches, and we no-op out.
                        try {
                            const _restore =
                                recvPlaintext.get(__osl_pre_msgId) ||
                                loadedHistory.get(__osl_pre_msgId) ||
                                selfSentPlaintext.get(__osl_pre_msgId);
                            if (
                                typeof _restore === "string" &&
                                div.textContent !== _restore
                            ) {
                                recvApplyPlaintext(div, _restore);
                            }
                        } catch (_) {}
                        return;
                    }
                    text = __osl_cachedWire;
                } else {
                    if (!window.__oslProseInFlight) {
                        window.__oslProseInFlight = new Set();
                    }
                    if (!window.__oslProseRetryCount) {
                        window.__oslProseRetryCount = new Map();
                    }
                    if (!window.__oslProseInFlight.has(__osl_pre_msgId)) {
                        let __osl_proseScope = null;
                        let __osl_scopeErr = null;
                        try {
                            const __osl_ctx =
                                typeof oslCurrentChannelContext ===
                                "function"
                                    ? oslCurrentChannelContext()
                                    : null;
                            __osl_proseScope =
                                __osl_ctx &&
                                typeof oslScopeForCurrentContext ===
                                    "function"
                                    ? oslScopeForCurrentContext(
                                          __osl_ctx
                                      )
                                    : null;
                        } catch (e) {
                            __osl_scopeErr = e;
                        }
                        if (__osl_proseScope) {
                            window.__oslProseRetryCount.delete(
                                __osl_pre_msgId
                            );
                            window.__oslProseInFlight.add(
                                __osl_pre_msgId
                            );
                            oslInvoke("osl_prose_token_recv", {
                                scopeInput: __osl_proseScope,
                                msg: text,
                            })
                                .then(function (__osl_resp) {
                                    window.__oslProseInFlight.delete(
                                        __osl_pre_msgId
                                    );
                                    if (
                                        __osl_resp &&
                                        __osl_resp.ok &&
                                        __osl_resp.value &&
                                        typeof __osl_resp.value.wire ===
                                            "string"
                                    ) {
                                        window.__oslProseWireByMsgId.set(
                                            __osl_pre_msgId,
                                            __osl_resp.value.wire
                                        );
                                        try {
                                            if (
                                                !window.__oslMsgIdToBlobId
                                            ) {
                                                window.__oslMsgIdToBlobId =
                                                    new Map();
                                            }
                                            window.__oslMsgIdToBlobId.set(
                                                __osl_pre_msgId,
                                                __osl_resp.value.blob_id
                                            );
                                        } catch (_) {}
                                        // Re-invoke now that the wire
                                        // is cached. The next pass
                                        // reads `text` from
                                        // __oslProseWireByMsgId.
                                        try {
                                            recvHandleDiv(div);
                                        } catch (_) {}
                                    } else {
                                        // Silent fall-through used to
                                        // leave the marker stuck with
                                        // no log; surface the bad
                                        // resolve so the failure is
                                        // diagnosable in F12.
                                        console.warn(
                                            "[OSL] prose_token_recv resolved without wire" +
                                                " msgId=" +
                                                __osl_pre_msgId +
                                                " ok=" +
                                                !!(
                                                    __osl_resp &&
                                                    __osl_resp.ok
                                                ) +
                                                " hasValue=" +
                                                !!(
                                                    __osl_resp &&
                                                    __osl_resp.value
                                                ),
                                            __osl_resp
                                        );
                                    }
                                })
                                .catch(function (e) {
                                    window.__oslProseInFlight.delete(
                                        __osl_pre_msgId
                                    );
                                    console.warn(
                                        "[OSL] prose_token_recv threw:",
                                        e
                                    );
                                });
                            // No wire yet -- existing path is a no-op
                            // until the async resolves + re-invokes.
                            return;
                        } else {
                            // Channel context isn't resolved yet --
                            // fiber/store/dom resolvers haven't
                            // hydrated. The mutation observer fires
                            // before that's ready in GCs more often
                            // than DMs, which is why this used to
                            // intermittently leave [Encryption -
                            // Ignore] stuck with no logs. Retry with
                            // backoff instead of bailing silently.
                            const __osl_retryN =
                                window.__oslProseRetryCount.get(
                                    __osl_pre_msgId
                                ) || 0;
                            const __OSL_PROSE_MAX_RETRIES = 6;
                            if (
                                __osl_retryN < __OSL_PROSE_MAX_RETRIES
                            ) {
                                const __osl_delay =
                                    200 * Math.pow(2, __osl_retryN);
                                window.__oslProseRetryCount.set(
                                    __osl_pre_msgId,
                                    __osl_retryN + 1
                                );
                                console.warn(
                                    "[OSL] prose_token scope null; retry " +
                                        (__osl_retryN + 1) +
                                        "/" +
                                        __OSL_PROSE_MAX_RETRIES +
                                        " in " +
                                        __osl_delay +
                                        "ms msgId=" +
                                        __osl_pre_msgId +
                                        (__osl_scopeErr
                                            ? " err=" +
                                              String(__osl_scopeErr)
                                            : "")
                                );
                                setTimeout(function () {
                                    try {
                                        recvHandleDiv(div);
                                    } catch (_) {}
                                }, __osl_delay);
                                return;
                            }
                            console.warn(
                                "[OSL] prose_token scope still null after " +
                                    __OSL_PROSE_MAX_RETRIES +
                                    " retries; giving up msgId=" +
                                    __osl_pre_msgId
                            );
                            window.__oslProseRetryCount.delete(
                                __osl_pre_msgId
                            );
                        }
                    }
                }
            }
        }
        if (!oslMessageIsStego(text)) {
            // Probe-5 v5: even if the DPC0:: prefix isn't at index
            // 0 (Discord prepended a reply quote or accessibility
            // chrome to textContent), still hide the <li> if it
            // contains DPC0:: anywhere. Dispatch is gated on the
            // strict prefix below so we don't mangle the cipher
            // wire passed to Rust.
            if (oslMessageContainsStego(text)) {
                try {
                    oslAutoHideCiphertext(div);
                } catch (_) {}
            }
            if (OSL_DEBUG_RECV) {
                console.log(
                    "[OSL] recvHandleDiv SKIP id=" +
                        __dbg_id +
                        " reason=no_DPC0_or_DPC1_prefix" +
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
        // Probe-5 perf-revert follow-up: when Discord re-renders a
        // previously-decrypted message (click, scroll, focus, etc.),
        // it replaces our plaintext span with the original ciphertext
        // span. The mutation observer fires here. If we have cached
        // plaintext for this msg id, re-apply it IMMEDIATELY -- no
        // auto-hide blank window, no waiting for the 1s sweep tick
        // to re-apply. This was the cause of "way more messages
        // blank" after the perf revert.
        try {
            const _cached =
                typeof messageId === "string"
                    ? recvPlaintext.get(messageId) ||
                      loadedHistory.get(messageId) ||
                      selfSentPlaintext.get(messageId)
                    : null;
            if (typeof _cached === "string") {
                recvApplyPlaintext(div, _cached);
                recvDone.add(messageId);
                return;
            }
        } catch (_) {}
        // No cached plaintext -- this is a genuinely-new DPC0::
        // arrival. Hide the ciphertext visually so the user never
        // sees the cipher blob during the IPC decrypt roundtrip.
        // textContent stays intact (sweep checks still work).
        // recvApplyPlaintext undoes this on successful decrypt.
        oslAutoHideCiphertext(div);

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
            // Item 2 fix: DO NOT delete recvCovers here. Keeping the
            // old cover means this detector keeps firing on every
            // observer/sweep tick (lastCover !== new wire) until a
            // *successful* decrypt overwrites recvCovers[id] with the
            // edited wire. Previously the delete made lastCover
            // undefined, so a first failed decrypt of the edited v=4
            // wire (expected when intervening traffic rotated the
            // ratchet) disarmed the detector permanently and the
            // .catch's recvDone.add stuck DPC0:: forever. Reset the
            // retry budget so the edited wire gets a fresh
            // RECV_MAX_RETRIES window of re-dispatches.
            recvRetries.delete(messageId);
            recvAuthorRetryCount.delete(messageId);
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
        // Probe-2 Boot Bug 9: burn-skip must run BEFORE the self-view
        // short-circuit; otherwise the sender's own messages in a
        // burned scope still render their stashed plaintext after any
        // re-mount, contradicting the locked "burn means burned for
        // everyone, no special case for self" policy.
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

        // Item 1 (self-view): our own encrypted v=4 DM cannot be
        // decrypted locally (v=4 wraps only to the peer's slot).
        // Render the send-time plaintext we stashed instead of
        // dispatching a decrypt that will correctly fail and leave
        // the DPC0:: wire on screen.
        const fromSelfSent = selfSentPlaintext.get(messageId);
        if (typeof fromSelfSent === "string") {
            recvApplyPlaintext(div, fromSelfSent);
            recvPlaintext.set(messageId, fromSelfSent);
            recvCovers.set(messageId, coverText);
            recvDone.add(messageId);
            console.log(
                "[OSL] self-view render msg=" +
                    messageId +
                    " (send-time plaintext, no decrypt)"
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
        // Bug 1: v=5 (GC sender-keys) decrypt needs the gc:<id>
        // scope to look up the sender's sender-key chain
        // (decrypt_v5_recv errors "scope required for sender-keys
        // lookup" without it). The receive invoke never passed
        // scope, so every GC message failed. Derive it the same way
        // the send path does — oslScopeForCurrentContext over the
        // current channel context. The rendered message is in the
        // current channel, so this scope matches what the sender
        // used (gc:<channelId>). null for unknown context is fine:
        // v=2/3/4 ignore scope, only v=5 requires it. The Rust
        // `osl_decrypt_message` command already accepts
        // scope_input: Option<ScopeInput> and threads it to
        // cmd_osl_decrypt_message_v2 → decrypt_v5_recv.
        let recvScopeInput = null;
        try {
            const recvCtx =
                typeof oslCurrentChannelContext === "function"
                    ? oslCurrentChannelContext()
                    : null;
            recvScopeInput = recvCtx
                ? oslScopeForCurrentContext(recvCtx)
                : null;
        } catch (_) {
            recvScopeInput = null;
        }
        const ipcPromise = invoke("osl_decrypt_message", {
            channelId: channelId,
            senderDiscordId: senderDiscordId,
            content: coverText,
            // Phase 5b3: opt into at-rest persistence. The
            // backend treats this as `Option<String>` (`None`
            // skips the store, `Some` writes the row).
            discordMessageId: messageId,
            scopeInput: recvScopeInput,
        });

        Promise.race([ipcPromise, timeoutPromise])
            .then(function (plaintext) {
                nativeClearTimeout(timeoutHandle);
                recvInFlight.delete(messageId);

                // Bug 1 part (b): an SKDM just installed the sender's
                // ReceiverChain for its scope (apply_skdm_recv ->
                // OSL_RESULT_SKDM_APPLIED). This is a CONTROL sentinel,
                // not user content — must not be cached or rendered.
                // Part (a) keeps an awaiting-SKDM failure non-terminal,
                // but a v=5 message that exhausted RECV_MAX_RETRIES
                // before the SKDM landed is already in recvDone. Revive
                // every v=5 cover still rendered (DPC0:: + wire byte 5)
                // that hasn't resolved yet by clearing its terminal +
                // retry state, so the next sweep re-dispatches it now
                // that the chain exists.
                //
                // Bounded blast radius: only live-DOM messages whose
                // cover decodes to wire v=5 and are NOT already in
                // recvPlaintext are touched — non-OSL / v=2-4 /
                // already-decrypted messages are untouched.
                //
                // Retry budget still ultimately bounds re-dispatch:
                // each revived message gets at most RECV_MAX_RETRIES
                // fresh attempts (recvDispatchDecrypt top-of-fn guard
                // re-adds recvDone on exhaustion), and any post-SKDM
                // failure that is NOT awaiting-SKDM terminalizes
                // immediately via the .catch guard. SKDMs are finite
                // inbound messages, so the revive itself can't spin —
                // a message that keeps failing for another reason
                // cannot loop forever.
                if (plaintext === OSL_RESULT_SKDM_APPLIED) {
                    console.log(
                        "[OSL] SKDM applied msg=" +
                            messageId +
                            " — reviving stuck v=5 covers (sender=" +
                            senderDiscordId +
                            ")"
                    );
                    // Probe-3 fix: mark the SKDM message itself DONE
                    // and hide its <li>. The original code DELETED
                    // recvDone for the SKDM, causing the next sweep
                    // to re-dispatch it, which re-applied idempotent-
                    // ly, which re-fired the revive -- an infinite
                    // log-spam loop (visible as the same msg IDs
                    // re-appearing on every sweep tick). Marking it
                    // done + hiding the <li> also fixes the "giant
                    // ciphertext blob" Discord message the user was
                    // seeing for every send: the bundled SKDM has no
                    // user-facing content, so it shouldn't render.
                    recvDone.add(messageId);
                    recvRetries.delete(messageId);
                    recvAuthorRetryCount.delete(messageId);
                    // Probe-3 final SKDM hide:
                    //   - if the closest <li> contains EXACTLY ONE
                    //     `message-content-` div, the <li> is owned
                    //     solely by this SKDM -> hide the <li>
                    //     entirely so no vertical gap remains
                    //   - if the <li> contains multiple message-
                    //     content divs (Discord's same-author group
                    //     wrapper, which also owns the avatar +
                    //     header for the first message in the group),
                    //     hide only the message-content div for this
                    //     SKDM and let Discord's surrounding chrome
                    //     stay so other messages in the group keep
                    //     their avatar
                    // This collapses cleanly to no visible row in
                    // the common "one message per <li>" case while
                    // protecting the avatar in the grouped case.
                    oslSkdmHiddenMsgIds.add(messageId);
                    try {
                        oslHideSkdmDom(messageId);
                    } catch (_) {}
                    try {
                        // Probe-2 Boot Bug 7: was reviving EVERY v=5
                        // cover in the current view regardless of
                        // sender. SKDMs are scoped per
                        // (peer, scope_id), so clearing state for
                        // messages from OTHER senders just burned
                        // their retry budgets a second time without
                        // unlocking anything. Narrow to covers whose
                        // author matches the SKDM sender.
                        //
                        // Phase 6.1 fix: post-prose-token-cutover the
                        // div's textContent is the PROSE COVER, not
                        // the DPC0:: wire, so `oslCoverWireVersion`
                        // against textContent always returns -1 and
                        // every awaiting v=5 was being skipped. Now
                        // we ALSO check the cached wire stashed by
                        // prose_token_recv in __oslProseWireByMsgId.
                        // If either source is a v=5 cover, the
                        // message qualifies for revive.
                        const _divs = document.querySelectorAll(
                            RECV_MESSAGE_DIV_SELECTOR
                        );
                        let _revived = 0;
                        for (const _d of _divs) {
                            const _mid = recvMessageIdOf(_d);
                            if (!_mid) continue;
                            // Try textContent first (covers the pre-
                            // prose path) then the cached wire.
                            let _isV5 =
                                oslCoverWireVersion(_d.textContent || "") === 5;
                            if (!_isV5 && window.__oslProseWireByMsgId) {
                                const _cw =
                                    window.__oslProseWireByMsgId.get(_mid);
                                if (typeof _cw === "string") {
                                    _isV5 = oslCoverWireVersion(_cw) === 5;
                                }
                            }
                            if (!_isV5) continue;
                            if (recvPlaintext.has(_mid)) continue;
                            const _author = recvExtractAuthorId(_d);
                            if (
                                senderDiscordId &&
                                _author &&
                                _author !== senderDiscordId
                            ) {
                                // Different sender — unrelated v=5
                                // chain. Skip.
                                continue;
                            }
                            recvDone.delete(_mid);
                            recvRetries.delete(_mid);
                            recvAuthorRetryCount.delete(_mid);
                            _revived++;
                        }
                        console.log(
                            "[OSL] SKDM revive: cleared terminal/retry " +
                                "state for " +
                                _revived +
                                " stuck v=5 message(s) from sender=" +
                                senderDiscordId
                        );
                    } catch (_) {}
                    return;
                }

                // Auto-recovery: an inbound SKDM_REQUEST we honored
                // produced a fresh SKDM wire. POST it back to this
                // channel so the requester's recv path applies it.
                // Control sentinel — never cache/render.
                if (
                    typeof plaintext === "string" &&
                    plaintext.indexOf(OSL_RESULT_SKDM_REREQUEST_PREFIX) === 0
                ) {
                    const _skdmWire = plaintext.slice(
                        OSL_RESULT_SKDM_REREQUEST_PREFIX.length
                    );
                    recvDone.add(messageId);
                    try {
                        // Phase 6.4: SKDM re-request response goes
                        // to the original requester's inbox (= the
                        // sender of the SKDM_REQUEST we just
                        // honored). No Discord channel POST.
                        oslSendControlOob(
                            [senderDiscordId],
                            recvScopeInput,
                            _skdmWire
                        ).then(function (r) {
                            console.log(
                                "[OSL] SKDM re-request: posted to inbox " +
                                    "recipient=" +
                                    senderDiscordId +
                                    " (msg=" +
                                    messageId +
                                    ") ok=" +
                                    (r && r.ok) +
                                    " fail=" +
                                    (r && r.fail)
                            );
                        });
                    } catch (e) {
                        console.log(
                            "[OSL] SKDM re-request POST threw: " +
                                (e && e.message ? e.message : e)
                        );
                    }
                    return;
                }
                // Auto-recovery: a peer-requested v=4 session reset was
                // applied locally, or a recovery request was guarded
                // away. Either way: control sentinel, suppress render.
                if (
                    plaintext === OSL_RESULT_SESSION_RESET_APPLIED ||
                    plaintext === OSL_RESULT_RECOVERY_IGNORED
                ) {
                    console.log(
                        "[OSL] recovery control msg=" +
                            messageId +
                            " (" +
                            plaintext +
                            ")"
                    );
                    recvDone.add(messageId);
                    return;
                }

                if (DEBUG) {
                    // Phase 9-A1: also surface which wire version
                    // we received. The version byte lives inside
                    // the base64-decoded payload (one byte past the
                    // DPC0:: prefix); peek it locally so this log
                    // doesn't require a Rust round-trip.
                    const _cwv = oslCoverWireVersion(coverText);
                    const wireVersion = _cwv >= 0 ? "v" + _cwv : "?";
                    console.log(
                        "[OSL] decrypt result for msg=" +
                            messageId +
                            ": ok (wire_version=" +
                            wireVersion +
                            ")"
                    );
                }
                // Probe-3 persistence diagnostic: always-on log line
                // (NOT gated on DEBUG) so the user can grep the
                // console for "persist-expected" against the
                // "[OSL] history apply msg=X" lines emitted by
                // recvLoadHistory on relaunch. If a message shows
                // persist-expected here but no history-apply line
                // on reopen, the write side failed; if it appears
                // in history-apply but the DOM still shows
                // ciphertext, the apply side failed.
                if (
                    typeof plaintext === "string" &&
                    plaintext.indexOf("__OSL_CONTROL_") !== 0
                ) {
                    console.log(
                        "[OSL] persist-expected msg=" +
                            messageId +
                            " channel=" +
                            channelId +
                            " sender=" +
                            senderDiscordId +
                            " plaintext_len=" +
                            plaintext.length
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

                // Probe-2 Boot Bug 1: previously this used
                // document.getElementById, which returns the FIRST
                // matching node in document order. Discord's
                // virtualised message list can transiently hold a
                // detached/stale node with the same id while the
                // visible re-mounted one trails behind, so the apply
                // could land on the off-screen node — the 100ms
                // delayed-check then re-read the same wrong node and
                // logged STUCK while the user kept seeing ciphertext.
                // Switch to querySelectorAll and apply to every match
                // so duplicates can never strand us on the wrong one.
                const liveDivs = document.querySelectorAll(
                    "[id='" + RECV_MESSAGE_ID_PREFIX + messageId + "']"
                );
                if (liveDivs.length === 0) {
                    if (DEBUG) {
                        console.log(
                            "[OSL] msg=" +
                                messageId +
                                " not in DOM at resolve time; sweep will apply"
                        );
                    }
                    return;
                }
                if (DEBUG && liveDivs.length > 1) {
                    console.log(
                        "[OSL] msg=" +
                            messageId +
                            " duplicate-id defense: " +
                            liveDivs.length +
                            " matching nodes in DOM; applying to each"
                    );
                }
                const before =
                    liveDivs[0] && liveDivs[0].textContent
                        ? liveDivs[0].textContent
                        : "";
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
                liveDivs.forEach(function (d) {
                    recvApplyPlaintext(d, plaintext);
                });
                if (DEBUG) {
                    const after =
                        liveDivs[0] && liveDivs[0].textContent
                            ? liveDivs[0].textContent
                            : "";
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
                        const sweepDivs = document.querySelectorAll(
                            "[id='" + RECV_MESSAGE_ID_PREFIX + messageId + "']"
                        );
                        if (sweepDivs.length === 0) {
                            console.log(
                                "[OSL] msg=" +
                                    messageId +
                                    " delayed-check: detached from DOM"
                            );
                            return;
                        }
                        let anyReverted = false;
                        sweepDivs.forEach(function (d) {
                            if (oslMessageIsStego(d.textContent || "")) {
                                anyReverted = true;
                                // Re-apply opportunistically — duplicate
                                // node may have been a fresh re-mount
                                // that raced the dispatch.
                                try {
                                    recvApplyPlaintext(d, plaintext);
                                } catch (_) {}
                            }
                        });
                        const sample = sweepDivs[0].textContent || "";
                        console.log(
                            "[OSL] msg=" +
                                messageId +
                                " delayed-check (100ms, " +
                                sweepDivs.length +
                                " node(s)): textContent=" +
                                sample.slice(0, 64) +
                                " (len=" +
                                sample.length +
                                ")" +
                                (anyReverted
                                    ? " REVERTED on at least one node — re-applied + sweep will reconverge"
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
                // Item 2 fix: a PENDING EDIT is recvCovers[id] defined
                // (we previously decrypted a *different* cover) AND it
                // differs from the wire we just failed on. Such a
                // failure is expected for an edited v=4 message when
                // intervening traffic rotated the ratchet; do NOT
                // terminalize it — leave recvDone unset so the next
                // observer/sweep tick re-dispatches the edited wire.
                // recvRetries was incremented above, so the top-of-fn
                // `tries >= RECV_MAX_RETRIES` guard still bounds this
                // (it terminalizes after the normal retry budget). A
                // brand-new message (recvCovers undefined) or a stale
                // re-decrypt of the same cover keeps the original
                // terminal behavior.
                const priorCover = recvCovers.get(messageId);
                const isPendingEdit =
                    priorCover !== undefined && priorCover !== coverText;
                // Bug 1: a v=5 (GC sender-keys) message that fails
                // purely because scope wasn't supplied for the
                // sender-key lookup must NOT be terminalized — the
                // fix below now threads gc:<id> scope into the
                // invoke, so a re-dispatch will succeed. Leave
                // recvDone unset (still bounded by recvRetries /
                // RECV_MAX_RETRIES via the top-of-fn guard).
                const isV5MissingScope =
                    msg.indexOf(
                        "v=5 decode: scope required for sender-keys lookup"
                    ) !== -1;
                // Bug 1 part (a): a v=5 message that fails purely
                // because the sender's ReceiverChain / SenderKeyState
                // isn't installed yet ("… no installed sender-key
                // state for peer … — awaiting SKDM", commands.rs
                // decrypt_v5_recv) is a normal first-contact /
                // message-ordering condition, NOT a permanent
                // failure: the SKDM arrives as a separate v=4 message
                // and `apply_skdm_recv` installs the chain. Do NOT
                // terminalize — leave recvDone unset so the periodic
                // sweep re-dispatches once the chain lands. Still
                // bounded by recvRetries / RECV_MAX_RETRIES via the
                // recvDispatchDecrypt top-of-fn guard (part (b) resets
                // that budget when the SKDM is actually applied).
                const isV5AwaitingSkdm =
                    msg.indexOf(
                        "no installed sender-key state for peer"
                    ) !== -1 || msg.indexOf("awaiting SKDM") !== -1;
                if (
                    !isPendingEdit &&
                    !isV5MissingScope &&
                    !isV5AwaitingSkdm
                ) {
                    recvDone.add(messageId);
                }

                // Auto-recovery: fire on the FIRST failure that matches
                // a recoverable pattern (not gated on retry exhaustion).
                // The OLD gate `(tries+1) >= RECV_MAX_RETRIES` was dead
                // code: the catch terminalizes most error classes on try
                // 1 via `recvDone.add`, so retries never accumulate, so
                // the guard was never satisfied for v=4 desync / v=4
                // not-a-recipient / v=5 not-a-recipient. The cooldown
                // inside oslMaybeEmitRecovery (only armed on success)
                // and the ipc-side RecoveryGuard are the real spam
                // throttles. These failure classes can't self-heal by
                // re-running the dispatch — we have to ask the peer to
                // do something (re-handshake / re-distribute sender
                // key / re-publish identity).
                if (senderDiscordId) {
                    const isV4Desync =
                        msg.indexOf("v=4 dr.decrypt") !== -1 ||
                        msg.indexOf("(desync)") !== -1;
                    // v=4 path's "not a recipient" carries the version
                    // prefix; v=5 path's does NOT. Differentiate so we
                    // route to the correct recovery (identity vs SKDM).
                    const isV4NotRecipient =
                        msg.indexOf("v=4 decode: not a recipient") !== -1;
                    const isV5NotRecipient =
                        !isV4NotRecipient &&
                        msg.indexOf("not a recipient of this message") !==
                            -1;
                    if (isV5AwaitingSkdm && recvScopeInput) {
                        // Sender key not yet installed for this peer +
                        // scope. Ask peer to (re)distribute SKDM.
                        oslMaybeEmitRecovery(
                            "skdm",
                            senderDiscordId,
                            channelId,
                            recvScopeInput
                        );
                    } else if (isV5NotRecipient && recvScopeInput) {
                        // Probe-3 fix: GC v=5 receiver lacks the
                        // sender's sender-key chain (formerly "the
                        // known secondary gap, deferred"). Same fix
                        // as awaiting-skdm: ask peer to redistribute.
                        oslMaybeEmitRecovery(
                            "skdm",
                            senderDiscordId,
                            channelId,
                            recvScopeInput
                        );
                    } else if (isV4Desync) {
                        // Ratchet desync: ask peer to drop their
                        // ratchet so next v=4 re-handshakes both ways.
                        oslMaybeEmitRecovery(
                            "session",
                            senderDiscordId,
                            channelId,
                            null
                        );
                    } else if (isV4NotRecipient) {
                        // Stale-identity: sender wrapped to an
                        // identity we no longer hold (they
                        // reinstalled/re-registered). Re-fetch their
                        // keyserver bundle so a real key change
                        // surfaces via the TOFU accept banner.
                        oslMaybeEmitRecovery(
                            "identity",
                            senderDiscordId,
                            channelId,
                            null
                        );
                    }
                }
            });
    }

    /**
     * Auto-recovery (4/4): build + POST a recovery request to `peer`.
     * `kind` is "skdm" (needs `scopeInput`) or "session" (peer-scoped).
     * JS cooldown only damps sweep re-fires — the ipc RecoveryGuard is
     * the authoritative throttle/replay/act-on-symptom guarantee, and
     * a throttled build returns a stable Err we treat as a benign
     * no-op. Fire-and-forget; never blocks the recv pipeline.
     */
    function oslMaybeEmitRecovery(kind, peer, channelId, scopeInput) {
        try {
            // Probe-3 fix: `invoke` was a bare reference here, which
            // threw "invoke is not defined" the moment recovery
            // actually fired (the trigger was previously dead code
            // so the latent bug never surfaced). Resolve through
            // `getTauriInvoke()` like every other invoke site does;
            // bail if Tauri isn't reachable.
            const invoke = getTauriInvoke();
            if (typeof invoke !== "function") {
                console.log(
                    "[OSL] auto-recovery: Tauri invoke not available; " +
                        "skipping " +
                        kind +
                        " for peer=" +
                        peer
                );
                return;
            }
            const scopeKey = scopeInput
                ? scopeInput.kind + ":" + scopeInput.id
                : "dm";
            const cdKey = kind + "|" + peer + "|" + scopeKey;
            const last = recvRecoveryCooldown.get(cdKey) || 0;
            const nowMs = Date.now();
            if (nowMs - last < RECV_RECOVERY_COOLDOWN_MS) {
                return;
            }
            // Probe-2 Boot Bug 4: previously we set the cooldown *here*,
            // before issuing the request. Any transient failure
            // (RecoveryGuard throttle, no peer pubkey, IPC blip) then
            // wedged the auto-recovery for the full 2-minute window
            // even though no recovery actually happened. Move the
            // cooldown set into the success arms below so failures
            // remain retryable on the next exhausted-retry tick.
            // Stale-identity kind: no wire to POST — just re-fetch the
            // peer's keyserver bundle. A genuine key change becomes a
            // pending TOFU alert (never auto-trusted); oslCheckSecurity
            // Alerts then surfaces the loud one-tap accept banner.
            if (kind === "identity") {
                console.log(
                    "[OSL] auto-recovery: re-fetching identity for peer=" +
                        peer +
                        " (stale-identity / 'not a recipient')"
                );
                Promise.resolve(
                    invoke("osl_recover_peer_identity", { discordId: peer })
                )
                    .then(function (changed) {
                        recvRecoveryCooldown.set(cdKey, Date.now());
                        console.log(
                            "[OSL] auto-recovery: identity re-fetch peer=" +
                                peer +
                                " changed=" +
                                changed +
                                (changed
                                    ? " — TOFU alert will surface if the key changed"
                                    : "")
                        );
                        try {
                            if (
                                typeof oslCheckSecurityAlerts === "function"
                            ) {
                                oslCheckSecurityAlerts();
                            }
                        } catch (_) {}
                    })
                    .catch(function (e) {
                        // Don't set the cooldown — let the next
                        // exhausted-retry tick try again.
                        console.log(
                            "[OSL] auto-recovery: identity re-fetch not done (" +
                                (e && e.message ? e.message : e) +
                                ") — cooldown not armed, will retry"
                        );
                    });
                return;
            }
            const cmd =
                kind === "skdm"
                    ? "osl_build_skdm_request"
                    : "osl_build_session_reset";
            const args =
                kind === "skdm"
                    ? { scopeInput: scopeInput, peerDiscordId: peer }
                    : { peerDiscordId: peer };
            console.log(
                "[OSL] auto-recovery: requesting " +
                    kind +
                    " recovery from peer=" +
                    peer +
                    " scope=" +
                    scopeKey
            );
            Promise.resolve(invoke(cmd, args))
                .then(function (wire) {
                    if (typeof wire !== "string" || wire.indexOf("DPC0::") !== 0) {
                        console.log(
                            "[OSL] auto-recovery: " +
                                kind +
                                " build returned no wire (skipped)"
                        );
                        return null;
                    }
                    // Phase 6.4: auto-recovery requests go to the
                    // peer's keyserver inbox, not the Discord
                    // channel. peer is the single recipient.
                    //
                    // BUGFIX: a SESSION_RESET is peer-level, so the
                    // recovery path has no scopeInput — oslSendControlOob
                    // then bailed with "no_scope" and the reset NEVER
                    // delivered, so a v=4 DM desync could never heal. A
                    // v=4 desync is always a DM, so synthesize the DM
                    // scope from the peer for the inbox label. (The SKDM
                    // path always has a real scopeInput.)
                    const effScope =
                        scopeInput || { kind: "dm", id: peer };
                    return oslSendControlOob([peer], effScope, wire);
                })
                .then(function (oobRes) {
                    if (oobRes && oobRes.ok > 0) {
                        // Only arm the cooldown on a confirmed
                        // successful inbox POST so transient
                        // throttle / no-pubkey / IPC failures
                        // remain retryable.
                        recvRecoveryCooldown.set(cdKey, Date.now());
                        console.log(
                            "[OSL] auto-recovery: " +
                                kind +
                                " request posted to inbox peer=" +
                                peer +
                                " channel=" +
                                channelId
                        );
                    }
                })
                .catch(function (e) {
                    // Don't set the cooldown — recv stays non-terminal
                    // and the next exhausted-retry window retries.
                    console.log(
                        "[OSL] auto-recovery: " +
                            kind +
                            " request not sent (" +
                            (e && e.message ? e.message : e) +
                            ") — cooldown not armed, will retry"
                    );
                });
        } catch (e) {
            console.log(
                "[OSL] auto-recovery: emit threw " +
                    (e && e.message ? e.message : e)
            );
        }
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
                    // Pre-paint hide for cached-hidden msgIds. Runs
                    // BEFORE oslScanLiAttachmentsV2 so we suppress
                    // visible flash even if attachment scan triggers
                    // additional layout. Synchronous + cheap (Map.get).
                    oslApplyCachedHideToLi(root);
                    oslScanLiAttachmentsV2(root).catch(function () {});
                }
                if (root.querySelectorAll) {
                    const lis = root.querySelectorAll(
                        'li[id^="chat-messages-"]'
                    );
                    for (const li of lis) {
                        oslApplyCachedHideToLi(li);
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
                        // Probe-5 refinement: clear recvDone +
                        // recvRetries ONLY when the live decrypt
                        // didn't succeed in-session (recvPlaintext
                        // not set). Previously this cleared
                        // unconditionally, which caused successfully-
                        // rendered messages to get re-dispatched on
                        // every channel-switch -- the IPC roundtrip
                        // would briefly show ciphertext in the DOM
                        // before re-applying. Now: if live decrypt
                        // already produced plaintext, leave recvDone
                        // intact; if it failed, clear so the next
                        // sweep tick re-tries via the loadedHistory
                        // short-circuit.
                        if (!recvPlaintext.has(mid)) {
                            recvDone.delete(mid);
                            recvRetries.delete(mid);
                        }
                        const span = document.getElementById(
                            RECV_MESSAGE_ID_PREFIX + mid
                        );
                        let rendered = false;
                        if (span) {
                            const t = span.textContent || "";
                            if (oslMessageIsStego(t)) {
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
            // Sidebar channel-lock sweep. Idempotent + cheap; only
            // touches the DOM when selection changes or state needs
            // refreshing.
            try {
                oslSweepSidebarChannelLock();
            } catch (_) {}
            // Blank-row sweep. Cached + viewport-gated: cached
            // rows skip the textContent read; uncached rows are
            // only evaluated when in (or near) the viewport.
            // recvApplyPlaintext invalidates the cache on decrypt.
            // Wrapped in scroll preservation so hides don't pull
            // the user up the chat history.
            try {
                oslWithScrollPreservation(oslSweepBlankRows);
            } catch (_) {}
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
                    // Switch detected: kick fast re-paints over the
                    // next ~250ms so the # → 🔒 swap (sidebar) AND the
                    // header lock land as soon as Discord paints the
                    // newly-selected channel, instead of waiting up to
                    // a full 1s for the next periodic tick. Switching
                    // SERVERS changes the channel id too, so this also
                    // covers the "locks don't show until I click a
                    // channel" case. force:true bypasses the
                    // scope-key throttle on the header refresh.
                    const repaintLocks = function () {
                        try {
                            oslSweepSidebarChannelLock();
                        } catch (_) {}
                        try {
                            oslRefreshHeaderState({ force: true });
                        } catch (_) {}
                    };
                    nativeSetTimeout(repaintLocks, 0);
                    nativeSetTimeout(repaintLocks, 80);
                    nativeSetTimeout(repaintLocks, 250);
                }
            } catch (e) {
                console.log(
                    "[OSL] history channel-switch threw: " +
                        (e && e.message ? e.message : e)
                );
            }

            // Probe-3 final: re-apply the smart SKDM hide on every
            // tick so Discord's re-mount on channel re-entry doesn't
            // restore the visible ciphertext blob. oslHideSkdmDom is
            // idempotent — it skips elements already hidden.
            if (oslSkdmHiddenMsgIds.size > 0) {
                for (const _mid of oslSkdmHiddenMsgIds) {
                    try {
                        oslHideSkdmDom(_mid);
                    } catch (_) {}
                }
            }

            const divs = document.querySelectorAll(RECV_MESSAGE_DIV_SELECTOR);
            let cachedCount = 0;
            let dispatchedCount = 0;
            for (const div of divs) {
                const text = div.textContent;
                if (!text) continue;
                // Probe-5 v5: hide-pass uses the permissive contains
                // check so chrome-prefixed messages also collapse.
                // The dispatch / cache path below still gates on the
                // strict prefix to avoid mangling cipher wires.
                if (oslMessageContainsStego(text)) {
                    try {
                        oslAutoHideCiphertext(div);
                    } catch (_) {}
                }
                if (!oslMessageIsStego(text)) continue;
                const messageId = recvMessageIdOf(div);
                // Probe-5 perf-revert follow-up: check cached
                // plaintext FIRST and re-apply immediately when
                // present. Auto-hide is for genuinely-pending
                // decrypts only; for messages we've already
                // decrypted (Discord just re-rendered the cipher
                // span), applying directly avoids the brief
                // empty-bubble flicker between auto-hide and the
                // next sweep re-apply.
                const cached =
                    recvPlaintext.get(messageId) ||
                    loadedHistory.get(messageId) ||
                    selfSentPlaintext.get(messageId);
                if (cached) {
                    recvApplyPlaintext(div, cached);
                    recvDone.add(messageId);
                    cachedCount++;
                    continue;
                }
                // Probe-2 Boot Bug 2: in the 5s editOverlayLocallyApplied
                // window after a successful self-edit, the sweep used to
                // happily re-dispatch a decrypt on the bounced-back NEW
                // ciphertext if Discord's MESSAGE_UPDATE swapped the
                // textContent — causing a visible flash + double-
                // dispatch. recvHandleDiv (mutation observer) honoured
                // the flag; the sweep didn't. Skip dispatch here too;
                // the 5s flag is short enough that legitimate stale
                // covers still get picked up by the next tick after
                // it clears.
                if (editOverlayLocallyApplied.has(messageId)) continue;
                // Probe-5 v4: re-enable sweep auto-hide so DPC0::
                // messages that were in DOM at startup (scrollback
                // on channel open, where the mutation observer
                // didn't fire) also get hidden. CSS-driven hide is
                // just a data-attribute toggle, so the per-tick
                // cost is an attribute check + a batched layout
                // pass; the browser's default scroll-anchor keeps
                // visible content stable when rows above the
                // viewport collapse. oslAutoHideCiphertext is
                // idempotent (early-returns on already-marked divs)
                // so subsequent ticks are no-ops.
                oslAutoHideCiphertext(div);
                if (recvDone.has(messageId)) continue;
                if (recvInFlight.has(messageId)) continue;
                recvDispatchDecrypt(div, messageId, text);
                dispatchedCount++;
            }

            // Fix B: attachment scanning was mutation-observer-only,
            // with NO sweep fallback (the text loop above `continue`s
            // on empty-text attachment messages). The single scan
            // fired before Discord rendered the card and was never
            // retried; gateway-delivered messages also have no URL
            // cache. Give attachments the same mutation-independent
            // retry the text path has: re-scan every chat-messages
            // <li> that hasn't been decrypted yet. oslScanLiAttachmentsV2
            // is idempotent (cache-replay when decrypted, cheap early
            // return when no candidates), so re-running per tick is
            // safe; the count is bounded by visible messages.
            let attachScanned = 0;
            try {
                const lis = document.querySelectorAll(
                    'li[id^="chat-messages-"]'
                );
                for (const li of lis) {
                    const lm = /chat-messages-(?:\d{15,22})-(\d{15,22})/.exec(
                        li.id || ""
                    );
                    if (!lm) continue;
                    const amid = lm[1];
                    // Skip messages already known to have no attachment.
                    // The mutation observer still re-scans on a real DOM
                    // change (it clears the mark on entry), so this only
                    // suppresses the wasteful every-tick re-walk of plain
                    // text messages that was pegging the main thread.
                    if (oslAttScannedEmpty.has(amid)) {
                        continue;
                    }
                    if (
                        window.__oslAttachmentDecrypted &&
                        window.__oslAttachmentDecrypted.has(amid)
                    ) {
                        // Already decrypted; the scanner's own
                        // cache-replay path re-applies the blob if
                        // React swapped the element. Still call it so
                        // a re-mounted element is re-swapped, but
                        // don't count it as a fresh scan.
                        oslScanLiAttachmentsV2(li).catch(function () {});
                        continue;
                    }
                    oslScanLiAttachmentsV2(li).catch(function () {});
                    attachScanned++;
                }
            } catch (_) {}

            if (OSL_DEBUG_SWEEP) {
                console.log(
                    "[OSL] periodic sweep tick (msgs=" +
                        divs.length +
                        ", cached=" +
                        cachedCount +
                        ", dispatched=" +
                        dispatchedCount +
                        ", attach_scanned=" +
                        attachScanned +
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
            // Wrap the entire batch in scroll preservation. Both
            // `recvScanSubtree` (cached-hide on <li> mount) and
            // `recvHandleDiv` (insertion-time DPC0:: hide) collapse
            // rows; doing all of them inside one snapshot/restore
            // keeps the visible viewport anchored.
            oslWithScrollPreservation(function () {
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

        // Phase 6.4: control-inbox drain loop. Polls keyserver
        // every CONTROL_INBOX_POLL_INTERVAL_MS for pending control
        // wires (SKDM, burn, SESSION_RESET, SKDM_REQUEST) addressed
        // to this user. Each tick is best-effort: a single failed
        // drain just leaves the rows on the server for the next
        // tick. Stored on `window` so the user can inspect / cancel
        // from DevTools.
        let inboxDrainInFlight = false;
        async function inboxDrainTick() {
            if (inboxDrainInFlight) return;
            inboxDrainInFlight = true;
            try {
                const resp = await oslInvoke("osl_control_inbox_drain", {});
                if (resp && resp.ok) {
                    const applied =
                        typeof resp.value === "number" ? resp.value : 0;
                    if (applied > 0) {
                        console.log(
                            "[OSL] control_inbox drain: applied=" + applied
                        );
                        // Phase 6.4 cache-reliability fix: applied SKDMs
                        // arrive via inbox now (not via the live decrypt
                        // IPC), so the per-sender v=5 revive sweep that
                        // the live SKDM_APPLIED path runs is bypassed.
                        // Mirror it here: clear terminal/retry state for
                        // every stuck v=5 cover currently in the DOM so
                        // the next periodic sweep re-dispatches with the
                        // freshly-installed sender chain. Author-
                        // unrestricted (we don't know which senders just
                        // installed) -- worst case is wasted dispatches
                        // on rows whose chain didn't change, all gated
                        // by recvPlaintext.has() short-circuit.
                        try {
                            const _divs = document.querySelectorAll(
                                RECV_MESSAGE_DIV_SELECTOR
                            );
                            let _revived = 0;
                            for (const _d of _divs) {
                                const _mid = recvMessageIdOf(_d);
                                if (!_mid) continue;
                                let _isV5 =
                                    oslCoverWireVersion(
                                        _d.textContent || ""
                                    ) === 5;
                                if (
                                    !_isV5 &&
                                    window.__oslProseWireByMsgId
                                ) {
                                    const _cw =
                                        window.__oslProseWireByMsgId.get(
                                            _mid
                                        );
                                    if (typeof _cw === "string") {
                                        _isV5 =
                                            oslCoverWireVersion(_cw) === 5;
                                    }
                                }
                                if (!_isV5) continue;
                                if (recvPlaintext.has(_mid)) continue;
                                recvDone.delete(_mid);
                                recvRetries.delete(_mid);
                                recvAuthorRetryCount.delete(_mid);
                                _revived++;
                            }
                            if (_revived > 0) {
                                console.log(
                                    "[OSL] inbox drain revive: cleared " +
                                        "terminal/retry state for " +
                                        _revived +
                                        " stuck v=5 message(s)"
                                );
                            }
                            // Burn markers / scope flag changes can also
                            // arrive via inbox; refresh the header so
                            // the lock badge reflects new state.
                            try {
                                oslRefreshHeaderState({ force: true });
                            } catch (_) {}
                        } catch (_) {}
                    }
                } else if (resp && !resp.ok) {
                    // Quietly log; transient keyserver issues are
                    // expected and don't warrant a toast.
                    console.log(
                        "[OSL] control_inbox drain err: " +
                            (resp && resp.error)
                    );
                }
            } catch (e) {
                console.log(
                    "[OSL] control_inbox drain threw: " +
                        (e && e.message ? e.message : e)
                );
            } finally {
                inboxDrainInFlight = false;
            }
        }
        const inboxDrainIntervalId = nativeSetInterval(
            inboxDrainTick,
            CONTROL_INBOX_POLL_INTERVAL_MS
        );
        window.__OSL_INBOX_DRAIN_INTERVAL__ = inboxDrainIntervalId;
        // Kick a first tick immediately so a relaunch picks up any
        // queued control wires without waiting a full poll interval.
        inboxDrainTick();
        console.log(
            "[OSL] control_inbox drain registered (interval=" +
                CONTROL_INBOX_POLL_INTERVAL_MS +
                "ms, id=" +
                String(inboxDrainIntervalId) +
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

    // ============================================================
    // 9-D: onboarding tour driver + VPN warning installer.
    //
    // The tour spans slides 1..9. Slides 1-5 and 8-9 fire in the
    // Discord main webview (this file); slides 6-7 fire in the
    // settings window (settings_window.html). Boot.js owns the
    // driver state machine for main-side slides and the cross-window
    // event handshake.
    //
    // Locked slide copy lives in TOUR_SLIDES; do not paraphrase —
    // the user-facing spec mandates exact wording.
    // ============================================================
    const TOUR_SLIDES = {
        1: {
            title: "Welcome to OSL",
            body:
                "OSL adds end-to-end encryption to Discord. Your messages stay private from Discord, your network, and anyone else watching.\n\nThis quick tour shows you what each button does. It takes about a minute.",
            buttonLabel: "Start →",
            target: null,
            skippable: false,
        },
        2: {
            title: "The encrypt toggle",
            body:
                "This lock icon appears in every channel header. Click to cycle through encryption states:\n\n🔓 No one in this channel is set up — messages are plain\n🟡 Some people are set up — messages encrypted for them\n🔒 Everyone is set up — fully encrypted channel",
            buttonLabel: "Next →",
            targetSelector: "[data-osl-encrypt-toggle]",
            skippable: true,
        },
        3: {
            title: "Burn this conversation",
            body:
                "Burns your encryption keys for this channel only. Messages you sent before become permanently unreadable — even to you.\n\nUse this if you no longer trust the people in this channel with your past messages.",
            buttonLabel: "Next →",
            targetSelector: "[data-osl-burn-btn]",
            skippable: true,
        },
        4: {
            title: "Account burn",
            body:
                "Burns everything. Every key, every conversation, every peer you've ever talked to. All your encrypted history becomes permanently unreadable.\n\nHold the icon for 3 seconds, or press Ctrl + Shift + Backspace twice.",
            buttonLabel: "Next →",
            targetSelector: "[data-osl-account-burn]",
            skippable: true,
        },
        5: {
            title: "Open your settings",
            body:
                "Click the gear to open OSL settings. We'll show you two features in there before finishing up.",
            buttonLabel: "Open settings →",
            targetSelector: "[data-osl-settings-btn='1']",
            skippable: true,
        },
        8: {
            title: "One more thing",
            body:
                "OSL encrypts everything on your computer, too — your keys, your message history, your peer list. None of it can be read without your password.\n\nSet a strong password on the next screen. If you forget it, your encrypted history is permanently lost. There is no recovery.",
            buttonLabel: "Set my password →",
            target: null,
            skippable: false,
        },
        // Slide 9 (password form) is constructed dynamically — it
        // embeds an HTMLFormElement in the spotlight card.
    };

    function oslTourWaitForElement(selector, timeoutMs) {
        const deadline = Date.now() + (timeoutMs || 10000);
        return new Promise(function (resolve) {
            function tick() {
                const el = document.querySelector(selector);
                if (el) {
                    resolve(el);
                    return;
                }
                if (Date.now() > deadline) {
                    resolve(null);
                    return;
                }
                nativeSetTimeout(tick, 200);
            }
            tick();
        });
    }

    function oslTourBuildPasswordForm(onSubmit) {
        const wrap = document.createElement("form");
        wrap.style.display = "flex";
        wrap.style.flexDirection = "column";
        wrap.style.gap = "10px";
        wrap.style.marginBottom = "16px";

        // 9-D-FIX1: copy reconciled to V1's actual 6-char floor
        // enforced by cmd_osl_set_main_password. The earlier "8 chars"
        // copy lied — accepted 6 but flagged it as "Too short".
        const pw = document.createElement("input");
        pw.type = "password";
        pw.placeholder = "Password";
        pw.autocomplete = "new-password";
        pw.minLength = 6;
        pw.style.padding = "8px 10px";
        pw.style.borderRadius = "4px";
        pw.style.background = "var(--input-background, #1e1f22)";
        pw.style.color = "var(--text-normal, #dbdee1)";
        pw.style.border = "1px solid var(--background-modifier-accent, #3f4147)";
        pw.style.fontSize = "14px";
        wrap.appendChild(pw);

        const confirm = document.createElement("input");
        confirm.type = "password";
        confirm.placeholder = "Confirm password";
        confirm.autocomplete = "new-password";
        confirm.minLength = 6;
        confirm.style.padding = "8px 10px";
        confirm.style.borderRadius = "4px";
        confirm.style.background = "var(--input-background, #1e1f22)";
        confirm.style.color = "var(--text-normal, #dbdee1)";
        confirm.style.border = "1px solid var(--background-modifier-accent, #3f4147)";
        confirm.style.fontSize = "14px";
        wrap.appendChild(confirm);

        const helper = document.createElement("div");
        helper.textContent = "At least 6 characters. Longer is stronger.";
        helper.style.fontSize = "12px";
        helper.style.color = "var(--text-muted, #949ba4)";
        wrap.appendChild(helper);

        const strength = document.createElement("div");
        strength.style.fontSize = "12px";
        strength.style.color = "var(--text-muted, #949ba4)";
        strength.style.minHeight = "16px";
        wrap.appendChild(strength);

        const MUTED = "var(--text-muted, #949ba4)";
        const DANGER = "var(--status-danger, #ed4245)";
        const WARN = "#f0b132";
        const OK_NEUTRAL = "var(--text-normal, #dbdee1)";
        const STRONG_OK = "var(--status-positive, #23a55a)";

        function rateStrength(s) {
            if (!s) return { label: "", color: MUTED };
            if (s.length < 6) {
                return { label: "Too short (" + s.length + "/6)", color: DANGER };
            }
            if (s.length < 10) return { label: "Weak", color: WARN };
            if (s.length < 16) return { label: "OK", color: OK_NEUTRAL };
            return { label: "Strong", color: STRONG_OK };
        }
        pw.addEventListener("input", function () {
            const r = rateStrength(pw.value);
            strength.textContent = r.label;
            strength.style.color = r.color;
        });

        wrap.__oslSubmit = function () {
            if (pw.value.length < 6) {
                strength.textContent = "Password must be at least 6 characters.";
                strength.style.color = DANGER;
                return false;
            }
            if (pw.value !== confirm.value) {
                strength.textContent = "Passwords don't match.";
                strength.style.color = DANGER;
                return false;
            }
            return pw.value;
        };

        return wrap;
    }

    async function oslTourStartFromSlide(startSlide) {
        let currentSpotlight = null;
        function close() {
            if (currentSpotlight) {
                try {
                    currentSpotlight.close();
                } catch (_) {}
                currentSpotlight = null;
            }
        }
        async function persistAdvance(n) {
            try {
                await oslInvoke("osl_tour_advance", { slide: n });
            } catch (_) {}
        }
        async function persistComplete() {
            try {
                await oslInvoke("osl_tour_complete", {});
            } catch (_) {}
        }
        async function persistSkip() {
            try {
                await oslInvoke("osl_tour_skip", {});
            } catch (_) {}
        }
        async function jumpToSlide8() {
            close();
            await persistSkip();
            renderSlide(8);
        }

        async function renderSlide(n) {
            close();
            await persistAdvance(n);
            if (n === 6) {
                // Slide 6 lives in the settings window; main waits for
                // osl:tour_return_to_main. Nothing to render here.
                return;
            }
            if (n === 9) {
                renderPasswordSlide();
                return;
            }
            const slide = TOUR_SLIDES[n];
            if (!slide) {
                console.warn("[OSL] tour: no slide for n=" + n);
                return;
            }
            let target = null;
            if (slide.targetSelector) {
                target = await oslTourWaitForElement(slide.targetSelector, 10000);
                if (!target) {
                    console.warn(
                        "[OSL] tour: target missing for slide " +
                            n +
                            " (" +
                            slide.targetSelector +
                            "), advancing"
                    );
                    if (n < 9) {
                        renderSlide(n + 1);
                    } else {
                        await persistComplete();
                    }
                    return;
                }
            }
            const opts = {
                title: slide.title,
                body: slide.body,
                buttonLabel: slide.buttonLabel,
                target: target,
                onAdvance: function () {
                    handleAdvance(n);
                },
            };
            if (slide.skippable) {
                opts.onSkip = jumpToSlide8;
            }
            currentSpotlight = oslSpotlight(opts);
        }

        function renderPasswordSlide() {
            close();
            const form = oslTourBuildPasswordForm();
            const opts = {
                title: "Set your password",
                body:
                    "This password protects everything OSL stores on your computer. Make it strong. Write it down somewhere safe.",
                buttonLabel: "Encrypt my keys →",
                target: null,
                formContent: form,
                onAdvance: async function () {
                    const pwValue = form.__oslSubmit && form.__oslSubmit();
                    if (!pwValue) {
                        // Validation failed — re-render the slide.
                        renderPasswordSlide();
                        return;
                    }
                    // 9-D-FIX2: stuck-tour recovery. If a previous
                    // tour attempt set the password but the next
                    // launch couldn't read `tour.completed=true`
                    // (encrypted app_preferences read before gate),
                    // the tour replays and slide 9 would loop forever
                    // because `osl_set_main_password` refuses to
                    // overwrite an existing marker. Detect that case
                    // and treat it as a clean completion.
                    const statusRes = await oslInvoke("osl_password_status", {});
                    if (statusRes.ok && statusRes.value && statusRes.value.is_set) {
                        await persistComplete();
                        oslToast("Tour complete — your keys are already encrypted.");
                        return;
                    }
                    const res = await oslInvoke("osl_set_main_password", {
                        password: pwValue,
                    });
                    if (!res.ok) {
                        oslToast("Setting password failed: " + res.error);
                        renderPasswordSlide();
                        return;
                    }
                    await persistComplete();
                    oslToast("Welcome to OSL — your keys are now encrypted.");
                },
            };
            currentSpotlight = oslSpotlight(opts);
        }

        function handleAdvance(n) {
            if (n === 5) {
                // Slide 5 → open settings window, mark slide 6 as
                // the resume cursor, emit advance event so settings
                // picks it up on its end.
                oslInvoke("osl_open_settings_window", {}).then(async function (r) {
                    if (!r.ok) {
                        oslToast("Failed to open settings: " + r.error);
                        return;
                    }
                    await persistAdvance(6);
                    try {
                        if (
                            window.__TAURI__ &&
                            window.__TAURI__.event &&
                            typeof window.__TAURI__.event.emit === "function"
                        ) {
                            window.__TAURI__.event.emit(
                                "osl:tour_advance_to_slide",
                                { slide: 6 }
                            );
                        }
                    } catch (e) {
                        console.warn("[OSL] tour: emit advance_to_slide failed", e);
                    }
                });
                return;
            }
            if (n === 8) {
                renderSlide(9);
                return;
            }
            renderSlide(n + 1);
        }

        // Boot the tour at startSlide. Slides 6/7 live in settings —
        // if startSlide is 6 or 7, we open settings and wait for the
        // settings-side handler to surface the slide.
        if (startSlide === 6 || startSlide === 7) {
            const r = await oslInvoke("osl_open_settings_window", {});
            if (r.ok) {
                try {
                    if (
                        window.__TAURI__ &&
                        window.__TAURI__.event &&
                        typeof window.__TAURI__.event.emit === "function"
                    ) {
                        window.__TAURI__.event.emit("osl:tour_advance_to_slide", {
                            slide: startSlide,
                        });
                    }
                } catch (_) {}
            }
            return;
        }
        renderSlide(startSlide);
    }

    let oslTourActive = false;

    /**
     * 9-F0-FIX2 B: poll the DOM for positive evidence that the user
     * is in Discord's logged-in shell, not on the login / register
     * screen. The pre-fix tour install waited on
     * `[class*="title_"], [class*="guilds_"]` which matched some
     * elements on Discord's auth pages too — slide 1 of the tour
     * would render on top of the login form before the user had
     * even authenticated.
     *
     * We now require TWO selectors to both resolve at the same
     * time, both of which exist only in the logged-in shell:
     *   - `nav[class*="guilds_"]`             — left server rail
     *   - `section[class*="panels__"]`        — bottom-left user pane
     *
     * Neither is present on Discord's auth pages. Polling continues
     * until the user signs in (up to 30 min, since they may take
     * arbitrary time on the auth page) or the page navigates.
     */
    function oslTourWaitForLoggedIn(timeoutMs) {
        const deadline = Date.now() + (timeoutMs || 30 * 60 * 1000);
        return new Promise(function (resolve) {
            function tick() {
                const guildsRail = document.querySelector('nav[class*="guilds_"]');
                const userPanel = document.querySelector('section[class*="panels__"]');
                if (guildsRail && userPanel) {
                    resolve(true);
                    return;
                }
                if (Date.now() > deadline) {
                    resolve(false);
                    return;
                }
                window.setTimeout(tick, 500);
            }
            tick();
        });
    }

    async function oslInstallTour() {
        if (oslTourActive) return;
        const state = await oslInvoke("osl_tour_get_state", {});
        if (!state.ok) {
            console.warn("[OSL] tour: get_state failed:", state.error);
            return;
        }
        if (state.value && state.value.completed) {
            return;
        }
        // 9-F0-FIX2 B: require POSITIVE logged-in detection (guilds
        // rail AND user panel both present) before the tour fires.
        // Pre-fix used a single-selector wait that matched both the
        // logged-in channel header AND a login-page header element,
        // letting slide 1 render on top of the login form.
        const loggedIn = await oslTourWaitForLoggedIn(30 * 60 * 1000);
        if (!loggedIn) {
            console.warn(
                "[OSL] tour: Discord logged-in shell not detected within 30 min; deferring"
            );
            return;
        }
        oslTourActive = true;
        const startSlide = state.value.last_slide && state.value.last_slide > 0
            ? state.value.last_slide
            : 1;
        console.log("[OSL] tour: starting at slide " + startSlide);
        oslTourStartFromSlide(startSlide);
    }

    function oslTourWireCrossWindowReturn() {
        const event =
            window.__TAURI__ && window.__TAURI__.event
                ? window.__TAURI__.event
                : null;
        if (!event || typeof event.listen !== "function") return;
        try {
            event.listen("osl:tour_return_to_main", function () {
                // Settings sent us back at slide 7-end. Render slide 8.
                console.log("[OSL] tour: return_to_main received");
                if (!oslTourActive) {
                    oslTourActive = true;
                    oslTourStartFromSlide(8);
                } else {
                    // Active tour: drive the state forward.
                    oslTourStartFromSlide(8);
                }
            });
        } catch (e) {
            console.warn("[OSL] tour: listener wire failed", e);
        }
        try {
            event.listen("osl:tour_replay_requested", function () {
                console.log("[OSL] tour: replay_requested received");
                oslTourActive = false;
                oslTourStartFromSlide(1);
            });
        } catch (e) {
            console.warn("[OSL] tour: replay listener wire failed", e);
        }
    }

    // W4: oslInstallVpnWarning removed with the VPN feature (broken
    // heuristic + per-launch IP leak to ipapi.co). oslBanner stays —
    // it's the generic banner used by the canary below.

    // ============================================================
    // 9-TD1.3: Discord-update canary.
    //
    // Discord redesigns silently — a class rename can break the
    // lock icon injection, the scanner anchor, the composer toggle,
    // or the profile-popup button without any error in our code.
    // The user just sees "OSL doesn't seem to be working." This
    // canary probes a fixed list of selectors ~10s after page load,
    // logs structured warnings for misses, and surfaces a banner if
    // critical surfaces are missing — turning silent UI breakage
    // into "OSL UI partially broken — Discord may have updated."
    //
    // We don't try to auto-recover. The expected response is a
    // selector patch from a developer; the canary just surfaces the
    // need.
    // ============================================================
    const OSL_CANARY_CHECKS = [
        {
            name: "channel_header",
            critical: true,
            probe: function () {
                return typeof oslFindChannelHeader === "function"
                    ? oslFindChannelHeader()
                    : null;
            },
        },
        {
            name: "user_panel",
            critical: true,
            probe: function () {
                return oslAnchorResolve("userPanel");
            },
        },
        {
            name: "composer",
            critical: true,
            probe: function () {
                return oslAnchorResolve("composer");
            },
        },
        {
            name: "guilds_rail",
            critical: true,
            probe: function () {
                return oslAnchorResolve("guildsRail");
            },
        },
        // Message content is keyed on per-message ids; if no channel
        // is open, none will be present. Treat as informational.
        {
            name: "message_content_present",
            critical: false,
            probe: function () {
                return document.querySelector('[id^="message-content-"]');
            },
        },
        // Our own settings-gear inject. Tests both that
        // `panels__` was found AND that `oslSettingsGearInject`
        // succeeded — confirms our injection pipeline still works.
        {
            name: "settings_gear_injected",
            critical: false,
            probe: function () {
                return document.querySelector("[data-osl-settings-btn='1']");
            },
        },
        // Profile popouts only exist when the user has opened one,
        // so this is best-effort. We probe but don't penalise misses.
        {
            name: "profile_surface_when_open",
            critical: false,
            probe: function () {
                return typeof oslFindProfileSurface === "function"
                    ? oslFindProfileSurface()
                    : null;
            },
        },
    ];

    let oslCanaryRan = false;
    // TD3-1.1 + W3: retry-before-escalate schedule. A single early
    // probe escalated to "fail" if Discord hadn't finished rendering
    // — common on cold-start under WSL2 / slower disks — and popped
    // the "OSL UI broken" banner on a perfectly healthy launch. The
    // probes now go through the resilient anchor resolver AND the
    // schedule gained a long final tail: only criticalMissing > 0 on
    // the FINAL attempt (~2 min total) counts as a real, persistent
    // break worth surfacing. Any earlier pass short-circuits silently
    // (no banner; "pass on retry N"). This is the #7 ask — the popup
    // appears only on genuine, sustained breakage.
    const OSL_CANARY_RETRY_DELAYS_MS = [10000, 15000, 30000, 60000];

    // Returns { level, criticalMissing, totalMissing, results } without
    // any logging or banner side-effects — the orchestrator decides
    // whether to log/banner based on attempt number.
    function oslProbeDomOnce() {
        const results = [];
        let criticalMissing = 0;
        let totalMissing = 0;
        for (const check of OSL_CANARY_CHECKS) {
            let el = null;
            try {
                el = check.probe();
            } catch (e) {
                el = null;
            }
            const ok = !!el;
            results.push({ name: check.name, ok: ok, critical: !!check.critical });
            if (!ok) {
                totalMissing++;
                if (check.critical) criticalMissing++;
            }
        }
        let level;
        if (criticalMissing === 0) level = "pass";
        else if (criticalMissing <= 1) level = "degraded";
        else level = "fail";
        return {
            level: level,
            criticalMissing: criticalMissing,
            totalMissing: totalMissing,
            results: results,
        };
    }

    function oslRunDomCanary(attempt) {
        if (oslCanaryRan) return;
        // 9-F0-FIX1: skip canary on Discord login / register routes.
        // None of the critical selectors (channel header, user panel,
        // composer, guilds rail) exist in the logged-out shell, so
        // the canary would always report "fail" and pop the banner
        // for every clean-install launch. The post-login navigation
        // re-injects boot.js and re-arms the canary on /channels/.
        try {
            const path =
                (typeof window !== "undefined" &&
                    window.location &&
                    window.location.pathname) ||
                "";
            if (
                path === "/login" ||
                path.startsWith("/login/") ||
                path === "/register" ||
                path.startsWith("/register/")
            ) {
                console.log(
                    "[OSL canary] skipped on " + path + " (no logged-in shell)"
                );
                oslCanaryRan = true;
                return;
            }
        } catch (_) {}

        const attemptNum = typeof attempt === "number" ? attempt : 1;
        const isFinalAttempt = attemptNum >= OSL_CANARY_RETRY_DELAYS_MS.length;
        const probe = oslProbeDomOnce();

        if (probe.level === "pass") {
            // Terminal pass — done.
            oslCanaryRan = true;
            console.log(
                "[OSL canary] result=pass" +
                    " critical_missing=0" +
                    " total_missing=" +
                    probe.totalMissing +
                    " of " +
                    OSL_CANARY_CHECKS.length +
                    (attemptNum > 1 ? " (passed on retry " + attemptNum + ")" : "")
            );
            return;
        }

        if (!isFinalAttempt) {
            // Non-terminal probe — log at debug (single line, no banner)
            // and schedule the next retry. This is the path that used to
            // false-banner the user on a slow render.
            console.log(
                "[OSL canary] attempt " +
                    attemptNum +
                    "/" +
                    OSL_CANARY_RETRY_DELAYS_MS.length +
                    " critical_missing=" +
                    probe.criticalMissing +
                    " — retrying in " +
                    OSL_CANARY_RETRY_DELAYS_MS[attemptNum] +
                    "ms"
            );
            window.setTimeout(function () {
                oslRunDomCanary(attemptNum + 1);
            }, OSL_CANARY_RETRY_DELAYS_MS[attemptNum]);
            return;
        }

        // Final attempt still failing — this is the real surface.
        oslCanaryRan = true;
        for (const r of probe.results) {
            if (r.ok) continue;
            if (r.critical) {
                console.warn("[OSL canary] missing: " + r.name + " (critical)");
            } else {
                console.log("[OSL canary] absent: " + r.name + " (non-critical)");
            }
        }
        console.log(
            "[OSL canary] result=" +
                probe.level +
                " critical_missing=" +
                probe.criticalMissing +
                " total_missing=" +
                probe.totalMissing +
                " of " +
                OSL_CANARY_CHECKS.length +
                " (final after " +
                OSL_CANARY_RETRY_DELAYS_MS.length +
                " attempts)"
        );
        try {
            if (typeof oslBanner === "function") {
                oslBanner({
                    message:
                        probe.level === "fail"
                            ? "OSL UI broken — Discord may have updated. " +
                              "Several core features are missing their anchors. " +
                              "Check for an OSL update."
                            : "OSL UI partially broken — Discord may have updated. " +
                              "Some features may be missing or misplaced. " +
                              "Check for an OSL update.",
                    actions: [{ label: "Dismiss" }],
                });
            }
        } catch (e) {
            console.warn("[OSL canary] banner display failed:", e);
        }
    }

    function oslScheduleDomCanary() {
        // TD3-1.1: schedule first probe; oslRunDomCanary itself
        // chains subsequent retries via OSL_CANARY_RETRY_DELAYS_MS.
        window.setTimeout(function () {
            oslRunDomCanary(1);
        }, OSL_CANARY_RETRY_DELAYS_MS[0]);
    }

    if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", recvInstallObserver);
        // Beta 1.0: re-enabled. The edit-tab observer swaps plaintext
        // into Discord's NATIVE edit box regardless of how the edit was
        // triggered (toolbar pencil, context menu, OR up-arrow keyboard
        // shortcut — the overlay click handler misses the latter two).
        // The prose-cover detection in editTabHandleTextbox now fires
        // for prose covers, and interceptEditBody no-ops if the swap
        // didn't take, so the worst case is "edit does nothing" rather
        // than corrupting the message.
        document.addEventListener("DOMContentLoaded", editTabStartObserver);
        document.addEventListener("DOMContentLoaded", editOverlayInstall);
        document.addEventListener("DOMContentLoaded", oslInstallPhase7c);
        document.addEventListener("DOMContentLoaded", oslInstallKeybinds);
        document.addEventListener("DOMContentLoaded", oslInstallTour);
        document.addEventListener("DOMContentLoaded", oslTourWireCrossWindowReturn);
        document.addEventListener("DOMContentLoaded", oslScheduleDomCanary);
    } else {
        recvInstallObserver();
        editTabStartObserver();
        editOverlayInstall();
        oslInstallPhase7c();
        oslInstallKeybinds();
        oslInstallTour();
        oslTourWireCrossWindowReturn();
        oslScheduleDomCanary();
    }
})();

// =====================================================================
// Phase F0: deep-link smoke test
//
// The Rust on_open_url callback (main.rs, registered in `setup`)
// emits "osl:deep-link-received" with the full URL as payload
// whenever Windows delivers an osl://... activation. We:
//
//   1. Log the URL to the JS console (proves the Rust → JS event
//      pipe is alive).
//   2. Invoke `osl_test_deep_link(url)` to round-trip the URL
//      through Rust's parser (proves JS → Rust IPC works AFTER
//      receiving the event, which is the inverse direction from
//      the deep-link arrival itself).
//   3. Show an oslToast with the extracted token so the manual
//      verification matrix's UX expectation is satisfied.
//
// All three steps are wrapped in try/catch so a failure in any
// one doesn't break the others. F0 is plumbing-only; F2 replaces
// this listener with the real ad-session unlock flow (validate
// token against keyserver, reset foreground timer).
//
// Race note: if osl://... fires while OSL is launching (scenario
// a of the F0 verification matrix), the Rust on_open_url emit
// may happen before this listener registers. The Rust console
// log still proves the URL arrived; the toast may not appear on
// the very first activation. F2 will add a buffered-URL replay
// pattern; F0 just documents the race.
// =====================================================================
(async () => {
    try {
        const T = window.__TAURI__;
        if (!T || !T.event || !T.core) {
            console.warn("[OSL deep-link] __TAURI__ globals not available; listener not installed");
            return;
        }
        await T.event.listen("osl:deep-link-received", async (event) => {
            const url = event && event.payload;
            console.log("[OSL deep-link] event:", url);
            if (typeof url !== "string") {
                console.warn("[OSL deep-link] payload was not a string:", event);
                return;
            }
            try {
                const parsed = await T.core.invoke("osl_test_deep_link", { url });
                console.log("[OSL deep-link] parsed:", parsed);
                const token = (parsed && parsed.token) || "(none)";
                if (typeof oslToast === "function") {
                    oslToast("Deep link received: " + token, { durationMs: 5000 });
                }
            } catch (err) {
                console.error("[OSL deep-link] osl_test_deep_link invoke failed:", err);
            }
        });
        console.log("[OSL deep-link] listener registered");
    } catch (e) {
        console.warn("[OSL deep-link] listener install failed:", e);
    }
})();

