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

function occurrenceCount(source: string, needle: string): number {
  return source.split(needle).length - 1;
}

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
  nativeProducerUsesProviderOwnedIdentity: boolean;
  semanticSelfAuthority: boolean;
  independentPeerAuthority: boolean;
  nativeProducerBindsExactRow: boolean;
  nativeProducerSnapshotFailsClosed: boolean;
  productionCallbackConsumesNativeProof: boolean;
  nativeReadPreservesFocusAndPlaintextSafety: boolean;
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
  commandDtoFailsClosed: boolean;
  uiProofIsExactAndUnique: boolean;
  uiAuthorsOnlyFromBackendAgreement: boolean;
  behavioralBoundaryMatrix: boolean;
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
  const publicRead = between(
    adapterSource,
    "pub fn read_visible_message_rows(\n    host:",
    "\n// ---------------------------------------------------------------------------\n// Guided deletion:",
  );
  const detachedRead = between(
    adapterSource,
    "    pub(super) fn read_visible_message_rows_detached(",
    "\n    /// What one transcript-container lookup",
  );
  const providerBinding = between(
    adapterSource,
    "fn native_row_attribution_from_provider(",
    "\n}\n\n/// Whole-snapshot refusal at the native producer.",
  );
  const producerBatch = between(
    adapterSource,
    "fn native_row_producer_batch_is_valid(",
    "\n}\n\n/// Why one rehydration walk stopped.",
  );
  const producerFinalizer = between(
    adapterSource,
    "fn finish_native_visible_rows(",
    "\n}\n\n/// One row's descendant text plus",
  );
  const selfProvider = between(
    adapterSource,
    "    fn native_discord_self_provider_identity(",
    "\n    /// Prove the expected one-to-one conversation participant",
  );
  const peerProvider = between(
    adapterSource,
    "    fn native_discord_peer_provider_identity(",
    "\n    /// Read one exact message-content AutomationId",
  );
  const namedAuthority = between(
    adapterSource,
    "    fn native_discord_named_authority(",
    "\n    fn native_authority_contains(",
  );
  const avatarExtractor = between(
    adapterSource,
    "    fn native_avatar_identity(",
    "\n    /// Prove the signed-in account",
  );
  const rowProvider = between(
    adapterSource,
    "    fn native_discord_row_provider_observation(",
    "\n    /// Read ONE row's descendant accessible text",
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
  const bindAuthenticated = between(
    brokerSource,
    "fn bind_authenticated_native_row(",
    "\n}\n\nfn authenticate_oriented_prose_pointer(",
  );
  const commandDto = between(
    brokerSource,
    "pub fn rehydrated_native_discord_row_dto(",
    "\n}\n\n/// Fixed labels for the decode leg",
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
  const proofProjection = between(
    uiProofSource,
    "export function projectNativeDiscordVisibleRow(",
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
    nativeProducerUsesProviderOwnedIdentity:
      selfProvider.includes("msaa_client_from_window(target.window)")
      && selfProvider.includes("msaa_object_belongs_to_target(&root, target, process_is_trusted)")
      && adapterSource.includes(
        "fn discord_identity_from_native_avatar_value(value: &str) -> Option<&str>",
      )
      && avatarExtractor.includes(
        "node.role() != Some(MSAA_ROLE_SYSTEM_GRAPHIC)",
      )
      && avatarExtractor.includes("discord_identity_from_native_avatar_value")
      && selfProvider.includes("native_avatar_identity(&node)")
      && selfProvider.includes("exact_msaa_process_element(")
      && selfProvider.includes("avatar_runtime_id = runtime_id(&element)")
      && selfProvider.includes("identities.len() > 1")
      && producer.includes("native_discord_self_provider_identity(")
      && producer.includes("native_discord_peer_provider_identity(")
      && rowProvider.includes("discord_message_id_from_native_automation_id(&automation_id)")
      && rowProvider.includes("native_avatar_identity(&node)")
      && rowProvider.includes("message_content.len() != 1 || posters.len() != 1")
      && !/\b(?:renderer|dataset|data-author-id|querySelector)\b/u.test(
        `${selfProvider}\n${rowProvider}`,
      ),
    semanticSelfAuthority:
      namedAuthority.includes("name.as_str() == expected_name")
      && namedAuthority.includes("node.role() == Some(expected_role)")
      && selfProvider.includes("NATIVE_SELF_USER_AREA_NAME")
      && selfProvider.includes("NATIVE_SELF_SETTINGS_NAME")
      && selfProvider.includes("MSAA_ROLE_SYSTEM_PUSHBUTTON")
      && selfProvider.includes("native_authority_contains(user_panel.bounds, avatar_bounds)")
      && selfProvider.includes("settings_runtime_ids.len() != 1")
      && selfProvider.includes("identity.avatar_runtime_id == identity.settings_runtime_id"),
    independentPeerAuthority:
      peerProvider.includes("NATIVE_CONVERSATION_HEADER_NAME")
      && peerProvider.includes("native_avatar_identity(&node)")
      && peerProvider.includes("identity == self_identity.identity")
      && peerProvider.includes("peers.len() != 1")
      && rowProvider.includes("expected_peer_identity: expected_peer.identity.clone()")
      && providerBinding.includes(
        "observation.poster_identity == observation.expected_peer_identity",
      )
      && providerBinding.includes(
        "observation.self_identity == observation.expected_peer_identity",
      )
      && producer.includes("let expected_peer = native_peer_identity.as_ref()?;"),
    nativeProducerBindsExactRow:
      providerBinding.includes("scope_binding_sha256")
      && providerBinding.includes("window_generation")
      && providerBinding.includes("row_index")
      && providerBinding.includes("observation.discord_message_id")
      && providerBinding.includes("observation.poster_identity")
      && providerBinding.includes("carrier_sha256")
      && [
        "native_provider_runtime_id_text(&observation.row_runtime_id)",
        "native_provider_runtime_id_text(&observation.message_content_runtime_id)",
        "native_provider_runtime_id_text(&observation.poster_avatar_runtime_id)",
        "native_provider_runtime_id_text(&observation.self_avatar_runtime_id)",
        "native_provider_runtime_id_text(&observation.self_user_panel_runtime_id)",
        "native_provider_runtime_id_text(&observation.self_settings_runtime_id)",
        "native_provider_runtime_id_text(&observation.peer_avatar_runtime_id)",
        "native_provider_runtime_id_text(&observation.peer_header_runtime_id)",
      ].every((binding) => providerBinding.includes(binding))
      && providerBinding.includes(
        '"discord-native-visible-row-provider-binding-v1"',
      )
      && providerBinding.includes(
        "observation.poster_identity == observation.self_identity",
      )
      && providerBinding.includes(".count()\n            != 1")
      && rowProvider.includes("msaa_object_belongs_to_target(row, target, process_is_trusted)")
      && rowProvider.includes("decode_candidates.iter().any(|candidate| candidate == name)")
      && rowProvider.includes("element_runtime_id = runtime_id(&element)")
      && rowProvider.includes("if !native_poster_avatar_geometry_is_valid(")
      && producer.includes("native_row_attribution_from_provider(")
      && producer.includes("finish_native_visible_rows(")
      && producerFinalizer.includes("attribution: row.attribution"),
    nativeProducerSnapshotFailsClosed:
      producerBatch.includes("if rows.is_empty() || window_generation == 0")
      && producerBatch.includes("let Some(evidence) = row.attribution.as_ref() else")
      && producerBatch.includes("evidence.row_index == row_index")
      && producerBatch.includes("evidence.window_generation == window_generation")
      && /evidence\.scope_binding_sha256(?:\.as_str\(\))?\s*==\s*expected_scope(?:\.as_str\(\))?/u.test(
        producerBatch,
      )
      && producerBatch.includes("evidence.native_locator_sha256 == row.locator_sha256")
      && producerBatch.includes("matching_carriers == 1")
      && producerBatch.includes("message_ids.insert(evidence.discord_message_id.clone())")
      && producerBatch.includes("locators.insert(evidence.native_locator_sha256.clone())")
      && producerBatch.includes("carriers.insert(evidence.carrier_sha256.clone())")
      && producer.includes(
        "            root_window_identity_holds(target, process_is_trusted),\n        )",
      )
      && producerFinalizer.includes("root_identity_still_holds")
      && producerFinalizer.includes("native_row_producer_batch_is_valid(")
      && producerFinalizer.includes(
        "if !producer_proof_is_valid {\n        for row in &mut visible_rows {\n            row.attribution = None;",
      ),
    productionCallbackConsumesNativeProof:
      publicRead.includes("Ok(windows::read_visible_message_rows_detached(")
      && detachedRead.includes("Some(read_visible_message_rows(")
      && producer.includes("native_discord_row_provider_observation(")
      && occurrenceCount(producer, "native_row_attribution_from_provider(") === 2
      && producer.includes("finish_native_visible_rows(")
      && command.includes("native_discord_adapter::read_visible_message_rows(")
      && command.includes(
        "broker::rehydrated_native_discord_row_dto(row, relative_rect)",
      )
      && command.includes("broker::rehydrate_native_discord_overlay_history("),
    nativeReadPreservesFocusAndPlaintextSafety:
      !/\b(?:SetForegroundWindow|SendInput|accSelect|SetFocus|Invoke)\s*\(/u.test(
        `${selfProvider}\n${rowProvider}\n${producer}\n${producerFinalizer}`,
      )
      && !/\b(?:println|eprintln|writeln|qa_place_stage)\s*!\s*\([^)]*(?:identity|carrier|message)/u
        .test(`${selfProvider}\n${rowProvider}\n${producer}\n${producerFinalizer}`),
    missingProofRefuses:
      nativeValidation.includes(
        "let Some(evidence) = row.attribution.as_ref() else {\n            return false;",
      )
      && rehydrate.includes("if !native_row_evidence_batch_is_valid(")
      && rehydrate.includes("rows: unproven_rehydrated_rows(rows)"),
    scopeAndGenerationAgree:
      nativeValidation.includes("evidence.window_generation == window_generation")
      && /evidence\.scope_binding_sha256(?:\.as_str\(\))?\s*==\s*expected_scope(?:\.as_str\(\))?/u.test(
        nativeValidation,
      )
      && /if\s+(?:rows\.is_empty\(\)\s*\|\|\s*)?window_generation == 0/u.test(nativeValidation),
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
      /&\[\s*PeerWireOrientation::PeerToSelf,\s*PeerWireOrientation::SelfToPeer,\s*\]/u.test(
        rehydrate,
      )
      && /NativeDiscordRowPoster::SelfAccount,\s+PeerWireOrientation::SelfToPeer/u
        .test(bindAuthenticated)
      && /NativeDiscordRowPoster::PeerAccount,\s+PeerWireOrientation::PeerToSelf/u
        .test(bindAuthenticated)
      && bindAuthenticated.includes("_ => return None,"),
    cryptoIdentifiersBound:
      bindAuthenticated.includes("blob_id: authenticated.blob_id")
      && bindAuthenticated.includes(
        "ciphertext_sha256: authenticated.ciphertext_sha256",
      )
      && bindAuthenticated.includes(
        "payload_id: authenticated.payload.message_id.clone()",
      )
      && bindAuthenticated.includes("carrier_sha256: evidence.carrier_sha256.clone()"),
    duplicateCryptoProofRefuses:
      ["discord_message_id", "native_locator_sha256", "carrier_sha256", "blob_id",
        "ciphertext_sha256", "payload_id"].every((field) =>
        cryptoUniqueness.includes(`attribution.${field}.clone()`))
      && rehydrate.includes("if !rehydrated_attribution_ids_are_unique(&rows)")
      && rehydrate.includes("row.attribution = None;"),
    trustedMainInputsOnly:
      command.includes("&scope_binding,\n            host.generation,\n            rows,")
      && command.includes(
        "broker::rehydrated_native_discord_row_dto(row, relative_rect)",
      )
      && !/\{\s*scope\s*,[^}]*attribution/su.test(overlaySource),
    commandDtoFailsClosed:
      commandDto.includes("let attribution_agrees = match (")
      && commandDto.includes("(None, None, None) => true")
      && commandDto.includes(
        "RehydratedRowPoster::SelfAccount,",
      )
      && commandDto.includes(
        "RehydratedRowOrientation::Outgoing",
      )
      && commandDto.includes(
        "RehydratedRowPoster::PeerAccount,",
      )
      && commandDto.includes(
        "RehydratedRowOrientation::Incoming",
      )
      && commandDto.includes("if !attribution_agrees")
      && commandDto.includes("row.plaintext = None;")
      && commandDto.includes("row.orientation = None;")
      && commandDto.includes("row.attribution = None;"),
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
      && proofProjection.includes("attribution.orientation !== orientation")
      && proofProjection.includes(
        'attribution.poster === "self_account" && orientation === "outgoing"',
      )
      && proofProjection.includes(
        'attribution.poster === "peer_account" && orientation === "incoming"',
      )
      && applyRows.includes("const visible = projectNativeDiscordVisibleRow(row);")
      && applyRows.includes(
        'author: visible.author === "self" ? localIdentity : verifiedFriendIdentity',
      )
      && applyRows.includes(
        "const key = visible.key",
      )
      && applyRows.includes(
        "nativeLocatorSha256: visible.nativeLocatorSha256",
      )
      && applyRows.includes("carrierSha256: visible.carrierSha256"),
    behavioralBoundaryMatrix:
      brokerSource.includes(
        "fn native_producer_broker_and_command_dto_matrix_is_behavioral_and_fail_closed()",
      )
      && brokerSource.includes("native_row_attribution_from_provider(")
      && brokerSource.includes("native_row_producer_batch_is_valid(")
      && brokerSource.includes("native_row_evidence_batch_is_valid(")
      && brokerSource.includes("bind_authenticated_native_row(")
      && brokerSource.includes("rehydrated_native_discord_row_dto(")
      && brokerSource.includes("assert!(!rehydrated_attribution_ids_are_unique(&crypto_replay))")
      && proofProjection.includes("return null")
      && applyRows.includes("projectNativeDiscordVisibleRow(row)"),
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
  it("is production-reachable through the provider-owned native proof callback", () => {
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

  it("detects removal of the real public callback or its proof handoff", () => {
    const noNativeCallback = nativeAdapter.replace(
      "Ok(windows::read_visible_message_rows_detached(",
      "Ok(windows::read_visible_message_rows_detached_DISABLED(",
    );
    const noProducerCall = nativeAdapter.replace(
      "                    native_row_attribution_from_provider(\n",
      "                    native_row_attribution_from_provider_DISABLED(\n",
    );
    const droppedMainProof = nativeMain.replace(
      "broker::rehydrated_native_discord_row_dto(row, relative_rect)",
      "broker::rehydrated_native_discord_row_dto_DISABLED(row, relative_rect)",
    );
    expect(noNativeCallback).not.toBe(nativeAdapter);
    expect(noProducerCall).not.toBe(nativeAdapter);
    expect(droppedMainProof).not.toBe(nativeMain);
    expect(detect(broker, nativeMain, noNativeCallback)
      .productionCallbackConsumesNativeProof).toBe(false);
    expect(detect(broker, nativeMain, noProducerCall)
      .productionCallbackConsumesNativeProof).toBe(false);
    expect(detect(broker, droppedMainProof)
      .productionCallbackConsumesNativeProof).toBe(false);
  });

  it("detects guessed message/poster authority and ambiguous native rows", () => {
    const noMessageAuthority = nativeAdapter.replace(
      "discord_message_id_from_native_automation_id(&automation_id)",
      "Some(\"333333333333333333\")",
    );
    const noSelfAuthority = nativeAdapter.replace(
      "            native_discord_self_provider_identity(\n",
      "            native_discord_self_provider_identity_DISABLED(\n",
    );
    const noPeerAuthority = nativeAdapter.replace(
      "                native_discord_peer_provider_identity(\n",
      "                native_discord_peer_provider_identity_DISABLED(\n",
    );
    const noSemanticSelfContainer = nativeAdapter.replace(
      "            NATIVE_SELF_USER_AREA_NAME,\n",
      "            \"avatar band\",\n",
    );
    const noSettingsAuthority = nativeAdapter.replace(
      "name.as_str() == NATIVE_SELF_SETTINGS_NAME",
      "false",
    );
    const acceptsForeignNonSelf = nativeAdapter.replace(
      "observation.poster_identity == observation.expected_peer_identity",
      "observation.poster_identity != observation.self_identity",
    );
    const acceptsAmbiguousPoster = nativeAdapter.replace(
      "if message_content.len() != 1 || posters.len() != 1",
      "if message_content.is_empty() || posters.is_empty()",
    );
    const acceptsAuthoredAvatarLink = nativeAdapter.replace(
      "if node.role() != Some(MSAA_ROLE_SYSTEM_GRAPHIC)",
      "if false",
    );
    const acceptsEmbeddedAvatar = nativeAdapter.replace(
      "if !native_poster_avatar_geometry_is_valid(",
      "if false && !native_poster_avatar_geometry_is_valid(",
    );
    expect(noMessageAuthority).not.toBe(nativeAdapter);
    expect(noSelfAuthority).not.toBe(nativeAdapter);
    expect(noPeerAuthority).not.toBe(nativeAdapter);
    expect(noSemanticSelfContainer).not.toBe(nativeAdapter);
    expect(noSettingsAuthority).not.toBe(nativeAdapter);
    expect(acceptsForeignNonSelf).not.toBe(nativeAdapter);
    expect(acceptsAmbiguousPoster).not.toBe(nativeAdapter);
    expect(acceptsAuthoredAvatarLink).not.toBe(nativeAdapter);
    expect(acceptsEmbeddedAvatar).not.toBe(nativeAdapter);
    expect(detect(broker, nativeMain, noMessageAuthority)
      .nativeProducerUsesProviderOwnedIdentity).toBe(false);
    expect(detect(broker, nativeMain, noSelfAuthority)
      .nativeProducerUsesProviderOwnedIdentity).toBe(false);
    expect(detect(broker, nativeMain, noPeerAuthority)
      .nativeProducerUsesProviderOwnedIdentity).toBe(false);
    expect(detect(broker, nativeMain, noSemanticSelfContainer)
      .semanticSelfAuthority).toBe(false);
    expect(detect(broker, nativeMain, noSettingsAuthority)
      .semanticSelfAuthority).toBe(false);
    expect(detect(broker, nativeMain, acceptsForeignNonSelf)
      .independentPeerAuthority).toBe(false);
    expect(detect(broker, nativeMain, acceptsAmbiguousPoster)
      .nativeProducerUsesProviderOwnedIdentity).toBe(false);
    expect(detect(broker, nativeMain, acceptsAuthoredAvatarLink)
      .nativeProducerUsesProviderOwnedIdentity).toBe(false);
    expect(detect(broker, nativeMain, acceptsEmbeddedAvatar)
      .nativeProducerBindsExactRow).toBe(false);
  });

  it("detects removal of scope/window/order/runtime/carrier native binding", () => {
    const noPosterRuntime = nativeAdapter.replace(
      "native_provider_runtime_id_text(&observation.poster_avatar_runtime_id),",
      "String::new(),",
    );
    const noCarrierUniqueness = nativeAdapter.replace(
      ".count()\n            != 1",
      ".count()\n            == usize::MAX",
    );
    const noPosterClass = nativeAdapter.replace(
      "observation.poster_identity == observation.self_identity",
      "true",
    );
    expect(noPosterRuntime).not.toBe(nativeAdapter);
    expect(noCarrierUniqueness).not.toBe(nativeAdapter);
    expect(noPosterClass).not.toBe(nativeAdapter);
    expect(detect(broker, nativeMain, noPosterRuntime).nativeProducerBindsExactRow)
      .toBe(false);
    expect(detect(broker, nativeMain, noCarrierUniqueness).nativeProducerBindsExactRow)
      .toBe(false);
    expect(detect(broker, nativeMain, noPosterClass).nativeProducerBindsExactRow)
      .toBe(false);
  });

  it("detects producer acceptance of missing duplicate reordered or stale proof", () => {
    const acceptsMissing = nativeAdapter.replace(
      "let Some(evidence) = row.attribution.as_ref() else {\n            return false;\n        };",
      "let evidence = row.attribution.as_ref().expect(\"mutation\");",
    );
    const acceptsReordered = nativeAdapter.replace(
      "evidence.row_index == row_index",
      "true",
    );
    const acceptsDuplicate = nativeAdapter.replace(
      "&& message_ids.insert(evidence.discord_message_id.clone())",
      "&& true",
    );
    const acceptsStaleWindow = nativeAdapter.replace(
      "&& evidence.window_generation == window_generation",
      "&& true",
    );
    const noFinalRootProof = nativeAdapter.replace(
      "            root_window_identity_holds(target, process_is_trusted),",
      "            true,",
    );
    for (const [label, mutated] of Object.entries({
      acceptsMissing,
      acceptsReordered,
      acceptsDuplicate,
      acceptsStaleWindow,
      noFinalRootProof,
    })) {
      expect(mutated).not.toBe(nativeAdapter);
      expect(
        detect(broker, nativeMain, mutated).nativeProducerSnapshotFailsClosed,
        label,
      )
        .toBe(false);
    }
  });

  it("detects focus/input mutation of the read-only native proof leg", () => {
    const focusesRow = nativeAdapter.replace(
      "        if !msaa_object_belongs_to_target(row, target, process_is_trusted) {",
      "        SetForegroundWindow(target.window);\n        if !msaa_object_belongs_to_target(row, target, process_is_trusted) {",
    );
    expect(focusesRow).not.toBe(nativeAdapter);
    expect(detect(broker, nativeMain, focusesRow).nativeReadPreservesFocusAndPlaintextSafety)
      .toBe(false);
  });

  it("detects removal of cross-author and wire-orientation agreement", () => {
    const noPosterIdentity = broker.replace(
      "&& poster_identity_agrees",
      "&& true",
    );
    const noOrientationAgreement = broker.replace(
      "NativeDiscordRowPoster::SelfAccount,\n            PeerWireOrientation::SelfToPeer",
      "NativeDiscordRowPoster::SelfAccount,\n            PeerWireOrientation::PeerToSelf",
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
      'author: visible.author === "self" ? localIdentity : verifiedFriendIdentity,',
      "author: localIdentity,",
    );
    expect(noPayloadId).not.toBe(broker);
    expect(rendererOwns).not.toBe(overlay);
    expect(detect(noPayloadId).cryptoIdentifiersBound).toBe(false);
    expect(detect(broker, nativeMain, nativeAdapter, rendererOwns)
      .uiAuthorsOnlyFromBackendAgreement).toBe(false);
  });

  it("detects a bypassed command DTO refusal or fabricated boundary matrix", () => {
    const acceptsInconsistentDto = broker.replace(
      "if !attribution_agrees {",
      "if false {",
    );
    const noBehavioralMatrix = broker.replace(
      "fn native_producer_broker_and_command_dto_matrix_is_behavioral_and_fail_closed()",
      "fn native_producer_broker_and_command_dto_matrix_DISABLED()",
    );
    expect(acceptsInconsistentDto).not.toBe(broker);
    expect(noBehavioralMatrix).not.toBe(broker);
    expect(detect(acceptsInconsistentDto).commandDtoFailsClosed).toBe(false);
    expect(detect(noBehavioralMatrix).behavioralBoundaryMatrix).toBe(false);
  });

  it("detects a UI parser that stops failing closed", () => {
    const looseUi = uiProof.replace(
      "if (!exactKeys(value, ATTRIBUTION_KEYS)) return null;",
      "if (typeof value !== \"object\") return null;",
    );
    expect(looseUi).not.toBe(uiProof);
    expect(detect(broker, nativeMain, nativeAdapter, overlay, looseUi).uiProofIsExactAndUnique)
      .toBe(false);
  });
});
