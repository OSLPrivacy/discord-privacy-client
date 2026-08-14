/**
 * TASK 0594 - say why the view once button is off on a free account.
 *
 * The finish line: a Free account sees the control off carrying BOTH sentences
 * (making one needs Pro / opening one is free), a Pro account sees it on, and
 * the count of places the control is off with no explanation is 0.
 *
 * Every surface that draws a view-once control is rendered here for real and
 * fed to the audit, and the shipped sources are swept so a control added
 * outside the shared renderer cannot hide from that count.
 */
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import type { HubPerson, LocalLoopbackContext, ManualPeerContext } from "./adapters";
import { blankLocalProtectedModel, localProtectedSheetMarkup } from "./local-protected-sheet";
import { blankPeerProtectedModel, peerProtectedSheetMarkup } from "./peer-protected-sheet";
import { oslChatsViewMarkup, type OslChatsViewModel } from "./osl-chats-view";
import {
  VIEW_ONCE_FREE_OPEN_SENTENCE,
  VIEW_ONCE_PRO_CREATE_SENTENCE,
  viewOnceControlAudit,
  viewOnceControlState,
  viewOnceControlsOffWithoutExplanation,
  viewOnceCreationAllowed,
} from "./view-once-tier";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

const OVERLAY_HTML = readRelative("../overlay.html");

const LOCAL_CONTEXT: LocalLoopbackContext = {
  contextToken: "token-local",
  serviceId: "discord",
  accountId: "account-1",
  conversationId: "local-000102030405060708090a0b0c0d0e",
};

const PEER_CONTEXT: ManualPeerContext = {
  contextToken: "token-peer",
  serviceId: "discord",
  accountId: "account-1",
  personId: "person-1",
  peerOslUserId: "osl-user-1",
  scopeApproved: true,
};

const PEOPLE: HubPerson[] = [{
  personId: "person-1",
  oslUserId: "osl-user-1",
  alias: "Rose",
  safetyNumber: "1234 5678",
  safetyNumberVerified: true,
  whitelistCount: 0,
  whitelistedScopes: [],
  whitelistedScopesTruncated: false,
  pendingKeyChange: false,
  reachBroadened: false,
  reachBroadenedAt: null,
  reachNarrowedScopes: [],
}];

function localSheet(viewOnceCreationAllowed: boolean): string {
  return localProtectedSheetMarkup({
    ...blankLocalProtectedModel(true),
    chatLabel: "Rose",
    context: LOCAL_CONTEXT,
    viewOnceCreationAllowed,
  });
}

function peerSheet(viewOnceCreationAllowed: boolean): string {
  return peerProtectedSheetMarkup({
    ...blankPeerProtectedModel(true),
    displayName: "Rose",
    personId: "person-1",
    context: PEER_CONTEXT,
    viewOnceCreationAllowed,
  }, PEOPLE);
}

function oslChats(viewOnceCreationAllowed: boolean): string {
  const model: OslChatsViewModel = {
    friends: [{
      personId: "person-1",
      nickname: "Rose",
      verified: true,
      ready: true,
      preview: null,
      previewVisible: true,
      unreadCount: 0,
      handshakeConfirmed: true,
    }],
    activePersonId: "person-1",
    messages: [],
    draft: "",
    busy: false,
    viewOnce: false,
    viewOnceCreationAllowed,
  };
  return oslChatsViewMarkup(model);
}

/** The named surfaces that draw a view-once control, for a given account tier. */
function surfaces(creationAllowed: boolean): { name: string; markup: string }[] {
  return [
    { name: "local-protected-sheet", markup: localSheet(creationAllowed) },
    { name: "peer-protected-sheet", markup: peerSheet(creationAllowed) },
    { name: "osl-chats-composer", markup: oslChats(creationAllowed) },
    // The overlay ships its control off with the reason in the file; the Pro
    // case is the runtime state below, since overlay.ts needs a live DOM.
    { name: "discord-overlay", markup: OVERLAY_HTML },
  ];
}

