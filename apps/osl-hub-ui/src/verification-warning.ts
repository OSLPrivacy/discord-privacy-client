export const VERIFICATION_WARNING_SETTINGS = ["every-time", "once", "before-send", "never"] as const;

export type VerificationWarningSetting = typeof VERIFICATION_WARNING_SETTINGS[number];
export type VerificationWarningMoment = "open-conversation" | "prepare-send";
export type VerificationWarningSurface = "conversation-open" | "before-send" | "none";

export interface VerificationWarningConversation {
  conversationId: string;
  checked: boolean;
}

export interface VerificationWarningDecision {
  warn: boolean;
  surface: VerificationWarningSurface;
  opensScreen: false;
}

export class VerificationWarningMemory {
  private readonly warnedConversationIds = new Set<string>();

  hasWarned(conversationId: string): boolean {
    return this.warnedConversationIds.has(conversationId);
  }

  remember(conversationId: string): void {
    this.warnedConversationIds.add(conversationId);
  }
}

const noWarning = (): VerificationWarningDecision => ({
  warn: false,
  surface: "none",
  opensScreen: false,
});

const warning = (surface: Exclude<VerificationWarningSurface, "none">): VerificationWarningDecision => ({
  warn: true,
  surface,
  opensScreen: false,
});

export function verificationWarningDecision(
  setting: VerificationWarningSetting,
  conversation: VerificationWarningConversation,
  moment: VerificationWarningMoment,
  memory = new VerificationWarningMemory(),
): VerificationWarningDecision {
  if (conversation.checked || conversation.conversationId.length === 0) return noWarning();

  switch (setting) {
    case "every-time":
      return moment === "open-conversation" ? warning("conversation-open") : noWarning();
    case "once":
      if (moment !== "open-conversation" || memory.hasWarned(conversation.conversationId)) return noWarning();
      memory.remember(conversation.conversationId);
      return warning("conversation-open");
    case "before-send":
      return moment === "prepare-send" ? warning("before-send") : noWarning();
    case "never":
      return noWarning();
  }
}
