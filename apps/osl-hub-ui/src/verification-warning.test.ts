import { describe, expect, it } from "vitest";
import {
  VERIFICATION_WARNING_SETTINGS,
  VerificationWarningMemory,
  verificationWarningDecision,
  type VerificationWarningDecision,
  type VerificationWarningSetting,
} from "./verification-warning";
import { oslChatsViewMarkup, type OslChatsViewModel } from "./osl-chats-view";

const uncheckedConversation = {
  conversationId: "conversation-unchecked-0746",
  checked: false,
};

interface WarningRun {
  setting: VerificationWarningSetting;
  firstOpen: VerificationWarningDecision;
  secondOpen: VerificationWarningDecision;
  prepareSend: VerificationWarningDecision;
}

function runUncheckedConversation(setting: VerificationWarningSetting): WarningRun {
  const memory = new VerificationWarningMemory();
  return {
    setting,
    firstOpen: verificationWarningDecision(setting, uncheckedConversation, "open-conversation", memory),
    secondOpen: verificationWarningDecision(setting, uncheckedConversation, "open-conversation", memory),
    prepareSend: verificationWarningDecision(setting, uncheckedConversation, "prepare-send", memory),
  };
}

function countWarnings(run: WarningRun): number {
  return [run.firstOpen, run.secondOpen, run.prepareSend].filter((decision) => decision.warn).length;
}

function renderedWarningCount(run: WarningRun): number {
  return [run.firstOpen, run.secondOpen, run.prepareSend].filter((decision) => {
    const model: OslChatsViewModel = {
      friends: [{
        personId: uncheckedConversation.conversationId,
        nickname: "Unchecked",
        verified: true,
        ready: true,
        preview: null,
        previewVisible: true,
        unreadCount: 0,
        handshakeConfirmed: uncheckedConversation.checked,
      }],
      activePersonId: uncheckedConversation.conversationId,
      messages: [],
      draft: decision.surface === "before-send" ? "prepared text" : "",
      busy: false,
      verificationWarningSurface: decision.surface,
    };
    const markup = oslChatsViewMarkup(model);
    expect(markup).toContain('aria-label="OSL direct chat with Unchecked"');
    return markup.includes("osl-chat-handshake-warning");
  }).length;
}

function resultLine(run: WarningRun): string {
  const openWarnings = [run.firstOpen, run.secondOpen].filter((decision) => decision.warn).length;
  const prepareWarnings = run.prepareSend.warn ? 1 : 0;
  const openedScreens = [run.firstOpen, run.secondOpen, run.prepareSend].filter((decision) => decision.opensScreen).length;
  const sequence = [
    `open-1:${run.firstOpen.surface}`,
    `open-2:${run.secondOpen.surface}`,
    `prepare:${run.prepareSend.surface}`,
  ].join(",");
  return `TASK-0746 ${run.setting} open-warnings=${openWarnings} prepare-warnings=${prepareWarnings} rendered-warnings=${renderedWarningCount(run)} opened-screens=${openedScreens} total-warnings=${countWarnings(run)} sequence=${sequence}`;
}

describe("verification warning setting", () => {
  it("drives one unchecked conversation through two opens and a send preparation for every setting", () => {
    expect(VERIFICATION_WARNING_SETTINGS).toEqual(["every-time", "once", "before-send", "never"]);

    const runs = VERIFICATION_WARNING_SETTINGS.map(runUncheckedConversation);
    const bySetting = Object.fromEntries(runs.map((run) => [run.setting, run]));

    expect(resultLine(bySetting["every-time"])).toBe("TASK-0746 every-time open-warnings=2 prepare-warnings=0 rendered-warnings=2 opened-screens=0 total-warnings=2 sequence=open-1:conversation-open,open-2:conversation-open,prepare:none");
    expect(resultLine(bySetting.once)).toBe("TASK-0746 once open-warnings=1 prepare-warnings=0 rendered-warnings=1 opened-screens=0 total-warnings=1 sequence=open-1:conversation-open,open-2:none,prepare:none");
    expect(resultLine(bySetting["before-send"])).toBe("TASK-0746 before-send open-warnings=0 prepare-warnings=1 rendered-warnings=1 opened-screens=0 total-warnings=1 sequence=open-1:none,open-2:none,prepare:before-send");
    expect(resultLine(bySetting.never)).toBe("TASK-0746 never open-warnings=0 prepare-warnings=0 rendered-warnings=0 opened-screens=0 total-warnings=0 sequence=open-1:none,open-2:none,prepare:none");

    for (const run of runs) {
      console.log(resultLine(run));
    }
  });

  it("does not warn for a checked conversation in any setting or moment", () => {
    const checkedConversation = { ...uncheckedConversation, checked: true };
    for (const setting of VERIFICATION_WARNING_SETTINGS) {
      const memory = new VerificationWarningMemory();
      expect(verificationWarningDecision(setting, checkedConversation, "open-conversation", memory), setting).toEqual({
        warn: false,
        surface: "none",
        opensScreen: false,
      });
      expect(verificationWarningDecision(setting, checkedConversation, "prepare-send", memory), setting).toEqual({
        warn: false,
        surface: "none",
        opensScreen: false,
      });
    }
  });
});
