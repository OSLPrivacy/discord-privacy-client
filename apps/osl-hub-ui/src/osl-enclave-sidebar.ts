/**
 * Enclave sidebar context actions and account-scoped personal state.
 *
 * The persistence ports deliberately expose decrypted JSON only at this
 * controller boundary. Implementations are required to encrypt it before it
 * reaches local storage or the profile-sync transport. Membership and
 * whitelist policy never enter this personal-state document.
 */

export const SIDEBAR_PERSONAL_STATE_FORMAT = "osl.enclave-sidebar.personal.v1" as const;

export interface SidebarWriteClock {
  readonly counter: number;
  readonly deviceId: string;
}

export interface SidebarPinOrder {
  readonly enclaveIds: readonly string[];
  readonly written: SidebarWriteClock;
}

export interface SidebarMuteRegister {
  readonly enclaveId: string;
  readonly muted: boolean;
  readonly written: SidebarWriteClock;
}

export interface SidebarReadCursorRegister {
  readonly enclaveId: string;
  readonly cursor: number;
  readonly written: SidebarWriteClock;
}

/** Strictly JSON-serializable, account-scoped personal sidebar state. */
export interface SidebarPersonalState {
  readonly format: typeof SIDEBAR_PERSONAL_STATE_FORMAT;
  readonly accountId: string;
  readonly pinOrder: SidebarPinOrder;
  readonly mutes: readonly SidebarMuteRegister[];
  readonly readCursors: readonly SidebarReadCursorRegister[];
}

export interface EncryptedSidebarStateRepository {
  /** Returns authenticated, decrypted JSON or null when no state exists. */
  loadDecrypted(accountId: string): Promise<string | null>;
  /** Encrypts/authenticates JSON before writing it to the repository. */
  saveEncrypted(accountId: string, serialized: string): Promise<void>;
}

export interface EncryptedSidebarProfileSync {
  /** Returns authenticated, decrypted profile JSON or null when absent. */
  downloadDecrypted(accountId: string): Promise<string | null>;
  /** Encrypts/authenticates JSON before uploading it to profile sync. */
  uploadEncrypted(accountId: string, serialized: string): Promise<void>;
}

export interface SidebarNavigator {
  openEnclaveSettings(enclaveId: string): Promise<void> | void;
}

export interface LeaveCapabilitySnapshot {
  readonly authorized: boolean;
  readonly membershipEpoch: string;
}

export interface SignedEnclaveLeaveCommand {
  readonly accountId: string;
  readonly enclaveId: string;
  readonly membershipEpoch: string;
  readonly signature: string;
}

export type LeaveAuthorizationResult =
  | { readonly authorized: false; readonly reason: "unauthorized" | "stale-membership" }
  | { readonly authorized: true; readonly command: SignedEnclaveLeaveCommand };

/** Enclave authority is the sole source of signed shared-state commands. */
export interface EnclaveSidebarAuthority {
  leaveCapability(accountId: string, enclaveId: string): LeaveCapabilitySnapshot;
  authorizeLeave(request: {
    readonly accountId: string;
    readonly enclaveId: string;
    readonly membershipEpoch: string;
  }): Promise<LeaveAuthorizationResult>;
}

/** The established leave subsystem owns all actual shared-state mutation. */
export interface EnclaveLeaveSubsystem {
  leave(command: SignedEnclaveLeaveCommand): Promise<boolean>;
}

export type SidebarInputMethod = "pointer" | "keyboard" | "touch";
export type SidebarContextActionId =
  | "pin"
  | "unpin"
  | "mute"
  | "unmute"
  | "mark-read"
  | "open-settings"
  | "leave";

export interface SidebarContextAction {
  readonly id: SidebarContextActionId;
  readonly label: "Pin" | "Unpin" | "Mute" | "Unmute" | "Mark read" | "Open settings" | "Leave";
  readonly enabled: boolean;
  /** A required, machine-dispatchable effect prevents decorative labels. */
  readonly effect: SidebarContextActionId;
}

