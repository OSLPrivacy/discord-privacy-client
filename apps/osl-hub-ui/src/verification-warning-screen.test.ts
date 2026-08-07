import { describe, expect, it } from "vitest";
import {
  DEFAULT_VERIFICATION_WARNING_CHOICE,
  VERIFICATION_WARNING_CHOICES,
  chooseVerificationWarning,
  initialVerificationWarningScreenState,
  isVerificationWarningSaved,
  resetVerificationWarningChoice,
  saveVerificationWarningChoice,
  verificationWarningEffectText,
  verificationWarningScreenMarkup,
  verificationWarningSettingFor,
  type VerificationWarningChoice,
} from "./verification-warning-screen";
import { VerificationWarningMemory, verificationWarningDecision } from "./verification-warning";

const unchecked = { conversationId: "conversation-unchecked-0748", checked: false };

/** What the effect sentence for each choice promises, in machine-checkable form. */
const PROMISED: Record<VerificationWarningChoice, { openWarnings: number; sendWarnings: number }> = {
  "every time": { openWarnings: 2, sendWarnings: 0 },
  once: { openWarnings: 1, sendWarnings: 0 },
  "before sending": { openWarnings: 0, sendWarnings: 1 },
  never: { openWarnings: 0, sendWarnings: 0 },
};

function actualWarnings(choice: VerificationWarningChoice): { openWarnings: number; sendWarnings: number } {
  const setting = verificationWarningSettingFor(choice);
  const memory = new VerificationWarningMemory();
  const opens = [
    verificationWarningDecision(setting, unchecked, "open-conversation", memory),
    verificationWarningDecision(setting, unchecked, "open-conversation", memory),
  ];
  const send = verificationWarningDecision(setting, unchecked, "prepare-send", memory);
  return {
    openWarnings: opens.filter((decision) => decision.warn).length,
    sendWarnings: send.warn ? 1 : 0,
  };
}

function selectedChoices(markup: string): string[] {
  return [...markup.matchAll(/value="([^"]+)"[^>]*\schecked/gu)].map((match) => match[1]);
}

describe("verification warning screen", () => {
  it("offers exactly the four choices the backend stores", () => {
    expect([...VERIFICATION_WARNING_CHOICES]).toEqual(["every time", "once", "before sending", "never"]);
    expect(DEFAULT_VERIFICATION_WARNING_CHOICE).toBe("every time");
  });

  it("draws one selected choice and the plain effect text for it", () => {
    for (const choice of VERIFICATION_WARNING_CHOICES) {
      const markup = verificationWarningScreenMarkup(initialVerificationWarningScreenState(choice));
      expect(selectedChoices(markup), choice).toEqual([choice]);
      expect(markup, choice).toContain(verificationWarningEffectText(choice));
      // Only the selected choice's sentence is on screen.
      const others = VERIFICATION_WARNING_CHOICES.filter((other) => other !== choice);
      for (const other of others) expect(markup, `${choice} vs ${other}`).not.toContain(verificationWarningEffectText(other));
      for (const label of VERIFICATION_WARNING_CHOICES) expect(markup, label).toContain(`aria-label="${label}"`);
    }
  });

  it("carries a title, a save control, and a reset control", () => {
    const markup = verificationWarningScreenMarkup(initialVerificationWarningScreenState());
    expect(markup).toContain(">Verification warning</h1>");
    expect(markup).toContain("data-vw-save");
    expect(markup).toContain(">Save</button>");
    expect(markup).toContain("data-vw-reset");
    expect(markup).toContain(">Reset</button>");
  });

  it("keeps the effect text honest: it matches what the warning engine actually does", () => {
    for (const choice of VERIFICATION_WARNING_CHOICES) {
      expect(actualWarnings(choice), choice).toEqual(PROMISED[choice]);
    }
  });

  it("uses no scary or technical vocabulary in the effect text", () => {
    const banned = /attack|impersonat|man-in-the-middle|fingerprint|key exchange|cryptograph|encrypt|spoof|compromis|MITM/iu;
    for (const choice of VERIFICATION_WARNING_CHOICES) {
      expect(verificationWarningEffectText(choice), choice).not.toMatch(banned);
    }
  });

  it("only saves on the save control, and resets to the default without saving", () => {
    const start = initialVerificationWarningScreenState("every time");
    expect(isVerificationWarningSaved(start)).toBe(true);

    const picked = chooseVerificationWarning(start, "before sending");
    expect(picked).toEqual({ selected: "before sending", saved: "every time" });
    expect(isVerificationWarningSaved(picked)).toBe(false);
    expect(verificationWarningScreenMarkup(picked)).toContain("Not saved yet. Saved choice is still every time.");

    const saved = saveVerificationWarningChoice(picked);
    expect(saved).toEqual({ selected: "before sending", saved: "before sending" });
    expect(verificationWarningScreenMarkup(saved)).toContain("Saved choice: before sending");

    const reset = resetVerificationWarningChoice(saved);
    expect(reset).toEqual({ selected: "every time", saved: "before sending" });
    expect(isVerificationWarningSaved(reset)).toBe(false);
  });

  it("maps every screen label onto a warning engine setting", () => {
    expect(VERIFICATION_WARNING_CHOICES.map(verificationWarningSettingFor)).toEqual([
      "every-time",
      "once",
      "before-send",
      "never",
    ]);
  });
});
