import { describe, expect, it } from "vitest";
import {
  nativeDiscordAttributionsAreUnique,
  parseNativeDiscordRowAttribution,
  projectNativeDiscordVisibleRow,
  type NativeDiscordRowAttribution,
} from "./discord-row-attribution";

const h = (character: string): string => character.repeat(64);

function proof(
  poster: "self_account" | "peer_account",
  orientation: "incoming" | "outgoing",
  suffix: string,
): NativeDiscordRowAttribution {
  return {
    discordMessageId: `discord-${suffix}`,
    posterIdentitySha256: h("a"),
    poster,
    nativeLocatorSha256: h(suffix === "own" ? "b" : "c"),
    carrierSha256: h(suffix === "own" ? "d" : "e"),
    blobId: (suffix === "own" ? "1" : "2").repeat(16),
    ciphertextSha256: h(suffix === "own" ? "3" : "4"),
    payloadId: `payload-${suffix}`,
    scopeBindingSha256: h("f"),
    windowGeneration: 7,
    orientation,
  };
}

describe("native Discord row attribution proof", () => {
  it("Qualify Discord visible-row read and attribution", () => {
    const own = proof("self_account", "outgoing", "own");
    const peer = proof("peer_account", "incoming", "peer");
    const parsedOwn = parseNativeDiscordRowAttribution(own);
    const parsedPeer = parseNativeDiscordRowAttribution(peer);
    expect(parsedOwn).toEqual(own);
    expect(parsedPeer).toEqual(peer);
    expect(nativeDiscordAttributionsAreUnique([parsedOwn!, parsedPeer!])).toBe(true);

    expect(projectNativeDiscordVisibleRow({
      plaintext: "own plaintext",
      orientation: "outgoing",
      attribution: parsedOwn,
      row: { leftPx: 1, topPx: 2, widthPx: 300, heightPx: 24 },
    })).toMatchObject({
      author: "self",
      direction: "outgoing",
      key: `decoded-${h("b")}`,
      discordMessageId: "discord-own",
    });
    expect(projectNativeDiscordVisibleRow({
      plaintext: "peer plaintext",
      orientation: "incoming",
      attribution: parsedPeer,
      row: { leftPx: 1, topPx: 30, widthPx: 300, heightPx: 24 },
    })).toMatchObject({
      author: "peer",
      direction: "incoming",
      key: `decoded-${h("c")}`,
      discordMessageId: "discord-peer",
    });

    expect(projectNativeDiscordVisibleRow({
      plaintext: "unqualified",
      orientation: "outgoing",
      attribution: null,
      row: { leftPx: 1, topPx: 2, widthPx: 300, heightPx: 24 },
    })).toBeNull();
    expect(projectNativeDiscordVisibleRow({
      plaintext: "substituted",
      orientation: "incoming",
      attribution: own,
      row: { leftPx: 1, topPx: 2, widthPx: 300, heightPx: 24 },
    })).toBeNull();
    expect(parseNativeDiscordRowAttribution({
      ...own,
      poster: "peer_account",
    })).toBeNull();
    expect(nativeDiscordAttributionsAreUnique([
      own,
      { ...peer, nativeLocatorSha256: own.nativeLocatorSha256 },
    ])).toBe(false);
  });

  it("accepts genuine own and peer orientation agreements", () => {
    expect(parseNativeDiscordRowAttribution(proof("self_account", "outgoing", "own")))
      .toEqual(proof("self_account", "outgoing", "own"));
    expect(parseNativeDiscordRowAttribution(proof("peer_account", "incoming", "peer")))
      .toEqual(proof("peer_account", "incoming", "peer"));
  });

  it("refuses missing, cross-author and malformed identity bindings", () => {
    const own = proof("self_account", "outgoing", "own");
    const { carrierSha256: _missing, ...missing } = own;
    expect(parseNativeDiscordRowAttribution(missing)).toBeNull();
    expect(parseNativeDiscordRowAttribution({ ...own, orientation: "incoming" })).toBeNull();
    expect(parseNativeDiscordRowAttribution({ ...own, poster: "peer_account" })).toBeNull();
    expect(parseNativeDiscordRowAttribution({ ...own, nativeLocatorSha256: h("A") })).toBeNull();
    expect(parseNativeDiscordRowAttribution({ ...own, windowGeneration: 0 })).toBeNull();
    expect(parseNativeDiscordRowAttribution({ ...own, rendererOwnsRow: true })).toBeNull();
  });

  it("refuses duplicate row or crypto identifiers across a response", () => {
    const own = proof("self_account", "outgoing", "own");
    const peer = proof("peer_account", "incoming", "peer");
    expect(nativeDiscordAttributionsAreUnique([own, peer])).toBe(true);
    for (const field of [
      "discordMessageId",
      "nativeLocatorSha256",
      "carrierSha256",
      "blobId",
      "ciphertextSha256",
      "payloadId",
    ] as const) {
      expect(nativeDiscordAttributionsAreUnique([
        own,
        { ...peer, [field]: own[field] },
      ]), field).toBe(false);
    }
  });

  it("projects genuine own and peer command DTOs to exact visible-row owners", () => {
    const own = projectNativeDiscordVisibleRow({
      plaintext: "own plaintext",
      orientation: "outgoing",
      attribution: proof("self_account", "outgoing", "own"),
      row: { leftPx: 1, topPx: 2, widthPx: 300, heightPx: 24 },
    });
    const peer = projectNativeDiscordVisibleRow({
      plaintext: "peer plaintext",
      orientation: "incoming",
      attribution: proof("peer_account", "incoming", "peer"),
      row: { leftPx: 1, topPx: 30, widthPx: 300, heightPx: 24 },
    });
    expect(own).toMatchObject({
      author: "self",
      direction: "outgoing",
      key: `decoded-${h("b")}`,
      discordMessageId: "discord-own",
    });
    expect(peer).toMatchObject({
      author: "peer",
      direction: "incoming",
      key: `decoded-${h("c")}`,
      discordMessageId: "discord-peer",
    });
  });

  it("refuses missing proof and every downstream ownership substitution", () => {
    const own = proof("self_account", "outgoing", "own");
    const base = {
      plaintext: "protected",
      orientation: "outgoing" as const,
      attribution: own,
      row: { leftPx: 1, topPx: 2, widthPx: 300, heightPx: 24 },
    };
    expect(projectNativeDiscordVisibleRow({ ...base, attribution: null })).toBeNull();
    expect(projectNativeDiscordVisibleRow({ ...base, orientation: null })).toBeNull();
    expect(projectNativeDiscordVisibleRow({ ...base, row: null })).toBeNull();
    expect(projectNativeDiscordVisibleRow({
      ...base,
      orientation: "incoming",
    })).toBeNull();
    expect(projectNativeDiscordVisibleRow({
      ...base,
      attribution: { ...own, poster: "peer_account" },
    })).toBeNull();
    expect(projectNativeDiscordVisibleRow({
      ...base,
      attribution: { ...own, orientation: "incoming" },
    })).toBeNull();
  });

  it("keeps row identity bound to native locators across reordered DTOs", () => {
    const inputs = [
      {
        plaintext: "own",
        orientation: "outgoing" as const,
        attribution: proof("self_account", "outgoing", "own"),
        row: { leftPx: 1, topPx: 2, widthPx: 300, heightPx: 24 },
      },
      {
        plaintext: "peer",
        orientation: "incoming" as const,
        attribution: proof("peer_account", "incoming", "peer"),
        row: { leftPx: 1, topPx: 30, widthPx: 300, heightPx: 24 },
      },
    ];
    const forward = inputs.map(projectNativeDiscordVisibleRow);
    const reverse = [...inputs].reverse().map(projectNativeDiscordVisibleRow);
    expect(reverse.map((row) => row?.key)).toEqual(
      [...forward].reverse().map((row) => row?.key),
    );
    expect(reverse.map((row) => row?.author)).toEqual(["peer", "self"]);
  });
});
