import "./chat-burn-confirmation.css";

/**
 * TASK 1355 - the chat burn confirmation screen.
 *
 * The last screen before a burn runs. It carries five controls:
 *
 *   1. scope       - how much to burn, drawn from the seven scopes TASK 1351
 *                    defined in `ChatBurnTargetScopeKind` (crates/ipc). Inside a
 *                    server the screen offers this channel AND the whole server,
 *                    so "burn this channel" can never quietly mean the server.
 *   2. side        - Your side / Their side / Both sides, the three variants of
 *                    `OslChatBurnChoice` in apps/osl-hub/src/broker.rs.
 *   3. hide-others - the `hide_others_messages` flag `burn_osl_chat_history`
 *                    takes. It hides, it does not delete: the backend always
 *                    reports `others_rows_destroyed: 0`.
 *   4. Back        - leaves without burning anything.
 *   5. Confirm     - stays unavailable until a scope and a side are both chosen.
 *
 * Like `message-defaults.ts` this module is pure markup + state; the caller owns
 * rendering and the IPC calls. `chatBurnScopeInput` produces exactly the shape
 * `cmd_osl_select_chat_burn_targets` accepts, and `chatBurnRequestDto` exactly
 * the arguments `burn_osl_chat_history` accepts.
 */

/** Mirrors `ChatBurnTargetScopeKind::wire_name` in crates/ipc/src/commands.rs. */
export type BurnScopeKind =
  | "direct_message"
  | "group"
  | "channel"
  | "server"
  | "thread"
  | "service"
  | "account";

/** Mirrors `OslChatBurnChoice` in apps/osl-hub/src/broker.rs (camelCase serde). */
export type BurnSide = "yourSide" | "theirSide" | "bothSides";

export const BURN_SIDES: readonly BurnSide[] = Object.freeze(["yourSide", "theirSide", "bothSides"]);

/** Where the burn screen was opened from. A server channel carries the server. */
export interface BurnPlace {
  kind: "direct_message" | "group" | "channel" | "thread";
  id: string;
  name: string;
  serviceId: string;
  serviceName: string;
  accountId: string;
  accountName: string;
  /** Present only when the place sits inside a server. */
  serverId?: string;
  serverName?: string;
  /** Present only for a thread: the channel the thread hangs off. */
  channelId?: string;
  channelName?: string;
  parentMessageId?: string;
}

export interface BurnScopeChoice {
  scope: BurnScopeKind;
  /** The id the scope burns; also the `scopeId` sent to TASK 1351's command. */
  id: string;
  label: string;
  /** What the scope names on screen, e.g. `#general` or `Study Hall`. */
  target: string;
  description: string;
}

/** True when the place sits inside a server, so a server-wide burn is offered. */
export function insideServer(place: BurnPlace): boolean {
  return typeof place.serverId === "string" && place.serverId.length > 0;
}

const SERVICE_SCOPE = (place: BurnPlace): BurnScopeChoice => ({
  scope: "service",
  id: place.serviceId,
  label: "Whole service",
  target: place.serviceName,
  description: `Everything OSL holds for ${place.serviceName} on this machine.`,
});

const ACCOUNT_SCOPE = (place: BurnPlace): BurnScopeChoice => ({
  scope: "account",
  id: place.accountId,
  label: "Whole account",
  target: place.accountName,
  description: `Every service and every chat under ${place.accountName}.`,
});

/**
 * The channel a burn from this place would clear, if there is one: the channel
 * itself, or the channel a thread hangs off.
 */
export function burnChannelIdentity(place: BurnPlace): { id: string; name: string } | null {
  if (place.kind === "channel") return { id: place.id, name: place.name };
  if (place.kind === "thread" && place.channelId && place.channelName) {
    return { id: place.channelId, name: place.channelName };
  }
  return null;
}

/**
 * The scopes offered from a place, widest last. Inside a server the list always
 * holds both `channel` and `server`, which is the choice TASK 1355 asks for.
 */
export function burnScopeChoices(place: BurnPlace): BurnScopeChoice[] {
  const choices: BurnScopeChoice[] = [];

  if (place.kind === "direct_message") {
    choices.push({
      scope: "direct_message",
      id: place.id,
      label: "This conversation",
      target: place.name,
      description: `Only the direct messages between you and ${place.name}.`,
    });
  } else if (place.kind === "group") {
    choices.push({
      scope: "group",
      id: place.id,
      label: "This group",
      target: place.name,
      description: `Only the messages in the ${place.name} group.`,
    });
  } else if (place.kind === "thread") {
    choices.push({
      scope: "thread",
      id: place.id,
      label: "This thread",
      target: place.name,
      description: `Only the replies inside ${place.name}. The rest of the channel stays.`,
    });
  }

  const channel = burnChannelIdentity(place);
  if (channel) {
    choices.push({
      scope: "channel",
      id: channel.id,
      label: "This channel",
      target: channel.name,
      description: `Only the messages in ${channel.name}. Every other channel stays.`,
    });
  }

  if (insideServer(place)) {
    choices.push({
      scope: "server",
      id: place.serverId as string,
      label: "Whole server",
      target: place.serverName as string,
      description: `Every channel in ${place.serverName}, not just this one.`,
    });
  }

  choices.push(SERVICE_SCOPE(place), ACCOUNT_SCOPE(place));
  return choices;
}