export interface SidebarContextTarget {
  readonly enclaveId: string;
  /** Highest message cursor currently present on this device. */
  readonly readThroughCursor: number;
}

export interface SidebarContextMenu {
  readonly inputMethod: SidebarInputMethod;
  readonly target: SidebarContextTarget;
  readonly membershipEpoch: string;
  readonly actions: readonly SidebarContextAction[];
}

export type SidebarDispatchResult =
  | { readonly applied: true; readonly action: SidebarContextActionId }
  | {
      readonly applied: false;
      readonly action: SidebarContextActionId;
      readonly reason: "disabled" | "wrong-input-method" | "unauthorized" | "stale-membership" | "leave-refused";
    };

export interface EnclaveSidebarControllerDependencies {
  readonly accountId: string;
  readonly deviceId: string;
  readonly localState: EncryptedSidebarStateRepository;
  readonly profileSync: EncryptedSidebarProfileSync;
  readonly authority: EnclaveSidebarAuthority;
  readonly navigator: SidebarNavigator;
  readonly leaveSubsystem: EnclaveLeaveSubsystem;
}

export interface EnclaveSidebarEntry {
  readonly enclaveId: string;
  readonly label: string;
  readonly readThroughCursor: number;
}

export interface EnclaveSidebarContextBindingOptions {
  readonly longPressMilliseconds?: number;
  readonly longPressMoveTolerance?: number;
  readonly onAfterDispatch?: (result: SidebarDispatchResult) => Promise<void> | void;
  readonly onDispatchError?: (error: unknown) => void;
}

export interface EnclaveSidebarContextBinding {
  close(): void;
  destroy(): void;
}

const EMPTY_CLOCK: SidebarWriteClock = Object.freeze({ counter: 0, deviceId: "initial" });
const TOP_LEVEL_KEYS = ["accountId", "format", "mutes", "pinOrder", "readCursors"] as const;

export function createEmptySidebarPersonalState(accountId: string): SidebarPersonalState {
  return {
    format: SIDEBAR_PERSONAL_STATE_FORMAT,
    accountId: requireId(accountId, "account"),
    pinOrder: { enclaveIds: [], written: EMPTY_CLOCK },
    mutes: [],
    readCursors: [],
  };
}

/** Parse untrusted decrypted state with exact keys and no implicit coercion. */
export function parseSidebarPersonalState(serialized: string, expectedAccountId: string): SidebarPersonalState {
  let input: unknown;
  try {
    input = JSON.parse(serialized) as unknown;
  } catch {
    throw new Error("Invalid sidebar personal state JSON");
  }
  const record = requireExactRecord(input, TOP_LEVEL_KEYS, "sidebar personal state");
  if (record.format !== SIDEBAR_PERSONAL_STATE_FORMAT) throw new Error("Unsupported sidebar personal state format");
  if (record.accountId !== requireId(expectedAccountId, "account")) throw new Error("Sidebar personal state account mismatch");

  const pin = requireExactRecord(record.pinOrder, ["enclaveIds", "written"], "pin order");
  const enclaveIds = requireUniqueIds(pin.enclaveIds, "pin order");
  const mutes = requireArray(record.mutes, "mutes").map((value) => {
    const entry = requireExactRecord(value, ["enclaveId", "muted", "written"], "mute register");
    if (typeof entry.muted !== "boolean") throw new Error("Invalid sidebar mute value");
    return { enclaveId: requireId(entry.enclaveId, "enclave"), muted: entry.muted, written: requireClock(entry.written) };
  });
  const readCursors = requireArray(record.readCursors, "read cursors").map((value) => {
    const entry = requireExactRecord(value, ["cursor", "enclaveId", "written"], "read cursor register");
    return {
      enclaveId: requireId(entry.enclaveId, "enclave"),
      cursor: requireCursor(entry.cursor),
      written: requireClock(entry.written),
    };
  });
  requireUniqueRegisterIds(mutes, "mute");
  requireUniqueRegisterIds(readCursors, "read cursor");
  return {
    format: SIDEBAR_PERSONAL_STATE_FORMAT,
    accountId: expectedAccountId,
    pinOrder: { enclaveIds, written: requireClock(pin.written) },
    mutes,
    readCursors,
  };
}

