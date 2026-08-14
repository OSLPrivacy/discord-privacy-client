// TASK 1340 - non-image clipboard content stays text.
//
// Pastes every payload in a fixture into the REAL OSL Chat composer: the
// listeners `bindWorkspace()` actually installs on `#osl-chat-draft`, the real
// `setOslChatDraft` path behind the composer's `input` event, and the real
// `osl-chat` workspace markup. Nothing here re-implements the composer.
//
// What a paste of non-image content is allowed to do:
//   * plain text  -> stays text. Nothing intercepts the paste (no
//     preventDefault), so the platform's own text insertion stands, and the
//     text the composer ends up holding is byte-for-byte the text that was
//     copied.
//   * an unsupported clipboard object -> nothing at all. No attachment card in
//     the rendered chat, no attachment/clipboard IPC command, no send.
//
// The payload list is a fixture, not a literal, so the check can be aimed
// elsewhere with TASK_1340_CLIPBOARD_FIXTURE=<path>. A fixture that carries no
// non-image clipboard content fails the check rather than passing vacuously:
// a green run over images alone says nothing about non-image content.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./styles.css", () => ({}));
vi.mock("./local-protected-sheet.css", () => ({}));
vi.mock("./friend-invite.css", () => ({}));
vi.mock("./recovery-screen.css", () => ({}));
vi.mock("./onboarding-mullvad.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

type Ui = typeof import("./main");

const DEFAULT_FIXTURE = resolve(__dirname, "fixtures/task_1340_clipboard_payloads.json");
const ATTACHMENT_CARD_MARKER = 'class="attachment-progress"';
/** Commands that would mean a paste became an attachment or a message. */
const ATTACHMENT_COMMAND_MARKERS = ["clipboard", "attachment", "prepare_osl_chat_text", "send"];

type ClipboardPayload = {
  name: string;
  kind: "text" | "object" | "image";
  media_type: string;
  text?: string;
  bytes_hex?: string;
};

type PasteProbe = {
  event: FixturePasteEvent;
  preventDefaultCalls: number;
};

/** The parts of a ClipboardEvent the composer could possibly read. */
type FixturePasteEvent = {
  type: "paste";
  defaultPrevented: boolean;
  preventDefault(): void;
  clipboardData: {
    types: string[];
    getData(type: string): string;
    items: Array<{ kind: "string" | "file"; type: string; getAsFile(): unknown; getAsString(callback: (value: string) => void): void }>;
    files: unknown[];
  };
};

type StubElement = {
  id: string;
  value: string;
  disabled: boolean;
  dataset: Record<string, string>;
  textContent: string;
  classList: { toggle(): void; add(): void; remove(): void };
  addEventListener(type: string, handler: (event: unknown) => void): void;
  closest(): null;
  querySelector(): null;
  querySelectorAll(): never[];
  dispatch(type: string, event: unknown): void;
};

const localStore = new Map<string, string>();
let ui: Ui;
let draftElement: StubElement;
let sendButton: StubElement;

function stubElement(id: string): StubElement {
  const listeners = new Map<string, Array<(event: unknown) => void>>();
  return {
    id,
    value: "",
    disabled: false,
    dataset: {},
    textContent: "",
    classList: { toggle: () => undefined, add: () => undefined, remove: () => undefined },
    addEventListener(type, handler) {
      const bucket = listeners.get(type) ?? [];
      bucket.push(handler);
      listeners.set(type, bucket);
    },
    closest: () => null,
    querySelector: () => null,
    querySelectorAll: () => [],
    dispatch(type, event) {
      for (const handler of listeners.get(type) ?? []) handler(event);
    },
  };
}

function documentStub(): Partial<Document> {
  return {
    querySelector: vi.fn((selector: string) => {
      if (selector === "#osl-chat-draft") return draftElement;
      if (selector === "button.osl-chat-send") return sendButton;
      return null;
    }) as unknown as Document["querySelector"],
    querySelectorAll: vi.fn(() => [] as unknown as NodeListOf<Element>) as unknown as Document["querySelectorAll"],
    createElement: vi.fn(() => ({ innerHTML: "", querySelector: vi.fn(() => null) } as unknown as HTMLElement)) as unknown as Document["createElement"],
    documentElement: { classList: { add: vi.fn() }, dataset: {} } as unknown as HTMLElement,
    addEventListener: vi.fn(),
    visibilityState: "visible",
  };
}

function fixturePath(): string {
  const override = process.env.TASK_1340_CLIPBOARD_FIXTURE;
  return override && override.trim().length > 0 ? resolve(override) : DEFAULT_FIXTURE;
}

function loadPayloads(path: string): ClipboardPayload[] {
  const document_ = JSON.parse(readFileSync(path, "utf8")) as { payloads?: ClipboardPayload[] };
  const payloads = document_.payloads;
  if (!Array.isArray(payloads)) throw new Error(`clipboard fixture ${path} has no 'payloads' array`);
  for (const payload of payloads) {
    if (!payload.name || !payload.kind || !payload.media_type) {
      throw new Error(`clipboard fixture ${path} has a payload missing name/kind/media_type`);
    }
    if (!["text", "object", "image"].includes(payload.kind)) {
      throw new Error(`clipboard payload ${payload.name}: kind must be text, object or image`);
    }
    const hasText = typeof payload.text === "string";
    const hasBytes = typeof payload.bytes_hex === "string";
    if (hasText === hasBytes) {
      throw new Error(`clipboard payload ${payload.name}: give exactly one of 'text' or 'bytes_hex'`);
    }
    if (payload.kind === "text" && !hasText) {
      throw new Error(`clipboard payload ${payload.name}: a text payload must carry its 'text'`);
    }
  }
  return payloads;
}

/**
 * The anti-vacuity gate. A run that never pasted anything non-image cannot have
 * shown that non-image clipboard content stays text, so it fails here instead
 * of reporting green.
 */
function requireNonImageClipboardContent(path: string, payloads: ClipboardPayload[]): void {
  const textPayloads = payloads.filter((payload) => payload.kind === "text");
  const objectPayloads = payloads.filter((payload) => payload.kind === "object");

  console.log(`TASK1340_FIXTURE=${path}`);
  console.log(`TASK1340_FIXTURE_PAYLOADS=${payloads.length}`);
  console.log(`TASK1340_FIXTURE_TEXT_PAYLOADS=${textPayloads.length}`);
  console.log(`TASK1340_FIXTURE_UNSUPPORTED_OBJECT_PAYLOADS=${objectPayloads.length}`);

  if (textPayloads.length < 1) {
    throw new Error(
      `clipboard fixture ${path} carries no plain-text payload: a check that never pasted text cannot show that text stays text`,
    );
  }
  if (objectPayloads.length < 1) {
    throw new Error(
      `clipboard fixture ${path} carries no unsupported clipboard object: a check that never pasted one cannot show it creates no attachment or message`,
    );
  }
  for (const payload of payloads.filter((entry) => entry.kind !== "image")) {
    if (payload.media_type.startsWith("image/")) {
      throw new Error(`clipboard payload ${payload.name} claims kind ${payload.kind} but carries image media type ${payload.media_type}`);
    }
  }
}

function pasteProbe(payload: ClipboardPayload): PasteProbe {
  const probe = { preventDefaultCalls: 0 } as PasteProbe;
  const text = payload.text ?? "";
  const items = payload.kind === "text"
    ? [{
        kind: "string" as const,
        type: payload.media_type,
        getAsFile: () => null,
        getAsString: (callback: (value: string) => void) => callback(text),
      }]
    : [{
        kind: "file" as const,
        type: payload.media_type,
        getAsFile: () => ({
          name: payload.name,
          type: payload.media_type,
          arrayBuffer: () => Promise.resolve(new Uint8Array(hexBytes(payload.bytes_hex ?? "")).buffer),
        }),
        getAsString: (callback: (value: string) => void) => callback(""),
      }];
  probe.event = {
    type: "paste",
    defaultPrevented: false,
    preventDefault() {
      probe.preventDefaultCalls += 1;
      probe.event.defaultPrevented = true;
    },
    clipboardData: {
      types: [payload.media_type],
      getData: (type: string) => (type === payload.media_type ? text : ""),
      items,
      files: payload.kind === "text" ? [] : [items[0].getAsFile()],
    },
  };
  return probe;
}

function hexBytes(hex: string): number[] {
  const bytes: number[] = [];
  for (let index = 0; index + 1 < hex.length; index += 2) bytes.push(Number.parseInt(hex.slice(index, index + 2), 16));
  return bytes;
}

function attachmentCommandCalls(): string[] {
  return mocks.invoke.mock.calls
    .map((call) => String(call[0] ?? ""))
    .filter((command) => ATTACHMENT_COMMAND_MARKERS.some((marker) => command.includes(marker)));
}

function attachmentCardCount(): number {
  const markup = ui.__oslHubUiTest.renderWorkspaceContent("osl-chat");
  return markup.split(ATTACHMENT_CARD_MARKER).length - 1;
}

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  draftElement = stubElement("osl-chat-draft");
  sendButton = stubElement("osl-chat-send");
  vi.stubGlobal("document", documentStub());
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
  mocks.invoke.mockReset();
  mocks.invoke.mockResolvedValue(undefined);
  draftElement = stubElement("osl-chat-draft");
  sendButton = stubElement("osl-chat-send");
  vi.stubGlobal("document", documentStub());
  ui.__oslHubUiTest.reset({ route: "home" });
});

