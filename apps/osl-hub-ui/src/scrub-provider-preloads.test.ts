import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { DISCORD_WEB_PRELOAD_SCHEMA, GMAIL_WEB_PRELOAD_SCHEMA, GmailWebScrubPreload, TELEGRAM_WEB_PRELOAD_SCHEMA } from "./scrub-provider-preloads";

const source = readFileSync(new URL("./scrub-provider-preloads.ts", import.meta.url), "utf8");

type PaceLogEntry =
  | { readonly kind: "action"; readonly label: string; readonly startedMs: number; readonly finishedMs: number; readonly active: number }
  | { readonly kind: "pause"; readonly label: string; readonly follows: string; readonly startedMs: number; readonly finishedMs: number; readonly configuredMs: number; readonly measuredMs: number };

describe("provider hosted-session preloads", () => {
  it("has a concrete Gmail-first schema and separate Discord and Telegram implementations", () => {
    expect(GMAIL_WEB_PRELOAD_SCHEMA).toMatchObject({ providerId: "gmail-web", allowedHosts: ["mail.google.com"], version: "gmail-web-ui-v1" });
    expect(DISCORD_WEB_PRELOAD_SCHEMA).toMatchObject({ providerId: "discord", allowedHosts: ["discord.com"] });
    expect(TELEGRAM_WEB_PRELOAD_SCHEMA).toMatchObject({ providerId: "telegram-web", allowedHosts: ["web.telegram.org"] });
    expect(source).toContain("class GmailWebScrubPreload");
    expect(source).toContain("class DiscordWebScrubPreload");
    expect(source).toContain("class TelegramWebScrubPreload");
  });

  it("contains no network, arbitrary evaluation, posting, joining, or reacting capability", () => {
    expect(source).not.toMatch(/\bfetch\s*\(|XMLHttpRequest|WebSocket|\.eval\s*\(|new Function|postMessage|sendMessage|joinGuild|addReaction/);
  });

  it("task 1430 fixture reader logs one scroll action at a time and pauses after every scroll", async () => {
    let clockMs = 0;
    let activeActions = 0;
    let scrolls = 0;
    let maxConcurrentActions = 0;
    let lastAction = "none";
    const log: PaceLogEntry[] = [];
    const heights = [600, 1_200, 1_200];
    const configuredPauseMs = 2_100;

    const root = {
      get scrollHeight() {
        return heights[Math.min(scrolls, heights.length - 1)];
      },
      querySelectorAll(_selector: string) {
        return [];
      },
      scrollTo(options: ScrollToOptions) {
        activeActions += 1;
        maxConcurrentActions = Math.max(maxConcurrentActions, activeActions);
        const startedMs = clockMs;
        clockMs += 7;
        scrolls += 1;
        lastAction = `scroll#${scrolls}`;
        log.push({
          kind: "action",
          label: lastAction,
          startedMs,
          finishedMs: clockMs,
          active: activeActions,
        });
        expect(options).toMatchObject({ top: heights[scrolls - 1], behavior: "auto" });
        activeActions -= 1;
      },
    };
    const document = {
      querySelector(selector: string) {
        return selector === GMAIL_WEB_PRELOAD_SCHEMA.historyRoot ? root : null;
      },
    };
    const location = { hostname: "mail.google.com", pathname: "/mail/u/0/#sent", hash: "#sent" };
    const preload = new GmailWebScrubPreload(
      document as unknown as Document,
      location as unknown as Location,
      "mail",
      "session-1",
      {
        scrollPauseMs: configuredPauseMs,
        wait: async (milliseconds) => {
          const startedMs = clockMs;
          clockMs += milliseconds;
          log.push({
            kind: "pause",
            label: `pause#${log.filter((entry) => entry.kind === "pause").length + 1}`,
            follows: lastAction,
            startedMs,
            finishedMs: clockMs,
            configuredMs: configuredPauseMs,
            measuredMs: milliseconds,
          });
        },
      },
    );

    const result = await preload.scrollHistory({ maxScrolls: 3, maxItems: 500, beforeUnixMs: 1_800_000_000_000 });
    const actions = log.filter((entry) => entry.kind === "action");
    const pauses = log.filter((entry) => entry.kind === "pause");
    const scrollsWithImmediateConfiguredPause = actions.filter((entry) => {
      const next = log[log.indexOf(entry) + 1];
      return next?.kind === "pause" && next.follows === entry.label && next.measuredMs === configuredPauseMs;
    });

    console.log(`TASK1430_CONFIGURED_SCROLL_PAUSE_MS=${configuredPauseMs}`);
    for (const entry of log) {
      if (entry.kind === "action") {
        console.log(`TASK1430_LOG_ACTION label=${entry.label} active=${entry.active} started_ms=${entry.startedMs} finished_ms=${entry.finishedMs}`);
      } else {
        console.log(`TASK1430_LOG_PAUSE label=${entry.label} follows=${entry.follows} configured_ms=${entry.configuredMs} measured_ms=${entry.measuredMs} started_ms=${entry.startedMs} finished_ms=${entry.finishedMs}`);
      }
    }
    console.log(`TASK1430_SCROLL_RESULT_COMPLETE=${result.complete}`);
    console.log(`TASK1430_ACTION_COUNT=${actions.length}`);
    console.log(`TASK1430_PAUSE_COUNT=${pauses.length}`);
    console.log(`TASK1430_MAX_CONCURRENT_ACTIONS=${maxConcurrentActions}`);
    console.log(`TASK1430_SCROLLS_WITH_IMMEDIATE_CONFIGURED_PAUSE=${scrollsWithImmediateConfiguredPause.length}/${actions.length}`);

    expect(result).toMatchObject({ ok: true, complete: true });
    expect(actions).toHaveLength(2);
    expect(pauses).toHaveLength(2);
    expect(maxConcurrentActions).toBe(1);
    expect(scrollsWithImmediateConfiguredPause).toHaveLength(actions.length);
  });
});