export function serializeSidebarPersonalState(state: SidebarPersonalState): string {
  // Re-parse the object as untrusted data so callers cannot serialize extra
  // runtime properties, non-JSON values, or another account's loose shape.
  const json = JSON.stringify(state);
  const exact = parseSidebarPersonalState(json, state.accountId);
  return JSON.stringify(exact);
}

/** Deterministically merge independent device copies for the same account. */
export function mergeSidebarPersonalState(
  left: SidebarPersonalState,
  right: SidebarPersonalState,
): SidebarPersonalState {
  if (left.accountId !== right.accountId) throw new Error("Cannot merge sidebar state across accounts");
  const pinOrder = compareClocks(left.pinOrder.written, right.pinOrder.written) >= 0 ? left.pinOrder : right.pinOrder;
  return {
    format: SIDEBAR_PERSONAL_STATE_FORMAT,
    accountId: left.accountId,
    pinOrder: { enclaveIds: [...pinOrder.enclaveIds], written: { ...pinOrder.written } },
    mutes: mergeRegisters(left.mutes, right.mutes, (a, b) => compareClocks(a.written, b.written)),
    // Read cursors are monotonic: the furthest authenticated cursor wins;
    // clocks only break equal-cursor ties.
    readCursors: mergeRegisters(left.readCursors, right.readCursors, (a, b) => (
      a.cursor === b.cursor ? compareClocks(a.written, b.written) : a.cursor - b.cursor
    )),
  };
}

export class EnclaveSidebarController {
  readonly #dependencies: EnclaveSidebarControllerDependencies;
  #state: SidebarPersonalState;
  #clock: number;

  private constructor(dependencies: EnclaveSidebarControllerDependencies, state: SidebarPersonalState) {
    this.#dependencies = dependencies;
    this.#state = state;
    this.#clock = maximumClock(state);
  }

  static async restore(dependencies: EnclaveSidebarControllerDependencies): Promise<EnclaveSidebarController> {
    requireId(dependencies.accountId, "account");
    requireId(dependencies.deviceId, "device");
    const [localSerialized, profileSerialized] = await Promise.all([
      dependencies.localState.loadDecrypted(dependencies.accountId),
      dependencies.profileSync.downloadDecrypted(dependencies.accountId),
    ]);
    const local = localSerialized === null
      ? createEmptySidebarPersonalState(dependencies.accountId)
      : parseSidebarPersonalState(localSerialized, dependencies.accountId);
    const profile = profileSerialized === null
      ? createEmptySidebarPersonalState(dependencies.accountId)
      : parseSidebarPersonalState(profileSerialized, dependencies.accountId);
    const merged = mergeSidebarPersonalState(local, profile);
    const controller = new EnclaveSidebarController(dependencies, merged);
    await controller.#persist(merged);
    return controller;
  }

