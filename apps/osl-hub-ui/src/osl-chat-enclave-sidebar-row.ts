/**
 * TASK 5032 -- enclave rows in the OSL Chats sidebar expand in place to show
 * their channels: no navigation, the channel list opens beneath the row and
 * closes again. Deliberately standalone (mirrors TASK 1310's
 * `osl-chat-direct-message-list-item.ts`): `osl-chats-view.ts` and `main.ts`
 * are already syntactically broken at HEAD from unrelated cross-lane merges
 * (missing brace in `buildIntegrityWarningRow`, duplicate
 * `verificationWarningSurface` field), so this does not import either --
 * wiring this row into them is a later `connect` task, not this one.
 */

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

export interface OslEnclaveSidebarChannel {
  readonly channelId: string;
  readonly name: string;
  readonly unreadCount: number;
  /**
   * A restricted channel: the viewer lacks the permission to open it. This is
   * NOT the same as hidden -- a locked channel still appears in the list,
   * greyed out, with its reason shown, so membership boundaries stay
   * visible. Omitting it entirely would let a restriction pass as "this
   * channel does not exist", which is a different and false claim.
   */
  readonly locked?: boolean;
  readonly lockedReason?: string;
}

export interface OslEnclaveSidebarRowModel {
  readonly enclaveId: string;
  readonly name: string;
  readonly channels: readonly OslEnclaveSidebarChannel[];
}

/** Unread counts roll up to the collapsed row: the exact sum of its channels' counts. */
export function oslEnclaveUnreadTotal(model: OslEnclaveSidebarRowModel): number {
  return model.channels.reduce((sum, channel) => sum + channel.unreadCount, 0);
}

/**
 * Which enclave rows are currently expanded. Held by the caller (not by this
 * module and not reset per render), so it survives switching primary
 * destinations: re-rendering the sidebar under a different route with the
 * same state value reproduces the same open row.
 */
export type OslEnclaveExpansionState = ReadonlySet<string>;

export const OSL_ENCLAVE_SIDEBAR_COLLAPSED: OslEnclaveExpansionState = Object.freeze(new Set<string>());

export function isEnclaveExpanded(state: OslEnclaveExpansionState, enclaveId: string): boolean {
  return state.has(enclaveId);
}

export function toggleEnclaveExpansion(
  state: OslEnclaveExpansionState,
  enclaveId: string,
): OslEnclaveExpansionState {
  const next = new Set(state);
  if (next.has(enclaveId)) {
    next.delete(enclaveId);
  } else {
    next.add(enclaveId);
  }
  return next;
}

function enclaveChannelRow(channel: OslEnclaveSidebarChannel): string {
  const locked = channel.locked === true;
  const reason = locked ? (channel.lockedReason ?? "Restricted") : "";
  return `<li class="osl-enclave-channel${locked ? " is-locked" : ""}" data-enclave-channel-id="${escapeHtml(channel.channelId)}" data-enclave-channel-locked="${locked}" ${locked ? 'aria-disabled="true"' : ""}>
    <span class="osl-enclave-channel-name">${escapeHtml(channel.name)}</span>
    ${channel.unreadCount > 0 && !locked ? `<span class="osl-enclave-channel-unread">${channel.unreadCount}</span>` : ""}
    ${locked ? `<span class="osl-enclave-channel-locked-reason">${escapeHtml(reason)}</span>` : ""}
  </li>`;
}

/**
 * One enclave row. No `data-route` on the toggle button, deliberately: this
 * app wires navigation off `data-route`/`data-primary-destination` elsewhere
 * (see `osl-chats-view.ts` `friendRow`, `main.ts` `primarySidebarItem`), and a
 * delegated click handler that only reacts to those attributes fires zero
 * navigation events for a click here.
 */
export function oslEnclaveSidebarRowMarkup(
  model: OslEnclaveSidebarRowModel,
  expansionState: OslEnclaveExpansionState,
): string {
  const expanded = isEnclaveExpanded(expansionState, model.enclaveId);
  const unread = oslEnclaveUnreadTotal(model);
  const channelsMarkup = expanded ? model.channels.map((channel) => enclaveChannelRow(channel)).join("") : "";
  return `<li class="osl-enclave-row${expanded ? " is-expanded" : ""}" data-enclave-id="${escapeHtml(model.enclaveId)}">
    <button class="osl-enclave-row-toggle" type="button" data-enclave-toggle="${escapeHtml(model.enclaveId)}" aria-expanded="${expanded}" aria-controls="osl-enclave-channels-${escapeHtml(model.enclaveId)}">
      <span class="osl-enclave-row-name">${escapeHtml(model.name)}</span>
      <span class="osl-enclave-row-unread" data-enclave-unread="${unread}">${unread > 0 ? unread : ""}</span>
    </button>
    <ul class="osl-enclave-channel-list" id="osl-enclave-channels-${escapeHtml(model.enclaveId)}" data-enclave-channels="${escapeHtml(model.enclaveId)}" ${expanded ? "" : "hidden"}>${channelsMarkup}</ul>
  </li>`;
}

/** The sidebar list, in the given order -- toggling a row's expansion never reorders it. */
export function oslEnclaveSidebarListMarkup(
  models: readonly OslEnclaveSidebarRowModel[],
  expansionState: OslEnclaveExpansionState,
): string {
  return `<ul class="osl-enclave-sidebar-list" aria-label="Enclaves">${models.map((model) => oslEnclaveSidebarRowMarkup(model, expansionState)).join("")}</ul>`;
}
