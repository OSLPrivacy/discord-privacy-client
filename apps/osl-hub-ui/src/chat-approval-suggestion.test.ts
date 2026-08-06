import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
  invoke: vi.fn(),
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./logos", () => ({
  browserLogo: vi.fn(() => ""),
  providerLogo: vi.fn(() => ""),
  serviceLogo: vi.fn(() => ""),
}));

class MemoryStorage implements Storage {
  readonly values = new Map<string, string>();
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

function installGlobals(): void {
  const root = { innerHTML: "", querySelectorAll: vi.fn(() => []), querySelector: vi.fn(() => null) };
  const body = { append: vi.fn() };
  vi.stubGlobal("localStorage", new MemoryStorage());
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
  vi.stubGlobal("document", {
    activeElement: null,
    body,
    querySelector: vi.fn((selector: string) => selector === "#app" ? root : null),
    querySelectorAll: vi.fn(() => []),
    createElement: vi.fn(() => ({ innerHTML: "", querySelectorAll: vi.fn(() => []) })),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    addEventListener: vi.fn(),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
  });
  vi.stubGlobal("navigator", { onLine: true });
  vi.stubGlobal("HTMLInputElement", class {});
  vi.stubGlobal("HTMLTextAreaElement", class {});
  vi.stubGlobal("HTMLSelectElement", class {});
}

function personRow(): Record<string, unknown> {
  return {
    personId: "person-0706",
    oslUserId: "OSLUSER-person-0706",
    alias: "Task 0706",
    safetyNumber: "0706 0706",
    safetyNumberVerified: true,
    whitelistCount: 0,
    whitelistedScopes: [],
    whitelistedScopesTruncated: false,
    pendingKeyChange: false,
    reachBroadened: false,
    reachBroadenedAt: null,
    reachNarrowedScopes: [],
  };
}

function promptCount(markup: string): number {
  return markup.split("Turn on this encrypted chat").length - 1;
}

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe("chat approval suggestion choice", () => {
  beforeEach(() => {
    installGlobals();
    mocks.invoke.mockReset();
  });

  it("toggles the recorded prompt for the same unchecked OSL Chat assessment", async () => {
    let recordedChoice: "on" | "off" = "off";
    const assessedContextTokens: string[] = [];
    const assessedPeople: string[] = [];
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "set_hub_screenshot_protection") return true;
      if (command === "list_hub_people") return [personRow()];
      if (command === "activate_osl_chat_context") {
        return {
          contextToken: "ctx-task0706",
          serviceId: "osl-chat",
          accountId: "osl-main",
          personId: "person-0706",
          peerOslUserId: "OSLUSER-person-0706",
          scopeApproved: false,
          ...(recordedChoice === "on" ? { suggestion: "offer_approval" } : {}),
        };
      }
      if (command === "set_hub_chat_approval_suggestion_choice") {
        recordedChoice = args?.choice === "on" ? "on" : "off";
        return { choice: recordedChoice };
      }
      if (command === "answer_hub_chat_approval_suggestion") {
        assessedContextTokens.push(String(args?.contextToken));
        assessedPeople.push(String(args?.personId));
        return { suggestion: recordedChoice === "on" ? "offer_approval" : "no_suggestion" };
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({
      coreReady: true,
      hubPeople: [{ personId: "person-0706", oslUserId: "OSLUSER-person-0706", safetyNumberVerified: true }],
    });

    await __oslHubUiTest.openOslChatConversation("person-0706");
    const initialOffCount = promptCount(__oslHubUiTest.renderWorkspaceContent("osl-chat"));
    console.log(`TASK0706 initial_off.recorded_prompt_count=${initialOffCount}`);
    expect(initialOffCount).toBe(0);

    await __oslHubUiTest.setChatApprovalSuggestionChoice(true);
    const onMarkup = __oslHubUiTest.renderWorkspaceContent("osl-chat");
    const onCount = promptCount(onMarkup);
    console.log("TASK0706 recorded_prompt_text=Turn on this encrypted chat");
    console.log(`TASK0706 on.recorded_prompt_count=${onCount}`);
    expect(onCount).toBe(1);

    await __oslHubUiTest.setChatApprovalSuggestionChoice(false);
    const offAgainCount = promptCount(__oslHubUiTest.renderWorkspaceContent("osl-chat"));
    console.log(`TASK0706 off.recorded_prompt_count=${offAgainCount}`);
    expect(offAgainCount).toBe(0);

    const sameUncheckedChat = assessedContextTokens.length === 2
      && assessedContextTokens.every((token) => token === "ctx-task0706")
      && assessedPeople.every((personId) => personId === "person-0706");
    console.log(`TASK0706 same_unchecked_chat_assessed=${sameUncheckedChat}`);
    expect(sameUncheckedChat).toBe(true);
  }, 30_000);
});
