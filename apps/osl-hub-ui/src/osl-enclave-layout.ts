import { escapeHtml } from "./services";

/**
 * The Enclave sidebar, permission grid and deletion dialog for customisable
 * categories and channels.
 *
 * Everything here renders facts the backend resolved. In particular:
 *
 * - the rendered order is read from the authoritative model, never from a
 *   drag gesture, so a reorder that never became a signed operation cannot
 *   show as a reordered list;
 * - each access cell carries whether it was inherited from the channel's mode
 *   or overridden by a named role, because "you cannot post here" and "this
 *   role was denied posting here" are different facts;
 * - collapse state is read and written per member on this device only, and is
 *   deliberately not part of any model that leaves it.
 */

/** The modes this release ships, in the order the product lists them. */
export const SHIPPED_CHANNEL_MODES = ["open", "read-only", "stewards"] as const;

export type ChannelMode = (typeof SHIPPED_CHANNEL_MODES)[number];

/** The label shown on a channel and in the mode picker. */
export const CHANNEL_MODE_LABELS: Record<ChannelMode, string> = {
  open: "OPEN",
  "read-only": "READ ONLY",
  stewards: "STEWARDS",
};

/** What each mode means, in the product's own words. */
export const CHANNEL_MODE_DESCRIPTIONS: Record<ChannelMode, string> = {
  open: "Every member can read and post.",
  "read-only": "Every member can read. Only members with authority can post.",
  stewards: "Only members with authority can read or post.",
};

export const CHANNEL_ACTIONS = ["read", "post", "manage"] as const;

export type ChannelAction = (typeof CHANNEL_ACTIONS)[number];

export const CHANNEL_ACTION_LABELS: Record<ChannelAction, string> = {
  read: "Read",
  post: "Post",
  manage: "Manage",
};

/** Where a resolved answer came from. The backend decides this, not the view. */
export type AccessSourceKind = "inherited" | "overridden" | "not-a-member";

export const ACCESS_SOURCE_LABELS: Record<AccessSourceKind, string> = {
  inherited: "Inherited",
  overridden: "Overridden",
  "not-a-member": "Not a member",
};

export interface AccessCell {
  readonly action: ChannelAction;
  readonly allowed: boolean;
  readonly source: AccessSourceKind;
  /** What the backend named as the reason: a mode, or the role that overrode it. */
  readonly because: string;
}

export interface ChannelRow {
  readonly channelId: string;
  readonly name: string;
  readonly mode: ChannelMode;
  readonly messageCount: number;
  readonly access: readonly AccessCell[];
}

export interface CategoryRow {
  readonly categoryId: string;
  readonly name: string;
  readonly channels: readonly ChannelRow[];
}

export interface EnclaveLayoutModel {
  readonly categories: readonly CategoryRow[];
}

export interface PermissionGridRow {
  readonly roleId: string;
  readonly roleName: string;
  readonly authority: boolean;
  readonly cells: readonly AccessCell[];
}

// ------------------------------------------------------------- collapse ----

/** The narrow slice of `Storage` this module needs, so tests supply their own. */
export interface LocalKeyValueStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

/**
 * Collapse state is filed under the member, so two profiles on one device
 * never share a folded sidebar.
 */
export function collapseStorageKey(memberId: string): string {
  if (!memberId.trim()) throw new Error("A collapse profile needs a member");
  return `osl.enclave.collapsed.${memberId}`;
}

export function readCollapsedCategories(
  store: LocalKeyValueStore,
  memberId: string,
): Set<string> {
  const raw = store.getItem(collapseStorageKey(memberId));
  if (!raw) return new Set();
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((id): id is string => typeof id === "string" && id.length > 0));
  } catch {
    return new Set();
  }
}

export function writeCollapsedCategories(
  store: LocalKeyValueStore,
  memberId: string,
  collapsed: ReadonlySet<string>,
): void {
  store.setItem(collapseStorageKey(memberId), JSON.stringify([...collapsed].sort()));
}

/** Folds one category shut or open for this member. Returns the new state. */
export function toggleCollapsedCategory(
  store: LocalKeyValueStore,
  memberId: string,
  categoryId: string,
): boolean {
  const collapsed = readCollapsedCategories(store, memberId);
  const nowCollapsed = !collapsed.has(categoryId);
  if (nowCollapsed) collapsed.add(categoryId); else collapsed.delete(categoryId);
  writeCollapsedCategories(store, memberId, collapsed);
  return nowCollapsed;
}

// -------------------------------------------------------------- sidebar ----

/**
 * The channel ids in the order the sidebar will render them.
 *
 * This reads the model and nothing else. A caller that wants a different order
 * has to change the model, which means emitting a signed reorder.
 */
export function renderedChannelOrder(model: EnclaveLayoutModel): string[] {
  return model.categories.flatMap((category) =>
    category.channels.map((channel) => channel.channelId));
}

