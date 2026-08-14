// TASK 1339 -- connect clipboard paste to the chat box
//
// A fixture clipboard paste of an image, on the OSL Chat composer, must stage
// exactly one image attachment card -- reusing the same attachment-progress
// pipeline the "Choose file" flow renders from -- and it must do so without
// ever touching Send.

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
    querySelector: vi.fn((selector: string) => (selector === "#app" ? root : null)),
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
    personId: "person-1339",
    oslUserId: "OSLUSER-person-1339",
    alias: "Task 1339",
    safetyNumber: "1339 1339",
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

/** A minimal ClipboardEvent fixture carrying one pasted PNG among other items. */
function fixtureClipboardImagePaste(bytes: Uint8Array, mediaType = "image/png"): { event: ClipboardEvent; preventDefault: ReturnType<typeof vi.fn> } {
  const preventDefault = vi.fn();
  const file = { type: mediaType, arrayBuffer: async () => bytes.buffer };
  const items = [
    { kind: "string", type: "text/plain", getAsFile: () => null },
    { kind: "file", type: mediaType, getAsFile: () => file },
  ];
  const event = {
    clipboardData: { items },
    preventDefault,
  } as unknown as ClipboardEvent;
  return { event, preventDefault };
}

function attachmentCardCount(markup: string): number {
  return (markup.match(/class="attachment-progress"/gu) ?? []).length;
}

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe("TASK 1339 clipboard paste attachment", () => {
  beforeEach(() => {
    installGlobals();
    mocks.invoke.mockReset();
  });

  it("a fixture image paste shows one image card before Send", async () => {
    let acceptedMediaType: string | null = null;
    let acceptedByteCount = 0;
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "set_hub_screenshot_protection") return true;
      if (command === "list_hub_people") return [personRow()];
      if (command === "activate_osl_chat_context") {
        return {
          contextToken: "ctx-task1339",
          serviceId: "osl-chat",
          accountId: "osl-main",
          personId: "person-1339",
          peerOslUserId: "OSLUSER-person-1339",
          scopeApproved: true,
        };
      }
      if (command === "list_osl_chat_history") return null;
      if (command === "open_osl_chat_text") return null;
      if (command === "list_osl_chat_attachments") {
        return {
          attachments: [],
          authenticatedEnvelope: true,
          authority: true,
          binding: true,
          consent: true,
          protocol: "osl-chat-attachments-v1",
        };
      }
      if (command === "accept_osl_chat_clipboard_image_attachment") {
        acceptedMediaType = args?.mediaType as string;
        acceptedByteCount = (args?.imageBytes as number[]).length;
        return {
          contextId: "ctx-task1339",
          job: {
            jobId: "job-task1339",
            metadata: { filename: "clipboard-image.png", mediaType: "image/png", size: acceptedByteCount },
            caption: "",
            viewOnce: false,
            stage: "selected",
            progress: 0,
            retryFrom: null,
            failure: null,
          },
        };
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({
      coreReady: true,
      hubPeople: [{ personId: "person-1339", oslUserId: "OSLUSER-person-1339", safetyNumberVerified: true }],
      licenseAccess: "pro",
    });
    await __oslHubUiTest.openOslChatConversation("person-1339");

    const beforePasteCards = attachmentCardCount(__oslHubUiTest.renderWorkspaceContent("osl-chat"));

    const bytes = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    const { event, preventDefault } = fixtureClipboardImagePaste(bytes);
    await __oslHubUiTest.pasteOslChatClipboardImage(event);

    const afterPasteMarkup = __oslHubUiTest.renderWorkspaceContent("osl-chat");
    const afterPasteCards = attachmentCardCount(afterPasteMarkup);
    const imageCards = __oslHubUiTest.oslChatAttachmentCards().filter((mediaType) => mediaType.startsWith("image/")).length;
    const sendAttempts = __oslHubUiTest.escapeAuditState().sendAttempts;

    console.log(`TASK1339_BEFORE_PASTE_CARDS=${beforePasteCards}`);
    console.log(`TASK1339_ACCEPTED_COMMAND_INVOKED=${mocks.invoke.mock.calls.some((call) => call[0] === "accept_osl_chat_clipboard_image_attachment")}`);
    console.log(`TASK1339_ACCEPTED_MEDIA_TYPE=${acceptedMediaType}`);
    console.log(`TASK1339_ACCEPTED_BYTE_COUNT=${acceptedByteCount}`);
    console.log(`TASK1339_AFTER_PASTE_CARDS=${afterPasteCards}`);
    console.log(`TASK1339_IMAGE_CARDS=${imageCards}`);
    console.log(`TASK1339_PREVENT_DEFAULT_CALLED=${preventDefault.mock.calls.length > 0}`);
    console.log(`TASK1339_SEND_ATTEMPTS=${sendAttempts}`);

    expect(beforePasteCards).toBe(0);
    expect(acceptedMediaType).toBe("image/png");
    expect(acceptedByteCount).toBe(bytes.length);
    expect(afterPasteCards).toBe(1);
    expect(imageCards).toBe(1);
    expect(preventDefault).toHaveBeenCalled();
    expect(sendAttempts).toBe(0);
  }, 30_000);

  it("ignores a text-only paste and stages no card", async () => {
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "set_hub_screenshot_protection") return true;
      if (command === "list_hub_people") return [personRow()];
      if (command === "activate_osl_chat_context") {
        return {
          contextToken: "ctx-task1339b",
          serviceId: "osl-chat",
          accountId: "osl-main",
          personId: "person-1339",
          peerOslUserId: "OSLUSER-person-1339",
          scopeApproved: true,
        };
      }
      if (command === "list_osl_chat_history") return null;
      if (command === "open_osl_chat_text") return null;
      if (command === "list_osl_chat_attachments") {
        return {
          attachments: [],
          authenticatedEnvelope: true,
          authority: true,
          binding: true,
          consent: true,
          protocol: "osl-chat-attachments-v1",
        };
      }
      throw new Error(`unexpected command ${command}`);
    });

    const { __oslHubUiTest } = await import("./main");
    __oslHubUiTest.reset({
      coreReady: true,
      hubPeople: [{ personId: "person-1339", oslUserId: "OSLUSER-person-1339", safetyNumberVerified: true }],
      licenseAccess: "pro",
    });
    await __oslHubUiTest.openOslChatConversation("person-1339");

    const preventDefault = vi.fn();
    const event = {
      clipboardData: { items: [{ kind: "string", type: "text/plain", getAsFile: () => null }] },
      preventDefault,
    } as unknown as ClipboardEvent;
    await __oslHubUiTest.pasteOslChatClipboardImage(event);

    const cards = attachmentCardCount(__oslHubUiTest.renderWorkspaceContent("osl-chat"));
    console.log(`TASK1339B_TEXT_PASTE_CARDS=${cards}`);
    console.log(`TASK1339B_ACCEPT_COMMAND_INVOKED=${mocks.invoke.mock.calls.some((call) => call[0] === "accept_osl_chat_clipboard_image_attachment")}`);

    expect(cards).toBe(0);
    expect(preventDefault).not.toHaveBeenCalled();
  }, 30_000);
});
