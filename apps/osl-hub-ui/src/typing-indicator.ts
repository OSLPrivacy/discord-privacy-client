// TASK 5012 - OSL Chat typing-indicator state and markup.

export const OSL_CHAT_TYPING_STOP_GRACE_MS = 750;
export const OSL_CHAT_TYPING_OUTGOING_EVENT = "osl:typing-signal-send";
export const OSL_CHAT_TYPING_INCOMING_EVENT = "osl:typing-signal-received";

export interface OslChatTypingSignal {
  personId: string;
  typing: boolean;
}

export interface OslChatTypingPreferences {
  /** Checked means this device emits no typing-presence signals. */
  hideOwnTyping: boolean;
  /** Unchecked means incoming typing-presence signals never become visible. */
  showIncomingTyping: boolean;
}

export interface OslChatTypingControllerOptions {
  preferences: OslChatTypingPreferences;
  send: (signal: OslChatTypingSignal) => void;
  visibilityChanged?: (personId: string | null) => void;
  setTimer?: (callback: () => void, delayMs: number) => ReturnType<typeof setTimeout>;
  clearTimer?: (timer: ReturnType<typeof setTimeout>) => void;
}

export interface OslChatTypingController {
  incomingPersonId(): string | null;
  localDraftChanged(personId: string | null, hasDraft: boolean): void;
  receive(signal: OslChatTypingSignal): void;
  setPreferences(preferences: OslChatTypingPreferences): void;
  dispose(): void;
}

function validPersonId(personId: string): boolean {
  return personId.length > 0 && personId.length <= 180;
}

/**
 * Owns the two privacy switches and the short-lived incoming display state.
 * Sending is edge-triggered so repeated input events do not turn every
 * keystroke into presence traffic.
 */
export function createOslChatTypingController(
  options: OslChatTypingControllerOptions,
): OslChatTypingController {
  let preferences = { ...options.preferences };
  let visiblePersonId: string | null = null;
  let sentPersonId: string | null = null;
  let stopTimer: ReturnType<typeof setTimeout> | null = null;
  const setTimer = options.setTimer ?? ((callback, delayMs) => setTimeout(callback, delayMs));
  const clearTimer = options.clearTimer ?? ((timer) => clearTimeout(timer));

  const cancelStop = (): void => {
    if (stopTimer === null) return;
    clearTimer(stopTimer);
    stopTimer = null;
  };

  const show = (personId: string | null): void => {
    if (visiblePersonId === personId) return;
    visiblePersonId = personId;
    options.visibilityChanged?.(visiblePersonId);
  };

  const stopOutgoing = (): void => {
    if (sentPersonId === null) return;
    options.send({ personId: sentPersonId, typing: false });
    sentPersonId = null;
  };

  return {
    incomingPersonId: () => visiblePersonId,

    localDraftChanged(personId, hasDraft): void {
      if (preferences.hideOwnTyping) return;
      const nextPersonId = hasDraft && personId && validPersonId(personId) ? personId : null;
      if (nextPersonId === sentPersonId) return;
      stopOutgoing();
      if (nextPersonId !== null) {
        options.send({ personId: nextPersonId, typing: true });
        sentPersonId = nextPersonId;
      }
    },

    receive(signal): void {
      if (!validPersonId(signal.personId) || !preferences.showIncomingTyping) return;
      if (signal.typing) {
        cancelStop();
        show(signal.personId);
        return;
      }
      if (visiblePersonId !== signal.personId) return;
      cancelStop();
      stopTimer = setTimer(() => {
        stopTimer = null;
        show(null);
      }, OSL_CHAT_TYPING_STOP_GRACE_MS);
    },

    setPreferences(nextPreferences): void {
      const wasSending = !preferences.hideOwnTyping;
      preferences = { ...nextPreferences };
      if (!preferences.showIncomingTyping) {
        cancelStop();
        show(null);
      }
      // If the owner opts out while an active edge is outstanding, send the
      // single stop edge needed to retire it. Once opted out, draft changes
      // produce zero new signals.
      if (wasSending && preferences.hideOwnTyping) stopOutgoing();
    },

    dispose(): void {
      cancelStop();
      stopOutgoing();
      show(null);
    },
  };
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function initials(value: string): string {
  return value.split(/\s+/u).filter(Boolean).map((part) => part[0]).join("").slice(0, 2).toUpperCase() || "?";
}

/** Exactly one faded avatar and exactly three decorative dots. */
export function oslChatTypingIndicatorMarkup(personName: string): string {
  return `<div class="osl-chat-typing-indicator" role="status" aria-label="${escapeHtml(personName)} is typing" data-osl-chat-typing-indicator><span class="osl-chat-avatar is-typing" data-faded-avatar="true" aria-hidden="true">${escapeHtml(initials(personName))}</span><span class="osl-chat-typing-dots" aria-hidden="true"><span data-typing-dot></span><span data-typing-dot></span><span data-typing-dot></span></span></div>`;
}

export function oslChatTypingSettingsMarkup(preferences: OslChatTypingPreferences): string {
  return `<label class="setting-line interactive"><span><strong>Don't show when I'm typing</strong><small>OSL sends no typing signal while this is on.</small></span><input id="osl-chat-hide-own-typing" type="checkbox" ${preferences.hideOwnTyping ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Show when others are typing</strong><small>Show the faded avatar and dots in this thread.</small></span><input id="osl-chat-show-incoming-typing" type="checkbox" ${preferences.showIncomingTyping ? "checked" : ""}/></label>`;
}