  personalState(): SidebarPersonalState {
    return parseSidebarPersonalState(serializeSidebarPersonalState(this.#state), this.#state.accountId);
  }

  openForPointer(target: SidebarContextTarget): SidebarContextMenu {
    return this.#open("pointer", target);
  }

  openForKeyboard(target: SidebarContextTarget): SidebarContextMenu {
    return this.#open("keyboard", target);
  }

  openForTouch(target: SidebarContextTarget): SidebarContextMenu {
    return this.#open("touch", target);
  }

  dispatchPointer(menu: SidebarContextMenu, action: SidebarContextActionId): Promise<SidebarDispatchResult> {
    return this.#dispatch("pointer", menu, action);
  }

  dispatchKeyboard(menu: SidebarContextMenu, action: SidebarContextActionId): Promise<SidebarDispatchResult> {
    return this.#dispatch("keyboard", menu, action);
  }

  dispatchTouch(menu: SidebarContextMenu, action: SidebarContextActionId): Promise<SidebarDispatchResult> {
    return this.#dispatch("touch", menu, action);
  }

  async setPinOrder(enclaveIds: readonly string[]): Promise<void> {
    const validated = requireUniqueIds(enclaveIds, "pin order");
    await this.#commit({ ...this.#state, pinOrder: { enclaveIds: validated, written: this.#nextClock() } });
  }

  #open(inputMethod: SidebarInputMethod, target: SidebarContextTarget): SidebarContextMenu {
    const enclaveId = requireId(target.enclaveId, "enclave");
    const readThroughCursor = requireCursor(target.readThroughCursor);
    const pinned = this.#state.pinOrder.enclaveIds.includes(enclaveId);
    const muted = this.#state.mutes.find((entry) => entry.enclaveId === enclaveId)?.muted ?? false;
    const currentCursor = this.#state.readCursors.find((entry) => entry.enclaveId === enclaveId)?.cursor ?? 0;
    const leave = this.#dependencies.authority.leaveCapability(this.#state.accountId, enclaveId);
    const membershipEpoch = requireId(leave.membershipEpoch, "membership epoch");
    return {
      inputMethod,
      target: { enclaveId, readThroughCursor },
      membershipEpoch,
      actions: [
        action(pinned ? "unpin" : "pin", pinned ? "Unpin" : "Pin", true),
        action(muted ? "unmute" : "mute", muted ? "Unmute" : "Mute", true),
        action("mark-read", "Mark read", readThroughCursor > currentCursor),
        action("open-settings", "Open settings", true),
        action("leave", "Leave", leave.authorized),
      ],
    };
  }

  async #dispatch(
    inputMethod: SidebarInputMethod,
    menu: SidebarContextMenu,
    actionId: SidebarContextActionId,
  ): Promise<SidebarDispatchResult> {
    if (menu.inputMethod !== inputMethod) {
      return { applied: false, action: actionId, reason: "wrong-input-method" };
    }
    const descriptor = menu.actions.find((candidate) => candidate.id === actionId);
    if (!descriptor?.enabled) return { applied: false, action: actionId, reason: "disabled" };
    if (descriptor.effect !== actionId) throw new Error(`Sidebar action ${actionId} has no matching effect`);

    const enclaveId = requireId(menu.target.enclaveId, "enclave");
    switch (descriptor.effect) {
      case "pin":
        await this.setPinOrder([...this.#state.pinOrder.enclaveIds.filter((id) => id !== enclaveId), enclaveId]);
        break;
      case "unpin":
        await this.setPinOrder(this.#state.pinOrder.enclaveIds.filter((id) => id !== enclaveId));
        break;
      case "mute":
        await this.#setMute(enclaveId, true);
        break;
      case "unmute":
        await this.#setMute(enclaveId, false);
        break;
      case "mark-read":
        await this.#markRead(enclaveId, menu.target.readThroughCursor);
        break;
      case "open-settings":
        await this.#dependencies.navigator.openEnclaveSettings(enclaveId);
        break;
      case "leave":
        return this.#leave(enclaveId, menu.membershipEpoch);
      default:
        return assertNever(descriptor.effect);
    }
    return { applied: true, action: actionId };
  }

  async #leave(enclaveId: string, displayedEpoch: string): Promise<SidebarDispatchResult> {
    // Re-read capability at dispatch so a stale menu can never authorize a
    // shared mutation. The authority then performs its own signed recheck.
    const current = this.#dependencies.authority.leaveCapability(this.#state.accountId, enclaveId);
    if (!current.authorized) return { applied: false, action: "leave", reason: "unauthorized" };
    if (current.membershipEpoch !== displayedEpoch) {
      return { applied: false, action: "leave", reason: "stale-membership" };
    }
    const authorization = await this.#dependencies.authority.authorizeLeave({
      accountId: this.#state.accountId,
      enclaveId,
      membershipEpoch: displayedEpoch,
    });
    if (!authorization.authorized) {
      return { applied: false, action: "leave", reason: authorization.reason };
    }
    const command = authorization.command;
    if (
      command.accountId !== this.#state.accountId
      || command.enclaveId !== enclaveId
      || command.membershipEpoch !== displayedEpoch
      || command.signature.trim().length === 0
    ) return { applied: false, action: "leave", reason: "unauthorized" };
    const left = await this.#dependencies.leaveSubsystem.leave(command);
    return left
      ? { applied: true, action: "leave" }
      : { applied: false, action: "leave", reason: "leave-refused" };
  }

