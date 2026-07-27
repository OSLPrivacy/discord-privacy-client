import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseNativeVisibleRowRuntimeReceipt } from "./discord-headless-qa-adapter";

const nativeMain = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const adapter = readFileSync(
  new URL("../../osl-hub/src/native_discord_adapter.rs", import.meta.url),
  "utf8",
);
const broker = readFileSync(new URL("../../osl-hub/src/broker.rs", import.meta.url), "utf8");
const permissions = readFileSync(
  new URL("../../osl-hub/permissions/hub.toml", import.meta.url),
  "utf8",
);
const capability = readFileSync(
  new URL("../../osl-hub/capabilities/hub.json", import.meta.url),
  "utf8",
);
const uiAdapter = readFileSync(new URL("./discord-headless-qa-adapter.ts", import.meta.url), "utf8");
const renderer = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

type Sources = {
  nativeMain: string;
  adapter: string;
  broker: string;
  permissions: string;
  capability: string;
  uiAdapter: string;
  renderer: string;
};

type RuntimeReceiptGate = {
  registration: boolean;
  acl: boolean;
  capability: boolean;
  caller: boolean;
  producer: boolean;
  broker: boolean;
  peerAnchor: boolean;
  refusals: boolean;
  persistence: boolean;
  privacy: boolean;
};

const baseline: Sources = {
  nativeMain,
  adapter,
  broker,
  permissions,
  capability,
  uiAdapter,
  renderer,
};