/**
 * True when a rendered list claims an order the authoritative model does not
 * have — a reorder that moved pixels and nothing else.
 */
export function isVisualOnlyReorder(
  model: EnclaveLayoutModel,
  renderedOrder: readonly string[],
): boolean {
  const authoritative = renderedChannelOrder(model);
  if (authoritative.length !== renderedOrder.length) return true;
  return authoritative.some((channelId, index) => channelId !== renderedOrder[index]);
}

/** The port the sidebar reorders through. It returns the authoritative model. */
export interface EnclaveLayoutPort {
  reorderChannel(channelId: string, position: number): EnclaveLayoutModel;
}

/**
 * Reorders a channel.
 *
 * The returned model is whatever the backend produced. This function
 * deliberately does not permute the incoming model, so if the operation was
 * refused the list does not move.
 */
export function reorderChannel(
  port: EnclaveLayoutPort,
  channelId: string,
  position: number,
): EnclaveLayoutModel {
  return port.reorderChannel(channelId, position);
}

function channelMarkup(channel: ChannelRow): string {
  const readable = channel.access.find((cell) => cell.action === "read");
  const denied = readable && !readable.allowed ? " enclave-channel--denied" : "";
  return `<li class="enclave-channel${denied}" data-channel-id="${escapeHtml(channel.channelId)}"><span class="enclave-channel-name">#${escapeHtml(channel.name)}</span><span class="enclave-channel-mode" data-mode="${channel.mode}">${CHANNEL_MODE_LABELS[channel.mode]}</span></li>`;
}

/** Renders the sidebar. `collapsed` is this member's own state. */
export function renderEnclaveLayout(
  model: EnclaveLayoutModel,
  collapsed: ReadonlySet<string>,
): string {
  const sections = model.categories.map((category) => {
    const isCollapsed = collapsed.has(category.categoryId);
    const channels = isCollapsed
      ? ""
      : `<ul class="enclave-channels">${category.channels.map(channelMarkup).join("")}</ul>`;
    return `<section class="enclave-category" data-category-id="${escapeHtml(category.categoryId)}"><h2><button class="enclave-category-toggle" type="button" data-collapse-category="${escapeHtml(category.categoryId)}" aria-expanded="${isCollapsed ? "false" : "true"}">${escapeHtml(category.name)}</button></h2>${channels}</section>`;
  });
  return `<nav class="enclave-layout" aria-label="Enclave channels">${sections.join("")}</nav>`;
}

// ------------------------------------------------------- permission grid ----

function cellMarkup(cell: AccessCell): string {
  return `<td class="enclave-permission-cell" data-action="${cell.action}" data-allowed="${cell.allowed}" data-source="${cell.source}"><span class="enclave-permission-verdict">${cell.allowed ? "Yes" : "No"}</span><small class="enclave-permission-source">${ACCESS_SOURCE_LABELS[cell.source]}</small><small class="enclave-permission-because">${escapeHtml(cell.because)}</small></td>`;
}

/**
 * Renders the grid of roles against read, post and manage. Every cell says
 * whether the answer was inherited or overridden, because a member cannot fix
 * an override they cannot see.
 */
export function renderPermissionGrid(rows: readonly PermissionGridRow[]): string {
  const headings = CHANNEL_ACTIONS
    .map((action) => `<th scope="col">${CHANNEL_ACTION_LABELS[action]}</th>`)
    .join("");
  const body = rows.map((row) => {
    const ordered = CHANNEL_ACTIONS.map((action) => {
      const cell = row.cells.find((candidate) => candidate.action === action);
      if (!cell) throw new Error(`Permission grid row is missing ${action}`);
      return cellMarkup(cell);
    }).join("");
    const authority = row.authority
      ? '<small class="enclave-role-authority">Carries authority</small>'
      : "";
    return `<tr data-role-id="${escapeHtml(row.roleId)}"><th scope="row">${escapeHtml(row.roleName)}${authority}</th>${ordered}</tr>`;
  }).join("");
  return `<table class="enclave-permission-grid"><thead><tr><th scope="col">Role</th>${headings}</tr></thead><tbody>${body}</tbody></table>`;
}

// ------------------------------------------------------------- deletion ----

export type ChannelDisposition =
  | { readonly kind: "none" }
  | { readonly kind: "move"; readonly channelId: string }
  | { readonly kind: "burn" };

export type DeletionPlan =
  | { readonly ok: true; readonly disposition: ChannelDisposition }
  | {
    readonly ok: false;
    readonly reason: "destination-required";
    readonly channelName: string;
    readonly messageCount: number;
  }
  | { readonly ok: false; readonly reason: "destination-missing" }
  | { readonly ok: false; readonly reason: "destination-is-the-channel" }
  | { readonly ok: false; readonly reason: "burn-not-confirmed" };

