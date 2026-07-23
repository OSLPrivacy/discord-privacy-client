import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  normalizeDiscordTranscriptPreferences,
  validateDiscordTranscriptWindow,
  type DiscordProtectedTranscriptRow,
} from "./discord-protected-transcript";

const identity = {
  id: "osl:alice",
  displayName: "Alice <admin>",
  avatarFallback: "A",
  provenance: "verified-osl" as const,
};

const timestamp = { epochMs: 1_750_000_000_000, label: "Today at 9:41 PM" };

function rows(): DiscordProtectedTranscriptRow[] {
  return [
    { key: "m1", kind: "text", author: identity, timestamp, plaintext: "**literal** <img src=x>" },
    {
      key: "m2",
      kind: "reply",
      author: identity,
      timestamp,
      plaintext: "Reply body",
      replyTo: { author: identity, excerpt: "Earlier message" },
      media: [{ key: "a1", kind: "image", label: "photo.png", state: "pending" }],
    },
    { key: "r1", kind: "receipt", author: identity, timestamp, status: "opened", label: "Opened in OSL" },
  ];
}

describe("Discord protected transcript", () => {
  it("accepts virtualized, stable-key text, reply, media, and receipt rows", () => {
    expect(() => validateDiscordTranscriptWindow({
      rows: rows(), startIndex: 40, totalRowCount: 100, beforePx: 2_000, afterPx: 3_000,
    })).not.toThrow();
    expect(() => validateDiscordTranscriptWindow({
      rows: [...rows(), rows()[0]], startIndex: 0, totalRowCount: 4,
    })).toThrow(/Duplicate transcript row key/u);
  });

  it("fails closed on unverified identities and invalid virtualization bounds", () => {
    const unverified = rows();
    unverified[0] = { ...unverified[0], author: { ...identity, provenance: "discord" as never } };
    expect(() => validateDiscordTranscriptWindow({ rows: unverified, startIndex: 0, totalRowCount: 3 }))
      .toThrow(/not verified by OSL/u);
    expect(() => validateDiscordTranscriptWindow({ rows: rows(), startIndex: 99, totalRowCount: 100 }))
      .toThrow(/Invalid transcript window/u);
  });

  it("rejects remote avatar URLs while allowing bounded embedded image data", () => {
    const remoteAvatar = rows();
    remoteAvatar[0] = {
      ...remoteAvatar[0],
      author: { ...identity, avatarUrl: "https://cdn.discordapp.com/avatar.png" },
    };
    expect(() => validateDiscordTranscriptWindow({ rows: remoteAvatar, startIndex: 0, totalRowCount: 3 }))
      .toThrow(/embedded verified OSL image/u);

    const embeddedAvatar = rows();
    embeddedAvatar[0] = {
      ...embeddedAvatar[0],
      author: { ...identity, avatarUrl: "data:image/png;base64,iVBORw0KGgo=" },
    };
    expect(() => validateDiscordTranscriptWindow({ rows: embeddedAvatar, startIndex: 0, totalRowCount: 3 }))
      .not.toThrow();
  });

  it("supports theme, density, and bounded zoom inputs", () => {
    expect(normalizeDiscordTranscriptPreferences({ theme: "system", density: "compact", zoom: 9 }))
      .toEqual({ theme: "system", density: "compact", zoom: 2 });
    expect(normalizeDiscordTranscriptPreferences({ theme: "discord-light", density: "cozy", zoom: 0.2 }).zoom)
      .toBe(0.75);
  });

  it("renders plaintext only through textContent and never innerHTML", () => {
    const source = readFileSync(new URL("./discord-protected-transcript.ts", import.meta.url), "utf8");
    expect(source).toContain("result.textContent = value");
    expect(source).not.toMatch(/\.innerHTML\s*=/u);
    expect(source).not.toMatch(/discordapp|discord\.com|webpack|localStorage|indexedDB/iu);
  });

  it("provides the perimeter cue, adaptive layout, and accessibility modes", () => {
    const styles = readFileSync(new URL("./discord-protected-transcript.css", import.meta.url), "utf8");
    expect(styles).toMatch(/border:\s*1px solid var\(--osl-transcript-cue\)/u);
    expect(styles).toContain("minmax(0, 1fr)");
    expect(styles).toContain("prefers-reduced-motion: reduce");
    expect(styles).toContain("forced-colors: active");
  });

  it("is integrated as the central overlay viewport above the existing composer", () => {
    const html = readFileSync(new URL("../overlay.html", import.meta.url), "utf8");
    const overlay = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
    const styles = readFileSync(new URL("./overlay.css", import.meta.url), "utf8");
    expect(html).toContain('aria-label="Messages prepared or opened in this OSL panel"');
    expect(html.indexOf('id="osl-message-list"')).toBeLessThan(html.indexOf('id="write-pane"'));
    expect(overlay).toContain("createDiscordProtectedTranscript({");
    expect(overlay).toContain('displayName: "You"');
    expect(overlay).toContain("displayName: state.friendLabel");
    expect(overlay).not.toMatch(/verifiedFriendIdentity[\s\S]{0,300}avatarUrl/u);
    expect(styles).toContain("grid-template-rows: minmax(0, 1fr) auto");
    expect(styles).toMatch(/border:\s*1px solid rgba\(73, 214, 255,/u);
    expect(styles).not.toMatch(/\bgreen\b|#(?:0f0|00ff00)\b/iu);
  });
});