  async #setMute(enclaveId: string, muted: boolean): Promise<void> {
    const next = this.#state.mutes.filter((entry) => entry.enclaveId !== enclaveId);
    next.push({ enclaveId, muted, written: this.#nextClock() });
    await this.#commit({ ...this.#state, mutes: next });
  }

  async #markRead(enclaveId: string, cursor: number): Promise<void> {
    const safeCursor = requireCursor(cursor);
    const old = this.#state.readCursors.find((entry) => entry.enclaveId === enclaveId)?.cursor ?? 0;
    const next = this.#state.readCursors.filter((entry) => entry.enclaveId !== enclaveId);
    next.push({ enclaveId, cursor: Math.max(old, safeCursor), written: this.#nextClock() });
    await this.#commit({ ...this.#state, readCursors: next });
  }

  #nextClock(): SidebarWriteClock {
    this.#clock += 1;
    return { counter: this.#clock, deviceId: this.#dependencies.deviceId };
  }

  async #commit(next: SidebarPersonalState): Promise<void> {
    const remoteSerialized = await this.#dependencies.profileSync.downloadDecrypted(this.#state.accountId);
    const merged = remoteSerialized === null
      ? next
      : mergeSidebarPersonalState(
          next,
          parseSidebarPersonalState(remoteSerialized, this.#state.accountId),
        );
    await this.#persist(merged);
    this.#state = merged;
    this.#clock = Math.max(this.#clock, maximumClock(merged));
  }

  async #persist(state: SidebarPersonalState): Promise<void> {
    const serialized = serializeSidebarPersonalState(state);
    await Promise.all([
      this.#dependencies.localState.saveEncrypted(this.#state.accountId, serialized),
      this.#dependencies.profileSync.uploadEncrypted(this.#state.accountId, serialized),
    ]);
  }
}

/**
 * Render sidebar rows without HTML interpolation. Pinned rows follow the
 * persisted personal order and mute is represented only as a personal
 * notification preference, never as membership or whitelist state.
 */
export function renderEnclaveSidebar(
  document: Document,
  entries: readonly EnclaveSidebarEntry[],
  controller: EnclaveSidebarController,
): HTMLElement {
  const state = controller.personalState();
  const pins = new Map(state.pinOrder.enclaveIds.map((id, index) => [id, index]));
  const muted = new Set(state.mutes.filter((entry) => entry.muted).map((entry) => entry.enclaveId));
  const indexed = entries.map((entry, sourceIndex) => ({ entry: validateSidebarEntry(entry), sourceIndex }));
  indexed.sort((left, right) => {
    const leftPin = pins.get(left.entry.enclaveId);
    const rightPin = pins.get(right.entry.enclaveId);
    if (leftPin !== undefined && rightPin !== undefined) return leftPin - rightPin;
    if (leftPin !== undefined) return -1;
    if (rightPin !== undefined) return 1;
    return left.sourceIndex - right.sourceIndex;
  });

  const nav = document.createElement("nav");
  nav.className = "osl-enclave-sidebar";
  nav.setAttribute("aria-label", "Enclaves");
  const list = document.createElement("ul");
  list.className = "osl-enclave-sidebar__list";
  for (const { entry } of indexed) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "osl-enclave-sidebar__entry";
    button.dataset.enclaveId = entry.enclaveId;
    button.dataset.readThroughCursor = String(entry.readThroughCursor);
    button.setAttribute("aria-haspopup", "menu");
    button.textContent = entry.label;
    if (pins.has(entry.enclaveId)) button.dataset.pinned = "true";
    if (muted.has(entry.enclaveId)) {
      button.dataset.muted = "true";
      button.setAttribute("aria-label", `${entry.label}, muted`);
    }
    item.append(button);
    list.append(item);
  }
  nav.append(list);
  return nav;
}

