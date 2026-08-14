/**
 * The pending-requests screen: two tabs, Pending and All, and the Accept /
 * Decline actions `task_0227_pending_request_actions.test.ts` drew for one
 * incoming request.
 *
 * Accept and Decline are request commands, not local list edits: `acceptRequest`
 * and `declineRequest` mirror what `cmd_osl_accept_friend_request` and
 * `cmd_osl_decline_or_revoke_friend_request` do in `crates/ipc/src/commands.rs`
 * -- accept moves the requester out of Pending and into All, decline drops the
 * request and adds nobody to All. Both return a new state; nothing here mutates
 * the state a caller passed in, so a screen that redraws from the returned
 * value can never show a request in both tabs at once, or in neither.
 *
 * The module holds no state of its own. `attachPendingRequestsScreen` is the
 * thin controller that keeps one state and one active tab for a mounted
 * screen, runs the command, and redraws.
 */

export interface PendingRequest {
  id: string;
  alias: string;
  discordId: string;
}

export interface AcceptedPerson {
  id: string;
  alias: string;
  discordId: string;
}

export interface PendingRequestsState {
  pending: readonly PendingRequest[];
  all: readonly AcceptedPerson[];
}

export type PendingRequestsTab = "pending" | "all";

export const PENDING_REQUESTS_TABS: readonly PendingRequestsTab[] = ["pending", "all"] as const;

export const PENDING_REQUESTS_TAB_LABELS: Readonly<Record<PendingRequestsTab, string>> = {
  pending: "Pending",
  all: "All",
};

/** Accept: the requester leaves Pending and lands in All. Rust `cmd_osl_accept_friend_request`. */
export function acceptRequest(state: PendingRequestsState, requestId: string): PendingRequestsState {
  const request = state.pending.find((entry) => entry.id === requestId);
  if (!request) return state;
  return {
    pending: state.pending.filter((entry) => entry.id !== requestId),
    all: [...state.all, { id: request.id, alias: request.alias, discordId: request.discordId }],
  };
}

/** Decline: the requester leaves Pending and nobody is added to All. Rust `cmd_osl_decline_or_revoke_friend_request`. */
export function declineRequest(state: PendingRequestsState, requestId: string): PendingRequestsState {
  return {
    pending: state.pending.filter((entry) => entry.id !== requestId),
    all: state.all,
  };
}

/**
 * Add: a new incoming request lands in Pending. This is what redeeming an
 * invite link produces -- `cmd_osl_redeem_friend_invite_link`
 * (crates/ipc/src/commands.rs) returns a `PendingFriendRequestRecord`, and
 * that record becomes exactly one new Pending row here, never touching All.
 */
export function addPendingRequest(state: PendingRequestsState, request: PendingRequest): PendingRequestsState {
  if (state.pending.some((entry) => entry.id === request.id)) return state;
  return {
    pending: [...state.pending, request],
    all: state.all,
  };
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;")
    .replace(/'/gu, "&#39;");
}

function tabsMarkup(activeTab: PendingRequestsTab, state: PendingRequestsState): string {
  return [
    `<nav class="pending-requests-tabs" role="tablist" aria-label="Requests">`,
    PENDING_REQUESTS_TABS.map((tab) => {
      const count = tab === "pending" ? state.pending.length : state.all.length;
      return [
        `<button type="button" role="tab" class="pending-requests-tab${tab === activeTab ? " pending-requests-tab-active" : ""}"`,
        ` aria-selected="${tab === activeTab}" data-request-tab="${tab}">`,
        `${escapeHtml(PENDING_REQUESTS_TAB_LABELS[tab])} (${count})`,
        `</button>`,
      ].join("");
    }).join(""),
    `</nav>`,
  ].join("");
}

function pendingRowMarkup(request: PendingRequest): string {
  return [
    `<li class="pending-request-row" data-request-id="${escapeHtml(request.id)}">`,
    `<span class="pending-request-alias">${escapeHtml(request.alias)}</span>`,
    `<div class="pending-request-actions">`,
    `<button type="button" class="pending-request-action" data-request-action="accept" data-request-id="${escapeHtml(request.id)}">Accept</button>`,
    `<button type="button" class="pending-request-action pending-request-action-quiet" data-request-action="decline" data-request-id="${escapeHtml(request.id)}">Decline</button>`,
    `</div>`,
    `</li>`,
  ].join("");
}

function allRowMarkup(person: AcceptedPerson): string {
  return [
    `<li class="pending-request-row" data-request-id="${escapeHtml(person.id)}">`,
    `<span class="pending-request-alias">${escapeHtml(person.alias)}</span>`,
    `</li>`,
  ].join("");
}

/** The whole screen. Styling lives in `pending-requests-screen.css`. */
export function renderPendingRequestsScreen(
  state: PendingRequestsState,
  activeTab: PendingRequestsTab,
): string {
  const list = activeTab === "pending"
    ? state.pending.map(pendingRowMarkup).join("")
    : state.all.map(allRowMarkup).join("");
  const empty = activeTab === "pending"
    ? `<p class="pending-requests-empty">No pending requests.</p>`
    : `<p class="pending-requests-empty">No one yet.</p>`;
  return [
    `<section class="pending-requests-screen" aria-label="Requests">`,
    tabsMarkup(activeTab, state),
    `<ul class="pending-requests-list" data-request-tab-panel="${activeTab}">`,
    list.length > 0 ? list : empty,
    `</ul>`,
    `</section>`,
  ].join("");
}

/**
 * Mount the screen on an element and keep its state and active tab. Accept
 * and Decline run the command against the state the controller holds, then
 * redraw -- the same state value the screen renders is the one the buttons
 * acted on, never a stale copy.
 */
export function attachPendingRequestsScreen(
  mount: HTMLElement,
  initial: PendingRequestsState,
  onAccept: (personId: string) => void = () => {},
): void {
  let state = initial;
  let activeTab: PendingRequestsTab = "pending";
  const draw = (): void => {
    mount.innerHTML = renderPendingRequestsScreen(state, activeTab);
  };
  mount.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const tabButton = target?.closest?.("[data-request-tab]") as HTMLElement | null;
    if (tabButton) {
      const tab = tabButton.dataset.requestTab as PendingRequestsTab | undefined;
      if (tab) {
        activeTab = tab;
        draw();
      }
      return;
    }
    const actionButton = target?.closest?.("[data-request-action]") as HTMLElement | null;
    if (!actionButton) return;
    const requestId = actionButton.dataset.requestId;
    if (!requestId) return;
    if (actionButton.dataset.requestAction === "accept") {
      state = acceptRequest(state, requestId);
      activeTab = "all";
      onAccept(requestId);
      draw();
      return;
    }
    if (actionButton.dataset.requestAction === "decline") {
      state = declineRequest(state, requestId);
      draw();
    }
  });
  draw();
}