function between(source: string, start: string, end: string): string {
  const from = source.indexOf(start);
  const to = source.indexOf(end, from + start.length);
  expect(from, `missing source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(to, `missing source marker: ${end}`).toBeGreaterThan(from);
  return source.slice(from, to);
}

function detect(sources: Sources): RuntimeReceiptGate {
  const command = between(
    sources.nativeMain,
    "#[cfg(feature = \"discord-qa-shell\")]\n#[tauri::command]\nasync fn request_native_discord_visible_row_qa_receipt(",
    "\n#[tauri::command]\nfn send_native_discord_overlay_carrier(",
  );
  const handlers = between(
    sources.nativeMain,
    "tauri::generate_handler![",
    "\n    ]);",
  );
  const probe = between(
    sources.adapter,
    "pub(crate) fn request_native_visible_row_qa_probe(",
    "\n}\n\n// ---------------------------------------------------------------------------",
  );
  const windowsRead = between(
    sources.adapter,
    "    fn read_visible_message_rows(",
    "\n    /// Run one rehydration read on its own detached thread",
  );
  const brokerRequest = between(
    sources.broker,
    "pub fn request_native_visible_row_runtime_receipt(",
    "\n}\n\n/// Commit the already reduced nonsecret receipt",
  );
  const evaluator = between(
    sources.broker,
    "fn evaluate_native_visible_row_runtime_probe(",
    "\n}\n\n/// Product-side runtime evidence path",
  );
  const writer = between(
    sources.broker,
    "fn write_native_visible_row_runtime_receipt_at(",
    "\n}\n\n#[cfg(all(feature = \"core\", feature = \"discord-qa-shell\"))]\nfn evaluate_",
  );
  const validator = between(
    sources.broker,
    "fn native_row_evidence_batch_is_valid(",
    "\n}\n\nfn unproven_rehydrated_rows(",
  );

  return {
    registration:
      handlers.includes("request_native_discord_visible_row_qa_receipt,")
      && command.includes("#[tauri::command]"),
    acl:
      sources.permissions.includes(
        'identifier = "allow-request-native-discord-visible-row-qa-receipt"',
      )
      && sources.permissions.includes(
        'commands.allow = ["request_native_discord_visible_row_qa_receipt"]',
      ),
    capability:
      sources.capability.includes(
        '"allow-request-native-discord-visible-row-qa-receipt"',
      ),
    caller:
      sources.uiAdapter.includes(
        "export async function requestNativeDiscordVisibleRowRuntimeReceipt()",
      )
      && sources.uiAdapter.includes(
        'invoke<unknown>("request_native_discord_visible_row_qa_receipt")',
      )
      && !sources.uiAdapter.includes(
        'invoke<unknown>("request_native_discord_visible_row_qa_receipt", {',
      )
      && sources.renderer.includes('id="discord-qa-row-proof"')
      && sources.renderer.includes(
        'document.querySelector<HTMLButtonElement>("#discord-qa-row-proof")',
      )
      && sources.renderer.includes(
        "await requestNativeDiscordVisibleRowRuntimeReceipt()",
      ),
    producer:
      probe.includes("with_current_discord_accessibility_target(")
      && probe.includes("read_visible_message_rows_qa_detached(")
      && probe.includes("target.window, target.process_id")
      && windowsRead.includes("native_discord_row_provider_observation(")
      && windowsRead.includes("native_row_attribution_from_provider("),
    broker:
      brokerRequest.includes("request_native_visible_row_qa_probe(")
      && brokerRequest.includes("evaluate_native_visible_row_runtime_probe(")
      && evaluator.includes("rehydrate_native_discord_overlay_history(")
      && evaluator.includes("RehydratedRowPoster::SelfAccount")
      && evaluator.includes("RehydratedRowOrientation::Outgoing")
      && evaluator.includes("RehydratedRowPoster::PeerAccount")
      && evaluator.includes("RehydratedRowOrientation::Incoming"),
    peerAnchor:
      windowsRead.includes("native_discord_peer_provider_identity(")
      && windowsRead.includes("foreign.poster_identity = [")
      && windowsRead.includes("foreign.expected_peer_identity")
      && windowsRead.includes("foreign_poster_acceptances")
      && evaluator.includes("different_non_self: probe.producer_controls.different_non_self"),
    refusals:
      validator.includes("if rows.is_empty() || window_generation == 0")
      && ["zero_rows", "missing_proof", "mixed_scope", "replay", "reorder"]
        .every((control) => evaluator.includes(control))
      && sources.broker.includes("NativeVisibleRowQaTriState::NotObserved")
      && sources.broker.includes("NativeVisibleRowQaTriState::Refused")
      && sources.broker.includes("NativeVisibleRowQaTriState::Accepted"),
    persistence:
      command.includes("broker::persist_native_visible_row_runtime_receipt(&receipt)?;")
      && command.match(/require_same_overlay_context\(&app, epoch, &context_host\)\?;/gu)?.length === 2
      && command.match(/require_engaged_lock\(&app\)\?;/gu)?.length === 2
      && command.lastIndexOf("require_same_overlay_context(&app, epoch, &context_host)?;")
        < command.indexOf("broker::persist_native_visible_row_runtime_receipt(&receipt)?;")
      && writer.includes("crate::atomic_file::write_recoverable(")
      && sources.broker.includes("discord-native-visible-row-runtime-receipt.json"),
    privacy:
      sources.uiAdapter.includes("if (!exactKeys(record, RECEIPT_KEYS)")
      && sources.uiAdapter.includes("if (!exactKeys(outcomes, OUTCOME_KEYS)")
      && !/pub\s+(?:message|poster|carrier|blob|ciphertext|payload|plaintext|hwnd|pid)\b/u
        .test(between(
          sources.broker,
          "pub struct NativeVisibleRowRuntimeReceipt {",
          "\n}",
        )),
  };
}

function allPass(gate: RuntimeReceiptGate): boolean {
  return Object.values(gate).every(Boolean);
}

function mutate(key: keyof Sources, from: string, to: string): Sources {
  const changed = baseline[key].replace(from, to);
  expect(changed, `${key} mutation must alter source`).not.toBe(baseline[key]);
  return { ...baseline, [key]: changed };
}

describe("native visible-row Windows runtime receipt", () => {
  it("has a complete registered product path", () => {
    const gate = detect(baseline);
    for (const [stage, passed] of Object.entries(gate)) {
      expect(passed, stage).toBe(true);
    }
    expect(allPass(gate)).toBe(true);
  });

  it("fails each requested source stage when its production edge is removed", () => {
    const mutations: Array<[keyof RuntimeReceiptGate, Sources]> = [
      ["registration", mutate(
        "nativeMain",
        "            request_native_discord_visible_row_qa_receipt,",
        "            request_native_discord_visible_row_qa_receipt_DISABLED,",
      )],
      ["acl", mutate(
        "permissions",
        'commands.allow = ["request_native_discord_visible_row_qa_receipt"]',
        'commands.allow = ["request_native_discord_visible_row_qa_receipt_DISABLED"]',
      )],
      ["capability", mutate(
        "capability",
        '"allow-request-native-discord-visible-row-qa-receipt"',
        '"allow-request-native-discord-visible-row-qa-receipt-DISABLED"',
      )],
      ["caller", mutate(
        "uiAdapter",
        'invoke<unknown>("request_native_discord_visible_row_qa_receipt")',
        'invoke<unknown>("request_native_discord_visible_row_qa_receipt_DISABLED")',
      )],
      ["producer", mutate(
        "adapter",
        "windows::read_visible_message_rows_qa_detached(",
        "windows::read_visible_message_rows_qa_detached_DISABLED(",
      )],
      ["broker", mutate(
        "broker",
        "let authenticated = rehydrate_native_discord_overlay_history(",
        "let authenticated = rehydrate_native_discord_overlay_history_DISABLED(",
      )],
      ["peerAnchor", mutate(
        "adapter",
        "&& *candidate != foreign.expected_peer_identity",
        "&& *candidate != foreign.self_identity",
      )],
      ["refusals", mutate(
        "broker",
        "if rows.is_empty() || window_generation == 0",
        "if window_generation == 0",
      )],
      ["persistence", mutate(
        "broker",
        "crate::atomic_file::write_recoverable(\n        path,\n        &encoded,\n        \"Native visible-row QA runtime receipt\",",
        "crate::atomic_file::write_recoverable_DISABLED(\n        path,\n        &encoded,\n        \"Native visible-row QA runtime receipt\",",
      )],
    ];
    for (const [stage, sources] of mutations) {
      expect(detect(sources)[stage], stage).toBe(false);
    }
  });

  it("parses only the exact bounded nonsecret DTO", () => {
    const receipt = {
      schemaVersion: 2,
      observedAtUnixMs: 1,
      buildHash: "a".repeat(40),
      oslTargetIdentitySha256: "b".repeat(64),
      discordTargetIdentitySha256: "c".repeat(64),
      scopeBindingSha256: "d".repeat(64),
      windowGeneration: 7,
      rowsObserved: 2,
      nativeProofSome: 2,
      nativeProofNone: 0,
      authenticatedOwnOutgoing: 1,
      authenticatedPeerIncoming: 1,
      brokerPlaintextRows: 2,
      brokerRefusedRows: 0,
      outcomes: {
        ownOutgoing: "accepted",
        peerIncoming: "accepted",
        peerAnchor: "accepted",
        zeroRows: "refused",
        missingProof: "refused",
        mixedScope: "refused",
        differentNonSelf: "refused",
        replay: "refused",
        reorder: "refused",
        persistence: "accepted",
      },
      accepted: true,
    };
    expect(parseNativeVisibleRowRuntimeReceipt(receipt)).toEqual(receipt);
    expect(parseNativeVisibleRowRuntimeReceipt({ ...receipt, plaintext: "secret" })).toBeNull();
    expect(parseNativeVisibleRowRuntimeReceipt({
      ...receipt,
      outcomes: { ...receipt.outcomes, replay: "not_a_state" },
    })).toBeNull();
    expect(parseNativeVisibleRowRuntimeReceipt({
      ...receipt,
      oslTargetIdentitySha256: "not-a-hash",
    })).toBeNull();
  });
});