/**
 * Bind one shared descriptor model to native right-click, keyboard context
 * menu keys, and cancellable touch long-press. The returned binding owns only
 * the generated menu and its listeners.
 */
export function bindEnclaveSidebarContextMenus(
  root: HTMLElement,
  controller: EnclaveSidebarController,
  options: EnclaveSidebarContextBindingOptions = {},
): EnclaveSidebarContextBinding {
  const document = root.ownerDocument;
  const longPressMilliseconds = options.longPressMilliseconds ?? 500;
  const moveTolerance = options.longPressMoveTolerance ?? 10;
  if (!Number.isFinite(longPressMilliseconds) || longPressMilliseconds < 0) throw new Error("Invalid long-press duration");
  if (!Number.isFinite(moveTolerance) || moveTolerance < 0) throw new Error("Invalid long-press move tolerance");

  let visibleMenu: HTMLElement | null = null;
  let returnFocus: HTMLElement | null = null;
  let touchTimer: ReturnType<typeof setTimeout> | null = null;
  let touchStart: { readonly pointerId: number; readonly x: number; readonly y: number; readonly entry: HTMLElement } | null = null;
  let suppressContextMenuUntil = 0;

  const clearLongPress = (): void => {
    if (touchTimer !== null) clearTimeout(touchTimer);
    touchTimer = null;
    touchStart = null;
  };

  const close = (restoreFocus = true): void => {
    visibleMenu?.remove();
    visibleMenu = null;
    if (restoreFocus) returnFocus?.focus();
    returnFocus = null;
  };

  const dispatch = async (
    input: SidebarInputMethod,
    menu: SidebarContextMenu,
    actionId: SidebarContextActionId,
  ): Promise<void> => {
    try {
      const result = input === "pointer"
        ? await controller.dispatchPointer(menu, actionId)
        : input === "keyboard"
          ? await controller.dispatchKeyboard(menu, actionId)
          : await controller.dispatchTouch(menu, actionId);
      await options.onAfterDispatch?.(result);
      close(true);
    } catch (error) {
      options.onDispatchError?.(error);
      close(true);
    }
  };

  const show = (entry: HTMLElement, input: SidebarInputMethod, x?: number, y?: number): void => {
    const target = targetFromEntry(entry);
    const model = input === "pointer"
      ? controller.openForPointer(target)
      : input === "keyboard"
        ? controller.openForKeyboard(target)
        : controller.openForTouch(target);
    close(false);
    returnFocus = entry;
    const menu = document.createElement("div");
    menu.className = "osl-enclave-sidebar-context-menu";
    menu.setAttribute("role", "menu");
    menu.setAttribute("aria-label", `${entry.textContent?.trim() || "Enclave"} actions`);
    menu.tabIndex = -1;
    menu.style.position = "fixed";
    const anchor = entry.getBoundingClientRect();
    menu.style.left = `${Math.max(0, x ?? anchor.left)}px`;
    menu.style.top = `${Math.max(0, y ?? anchor.bottom)}px`;

    for (const descriptor of model.actions) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "osl-enclave-sidebar-context-menu__item";
      item.setAttribute("role", "menuitem");
      item.dataset.action = descriptor.id;
      item.textContent = descriptor.label;
      item.disabled = !descriptor.enabled;
      item.setAttribute("aria-disabled", String(!descriptor.enabled));
      item.tabIndex = -1;
      item.addEventListener("click", () => { void dispatch(input, model, descriptor.effect); });
      menu.append(item);
    }
    document.body.append(menu);
    visibleMenu = menu;
    focusMenuItem(menu, 0);
  };

  const onContextMenu = (event: MouseEvent): void => {
    const entry = closestSidebarEntry(event.target);
    if (!entry) return;
    event.preventDefault();
    if (Date.now() < suppressContextMenuUntil) return;
    show(entry, "pointer", event.clientX, event.clientY);
  };

  const onKeyDown = (event: KeyboardEvent): void => {
    if (event.defaultPrevented) return;
    if (event.key === "Escape" && visibleMenu) {
      event.preventDefault();
      close(true);
      return;
    }
    if (visibleMenu && ["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      moveMenuFocus(visibleMenu, event.key);
      return;
    }
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
    const entry = closestSidebarEntry(event.target);
    if (!entry) return;
    event.preventDefault();
    show(entry, "keyboard");
  };

  const onDocumentKeyDown = (event: KeyboardEvent): void => {
    if (event.defaultPrevented || !visibleMenu) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close(true);
    } else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      moveMenuFocus(visibleMenu, event.key);
    }
  };

  const onPointerDown = (event: PointerEvent): void => {
    if (event.pointerType !== "touch") return;
    const entry = closestSidebarEntry(event.target);
    if (!entry) return;
    clearLongPress();
    touchStart = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, entry };
    touchTimer = setTimeout(() => {
      const pending = touchStart;
      touchTimer = null;
      touchStart = null;
      if (!pending) return;
      suppressContextMenuUntil = Date.now() + 750;
      show(pending.entry, "touch", pending.x, pending.y);
    }, longPressMilliseconds);
  };

  const onPointerMove = (event: PointerEvent): void => {
    if (!touchStart || event.pointerId !== touchStart.pointerId) return;
    if (Math.hypot(event.clientX - touchStart.x, event.clientY - touchStart.y) > moveTolerance) clearLongPress();
  };

  const onPointerEnd = (event: PointerEvent): void => {
    if (touchStart && event.pointerId === touchStart.pointerId) clearLongPress();
  };

  const onDocumentPointerDown = (event: PointerEvent): void => {
    if (visibleMenu && event.target !== visibleMenu && !visibleMenu.contains(event.target as Node)) close(false);
  };

  root.addEventListener("contextmenu", onContextMenu);
  root.addEventListener("keydown", onKeyDown);
  root.addEventListener("pointerdown", onPointerDown);
  root.addEventListener("pointermove", onPointerMove);
  root.addEventListener("pointerup", onPointerEnd);
  root.addEventListener("pointercancel", onPointerEnd);
  document.addEventListener("keydown", onDocumentKeyDown);
  document.addEventListener("pointerdown", onDocumentPointerDown, true);

  return {
    close: () => close(true),
    destroy: () => {
      clearLongPress();
      close(false);
      root.removeEventListener("contextmenu", onContextMenu);
      root.removeEventListener("keydown", onKeyDown);
      root.removeEventListener("pointerdown", onPointerDown);
      root.removeEventListener("pointermove", onPointerMove);
      root.removeEventListener("pointerup", onPointerEnd);
      root.removeEventListener("pointercancel", onPointerEnd);
      document.removeEventListener("keydown", onDocumentKeyDown);
      document.removeEventListener("pointerdown", onDocumentPointerDown, true);
    },
  };
}