describe("TASK 0594 the view once control says why it is off on a free account", () => {
  it("free: every surface draws the control off with both sentences", () => {
    expect(viewOnceCreationAllowed("free")).toBe(false);
    for (const surface of surfaces(false)) {
      const entries = viewOnceControlAudit(surface.markup);
      expect(entries).toHaveLength(1);
      const [entry] = entries;
      const segment = surface.markup.slice(
        surface.markup.indexOf(`data-osl-view-once-control="${entry.id}"`),
        surface.markup.indexOf("</label>", surface.markup.indexOf(`data-osl-view-once-control="${entry.id}"`)),
      );
      console.log(`TASK0594 free ${surface.name} id=${entry.id} off=${entry.off} reason=${entry.reason} proSentence=${segment.includes(VIEW_ONCE_PRO_CREATE_SENTENCE)} freeSentence=${segment.includes(VIEW_ONCE_FREE_OPEN_SENTENCE)}`);
      expect(entry.off).toBe(true);
      expect(entry.reason).toBe("tier");
      expect(segment).toContain(VIEW_ONCE_PRO_CREATE_SENTENCE);
      expect(segment).toContain(VIEW_ONCE_FREE_OPEN_SENTENCE);
      expect(entry.explained).toBe(true);
    }
  });

  it("pro: every surface draws the control on", () => {
    expect(viewOnceCreationAllowed("pro")).toBe(true);
    expect(viewOnceCreationAllowed("offlineGrace")).toBe(true);
    for (const surface of surfaces(true)) {
      if (surface.name === "discord-overlay") continue;
      const [entry] = viewOnceControlAudit(surface.markup);
      console.log(`TASK0594 pro ${surface.name} id=${entry.id} off=${entry.off} reason="${entry.reason}"`);
      expect(entry.off).toBe(false);
      expect(entry.reason).toBe("");
      expect(surface.markup).not.toContain(VIEW_ONCE_PRO_CREATE_SENTENCE);
    }
    // The overlay's runtime decision, which is what refreshControls() applies.
    const proReady = viewOnceControlState({ creationAllowed: true, unavailable: false });
    console.log(`TASK0594 pro discord-overlay off=${proReady.off} reason="${proReady.reason}"`);
    expect(proReady).toEqual({ off: false, reason: "", explanation: "" });

    const freeReady = viewOnceControlState({ creationAllowed: false, unavailable: false });
    expect(freeReady.off).toBe(true);
    expect(freeReady.explanation).toContain(VIEW_ONCE_PRO_CREATE_SENTENCE);
    expect(freeReady.explanation).toContain(VIEW_ONCE_FREE_OPEN_SENTENCE);
  });

  it("counts 0 places where the control is off with no explanation", () => {
    const rendered = [...surfaces(false), ...surfaces(true)];
    const unexplained = rendered.flatMap((surface) =>
      viewOnceControlsOffWithoutExplanation(surface.markup).map((id) => `${surface.name}:${id}`));
    console.log(`TASK0594 surfaces_audited=${rendered.length} off_without_explanation=${unexplained.length} ${JSON.stringify(unexplained)}`);
    expect(unexplained).toEqual([]);
    expect(unexplained.length).toBe(0);
  });

  it("sweeps the shipped sources: no view-once checkbox escapes the shared control", () => {
    const sourceDirectory = fileURLToPath(new URL(".", import.meta.url));
    const sources = readdirSync(sourceDirectory)
      .filter((name) => name.endsWith(".ts") && !name.endsWith(".test.ts"))
      .map((name) => ({ name, text: readFileSync(`${sourceDirectory}${name}`, "utf8") }));
    const files = [...sources, { name: "overlay.html", text: OVERLAY_HTML }];
    const escaped: string[] = [];
    for (const file of files) {
      for (const match of file.text.matchAll(/<input\b[^>]*id="([^"]*view-once[^"]*)"[^>]*>/giu)) {
        const before = file.text.slice(0, match.index ?? 0);
        const openedControl = before.lastIndexOf("data-osl-view-once-control=");
        const closedLabel = before.lastIndexOf("</label>");
        if (openedControl === -1 || openedControl < closedLabel) escaped.push(`${file.name}:${match[1]}`);
      }
    }
    console.log(`TASK0594 files_swept=${files.length} view_once_checkboxes_outside_the_shared_control=${escaped.length} ${JSON.stringify(escaped)}`);
    expect(escaped).toEqual([]);
  });
});
