import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  blankLocalProtectedModel,
  LOCAL_TTL_OPTIONS,
  loadOrCreateLocalConversationId,
  localConversationStorageKey,
  localProtectedSheetMarkup,
  validLocalChatLabel,
} from "./local-protected-sheet";

class MemoryStorage {
  readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
}

describe("local protected side sheet", () => {
  it("offers exactly the enforced timers and defaults to one hour", () => {
    expect(LOCAL_TTL_OPTIONS).toEqual([3_600, 86_400, 259_200, 604_800]);
    const model = blankLocalProtectedModel(true);
    expect(model.ttlSeconds).toBe(3_600);
    const markup = localProtectedSheetMarkup({
      ...model,
      context: {
        contextToken: "ctx-1-abc",
        serviceId: "discord",
        accountId: "account-1",
        conversationId: "local-abababababababababababababababab",
      },
    });
    expect(markup.match(/<option /gu)).toHaveLength(4);
    expect(markup).toContain('<option value="3600" selected>1 hour</option>');
    expect(markup).not.toContain('value="0"');
    expect(markup).not.toContain("No timer");
  });

  it("persists only an opaque random context id, never the chat label", () => {
    const storage = new MemoryStorage();
    const random = (bytes: Uint8Array): Uint8Array => { bytes.fill(0xab); return bytes; };
    const first = loadOrCreateLocalConversationId(storage, "discord", "account-1", random);
    const second = loadOrCreateLocalConversationId(storage, "discord", "account-1", () => {
      throw new Error("random should not run twice");
    });
    expect(first).toBe("local-abababababababababababababababab");
    expect(second).toBe(first);
    expect([...storage.values.entries()]).toEqual([[
      localConversationStorageKey("discord", "account-1"),
      first,
    ]]);
    expect(JSON.stringify([...storage.values.entries()])).not.toContain("Rose");
  });

  it("accepts simple transient labels and rejects controls or bidi overrides", () => {
    expect(validLocalChatLabel("Rose")).toBe(true);
    expect(validLocalChatLabel("  family chat  ")).toBe(true);
    expect(validLocalChatLabel(" ")).toBe(false);
    expect(validLocalChatLabel(`Rose\u202eexe`)).toBe(false);
    expect(validLocalChatLabel("x".repeat(49))).toBe(false);
  });

  it("renders honest manual protection language and no automatic authority", () => {
    const setup = blankLocalProtectedModel(true);
    const setupMarkup = localProtectedSheetMarkup(setup);
    expect(setupMarkup).toContain("On this device");
    expect(setupMarkup).toContain("Only a random ID is saved");
    expect(setupMarkup).toContain("OSL cannot see the service page");
    expect(setupMarkup).not.toMatch(/auto(?:matic)? send|read the page|person-to-person E2EE is active/i);

    const ready = {
      ...setup,
      chatLabel: "<Rose>",
      context: {
        contextToken: "ctx-1-abc",
        serviceId: "discord",
        accountId: "account-1",
        conversationId: "local-abababababababababababababababab",
      },
      capsule: "<encrypted>",
      openedPlaintext: "<private>",
    };
    const readyMarkup = localProtectedSheetMarkup(ready);
    expect(readyMarkup).toContain("Manual copy & paste");
    expect(readyMarkup).toContain("no page access");
    expect(readyMarkup).toContain("not person-to-person E2EE");
    expect(readyMarkup).not.toContain("<Rose>");
    expect(readyMarkup).not.toContain("<private>");
    const openMarkup = localProtectedSheetMarkup({ ...ready, pane: "open" });
    expect(openMarkup).toContain('id="local-decrypt-display"');
    expect(openMarkup).toContain("Only for this local chat.");
    expect(localProtectedSheetMarkup(ready, "clipboard")).toContain("Encrypt & copy");
    expect(localProtectedSheetMarkup(ready, "double")).toContain("Prepare · Double Enter");
    expect(localProtectedSheetMarkup(ready, "single")).toContain("Prepare · Single Enter");
    expect(readyMarkup).toContain('<option value="259200"');
    expect(readyMarkup).toContain("3 days");
  });

  it("keeps the sheet absent until an embedded profile explicitly opens it", () => {
    expect(localProtectedSheetMarkup(blankLocalProtectedModel(false))).toBe("");
  });

  it("keeps every new surface square", () => {
    const styles = readFileSync(new URL("./local-protected-sheet.css", import.meta.url), "utf8");
    expect(styles).not.toMatch(/border-radius\s*:/u);
  });
});