function validateSidebarEntry(entry: EnclaveSidebarEntry): EnclaveSidebarEntry {
  if (entry.label.trim().length === 0) throw new Error("Invalid sidebar entry label");
  return {
    enclaveId: requireId(entry.enclaveId, "enclave"),
    label: entry.label,
    readThroughCursor: requireCursor(entry.readThroughCursor),
  };
}

function closestSidebarEntry(target: EventTarget | null): HTMLElement | null {
  const candidate = target as { closest?: (selector: string) => Element | null } | null;
  return candidate?.closest?.("[data-enclave-id]") as HTMLElement | null ?? null;
}

function targetFromEntry(entry: HTMLElement): SidebarContextTarget {
  return {
    enclaveId: requireId(entry.dataset.enclaveId, "enclave"),
    readThroughCursor: requireCursor(Number(entry.dataset.readThroughCursor)),
  };
}

function enabledMenuItems(menu: HTMLElement): HTMLButtonElement[] {
  return [...menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].filter((item) => !item.disabled);
}

function focusMenuItem(menu: HTMLElement, index: number): void {
  const items = enabledMenuItems(menu);
  const item = items.at(index);
  if (!item) return;
  for (const candidate of items) candidate.tabIndex = candidate === item ? 0 : -1;
  item.focus();
}

