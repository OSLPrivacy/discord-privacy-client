import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const broker = readFileSync(new URL("../../osl-hub/src/broker.rs", import.meta.url), "utf8");
const nativeMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const nativeAdapter = readFileSync(
  new URL("../../osl-hub/src/native_discord_adapter.rs", import.meta.url),
  "utf8",
);
const proseToken = readFileSync(
  new URL("../../../crates/ipc/src/prose_token.rs", import.meta.url),
  "utf8",
);
const overlay = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");

function between(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

type AttributionGate = {
  commandDefined: boolean;
  commandRegistered: boolean;
  uiInvokesCommand: boolean;
  uiAppliesRows: boolean;
  scopeBoundTokenHasNoRowPosterInput: boolean;
  nativeRowHasNoPosterProof: boolean;
  peerWireMapsToPeerIdentity: boolean;
  signaturePrecedesDecryptAndValidation: boolean;
  rehydrateAcceptsPeerWireOnly: boolean;
  rehydrateEmitsIncomingOnly: boolean;
  rendererAcceptsIncomingOnly: boolean;
  rendererCannotAuthorOwnership: boolean;
};

function detectAttributionGate(
  brokerSource: string,
  mainSource: string,
  adapterSource: string,
  tokenSource: string,
  overlaySource: string,
): AttributionGate {
  const command = between(
    mainSource,
    "async fn rehydrate_native_discord_overlay_history(",
    "\n#[tauri::command]",
  );
  const handlers = between(mainSource, "tauri::generate_handler![", "\n    ]);");
  const visibleRow = between(
    adapterSource,
    "pub struct VisibleMessageRow {",
    "\n}",
  );
  const tokenReceive = between(
    tokenSource,
    "pub fn prose_token_recv_classified(",
    "\n}",
  );
  const wireSender = between(
    brokerSource,
    "fn wire_sender(self) -> ManualWireSender {",
    "\n    }",
  );
  const authenticate = between(
    brokerSource,
    "fn authenticate_oriented_prose_pointer(",
    "\n}\n\n/// A committed protected message",
  );
  const rehydrate = between(
    brokerSource,
    "pub fn rehydrate_native_discord_overlay_history(",
    "\n}\n\n/// Pair each row",
  );
  const orientation = between(
    brokerSource,
    "pub enum RehydratedRowOrientation {",
    "\n}",
  );
  const parseRow = between(
    overlaySource,
    "function parseRehydratedDiscordRow(",
    "\n}\n\nfunction parseRehydratedDiscordTranscript",
  );
  const applyRows = between(
    overlaySource,
    "function applyDecodedTranscript(",
    "\n}\n\nfunction clearDecodedTranscript",
  );

  const verifyIndex = authenticate.indexOf("verify_manual_v3(");
  const decryptIndex = authenticate.indexOf("decrypt_direct_manual_v3(");
  const validateIndex = authenticate.indexOf("validate_peer_protected_payload(");
  const visibleRowFields = [...visibleRow.matchAll(
    /^\s*pub\s+([a-z][a-z0-9_]*):/gmu,
  )].map((match) => match[1]);

  return {
    commandDefined: command.includes(
      "broker::rehydrate_native_discord_overlay_history(",
    ),
    commandRegistered: /\brehydrate_native_discord_overlay_history\b/u.test(handlers),
    uiInvokesCommand:
      /invoke<unknown>\(\s*"rehydrate_native_discord_overlay_history"/u.test(overlaySource),
    uiAppliesRows:
      overlaySource.includes("applyDecodedTranscript(result.rows);"),
    // The current token authenticates `scope_input` and `msg`; it has no Discord
    // row/message/poster argument. This premise keeps the refusal from becoming
    // stale if a real row binding is introduced later.
    scopeBoundTokenHasNoRowPosterInput:
      /scope_input:\s*&ScopeInput/u.test(tokenReceive)
      && /msg:\s*&str/u.test(tokenReceive)
      && !/\b(?:row|poster|author|discord_message_id)\b/iu.test(
        tokenReceive.slice(0, tokenReceive.indexOf(") ->")),
      ),
    nativeRowHasNoPosterProof:
      visibleRowFields.includes("locator_sha256")
      && visibleRowFields.includes("decode_candidates")
      && !visibleRowFields.some((field) =>
        /^(?:poster|author|sender|message_id|carrier_sha256)$/u.test(field)
      ),
    peerWireMapsToPeerIdentity:
      /Self::PeerToSelf\s*=>\s*ManualWireSender::Peer/u.test(wireSender),
    signaturePrecedesDecryptAndValidation:
      verifyIndex >= 0
      && decryptIndex > verifyIndex
      && validateIndex > decryptIndex,
    rehydrateAcceptsPeerWireOnly:
      /&\[\s*PeerWireOrientation::PeerToSelf\s*,?\s*\]/u.test(rehydrate)
      && !rehydrate.includes("PeerWireOrientation::SelfToPeer"),
    rehydrateEmitsIncomingOnly:
      orientation.includes("Incoming")
      && !orientation.includes("Outgoing")
      && rehydrate.includes("RehydratedRowOrientation::Incoming")
      && !rehydrate.includes("RehydratedRowOrientation::from"),
    rendererAcceptsIncomingOnly:
      parseRow.includes(
        'if (record.orientation !== null && record.orientation !== "incoming") return null;',
      )
      && !parseRow.includes('record.orientation !== "outgoing"'),
    rendererCannotAuthorOwnership:
      parseRow.includes(
        'exactKeys(value, ["flagtext", "plaintext", "orientation", "row"])',
      )
      && applyRows.includes('direction: "incoming"')
      && applyRows.includes("author: verifiedFriendIdentity")
      && !applyRows.includes("author: localIdentity")
      && !applyRows.includes('row.orientation === "outgoing"'),
  };
}

function allStagesPass(gate: AttributionGate): boolean {
  return Object.values(gate).every(Boolean);
}

describe("authenticated prose carrier row attribution", () => {
  it("is production-reachable and fails closed where trusted row-poster proof is absent", () => {
    const gate = detectAttributionGate(
      broker,
      nativeMain,
      nativeAdapter,
      proseToken,
      overlay,
    );

    expect(gate.commandDefined).toBe(true);
    expect(gate.commandRegistered).toBe(true);
    expect(gate.uiInvokesCommand).toBe(true);
    expect(gate.uiAppliesRows).toBe(true);
    expect(gate.scopeBoundTokenHasNoRowPosterInput).toBe(true);
    expect(gate.nativeRowHasNoPosterProof).toBe(true);
    expect(gate.peerWireMapsToPeerIdentity).toBe(true);
    expect(gate.signaturePrecedesDecryptAndValidation).toBe(true);
    expect(gate.rehydrateAcceptsPeerWireOnly).toBe(true);
    expect(gate.rehydrateEmitsIncomingOnly).toBe(true);
    expect(gate.rendererAcceptsIncomingOnly).toBe(true);
    expect(gate.rendererCannotAuthorOwnership).toBe(true);
    expect(allStagesPass(gate)).toBe(true);
  });

  it("fails if history rehydration again accepts a locally signed replay", () => {
    const mutated = broker.replace(
      "&[PeerWireOrientation::PeerToSelf]",
      "&[PeerWireOrientation::PeerToSelf, PeerWireOrientation::SelfToPeer]",
    );
    expect(mutated).not.toBe(broker);
    expect(
      detectAttributionGate(mutated, nativeMain, nativeAdapter, proseToken, overlay)
        .rehydrateAcceptsPeerWireOnly,
    ).toBe(false);
  });

  it("fails if peer orientation stops selecting the peer verification key", () => {
    const mutated = broker.replace(
      "Self::PeerToSelf => ManualWireSender::Peer,",
      "Self::PeerToSelf => ManualWireSender::SelfIdentity,",
    );
    expect(mutated).not.toBe(broker);
    expect(
      detectAttributionGate(mutated, nativeMain, nativeAdapter, proseToken, overlay)
        .peerWireMapsToPeerIdentity,
    ).toBe(false);
  });

  it("fails if the renderer accepts or authors an outgoing history verdict", () => {
    const acceptsOutgoing = overlay.replace(
      'record.orientation !== "incoming") return null;',
      'record.orientation !== "incoming" && record.orientation !== "outgoing") return null;',
    );
    const authorsLocal = overlay.replace(
      "author: verifiedFriendIdentity,",
      "author: localIdentity,",
    );
    expect(acceptsOutgoing).not.toBe(overlay);
    expect(authorsLocal).not.toBe(overlay);
    expect(
      detectAttributionGate(
        broker,
        nativeMain,
        nativeAdapter,
        proseToken,
        acceptsOutgoing,
      ).rendererAcceptsIncomingOnly,
    ).toBe(false);
    expect(
      detectAttributionGate(
        broker,
        nativeMain,
        nativeAdapter,
        proseToken,
        authorsLocal,
      ).rendererCannotAuthorOwnership,
    ).toBe(false);
  });

  it("has failure-capable production reachability controls", () => {
    const unregistered = nativeMain.replace(
      "        rehydrate_native_discord_overlay_history,\n",
      "",
    );
    const uncalled = overlay.replace(
      "    applyDecodedTranscript(result.rows);",
      "    // mutation: decoded rows are not applied",
    );
    expect(unregistered).not.toBe(nativeMain);
    expect(uncalled).not.toBe(overlay);
    expect(
      detectAttributionGate(broker, unregistered, nativeAdapter, proseToken, overlay)
        .commandRegistered,
    ).toBe(false);
    expect(
      detectAttributionGate(broker, nativeMain, nativeAdapter, proseToken, uncalled)
        .uiAppliesRows,
    ).toBe(false);
  });
});
