import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { blankLocalProtectedModel, localProtectedSheetMarkup } from "./local-protected-sheet";

const KEY_DESTRUCTION_CLAIM =
  /\b(?:delete(?:d)?|destroy(?:ed)?|erase(?:d)?|remove(?:d)?|shred(?:ded)?|wipe(?:d)?)\b[^.\n<]{0,80}\b(?:per-message\s+|local\s+|message\s+|decryption\s+)?keys?\b|\b(?:per-message\s+|local\s+|message\s+|decryption\s+)?keys?\b[^.\n<]{0,80}\b(?:delete(?:d)?|destroy(?:ed)?|erase(?:d)?|remove(?:d)?|shred(?:ded)?|wipe(?:d)?)\b/iu;

function sourceBetween(source: string, start: string, end: string): string {
  const startIndex = source.indexOf(start);
  const endIndex = source.indexOf(end, startIndex + start.length);
  expect(startIndex, `missing production source marker: ${start}`).toBeGreaterThanOrEqual(0);
  expect(endIndex, `missing production source marker: ${end}`).toBeGreaterThan(startIndex);
  return source.slice(startIndex, endIndex);
}

describe("reachable local-protected key claims", () => {
  it("describes ledger authorization expiry and consumption without claiming key destruction", () => {
    const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const brokerSource = readFileSync(
      new URL("../../osl-hub/src/broker.rs", import.meta.url),
      "utf8",
    );
    const localHandlers = sourceBetween(
      mainSource,
      "async function startLocalProtectedContext",
      "function bindLocalProtectedSheet",
    );
    const localBindings = sourceBetween(
      mainSource,
      "function bindLocalProtectedSheet",
      "function bindWorkspace",
    );
    const workspaceRenderer = sourceBetween(
      mainSource,
      "function renderWorkspace",
      "function appLauncherStrip",
    );
    const recordDefinition = sourceBetween(
      brokerSource,
      "struct LocalProtectedRecord",
      "struct LocalProtectedLedger",
    );
    const successfulOpenPolicy = sourceBetween(
      brokerSource,
      "fn apply_successful_open_policy",
      "fn local_protected_identity",
    );
    const setupMarkup = localProtectedSheetMarkup(blankLocalProtectedModel(true));
    const readyModel = {
      ...blankLocalProtectedModel(true),
      context: {
        contextToken: "ctx-1-abc",
        serviceId: "discord",
        accountId: "account-1",
        conversationId: "local-abababababababababababababababab",
      },
    };
    const reachableUiCopy = [
      setupMarkup,
      localProtectedSheetMarkup(readyModel),
      localProtectedSheetMarkup({ ...readyModel, pane: "open" }),
      localHandlers,
    ].join("\n");

    expect(workspaceRenderer).toContain("localProtectedSheetMarkup(localProtectedSheet, setup.sendMode)");
    expect(localBindings).toContain(
      'querySelector<HTMLFormElement>("#local-context-form")?.addEventListener("submit", (event) => void startLocalProtectedContext(event))',
    );
    expect(localBindings).toContain(
      'querySelector<HTMLFormElement>("#local-protect-form")?.addEventListener("submit", (event) => void prepareLocalProtectedDraft(event))',
    );
    expect(localBindings).toContain(
      'querySelector<HTMLFormElement>("#local-open-form")?.addEventListener("submit", (event) => void openLocalProtectedCapsule(event))',
    );

    const storesPerMessageKey =
      /\b(?:per_message_key|message_key|local_key|capsule_key|decryption_key|key_material)\s*:/u
      .test(recordDefinition);
    const destroysStoredPerMessageKey =
      /\b(?:zeroize|fill\s*\(\s*0\s*\)|destroy_local_protected_message_key|wipe_local_protected_message_key)\b/u
        .test(successfulOpenPolicy);
    const hasStoredPerMessageKeyLifecycle = storesPerMessageKey && destroysStoredPerMessageKey;
    expect(hasStoredPerMessageKeyLifecycle).toBe(false);
    expect(successfulOpenPolicy).toContain("ledger.records.remove(local_message_id)");

    if (!hasStoredPerMessageKeyLifecycle) {
      expect(reachableUiCopy).not.toMatch(KEY_DESTRUCTION_CLAIM);
    }

    expect("Delete the local key after one hour").toMatch(KEY_DESTRUCTION_CLAIM);
    expect("Its per-message key was removed").toMatch(KEY_DESTRUCTION_CLAIM);
    expect(reachableUiCopy).toContain("Opening authorization expires after");
    expect(reachableUiCopy).toContain("After expiry, OSL refuses to open this text on this device.");
    expect(localHandlers).toContain("Opened once. Local authorization was consumed.");
  });
});