function moveMenuFocus(menu: HTMLElement, key: string): void {
  const items = enabledMenuItems(menu);
  if (items.length === 0) return;
  const current = items.findIndex((item) => item === menu.ownerDocument.activeElement);
  const next = key === "Home"
    ? 0
    : key === "End"
      ? items.length - 1
      : key === "ArrowUp"
        ? (current <= 0 ? items.length - 1 : current - 1)
        : (current + 1) % items.length;
  focusMenuItem(menu, next);
}

function action(id: SidebarContextActionId, label: SidebarContextAction["label"], enabled: boolean): SidebarContextAction {
  return { id, label, enabled, effect: id };
}

function mergeRegisters<T extends { readonly enclaveId: string }>(
  left: readonly T[],
  right: readonly T[],
  compare: (left: T, right: T) => number,
): T[] {
  const merged = new Map<string, T>();
  for (const entry of [...left, ...right]) {
    const previous = merged.get(entry.enclaveId);
    if (!previous || compare(entry, previous) > 0) merged.set(entry.enclaveId, entry);
  }
  return [...merged.values()].sort((a, b) => a.enclaveId.localeCompare(b.enclaveId));
}

function compareClocks(left: SidebarWriteClock, right: SidebarWriteClock): number {
  return left.counter === right.counter
    ? left.deviceId.localeCompare(right.deviceId)
    : left.counter - right.counter;
}

function maximumClock(state: SidebarPersonalState): number {
  return Math.max(
    state.pinOrder.written.counter,
    ...state.mutes.map((entry) => entry.written.counter),
    ...state.readCursors.map((entry) => entry.written.counter),
  );
}

function requireClock(input: unknown): SidebarWriteClock {
  const clock = requireExactRecord(input, ["counter", "deviceId"], "write clock");
  if (!Number.isSafeInteger(clock.counter) || (clock.counter as number) < 0) throw new Error("Invalid sidebar write counter");
  return { counter: clock.counter as number, deviceId: requireId(clock.deviceId, "device") };
}

function requireCursor(input: unknown): number {
  if (!Number.isSafeInteger(input) || (input as number) < 0) throw new Error("Invalid sidebar read cursor");
  return input as number;
}

function requireId(input: unknown, kind: string): string {
  if (typeof input !== "string" || input.trim().length === 0 || input !== input.trim()) {
    throw new Error(`Invalid sidebar ${kind} identifier`);
  }
  return input;
}

function requireUniqueIds(input: unknown, name: string): string[] {
  const ids = requireArray(input, name).map((value) => requireId(value, "enclave"));
  if (new Set(ids).size !== ids.length) throw new Error(`Duplicate enclave in sidebar ${name}`);
  return ids;
}

function requireUniqueRegisterIds(entries: readonly { readonly enclaveId: string }[], name: string): void {
  if (new Set(entries.map((entry) => entry.enclaveId)).size !== entries.length) {
    throw new Error(`Duplicate sidebar ${name} register`);
  }
}

function requireArray(input: unknown, name: string): unknown[] {
  if (!Array.isArray(input)) throw new Error(`Invalid sidebar ${name}`);
  return input;
}

function requireExactRecord<const Keys extends readonly string[]>(
  input: unknown,
  keys: Keys,
  name: string,
): Record<Keys[number], unknown> {
  if (typeof input !== "object" || input === null || Array.isArray(input)) throw new Error(`Invalid ${name}`);
  const actual = Object.keys(input).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    throw new Error(`Invalid ${name} fields`);
  }
  return input as Record<Keys[number], unknown>;
}

function assertNever(value: never): never {
  throw new Error(`Unhandled sidebar action: ${String(value)}`);
}
