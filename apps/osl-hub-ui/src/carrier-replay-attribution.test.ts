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
const uiProof = readFileSync(new URL("./discord-row-attribution.ts", import.meta.url), "utf8");

function between(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

type AttributionGate = {
  productionReachable: boolean;
  nativeContractComplete: boolean;
  unavailableProducerFailsClosed: boolean;
  missingProofRefuses: boolean;
  scopeAndGenerationAgree: boolean;
  rowAndCarrierAgree: boolean;
  posterIdentityAgrees: boolean;
  reorderedAndDuplicateNativeProofRefuses: boolean;
  signaturePrecedesDecryptAndValidation: boolean;
  posterAndWireOrientationAgree: boolean;
  cryptoIdentifiersBound: boolean;
  duplicateCryptoProofRefuses: boolean;
  trustedMainInputsOnly: boolean;
  uiProofIsExactAndUnique: boolean;
  uiAuthorsOnlyFromBackendAgreement: boolean;
};

function detectAttributionGate(
  brokerSource: string,
  mainSource: string,
  adapterSource: string,
  tokenSource: string,
  overlaySource: string,
  uiProofSource: string,
): AttributionGate {
  const command = between(
    mainSource,
    "async fn rehydrate_native_discord_overlay_history(",
    "\n#[tauri::command]",
  );
  const handlers = between(mainSource, "tauri::generate_handler![", "\n    ]);");
  const nativeEvidence = between(
    adapterSource,
    "pub struct NativeDiscordRowAttributionEvidence {",
    "\n}",
  );
  const visibleRow = between(adapterSource, "pub struct VisibleMessageRow {", "\n}");
  const producer = between(
    adapterSource,
    "    fn read_visible_message_rows(\n        target:",
    "    pub(super) fn read_visible_message_rows_detached(",
  );
  const nativeValidation = between(
    brokerSource,
    "fn native_row_evidence_batch_is_valid(",
    "\n}\n\nfn unproven_rehydrated_rows(",
  );
  const cryptoUniqueness = between(
    brokerSource,
    "fn rehydrated_attribution_ids_are_unique(",
    "\n}\n\n/// Turn the rows",
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
  const tokenReceive = between(
    tokenSource,
    "pub fn prose_token_recv_classified(",
    "\n}",
  );
  const parseRow = between(
    overlaySource,
    "function parseRehydratedDiscordRow(",
    "\n}\n\nfunction parseRehydratedDiscordTranscript",
  );
  const parseTranscript = between(
    overlaySource,
    "function parseRehydratedDiscordTranscript(",
    "\n}\n\n/**\n * Ask the backend",
  );
  const applyRows = between(
    overlaySource,
    "function applyDecodedTranscript(",
    "\n}\n\nfunction clearDecodedTranscript",
  );
  const proofParser = between(
    uiProofSource,
    "export function parseNativeDiscordRowAttribution(",
    "\n}\n\n/** Refuse an ambiguous response",
  );
  const proofUnique = between(
    uiProofSource,
    "export function nativeDiscordAttributionsAreUnique(",
    "\n}",
  );

  const verifyIndex = authenticate.indexOf("verify_manual_v3(");
  const decryptIndex = authenticate.indexOf("decrypt_direct_manual_v3(");
  const validateIndex = authenticate.indexOf("validate_oriented_peer_protected_payload(");
  const nativeFields = [...nativeEvidence.matchAll(
    /^\s*pub\s+([a-z][a-z0-9_]*):/gmu,
  )].map((match) => match[1]).sort();
  const tokenArguments = tokenReceive.slice(0, tokenReceive.indexOf(") ->"));

  return {
    productionReachable:
      command.includes("broker::rehydrate_native_discord_overlay_history(")
      && /\brehydrate_native_discord_overlay_history\b/u.test(handlers)
      && /invoke<unknown>\(\s*"rehydrate_native_discord_overlay_history",\s*\{\s*scope\s*\}/u
        .test(overlaySource)
      && overlaySource.includes("applyDecodedTranscript(result.rows);"),
    nativeContractComplete:
      visibleRow.includes(
        "pub attribution: Option<NativeDiscordRowAttributionEvidence>",
      )
      && nativeFields.join(",") === [
        "carrier_sha256",
        "discord_message_id",
        "native_locator_sha256",
        "poster",
        "poster_identity_sha256",
        "row_index",
        "scope_binding_sha256",
        "window_generation",
      ].join(","),
    unavailableProducerFailsClosed:
      producer.includes("Discord's current MSAA provider does not expose")
      && producer.includes("attribution: None"),
    missingProofRefuses:
      nativeValidation.includes(
        "let Some(evidence) = row.attribution.as_ref() else {\n            return false;",
      )
      && rehydrate.includes("if !native_row_evidence_batch_is_valid(")
      && rehydrate.includes("rows: unproven_rehydrated_rows(rows)"),
    scopeAndGenerationAgree:
      nativeValidation.includes("evidence.window_generation == window_generation")
      && nativeValidation.includes(
        "evidence.scope_binding_sha256.as_str() == expected_scope.as_str()",
      )
      && nativeValidation.includes("if window_generation == 0"),
    rowAndCarrierAgree:
      nativeValidation.includes(
        "evidence.native_locator_sha256.as_str() == row.locator_sha256.as_str()",
      )
      && nativeValidation.includes(
        "native_row_attribution_carrier_sha256(candidate)",
      )
      && nativeValidation.includes("matching_carriers == 1"),
    posterIdentityAgrees:
      nativeValidation.includes(
        "NativeDiscordRowPoster::SelfAccount => {",
      )
      && nativeValidation.includes(
        "NativeDiscordRowPoster::PeerAccount => {",
      )
      && nativeValidation.includes(
        "peer_poster_identity.as_deref()",
      )
      && nativeValidation.includes(
        "self_poster_identity.as_deref()",
      )
      && nativeValidation.includes(
        "&& poster_identity_agrees",
      ),
    reorderedAndDuplicateNativeProofRefuses:
      nativeValidation.includes("evidence.row_index == row_index")
      && nativeValidation.includes(
        "message_ids.insert(evidence.discord_message_id.clone())",
      )
      && nativeValidation.includes(
        "locators.insert(evidence.native_locator_sha256.clone())",
      )
      && nativeValidation.includes(
        "carriers.insert(evidence.carrier_sha256.clone())",
      ),
    signaturePrecedesDecryptAndValidation:
      verifyIndex >= 0 && decryptIndex > verifyIndex && validateIndex > decryptIndex
      && /scope_input:\s*&ScopeInput/u.test(tokenArguments)
      && /msg:\s*&str/u.test(tokenArguments)
      && !/\b(?:row|poster|discord_message_id)\b/iu.test(tokenArguments),
    posterAndWireOrientationAgree:
      rehydrate.includes(
        "&[PeerWireOrientation::PeerToSelf, PeerWireOrientation::SelfToPeer]",
      )
      && rehydrate.includes(
        "NativeDiscordRowPoster::SelfAccount,\n                    PeerWireOrientation::SelfToPeer",
      )
      && rehydrate.includes(
        "NativeDiscordRowPoster::PeerAccount,\n                    PeerWireOrientation::PeerToSelf",
      )
      && rehydrate.includes("return None;\n                }\n            };"),
    cryptoIdentifiersBound:
      rehydrate.includes("blob_id: authenticated.blob_id")
      && rehydrate.includes(
        "ciphertext_sha256: authenticated.ciphertext_sha256",
      )
      && rehydrate.includes(
        "payload_id: authenticated.payload.message_id.clone()",
      )
      && rehydrate.includes("carrier_sha256: evidence.carrier_sha256.clone()"),
    duplicateCryptoProofRefuses:
      ["discord_message_id", "native_locator_sha256", "carrier_sha256", "blob_id",
        "ciphertext_sha256", "payload_id"].every((field) =>
        cryptoUniqueness.includes(`attribution.${field}.clone()`))
      && rehydrate.includes("if !rehydrated_attribution_ids_are_unique(&rows)")
      && rehydrate.includes("row.attribution = None;"),
    trustedMainInputsOnly:
      command.includes("&scope_binding,\n            host.generation,\n            rows,")
      && command.includes("attribution: row.attribution")
      && !/\{\s*scope\s*,[^}]*attribution/su.test(overlaySource),
    uiProofIsExactAndUnique:
      proofParser.includes("if (!exactKeys(value, ATTRIBUTION_KEYS)) return null;")
      && proofParser.includes(
        '(poster !== "self_account" || orientation !== "outgoing")',
      )
      && proofParser.includes(
        '(poster !== "peer_account" || orientation !== "incoming")',
      )
      && proofUnique.includes('"discordMessageId"')
      && proofUnique.includes('"payloadId"')
      && parseTranscript.includes(
        "if (!nativeDiscordAttributionsAreUnique(attributions)) return null;",
      ),
    uiAuthorsOnlyFromBackendAgreement:
      parseRow.includes(
        "(attribution !== null && attribution.orientation !== record.orientation)",
      )
      && applyRows.includes(
        'author: row.orientation === "outgoing" ? localIdentity : verifiedFriendIdentity',
      )
      && applyRows.includes(
        "const key = `decoded-${row.attribution.nativeLocatorSha256}`",
      )
      && applyRows.includes(
        "nativeLocatorSha256: row.attribution.nativeLocatorSha256",
      )
      && applyRows.includes("carrierSha256: row.attribution.carrierSha256"),
  };
}

function allStagesPass(gate: AttributionGate): boolean {
  return Object.values(gate).every(Boolean);
}

const detect = (
  brokerSource = broker,
  mainSource = nativeMain,
  adapterSource = nativeAdapter,
  overlaySource = overlay,
  uiProofSource = uiProof,
): AttributionGate => detectAttributionGate(
  brokerSource,
  mainSource,
  adapterSource,
  proseToken,
  overlaySource,
  uiProofSource,
);

describe("native Discord visible-row attribution contract", () => {
  it("is production-reachable but remains fail-closed while native proof is unavailable", () => {
    const gate = detect();
    for (const [stage, passed] of Object.entries(gate)) {
      expect(passed, stage).toBe(true);
    }
    expect(allStagesPass(gate)).toBe(true);
  });

  it("has failure-capable reachability and missing-proof controls", () => {
    const unregistered = nativeMain.replace(
      "        rehydrate_native_discord_overlay_history,\n",
      "",
    );
    const acceptsMissing = broker.replace(
      "let Some(evidence) = row.attribution.as_ref() else {\n            return false;\n        };",
      "let evidence = row.attribution.as_ref().expect(\"mutation accepts missing proof\");",
    );
    expect(unregistered).not.toBe(nativeMain);
    expect(acceptsMissing).not.toBe(broker);
    expect(detect(broker, unregistered).productionReachable).toBe(false);
    expect(detect(acceptsMissing).missingProofRefuses).toBe(false);
  });

  it("detects removal of cross-author and wire-orientation agreement", () => {
    const noPosterIdentity = broker.replace(
      "&& poster_identity_agrees",
      "&& true",
    );
    const noOrientationAgreement = broker.replace(
      "NativeDiscordRowPoster::SelfAccount,\n                    PeerWireOrientation::SelfToPeer",
      "NativeDiscordRowPoster::SelfAccount,\n                    PeerWireOrientation::PeerToSelf",
    );
    expect(noPosterIdentity).not.toBe(broker);
    expect(noOrientationAgreement).not.toBe(broker);
    expect(detect(noPosterIdentity).posterIdentityAgrees).toBe(false);
    expect(detect(noOrientationAgreement).posterAndWireOrientationAgree).toBe(false);
  });

  it("detects removal of cross-row carrier and locator binding", () => {
    const noLocator = broker.replace(
      "&& evidence.native_locator_sha256.as_str() == row.locator_sha256.as_str()",
      "&& true",
    );
    const noCarrier = broker.replace("&& matching_carriers == 1", "&& true");
    expect(noLocator).not.toBe(broker);
    expect(noCarrier).not.toBe(broker);
    expect(detect(noLocator).rowAndCarrierAgree).toBe(false);
    expect(detect(noCarrier).rowAndCarrierAgree).toBe(false);
  });

  it("detects reordered, duplicate-native and duplicate-crypto false-greens", () => {
    const noOrder = broker.replace("evidence.row_index == row_index", "true");
    const noNativeDuplicate = broker.replace(
      "&& message_ids.insert(evidence.discord_message_id.clone())",
      "&& true",
    );
    const noCryptoDuplicate = broker.replace(
      "if !rehydrated_attribution_ids_are_unique(&rows)",
      "if false",
    );
    expect(noOrder).not.toBe(broker);
    expect(noNativeDuplicate).not.toBe(broker);
    expect(noCryptoDuplicate).not.toBe(broker);
    expect(detect(noOrder).reorderedAndDuplicateNativeProofRefuses).toBe(false);
    expect(detect(noNativeDuplicate).reorderedAndDuplicateNativeProofRefuses).toBe(false);
    expect(detect(noCryptoDuplicate).duplicateCryptoProofRefuses).toBe(false);
  });

  it("detects wrong-scope and wrong-window-generation acceptance", () => {
    const noScope = broker.replace(
      "&& evidence.scope_binding_sha256.as_str() == expected_scope.as_str()",
      "&& true",
    );
    const noGeneration = broker.replace(
      "evidence.window_generation == window_generation",
      "true",
    );
    expect(noScope).not.toBe(broker);
    expect(noGeneration).not.toBe(broker);
    expect(detect(noScope).scopeAndGenerationAgree).toBe(false);
    expect(detect(noGeneration).scopeAndGenerationAgree).toBe(false);
  });

  it("detects dropped crypto identifiers and renderer-authored ownership", () => {
    const noPayloadId = broker.replace(
      "payload_id: authenticated.payload.message_id.clone(),",
      'payload_id: "renderer".to_owned(),',
    );
    const rendererOwns = overlay.replace(
      'author: row.orientation === "outgoing" ? localIdentity : verifiedFriendIdentity,',
      "author: localIdentity,",
    );
    expect(noPayloadId).not.toBe(broker);
    expect(rendererOwns).not.toBe(overlay);
    expect(detect(noPayloadId).cryptoIdentifiersBound).toBe(false);
    expect(detect(broker, nativeMain, nativeAdapter, rendererOwns)
      .uiAuthorsOnlyFromBackendAgreement).toBe(false);
  });

  it("detects a producer or UI parser that stops failing closed", () => {
    const guessedProducer = nativeAdapter.replace(
      "attribution: None,",
      "attribution: guessed_attribution,",
    );
    const looseUi = uiProof.replace(
      "if (!exactKeys(value, ATTRIBUTION_KEYS)) return null;",
      "if (typeof value !== \"object\") return null;",
    );
    expect(guessedProducer).not.toBe(nativeAdapter);
    expect(looseUi).not.toBe(uiProof);
    expect(detect(broker, nativeMain, guessedProducer).unavailableProducerFailsClosed)
      .toBe(false);
    expect(detect(broker, nativeMain, nativeAdapter, overlay, looseUi).uiProofIsExactAndUnique)
      .toBe(false);
  });
});