describe("TASK 1340 non-image clipboard content stays text", () => {
  it("keeps pasted text as text and makes no attachment or message from an unsupported object", () => {
    const path = fixturePath();
    const payloads = loadPayloads(path);
    requireNonImageClipboardContent(path, payloads);

    // Real approved OSL Chat context with an empty composer, then the real
    // composer wiring.
    ui.__oslHubUiTest.escapeAuditTypeHalfMessage("osl-chat", "");
    ui.__oslHubUiTest.bindWorkspace();

    const cardsBefore = attachmentCardCount();
    let pasted = 0;
    let interceptedPastes = 0;
    let textStayedText = 0;
    let objectsThatChangedTheDraft = 0;

    for (const payload of payloads.filter((entry) => entry.kind !== "image")) {
      pasted += 1;
      const draftBefore = ui.__oslHubUiTest.escapeAuditDraft("osl-chat");
      const probe = pasteProbe(payload);
      draftElement.dispatch("paste", probe.event);
      if (probe.preventDefaultCalls > 0) interceptedPastes += 1;

      if (payload.kind === "text") {
        // Nothing swallowed the paste, so the platform's own text insertion is
        // what happens next. Reproduce exactly that -- the character data lands
        // in the textarea -- and let the app's real `input` listener run.
        draftElement.value = draftBefore + (payload.text ?? "");
        draftElement.dispatch("input", { type: "input" });
        const draftAfter = ui.__oslHubUiTest.escapeAuditDraft("osl-chat");
        expect(draftAfter).toBe(draftBefore + payload.text);
        expect(typeof draftAfter).toBe("string");
        textStayedText += 1;
        console.log(`TASK1340_TEXT_STAYS_TEXT name=${payload.name} media_type=${payload.media_type} prevent_default=${probe.preventDefaultCalls} draft=${JSON.stringify(draftAfter)}`);
        // Put the composer back so the next payload starts from a known draft.
        draftElement.value = draftBefore;
        draftElement.dispatch("input", { type: "input" });
      } else {
        const draftAfter = ui.__oslHubUiTest.escapeAuditDraft("osl-chat");
        if (draftAfter !== draftBefore) objectsThatChangedTheDraft += 1;
        console.log(`TASK1340_UNSUPPORTED_OBJECT name=${payload.name} media_type=${payload.media_type} prevent_default=${probe.preventDefaultCalls} draft_changed=${draftAfter !== draftBefore}`);
      }
    }

    const cardsAfter = attachmentCardCount();
    const commands = attachmentCommandCalls();
    const sendAttempts = ui.__oslHubUiTest.escapeAuditState().sendAttempts;

    console.log(`TASK1340_NON_IMAGE_PASTES=${pasted}`);
    console.log(`TASK1340_INTERCEPTED_PASTES=${interceptedPastes}`);
    console.log(`TASK1340_TEXT_PAYLOADS_STAYED_TEXT=${textStayedText}`);
    console.log(`TASK1340_ATTACHMENT_CARDS_BEFORE=${cardsBefore}`);
    console.log(`TASK1340_ATTACHMENT_CARDS_AFTER=${cardsAfter}`);
    console.log(`TASK1340_ATTACHMENT_OR_SEND_COMMANDS=${commands.length}${commands.length ? ` (${commands.join(",")})` : ""}`);
    console.log(`TASK1340_SEND_ATTEMPTS=${sendAttempts}`);
    console.log(`TASK1340_OBJECT_PASTES_THAT_CHANGED_THE_DRAFT=${objectsThatChangedTheDraft}`);

    expect(pasted).toBe(payloads.filter((entry) => entry.kind !== "image").length);
    expect(interceptedPastes).toBe(0);
    expect(textStayedText).toBe(payloads.filter((entry) => entry.kind === "text").length);
    expect(cardsBefore).toBe(0);
    expect(cardsAfter).toBe(0);
    expect(commands).toEqual([]);
    expect(sendAttempts).toBe(0);
    expect(objectsThatChangedTheDraft).toBe(0);
  });

  it("still renders the composer it just pasted into", () => {
    // Guard against the previous case passing because nothing was there at all:
    // the chat that showed zero attachment cards is a real, rendered composer.
    ui.__oslHubUiTest.escapeAuditTypeHalfMessage("osl-chat", "TASK1340 composer is real");
    const markup = ui.__oslHubUiTest.renderWorkspaceContent("osl-chat");
    const hasComposer = markup.includes('id="osl-chat-draft"');
    console.log(`TASK1340_COMPOSER_RENDERED=${hasComposer}`);
    console.log(`TASK1340_COMPOSER_DRAFT=${JSON.stringify(ui.__oslHubUiTest.escapeAuditDraft("osl-chat"))}`);
    expect(hasComposer).toBe(true);
    expect(ui.__oslHubUiTest.escapeAuditDraft("osl-chat")).toBe("TASK1340 composer is real");
  });
});