export interface ChatBurnConfirmationState {
  place: BurnPlace;
  /** Nothing is pre-selected: a burn scope is never chosen by default. */
  scope: BurnScopeKind | null;
  side: BurnSide | null;
  hideOthers: boolean;
  outcome: "choosing" | "went-back" | "confirmed";
}

export function initialChatBurnConfirmation(place: BurnPlace): ChatBurnConfirmationState {
  return { place, scope: null, side: null, hideOthers: false, outcome: "choosing" };
}

export type ChatBurnConfirmationEvent =
  | { kind: "choose-scope"; scope: BurnScopeKind }
  | { kind: "choose-side"; side: BurnSide }
  | { kind: "set-hide-others"; hide: boolean }
  | { kind: "back" }
  | { kind: "confirm" };

/** Confirm only becomes available once a scope and a side are both chosen. */
export function canConfirmBurn(state: ChatBurnConfirmationState): boolean {
  return state.scope !== null && state.side !== null && state.outcome === "choosing";
}

/** Pure. Unknown scopes, unknown sides and premature Confirm leave state alone. */
export function applyChatBurnConfirmationEvent(
  state: ChatBurnConfirmationState,
  event: ChatBurnConfirmationEvent,
): ChatBurnConfirmationState {
  if (state.outcome !== "choosing") return state;
  if (event.kind === "choose-scope") {
    if (!burnScopeChoices(state.place).some((choice) => choice.scope === event.scope)) return state;
    return { ...state, scope: event.scope };
  }
  if (event.kind === "choose-side") {
    if (!BURN_SIDES.includes(event.side)) return state;
    return { ...state, side: event.side };
  }
  if (event.kind === "set-hide-others") {
    if (typeof event.hide !== "boolean") return state;
    return { ...state, hideOthers: event.hide };
  }
  if (event.kind === "back") return { ...state, outcome: "went-back" };
  if (!canConfirmBurn(state)) return state;
  return { ...state, outcome: "confirmed" };
}

/** Maps a clicked control (`data-burn-action` / `data-burn-value`) to an event. */
export function chatBurnEventForAction(
  action: string | undefined | null,
  value: string | undefined | null,
): ChatBurnConfirmationEvent | null {
  if (action === "back") return { kind: "back" };
  if (action === "confirm") return { kind: "confirm" };
  if (action === "hide-others") return { kind: "set-hide-others", hide: value === "on" };
  if (action === "side" && (BURN_SIDES as readonly string[]).includes(value ?? "")) {
    return { kind: "choose-side", side: value as BurnSide };
  }
  if (action === "scope" && value) {
    return { kind: "choose-scope", scope: value as BurnScopeKind };
  }
  return null;
}

/** The exact `ChatBurnTargetScopeInput` shape `cmd_osl_select_chat_burn_targets` takes. */
export function chatBurnScopeInput(
  place: BurnPlace,
  scope: BurnScopeKind,
): Record<string, string> | null {
  const choice = burnScopeChoices(place).find((entry) => entry.scope === scope);
  if (!choice) return null;
  const input: Record<string, string> = { scopeKind: scope, scopeId: choice.id };
  input.serviceId = place.serviceId;
  input.accountId = place.accountId;
  if (place.serverId) input.serverId = place.serverId;
  const channel = burnChannelIdentity(place);
  if (channel && (scope === "channel" || scope === "thread")) input.channelId = channel.id;
  if (scope === "thread") {
    input.threadId = place.id;
    if (place.parentMessageId) input.parentMessageId = place.parentMessageId;
  }
  return input;
}

/**
 * Everything the caller needs after Confirm: the `burn_osl_chat_history`
 * arguments and the TASK 1351 scope input that names what is about to go.
 */
export function chatBurnRequestDto(state: ChatBurnConfirmationState): {
  choice: BurnSide;
  hideOthersMessages: boolean;
  scope: Record<string, string>;
} | null {
  if (state.scope === null || state.side === null) return null;
  const scope = chatBurnScopeInput(state.place, state.scope);
  if (!scope) return null;
  return { choice: state.side, hideOthersMessages: state.hideOthers, scope };
}

