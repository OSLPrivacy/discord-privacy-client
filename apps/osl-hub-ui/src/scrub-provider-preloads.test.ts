import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { DISCORD_WEB_PRELOAD_SCHEMA, GMAIL_WEB_PRELOAD_SCHEMA, TELEGRAM_WEB_PRELOAD_SCHEMA } from "./scrub-provider-preloads";

const source = readFileSync(new URL("./scrub-provider-preloads.ts", import.meta.url), "utf8");

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
});