export interface DeletionRequest {
  readonly channel: ChannelRow;
  /** Channels that still exist and could receive the messages. */
  readonly destinations: readonly ChannelRow[];
  readonly chosenDestinationId?: string;
  readonly burnRequested?: boolean;
  readonly burnConfirmed?: boolean;
}

/**
 * Decides whether a deletion may proceed.
 *
 * A nonempty channel is never deleted by default: the member has to name a
 * surviving channel for the messages, or ask for a burn and confirm it.
 */
export function planChannelDeletion(request: DeletionRequest): DeletionPlan {
  const { channel, destinations, chosenDestinationId, burnRequested, burnConfirmed } = request;
  if (channel.messageCount === 0 && !chosenDestinationId && !burnRequested) {
    return { ok: true, disposition: { kind: "none" } };
  }
  if (burnRequested) {
    if (!burnConfirmed) return { ok: false, reason: "burn-not-confirmed" };
    return { ok: true, disposition: { kind: "burn" } };
  }
  if (!chosenDestinationId) {
    return {
      ok: false,
      reason: "destination-required",
      channelName: channel.name,
      messageCount: channel.messageCount,
    };
  }
  if (chosenDestinationId === channel.channelId) {
    return { ok: false, reason: "destination-is-the-channel" };
  }
  if (!destinations.some((candidate) => candidate.channelId === chosenDestinationId)) {
    return { ok: false, reason: "destination-missing" };
  }
  return { ok: true, disposition: { kind: "move", channelId: chosenDestinationId } };
}

/** The words the deletion dialog shows for a plan that cannot proceed yet. */
export function deletionRefusalWords(plan: DeletionPlan): string {
  if (plan.ok) return "";
  switch (plan.reason) {
    case "destination-required":
      return `"${plan.channelName}" still holds ${plan.messageCount} ${plan.messageCount === 1 ? "message" : "messages"}. Choose a channel to move them to, or burn them on purpose.`;
    case "destination-is-the-channel":
      return "Messages cannot move into the channel being deleted.";
    case "destination-missing":
      return "That channel is not available to receive the messages.";
    case "burn-not-confirmed":
      return "Burning destroys these messages on this device and asks other members' devices to do the same. Confirm to continue.";
    default:
      return "";
  }
}

export function renderDeleteChannelDialog(request: DeletionRequest): string {
  const plan = planChannelDeletion(request);
  const options = request.destinations
    .filter((candidate) => candidate.channelId !== request.channel.channelId)
    .map((candidate) => `<option value="${escapeHtml(candidate.channelId)}"${candidate.channelId === request.chosenDestinationId ? " selected" : ""}>#${escapeHtml(candidate.name)}</option>`)
    .join("");
  const refusal = plan.ok
    ? ""
    : `<p class="enclave-delete-refusal" role="alert">${escapeHtml(deletionRefusalWords(plan))}</p>`;
  return `<section class="enclave-delete-channel" role="alertdialog" aria-label="Delete channel"><h2>Delete #${escapeHtml(request.channel.name)}</h2><p>${request.channel.messageCount} ${request.channel.messageCount === 1 ? "message is" : "messages are"} in this channel.</p>${refusal}<label class="enclave-delete-destination"><span>Move messages to</span><select data-delete-destination><option value="">Choose a channel</option>${options}</select></label><label class="enclave-delete-burn"><input type="checkbox" data-delete-burn${request.burnRequested ? " checked" : ""}/><span>Burn the messages instead</span></label><button class="button" type="button" data-delete-confirm${plan.ok ? "" : " disabled"}>Delete channel</button></section>`;
}

// ------------------------------------------------------------- capacity ----

export interface MeasuredLayoutLimit {
  readonly budgetBytes: number;
  readonly categories: number;
  readonly channels: number;
  readonly logBytes: number;
  readonly bytesPerChannel: number;
}

/**
 * Shows what the device measured, not a documented ceiling.
 *
 * The numbers arrive from the backend measurement; this function refuses to
 * render a limit it was not given, so there is nowhere for a stale "two
 * categories and five channels" to survive.
 */
export function renderMeasuredLimit(limit: MeasuredLayoutLimit): string {
  for (const [field, value] of Object.entries(limit)) {
    if (!Number.isFinite(value) || value < 0) {
      throw new Error(`A measured layout limit needs a real ${field}`);
    }
  }
  return `<p class="enclave-measured-limit"><strong>Measured on this device</strong> ${limit.categories} categories and ${limit.channels} channels fit in a ${limit.budgetBytes}-byte enclave layout log (${limit.bytesPerChannel} bytes per channel measured over ${limit.logBytes} bytes). This is a measured limit, not a fixed ceiling.</p>`;
}