const SIDE_LABELS: Record<BurnSide, string> = {
  yourSide: "Your side",
  theirSide: "Their side",
  bothSides: "Both sides",
};

const SIDE_DESCRIPTIONS: Record<BurnSide, string> = {
  yourSide: "Deletes your own copy on this machine. Their copy is left alone.",
  theirSide: "Asks the other OSL app to delete its copy. Your copy is left alone.",
  bothSides: "Deletes your copy and asks the other OSL app to delete its copy too.",
};

export function burnSideLabel(side: BurnSide): string {
  return SIDE_LABELS[side];
}

/** One plain sentence naming exactly what Confirm will do. */
export function burnSummarySentence(state: ChatBurnConfirmationState): string {
  if (state.scope === null || state.side === null) {
    return "Choose how much to burn and whose copies it reaches.";
  }
  const choice = burnScopeChoices(state.place).find((entry) => entry.scope === state.scope);
  const target = choice ? `${choice.label.toLowerCase()} (${choice.target})` : String(state.scope);
  const side = SIDE_LABELS[state.side].toLowerCase();
  const hide = state.hideOthers
    ? " Other people's messages are also hidden from your view."
    : "";
  return `Confirm burns ${target}, ${side}.${hide}`;
}

function escapeText(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

function scopeButton(choice: BurnScopeChoice, pressed: boolean): string {
  return `<button class="burnconf-choice" type="button" data-burn-action="scope" data-burn-value="${choice.scope}" aria-pressed="${pressed}">
    <span class="burnconf-choice-label">${escapeText(choice.label)}</span>
    <span class="burnconf-choice-target">${escapeText(choice.target)}</span>
    <span class="burnconf-choice-note">${escapeText(choice.description)}</span>
  </button>`;
}

function sideButton(side: BurnSide, pressed: boolean): string {
  return `<button class="burnconf-choice" type="button" data-burn-action="side" data-burn-value="${side}" aria-pressed="${pressed}">
    <span class="burnconf-choice-label">${SIDE_LABELS[side]}</span>
    <span class="burnconf-choice-note">${escapeText(SIDE_DESCRIPTIONS[side])}</span>
  </button>`;
}

export function chatBurnConfirmationMarkup(state: ChatBurnConfirmationState): string {
  const { place } = state;
  const scopes = burnScopeChoices(place);
  const where = insideServer(place)
    ? `${escapeText(place.name)} in ${escapeText(place.serverName as string)}`
    : escapeText(place.name);

  const scopeButtons = scopes
    .map((choice) => scopeButton(choice, choice.scope === state.scope))
    .join("");
  const sideButtons = BURN_SIDES.map((side) => sideButton(side, side === state.side)).join("");

  return `<section class="burnconf-screen" aria-labelledby="chat-burn-heading">
    <p class="burnconf-where">Burning from ${where}</p>
    <h1 id="chat-burn-heading" tabindex="-1" class="burnconf-title">Confirm this burn</h1>
    <p class="burnconf-intro">Burn cleans up. It does not un-send. Nothing is chosen for you: pick how much to burn and whose copies it reaches, then confirm.</p>
    <div class="burnconf-groups">
      <fieldset class="burnconf-group">
        <legend>How much to burn</legend>
        <div class="burnconf-choices" role="group" aria-label="How much to burn">${scopeButtons}</div>
      </fieldset>
      <fieldset class="burnconf-group">
        <legend>Whose copies</legend>
        <div class="burnconf-choices" role="group" aria-label="Whose copies">${sideButtons}</div>
      </fieldset>
    </div>
    <div class="burnconf-hide">
      <label class="burnconf-hide-label" for="chat-burn-hide-others">
        <input type="checkbox" id="chat-burn-hide-others" data-burn-action="hide-others" data-burn-value="${state.hideOthers ? "off" : "on"}"${state.hideOthers ? " checked" : ""}>
        <span>Also hide other people's messages</span>
      </label>
      <small class="burnconf-hide-note">Hiding only changes what this machine shows you. It does not delete anyone else's messages.</small>
    </div>
    <p class="burnconf-summary" aria-live="polite">${escapeText(burnSummarySentence(state))}</p>
    <div class="burnconf-actions">
      <button class="button ghost" type="button" id="chat-burn-back" data-burn-action="back">Back</button>
      <button class="button danger" type="button" id="chat-burn-confirm" data-burn-action="confirm"${canConfirmBurn(state) ? "" : " disabled"}>Confirm burn</button>
    </div>
  </section>`;
}
