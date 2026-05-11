// survey-discord-selectors.js
//
// Read-only DevTools probe for Phase 7c selector planning.
//
// USAGE
//   Open Discord in `cargo tauri dev`. Navigate to a DM with another
//   user. Click their avatar to open the profile popout. THEN paste
//   this script in DevTools console (the popout must be open before
//   pasting so the survey can inspect it). Save the JSON output.
//   Then navigate to a GC. Re-run. Then a server channel. Re-run.
//   Save each output to a separate file so 7c selector docs can
//   diff them.
//
// SAFETY
//   - IIFE-wrapped; the only global it touches is `window.__oslSurveyResults`.
//   - Pure DOM + React-fiber reads. Never mutates a node, attribute,
//     property, style, or React state.
//   - Catches every fiber walk in a try/catch so a hostile shape
//     never throws past the script boundary.
//
// OUTPUT
//   One JSON object printed to the console. Each surface follows:
//     {
//       "surface_name": {
//         "found":  true | false,
//         "selectors": { ... },
//         "react_fiber_path": "memoizedProps.user.id (depth N)",
//         "sample_value":     "<observed value or null>",
//         "notes":            "..."
//       }
//     }

(function () {
    "use strict";

    const REACT_FIBER_KEY_PATTERN =
        /^__reactFiber\$|^__reactInternalInstance\$/;
    const FIBER_WALK_MAX_DEPTH = 30;

    // ---- helpers ----

    /** Return the React fiber node attached to a DOM element, or null. */
    function fiberOf(el) {
        if (!el || typeof el !== "object") return null;
        for (const key of Object.keys(el)) {
            if (REACT_FIBER_KEY_PATTERN.test(key)) return el[key];
        }
        return null;
    }

    /**
     * Walk a fiber chain upward via `.return`, applying `probe(fiber)`
     * at each level. The first non-undefined return value wins.
     * Returns `{ value, depth }` on hit or null on miss. Every step
     * is wrapped in try/catch so a weird fiber shape can't throw.
     */
    function walkFiberUp(startEl, probe) {
        let fiber = fiberOf(startEl);
        for (let depth = 0; depth < FIBER_WALK_MAX_DEPTH && fiber; depth++) {
            try {
                const v = probe(fiber, depth);
                if (v !== undefined) return { value: v, depth: depth };
            } catch (e) {
                // Ignore — keep walking.
            }
            fiber = fiber.return;
        }
        return null;
    }

    /** First match of a CSS selector under the document. */
    function q(sel) {
        try {
            return document.querySelector(sel);
        } catch (e) {
            return null;
        }
    }

    /** All matches of a CSS selector under the document. */
    function qa(sel) {
        try {
            return Array.from(document.querySelectorAll(sel));
        } catch (e) {
            return [];
        }
    }

    /**
     * Read the `className` of an element as a string. Discord uses
     * a mix of `className` strings and `classList` tokens; we
     * normalise to the raw `className` string.
     */
    function cls(el) {
        if (!el) return null;
        const c = el.className;
        if (typeof c === "string") return c;
        if (c && typeof c.baseVal === "string") return c.baseVal; // SVG
        return null;
    }

    /** Compact element descriptor for output ("button.foo bar baz"). */
    function descriptor(el) {
        if (!el) return null;
        const tag = (el.tagName || "?").toLowerCase();
        const c = cls(el);
        return c ? `${tag}.${c.replace(/\s+/g, ".")}` : tag;
    }

    // ---- surfaces ----

    function surveyUserProfilePopout() {
        const popout = q('[class*="user-profile-popout"]');
        if (!popout) {
            return {
                found: false,
                notes:
                    "No element matched [class*='user-profile-popout']. " +
                    "Click a user's avatar to open the popout, then re-run.",
            };
        }
        // Inner action buttons — try a few candidates.
        const candidates = {
            "[class*='profileActions']": q(
                "[class*='user-profile-popout'] [class*='profileActions']"
            ),
            "[class*='button-']": q(
                "[class*='user-profile-popout'] [role='button'][class*='button-']"
            ),
            "[role='button']": q(
                "[class*='user-profile-popout'] [role='button']"
            ),
        };
        const buttonHits = {};
        for (const [sel, el] of Object.entries(candidates)) {
            buttonHits[sel] = el ? cls(el) : null;
        }

        // User id via fiber walk.
        const userIdHit = walkFiberUp(popout, function (f) {
            const p = f && f.memoizedProps;
            if (p && p.user && typeof p.user.id === "string") {
                return p.user.id;
            }
            // Some popouts namespace under `userId` directly.
            if (p && typeof p.userId === "string") return p.userId;
            return undefined;
        });

        return {
            found: true,
            selectors: {
                container_class: cls(popout),
                container_tag: popout.tagName.toLowerCase(),
                action_button_candidates: buttonHits,
            },
            react_fiber_path: userIdHit
                ? `memoizedProps.user.id (depth ${userIdHit.depth})`
                : null,
            sample_value: userIdHit ? userIdHit.value : null,
            notes:
                "container_class is the exact className on the popout " +
                "root; action_button_candidates is { selector → className } " +
                "for the first match of each candidate selector inside the " +
                "popout. Use the first non-null hit as the styling base for " +
                "the OSL 'Whitelist user…' button.",
        };
    }

    function surveyChannelHeader() {
        // Discord's channel header is a <section> at the top of the
        // chat column. The reliable hook is "section containing a
        // descendant whose textContent is the channel name" — but we
        // can't know the name a priori. Heuristic: find a <section>
        // whose first heading is non-empty.
        const sections = qa("section");
        let header = null;
        for (const s of sections) {
            const h = s.querySelector("h1, h2, h3");
            if (h && (h.textContent || "").trim().length > 0) {
                // Heuristic: must be near the top of the viewport.
                const rect = s.getBoundingClientRect();
                if (rect.top < 200) {
                    header = s;
                    break;
                }
            }
        }
        if (!header) {
            return {
                found: false,
                notes:
                    "No <section> with a populated heading near the viewport " +
                    "top. Make sure you're viewing a channel (not the home " +
                    "screen or settings), then re-run.",
            };
        }

        const headingEl = header.querySelector("h1, h2, h3");
        const iconButtons = Array.from(
            header.querySelectorAll("[role='button']")
        );
        const iconButtonClasses = iconButtons
            .slice(0, 8)
            .map(function (b) {
                return {
                    aria_label: b.getAttribute("aria-label"),
                    className: cls(b),
                };
            });

        // Channel-type heuristics.
        const path = (window.location && window.location.pathname) || "";
        const pathHeuristic = (function () {
            const m = path.match(/\/channels\/([^/]+)\/([^/]+)/);
            if (!m) return { kind: "unknown", reason: "no channel in path" };
            if (m[1] === "@me") {
                // DM or GC. DM has exactly one peer avatar inline.
                const avatars = header.querySelectorAll("img[src*='avatars/']");
                return {
                    kind: avatars.length === 1 ? "dm" : "gc",
                    reason: `path is /channels/@me/...; ${avatars.length} avatar(s) in header`,
                };
            }
            return {
                kind: "server_channel",
                reason: `path is /channels/<guild>/<channel> (guild=${m[1]})`,
            };
        })();

        // Hash-prefix heuristic (server text channels often render
        // a '#' glyph before the name).
        const hasHashGlyph =
            !!header.querySelector(
                "svg[class*='hash'], [class*='hashtag-'], [class*='icon-hash']"
            ) ||
            (headingEl && (headingEl.textContent || "").trim().startsWith("#"));
        const hashHeuristic = {
            saw_hash_marker: hasHashGlyph,
            note: "server text channels usually render a '#' icon; absence " +
                "isn't proof of DM/GC.",
        };

        // Channel + guild ids via fiber walk.
        const channelIdHit = walkFiberUp(header, function (f) {
            const p = f && f.memoizedProps;
            if (p && typeof p.channelId === "string") return p.channelId;
            if (p && p.channel && typeof p.channel.id === "string") {
                return p.channel.id;
            }
            return undefined;
        });
        const guildIdHit = walkFiberUp(header, function (f) {
            const p = f && f.memoizedProps;
            if (p && typeof p.guildId === "string") return p.guildId;
            if (p && p.guild && typeof p.guild.id === "string") {
                return p.guild.id;
            }
            return undefined;
        });
        const channelTypeHit = walkFiberUp(header, function (f) {
            const p = f && f.memoizedProps;
            if (
                p &&
                p.channel &&
                typeof p.channel.type === "number"
            ) {
                return p.channel.type;
            }
            return undefined;
        });

        return {
            found: true,
            selectors: {
                container_tag: header.tagName.toLowerCase(),
                container_class: cls(header),
                heading_tag: headingEl ? headingEl.tagName.toLowerCase() : null,
                heading_class: headingEl ? cls(headingEl) : null,
                heading_text: headingEl
                    ? (headingEl.textContent || "").trim().slice(0, 40)
                    : null,
                icon_button_sample: iconButtonClasses,
            },
            channel_type_heuristics: {
                path_based: pathHeuristic,
                hash_marker: hashHeuristic,
                fiber_channel_type: channelTypeHit
                    ? {
                          value: channelTypeHit.value,
                          interpretation:
                              channelTypeHit.value === 0
                                  ? "server text channel"
                                  : channelTypeHit.value === 1
                                  ? "DM"
                                  : channelTypeHit.value === 3
                                  ? "GC"
                                  : `other (${channelTypeHit.value})`,
                          react_fiber_path: `memoizedProps.channel.type (depth ${channelTypeHit.depth})`,
                      }
                    : null,
            },
            react_fiber_path: channelIdHit
                ? `memoizedProps.channelId (depth ${channelIdHit.depth})`
                : null,
            sample_value: channelIdHit ? channelIdHit.value : null,
            guild_id: guildIdHit
                ? {
                      value: guildIdHit.value,
                      react_fiber_path: `memoizedProps.guildId (depth ${guildIdHit.depth})`,
                  }
                : null,
            notes:
                "icon_button_sample lists the first 8 [role='button'] " +
                "descendants of the header for picking a style template. " +
                "Prefer fiber_channel_type over path_based when both are " +
                "available — it's the source Discord itself uses.",
        };
    }

    function surveyChannelAndGuildIdsFromMessages() {
        // Independent of the channel header — walk up from a rendered
        // message. message-content divs have ids like
        // "message-content-<snowflake>"; their fibers usually
        // expose channelId/guildId on a few ancestors up.
        const msg = q('[id^="message-content-"]');
        if (!msg) {
            return {
                found: false,
                notes:
                    "No rendered message found. Scroll to a channel with " +
                    "at least one message visible, then re-run.",
            };
        }
        const channelIdHit = walkFiberUp(msg, function (f) {
            const p = f && f.memoizedProps;
            if (p && typeof p.channelId === "string") return p.channelId;
            if (p && p.channel && typeof p.channel.id === "string") {
                return p.channel.id;
            }
            if (p && p.message && typeof p.message.channel_id === "string") {
                return p.message.channel_id;
            }
            return undefined;
        });
        const guildIdHit = walkFiberUp(msg, function (f) {
            const p = f && f.memoizedProps;
            if (p && typeof p.guildId === "string") return p.guildId;
            if (p && p.guild && typeof p.guild.id === "string") {
                return p.guild.id;
            }
            return undefined;
        });
        return {
            found: true,
            selectors: {
                anchor_element: descriptor(msg),
            },
            react_fiber_path: channelIdHit
                ? `memoizedProps.channelId (depth ${channelIdHit.depth})`
                : null,
            sample_value: channelIdHit ? channelIdHit.value : null,
            guild_id: guildIdHit
                ? {
                      value: guildIdHit.value,
                      react_fiber_path: `memoizedProps.guildId (depth ${guildIdHit.depth})`,
                  }
                : null,
            notes:
                "Cross-check against the channel-header fiber walk. " +
                "Disagreement here would be a bug in our scope detection.",
        };
    }

    function surveyMembersList() {
        // Server channels: right-side members panel.
        // Selectors over time have included:
        //   [class*="members-"]
        //   [class*="membersWrap-"]
        //   [aria-label="Members"]
        const candidates = {
            "[aria-label='Members']": q("[aria-label='Members']"),
            "[class*='membersWrap']": q("[class*='membersWrap']"),
            "[class*='members-']": q("[class*='members-']"),
        };
        let panel = null;
        const hits = {};
        for (const [sel, el] of Object.entries(candidates)) {
            hits[sel] = el ? cls(el) : null;
            if (!panel && el) panel = el;
        }
        // Each row in the panel.
        let row_sample = null;
        let row_count = null;
        if (panel) {
            const rows = panel.querySelectorAll(
                "[class*='member-'][class*='container'], [class*='member-'] [class*='content'], [role='listitem']"
            );
            row_count = rows.length;
            if (rows.length > 0) row_sample = cls(rows[0]);
        }

        // GC recipients via fiber walk from the channel header or
        // first message.
        const anchor = q("section") || q('[id^="message-content-"]');
        const recipientsHit = walkFiberUp(anchor, function (f) {
            const p = f && f.memoizedProps;
            if (
                p &&
                p.channel &&
                Array.isArray(p.channel.recipients) &&
                p.channel.recipients.length > 0
            ) {
                return p.channel.recipients.length;
            }
            if (
                p &&
                Array.isArray(p.recipients) &&
                p.recipients.length > 0
            ) {
                return p.recipients.length;
            }
            return undefined;
        });

        return {
            found: !!panel || !!recipientsHit,
            selectors: {
                panel_candidates: hits,
                row_sample_class: row_sample,
                row_count: row_count,
            },
            react_fiber_path: recipientsHit
                ? `memoizedProps.channel.recipients (depth ${recipientsHit.depth})`
                : null,
            sample_value: recipientsHit
                ? `recipients.length = ${recipientsHit.value}`
                : null,
            notes:
                "Server channels: members panel selectors. GCs: the " +
                "fiber-walked `recipients` array is authoritative (the " +
                "members panel typically isn't visible for GCs). For DM " +
                "scopes there's exactly one recipient.",
        };
    }

    function surveyBottomLeftUserArea() {
        // Discord's bottom-left "account" panel sits inside a
        // <section> with a recognisable class. The mic/deafen/gear
        // icons are inside it.
        const candidates = {
            "[class*='panels-']": q("[class*='panels-']"),
            "[class*='panel__']": q("[class*='panel__']"),
            "section[class*='account']": q("section[class*='account']"),
            "[aria-label='User area']": q("[aria-label='User area']"),
        };
        let area = null;
        const hits = {};
        for (const [sel, el] of Object.entries(candidates)) {
            hits[sel] = el ? cls(el) : null;
            if (!area && el) {
                // Sanity: must be at the bottom of the viewport.
                const r = el.getBoundingClientRect();
                if (r.bottom > window.innerHeight - 200) {
                    area = el;
                }
            }
        }
        let buttons = [];
        if (area) {
            buttons = Array.from(area.querySelectorAll("[role='button']"))
                .slice(0, 6)
                .map(function (b) {
                    return {
                        aria_label: b.getAttribute("aria-label"),
                        className: cls(b),
                    };
                });
        }
        return {
            found: !!area,
            selectors: {
                container_candidates: hits,
                container_chosen: area ? descriptor(area) : null,
                icon_button_sample: buttons,
            },
            notes:
                "Pick the candidate whose match is at the BOTTOM of the " +
                "viewport (we filtered by rect.bottom > innerHeight − 200). " +
                "icon_button_sample lists the existing controls so the " +
                "OSL gear injection can mimic their style.",
        };
    }

    function surveyCssCustomProperties() {
        // Read every CSS custom property whose name starts with one
        // of the requested prefixes from the document root.
        const PREFIXES = [
            "--background",
            "--text",
            "--brand",
            "--status",
            "--button",
        ];
        const cs = getComputedStyle(document.documentElement);
        const out = {};
        // CSSStyleDeclaration is iterable as an indexed object of
        // property names (only set ones — exactly what we want).
        for (let i = 0; i < cs.length; i++) {
            const name = cs[i];
            if (!name.startsWith("--")) continue;
            if (!PREFIXES.some((p) => name.startsWith(p))) continue;
            out[name] = cs.getPropertyValue(name).trim();
        }
        return {
            found: Object.keys(out).length > 0,
            selectors: {
                source: "getComputedStyle(document.documentElement)",
                filter_prefixes: PREFIXES,
            },
            sample_value: Object.keys(out).length,
            properties: out,
            notes:
                "Use these as the var(--…) values for any injected UI so " +
                "OSL chrome matches Discord's current theme. Empty " +
                "{properties} usually means Discord deferred the theme " +
                "load — wait a few seconds after channel load and re-run.",
        };
    }

    // ---- run all + emit ----

    const results = {
        captured_at: new Date().toISOString(),
        location_pathname:
            (window.location && window.location.pathname) || null,
        user_profile_popout: safe(surveyUserProfilePopout),
        channel_header: safe(surveyChannelHeader),
        channel_guild_ids_from_message: safe(
            surveyChannelAndGuildIdsFromMessages
        ),
        members_list: safe(surveyMembersList),
        bottom_left_user_area: safe(surveyBottomLeftUserArea),
        css_custom_properties: safe(surveyCssCustomProperties),
    };

    function safe(fn) {
        try {
            return fn();
        } catch (e) {
            return {
                found: false,
                error: e && e.message ? e.message : String(e),
                notes:
                    "Survey function threw — this is a bug in the script, " +
                    "not in Discord. Capture this output and the stack trace.",
            };
        }
    }

    window.__oslSurveyResults = results;
    console.log(JSON.stringify(window.__oslSurveyResults, null, 2));
})();
