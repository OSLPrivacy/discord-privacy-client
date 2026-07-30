import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  nativeDiscordAttributionsAreUnique,
  parseNativeDiscordRowAttribution,
  projectNativeDiscordVisibleRow,
  type NativeDiscordRowAttribution,
} from "./discord-row-attribution";

const nativeAdapter = readFileSync(
  new URL("../../osl-hub/src/native_discord_adapter.rs", import.meta.url),
  "utf8",
);

const h = (character: string): string => character.repeat(64);

function between(source: string, start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  expect(from, `missing source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(to, `missing source marker: ${end}`).toBeGreaterThan(from);
  return source.slice(from, to);
}

function nativeReadQualifiesAttribution(source: string): boolean {
  const qualifier = between(
    source,
    "fn qualify_native_visible_rows(",
    "\n}\n\n/// Why one rehydration walk stopped.",
  );
  const finalizer = between(
    source,
    "fn finish_native_visible_rows(",
    "\n}\n\n/// One row's descendant text plus",
  );
  return qualifier.includes("rows_observed: rows.len()")
    && qualifier.includes("qualification.proof_some = qualification.proof_some.saturating_add(1)")
    && qualifier.includes("qualification.proof_none = qualification.proof_none.saturating_add(1)")
    && finalizer.includes("let qualification = qualify_native_visible_rows(&visible_rows);")
    && finalizer.includes("qualification.proof_some == visible_rows.len()")
    && finalizer.includes("qualification.proof_none == 0")
    && finalizer.includes("if !producer_proof_is_valid {\n        for row in &mut visible_rows {\n            row.attribution = None;");
}

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
  it("qualifies the native visible-row read before exposing attribution", () => {
    expect(nativeReadQualifiesAttribution(nativeAdapter)).toBe(true);

    const noQualification = nativeAdapter.replace(
      "let qualification = qualify_native_visible_rows(&visible_rows);",
      "let qualification = NativeVisibleRowQualification::default();",
    );
    const acceptsMissingProof = nativeAdapter.replace(
      "&& qualification.proof_none == 0",
      "&& true",
    );
    expect(noQualification).not.toBe(nativeAdapter);
    expect(acceptsMissingProof).not.toBe(nativeAdapter);
    expect(nativeReadQualifiesAttribution(noQualification)).toBe(false);
    expect(nativeReadQualifiesAttribution(acceptsMissingProof)).toBe(false);
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
