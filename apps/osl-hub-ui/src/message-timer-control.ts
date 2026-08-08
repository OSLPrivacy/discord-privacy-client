/**
 * Admission and presentation for timers on ordinary messages in connected apps.
 *
 * The writer is injected so this boundary can refuse an unsupported surface
 * before any durable waiting record is created.  The backend timed-delete
 * ledger deliberately accepts opaque app ids, so callers must not rely on it
 * to decide whether another person's provider copy can actually be removed.
 */

export const UNREMOVABLE_OTHER_COPY_REASON = "the other person's copy cannot be removed";

export type MessageTimerContext =
  | {
    appId: "discord";
    conversationKind: "ordinary_message";
    conversationId: string;
    messageLocator: string;
  }
  | {
    appId: "x";
    conversationKind: "direct_message";
    conversationId: string;
    messageLocator: string;
  }
  | {
    appId: "email";
    conversationKind: "ordinary_email";
    conversationId: string;
    messageLocator: string;
  };

export interface MessageTimerAvailability {
  state: "on" | "off";
  enabled: boolean;
  reason: string | null;
}

export interface MessageTimerWaitingRecord {
  appId: MessageTimerContext["appId"];
  conversationKind: MessageTimerContext["conversationKind"];
  conversationId: string;
  messageLocator: string;
  durationSeconds: number;
}

export type MessageTimerWaitingRecordWriter = (
  record: MessageTimerWaitingRecord,
) => void | Promise<void>;

export type SetMessageTimerResult =
  | { accepted: true; record: MessageTimerWaitingRecord }
  | { accepted: false; reason: string };

export function messageTimerAvailability(
  context: MessageTimerContext,
): MessageTimerAvailability {
  if (context.appId === "discord" && context.conversationKind === "ordinary_message") {
    return { state: "on", enabled: true, reason: null };
  }
  return {
    state: "off",
    enabled: false,
    reason: UNREMOVABLE_OTHER_COPY_REASON,
  };
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"]/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
  })[character] ?? character);
}

/**
 * Render the timer button and its explanation as one accessible control group.
 * An off button always points at a visible plain-language reason; it is never a
 * bare disabled control whose meaning depends on colour or a tooltip.
 */
export function messageTimerControlMarkup(
  context: MessageTimerContext,
  descriptionId = "message-timer-off-reason",
): string {
  const availability = messageTimerAvailability(context);
  if (availability.enabled) {
    return '<div class="message-timer-control" data-message-timer-state="on">'
      + '<button type="button" data-message-timer-button aria-pressed="false">Set timer</button>'
      + "</div>";
  }

  const safeDescriptionId = escapeHtml(descriptionId);
  return '<div class="message-timer-control" data-message-timer-state="off">'
    + `<button type="button" data-message-timer-button data-timer-state="off" aria-pressed="false" aria-describedby="${safeDescriptionId}" disabled>Timer off</button>`
    + `<p id="${safeDescriptionId}" data-message-timer-reason>${escapeHtml(availability.reason ?? "")}</p>`
    + "</div>";
}

/** Refuse unsupported surfaces before invoking the durable-record writer. */
export async function setMessageTimer(
  context: MessageTimerContext,
  durationSeconds: number,
  writeWaitingRecord: MessageTimerWaitingRecordWriter,
): Promise<SetMessageTimerResult> {
  const availability = messageTimerAvailability(context);
  if (!availability.enabled) {
    return {
      accepted: false,
      reason: availability.reason ?? UNREMOVABLE_OTHER_COPY_REASON,
    };
  }
  if (!Number.isSafeInteger(durationSeconds) || durationSeconds <= 0) {
    return { accepted: false, reason: "the timer duration must be a positive number of seconds" };
  }

  const record: MessageTimerWaitingRecord = {
    appId: context.appId,
    conversationKind: context.conversationKind,
    conversationId: context.conversationId,
    messageLocator: context.messageLocator,
    durationSeconds,
  };
  await writeWaitingRecord(record);
  return { accepted: true, record };
}
