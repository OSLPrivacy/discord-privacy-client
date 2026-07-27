import { describe, expect, it } from "vitest";
import {
  nativeDiscordAttributionsAreUnique,
  parseNativeDiscordRowAttribution,
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
});
