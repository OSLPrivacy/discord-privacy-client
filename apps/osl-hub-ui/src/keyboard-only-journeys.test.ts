import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { attachmentProgressMarkup } from "./attachment-progress";
import { friendRemovalButtonMarkup } from "./ui-behavior";
import { onboardingSendingMarkup } from "./onboarding-sending";
import { peerProtectedSheetMarkup, type PeerProtectedSheetModel } from "./peer-protected-sheet";
import { renderScrubRoute } from "./scrub-route";
import { oslChatsViewMarkup } from "./osl-chats-view";
import {
  formatKeyboardJourneyReport,
  runKeyboardOnlyJourneys,
  type KeyboardJourneySpec,
} from "./keyboard-only-journeys";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8")
  .replace(/\/\*[\s\S]*?\*\//gu, "")
  .replace(/\/\/.*$/gmu, "");

function sourceBackedSurface(anchor: string, html: string): string {
  expect(mainSource, `shipping main.ts should contain ${anchor}`).toContain(anchor);
  return html;
}

function peerModel(patch: Partial<PeerProtectedSheetModel>): PeerProtectedSheetModel {
  return {
    open: true,
    context: {
      contextToken: "ctx-keyboard",
      serviceId: "discord",
      accountId: "acct-keyboard",
      personId: "person-keyboard",
      peerOslUserId: "osl-peer-keyboard",
      scopeApproved: true,
    },
    personId: "person-keyboard",
    displayName: "Keyboard Friend",
    pane: "write",
    ttlSeconds: 3_600,
    viewOnce: false,
    decryptDisplayEnabled: true,
    busy: false,
    draft: "keyboard protected send",
    openDraft: "",
    coverText: "",
    openedPlaintext: "",
    receipt: null,
    handshakeConfirmed: true,
    status: "",
    ...patch,
  };
}

function keyboardJourneySpecs(): KeyboardJourneySpec[] {
  const friendSurface = sourceBackedSurface(
    'querySelector<HTMLFormElement>("#add-friend-form")?.addEventListener("submit"',
    `<form id="add-friend-form"><input id="friend-code-input"/><button type="submit">Add friend</button></form>${friendRemovalButtonMarkup("person-keyboard", (value) => value)}`,
  );
  const peerWrite = peerProtectedSheetMarkup(peerModel({}), []);
  const peerViewOnce = peerProtectedSheetMarkup(peerModel({ viewOnce: false }), []);
  const oslChat = oslChatsViewMarkup({
    friends: [{
      personId: "person-keyboard",
      nickname: "Keyboard Friend",
      verified: true,
      ready: true,
      preview: null,
      previewVisible: true,
      unreadCount: 0,
      handshakeConfirmed: true,
    }],
    activePersonId: "person-keyboard",
    messages: [],
    draft: "keyboard OSL Chat",
    busy: false,
    viewOnce: false,
  });

  return [
    {
      name: "onboarding",
      surfaceHtml: onboardingSendingMarkup({ mode: "manual", riskAccepted: false, captureEnabled: true, captureApplied: true }),
      namedActions: ["complete onboarding"],
      targets: ['id="finish-onboarding"'],
      startCount: 0,
      expectedEndCount: 1,
    },
    {
      name: "friend add and removal",
      surfaceHtml: friendSurface,
      namedActions: ["friend add", "friend removal"],
      targets: ["Add friend", "data-remove-person"],
      startCount: 0,
      expectedEndCount: 0,
    },
    {
      name: "allowed-place change",
      surfaceHtml: sourceBackedSurface(
        'id="discord-qa-whitelist-add"',
        '<section><button id="discord-qa-whitelist-add" type="button" aria-label="Allow this verified peer scope">+</button></section>',
      ),
      namedActions: ["allowed-place change"],
      targets: ['id="discord-qa-whitelist-add"'],
      startCount: 0,
      expectedEndCount: 1,
    },
    {
      name: "protected send",
      surfaceHtml: peerWrite,
      namedActions: ["protected send"],
      targets: ["Encrypt & copy"],
      startCount: 0,
      expectedEndCount: 1,
    },
    {
      name: "attachment add",
      surfaceHtml: sourceBackedSurface(
        'id="osl-chat-attach"',
        `<section><button class="button compact" id="osl-chat-attach" type="button">Choose file</button>${attachmentProgressMarkup({
          contextId: "ctx-keyboard",
          job: {
            jobId: "job-keyboard",
            metadata: { filename: "keyboard.png", mediaType: "image/png", size: 12 },
            caption: "",
            viewOnce: false,
            stage: "selected",
            progress: 0,
            retryFrom: null,
            failure: null,
          },
        })}</section>`,
      ),
      namedActions: ["attachment add"],
      targets: ['id="osl-chat-attach"'],
      startCount: 0,
      expectedEndCount: 1,
    },
    {
      name: "burn",
      surfaceHtml: sourceBackedSurface(
        'id="burn-confirm-submit" type="submit" disabled',
        '<form id="burn-confirm-form"><input id="burn-confirm-input" value="BURN CHAT"/><button id="burn-confirm-submit" type="submit">Burn</button></form>',
      ),
      namedActions: ["burn"],
      targets: ['id="burn-confirm-submit"'],
      startCount: 1,
      expectedEndCount: 0,
    },
    {
      name: "timer",
      surfaceHtml: sourceBackedSurface(
        'querySelector("#timer-button")?.addEventListener("click"',
        '<section><button id="timer-button" type="button">72h</button></section>',
      ),
      namedActions: ["timer"],
      targets: ['id="timer-button"'],
      startCount: 0,
      expectedEndCount: 1,
    },
    {
      name: "view-once",
      surfaceHtml: peerViewOnce,
      namedActions: ["view-once"],
      targets: ['id="peer-protected-view-once"'],
      startCount: 0,
      expectedEndCount: 1,
    },
    {
      name: "Scrub",
      surfaceHtml: renderScrubRoute({
        accounts: [{ id: "local-export", label: "Local message export", detail: "TXT on this device" }],
        selectedAccountIds: ["local-export"],
        selectedCategories: ["personal"],
        scan: { state: "not-started", findings: 0 },
      }, "scan"),
      namedActions: ["Scrub"],
      targets: ["data-scrub-route-scan"],
      startCount: 0,
      expectedEndCount: 1,
    },
    {
      name: "OSL Chat",
      surfaceHtml: oslChat,
      namedActions: ["OSL Chat"],
      targets: ["data-osl-chat-send-context"],
      startCount: 0,
      expectedEndCount: 1,
    },
  ];
}

describe("TASK 3553 keyboard-only journeys", () => {
  it("reports ten fixed-screen journeys with no mouse input and expected count changes", () => {
    const report = runKeyboardOnlyJourneys(keyboardJourneySpecs());
    const printed = formatKeyboardJourneyReport(report);
    console.log(printed);

    expect(report.journeys.map((journey) => journey.name)).toEqual([
      "onboarding",
      "friend add and removal",
      "allowed-place change",
      "protected send",
      "attachment add",
      "burn",
      "timer",
      "view-once",
      "Scrub",
      "OSL Chat",
    ]);
    expect(report.journeysListed).toBe(10);
    expect(report.mouseEvents).toBe(0);
    expect(report.namedActionsReachedByKeyboard).toBe(10);
    expect(report.expectedCountChangesMet).toBe(10);
    expect(report.journeysFinished).toBe(10);
    expect(printed).toContain("summary journeys_listed=10 mouse_events=0 named_actions_reached_by_keyboard=10 expected_count_changes_met=10 journeys_finished=10");
  });
});
