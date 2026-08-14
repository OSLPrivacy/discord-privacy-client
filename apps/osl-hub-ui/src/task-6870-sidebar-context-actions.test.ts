import { describe, expect, it, vi } from "vitest";
import {
  EnclaveSidebarController,
  type EnclaveLeaveSubsystem,
  type EnclaveSidebarAuthority,
  type EnclaveSidebarControllerDependencies,
  type EncryptedSidebarProfileSync,
  type EncryptedSidebarStateRepository,
  type LeaveAuthorizationResult,
  type LeaveCapabilitySnapshot,
  type SidebarContextActionId,
  type SidebarContextMenu,
  type SidebarInputMethod,
  type SidebarNavigator,
  type SignedEnclaveLeaveCommand,
  bindEnclaveSidebarContextMenus,
  parseSidebarPersonalState,
  renderEnclaveSidebar,
  serializeSidebarPersonalState,
} from "./osl-enclave-sidebar";

class EncryptedVault implements EncryptedSidebarStateRepository, EncryptedSidebarProfileSync {
  readonly ciphertextByAccount = new Map<string, string>();

  async loadDecrypted(accountId: string): Promise<string | null> {
    return this.decrypt(accountId);
  }

  async saveEncrypted(accountId: string, serialized: string): Promise<void> {
    this.encrypt(accountId, serialized);
  }

  async downloadDecrypted(accountId: string): Promise<string | null> {
    return this.decrypt(accountId);
  }

  async uploadEncrypted(accountId: string, serialized: string): Promise<void> {
    this.encrypt(accountId, serialized);
  }

  private encrypt(accountId: string, plaintext: string): void {
    // Test double for an authenticated encryption boundary: plaintext is never
    // held by the at-rest/transport map used in assertions.
    this.ciphertextByAccount.set(accountId, `test-aead-v1:${Buffer.from(plaintext).toString("base64url")}`);
  }

  private decrypt(accountId: string): string | null {
    const ciphertext = this.ciphertextByAccount.get(accountId);
    if (!ciphertext) return null;
    const prefix = "test-aead-v1:";
    if (!ciphertext.startsWith(prefix)) throw new Error("unauthenticated sidebar ciphertext");
    return Buffer.from(ciphertext.slice(prefix.length), "base64url").toString();
  }
}

class MutableAuthority implements EnclaveSidebarAuthority {
  capability: LeaveCapabilitySnapshot = { authorized: true, membershipEpoch: "epoch-1" };
  authorizationCalls = 0;

  leaveCapability(_accountId: string, _enclaveId: string): LeaveCapabilitySnapshot {
    return { ...this.capability };
  }

  async authorizeLeave(request: {
    readonly accountId: string;
    readonly enclaveId: string;
    readonly membershipEpoch: string;
  }): Promise<LeaveAuthorizationResult> {
    this.authorizationCalls += 1;
    if (!this.capability.authorized) return { authorized: false, reason: "unauthorized" };
    if (request.membershipEpoch !== this.capability.membershipEpoch) {
      return { authorized: false, reason: "stale-membership" };
    }
    return {
      authorized: true,
      command: { ...request, signature: `signed:${request.accountId}:${request.enclaveId}:${request.membershipEpoch}` },
    };
  }
}

class RecordingNavigator implements SidebarNavigator {
  readonly settings: string[] = [];

  openEnclaveSettings(enclaveId: string): void {
    this.settings.push(enclaveId);
  }
}

class RecordingLeaveSubsystem implements EnclaveLeaveSubsystem {
  readonly commands: SignedEnclaveLeaveCommand[] = [];
  accept = true;

  async leave(command: SignedEnclaveLeaveCommand): Promise<boolean> {
    if (this.accept) this.commands.push(command);
    return this.accept;
  }
}

interface Harness {
  readonly local: EncryptedVault;
  readonly profile: EncryptedVault;
  readonly authority: MutableAuthority;
  readonly navigator: RecordingNavigator;
  readonly leaves: RecordingLeaveSubsystem;
  readonly dependencies: EnclaveSidebarControllerDependencies;
}

function harness(
  accountId = "account-alice",
  deviceId = "device-a",
  profile = new EncryptedVault(),
  local = new EncryptedVault(),
): Harness {
  const authority = new MutableAuthority();
  const navigator = new RecordingNavigator();
  const leaves = new RecordingLeaveSubsystem();
  return {
    local,
    profile,
    authority,
    navigator,
    leaves,
    dependencies: { accountId, deviceId, localState: local, profileSync: profile, authority, navigator, leaveSubsystem: leaves },
  };
}

function actionProjection(menu: SidebarContextMenu): readonly unknown[] {
  return menu.actions.map(({ id, label, enabled, effect }) => ({ id, label, enabled, effect }));
}

type FakeListener = (event: Record<string, unknown>) => void;

class FakeDocument {
  readonly body = new FakeElement("body", this);
  activeElement: FakeElement | null = null;
  readonly #listeners = new Map<string, FakeListener[]>();

  createElement(tagName: string): HTMLElement {
    return new FakeElement(tagName, this) as unknown as HTMLElement;
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    this.#listeners.set(type, [...(this.#listeners.get(type) ?? []), listener as unknown as FakeListener]);
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    const normalized = listener as unknown as FakeListener;
    this.#listeners.set(type, (this.#listeners.get(type) ?? []).filter((candidate) => candidate !== normalized));
  }

  emit(type: string, init: Record<string, unknown>): void {
    const event = fakeEvent(type, init);
    for (const listener of this.#listeners.get(type) ?? []) listener(event);
  }
}

class FakeElement {
  className = "";
  textContent = "";
  type = "";
  disabled = false;
  tabIndex = 0;
  readonly dataset: Record<string, string> = {};
  readonly style: Record<string, string> = {};
  readonly attributes = new Map<string, string>();
  readonly children: FakeElement[] = [];
  parentElement: FakeElement | null = null;
  readonly #listeners = new Map<string, FakeListener[]>();

  constructor(readonly tagName: string, readonly ownerDocument: FakeDocument) {}

  append(...children: FakeElement[]): void {
    for (const child of children) {
      child.parentElement = this;
      this.children.push(child);
    }
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    this.#listeners.set(type, [...(this.#listeners.get(type) ?? []), listener as unknown as FakeListener]);
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    const normalized = listener as unknown as FakeListener;
    this.#listeners.set(type, (this.#listeners.get(type) ?? []).filter((candidate) => candidate !== normalized));
  }

  emit(type: string, init: Record<string, unknown> = {}): void {
    const event = fakeEvent(type, { target: this, ...init });
    for (const listener of this.#listeners.get(type) ?? []) listener(event);
  }

  closest(selector: string): FakeElement | null {
    if (selector === "[data-enclave-id]" && this.dataset.enclaveId) return this;
    return this.parentElement?.closest(selector) ?? null;
  }

  contains(candidate: FakeElement): boolean {
    return candidate === this || this.children.some((child) => child.contains(candidate));
  }

  querySelectorAll<T>(selector: string): T[] {
    const matches: FakeElement[] = [];
    for (const child of this.children) {
      if (selector === '[role="menuitem"]' && child.attributes.get("role") === "menuitem") matches.push(child);
      matches.push(...child.querySelectorAll<FakeElement>(selector));
    }
    return matches as T[];
  }

  getBoundingClientRect(): DOMRect {
    return { left: 10, bottom: 20 } as DOMRect;
  }

  focus(): void {
    this.ownerDocument.activeElement = this;
  }

  remove(): void {
    if (!this.parentElement) return;
    const index = this.parentElement.children.indexOf(this);
    if (index >= 0) this.parentElement.children.splice(index, 1);
    this.parentElement = null;
  }
}

function fakeEvent(type: string, init: Record<string, unknown>): Record<string, unknown> {
  const event: Record<string, unknown> = {
    type,
    defaultPrevented: false,
    preventDefault: () => { event.defaultPrevented = true; },
    ...init,
  };
  return event;
}

describe("TASK 6870 Enclave sidebar persistent context actions", () => {
  it("renders and binds the same real menu for right-click, both keyboard gestures and touch long-press", async () => {
    vi.useFakeTimers();
    try {
      const test = harness();
      const controller = await EnclaveSidebarController.restore(test.dependencies);
      await controller.setPinOrder(["enclave-red"]);
      const document = new FakeDocument();
      const root = renderEnclaveSidebar(document as unknown as Document, [
        { enclaveId: "enclave-blue", label: "Blue", readThroughCursor: 2 },
        { enclaveId: "enclave-red", label: "Red", readThroughCursor: 7 },
      ], controller) as unknown as FakeElement;
      const list = root.children[0];
      const red = list?.children[0]?.children[0];
      expect(red?.dataset).toMatchObject({ enclaveId: "enclave-red", readThroughCursor: "7", pinned: "true" });
      expect(root.attributes.get("aria-label")).toBe("Enclaves");
      const binding = bindEnclaveSidebarContextMenus(root as unknown as HTMLElement, controller, {
        longPressMilliseconds: 100,
        longPressMoveTolerance: 5,
      });

      root.emit("contextmenu", { target: red, clientX: 30, clientY: 40 });
      let menu = document.body.children[0];
      expect(menu?.attributes.get("role")).toBe("menu");
      expect(menu?.children.map((item) => [item.attributes.get("role"), item.textContent, item.disabled])).toEqual([
        ["menuitem", "Unpin", false],
        ["menuitem", "Mute", false],
        ["menuitem", "Mark read", false],
        ["menuitem", "Open settings", false],
        ["menuitem", "Leave", false],
      ]);
      document.emit("keydown", { target: menu, key: "Escape", shiftKey: false });
      expect(document.body.children).toHaveLength(0);
      expect(document.activeElement).toBe(red);

      root.emit("keydown", { target: red, key: "ContextMenu", shiftKey: false });
      expect(document.body.children[0]?.attributes.get("role")).toBe("menu");
      binding.close();
      root.emit("keydown", { target: red, key: "F10", shiftKey: true });
      expect(document.body.children[0]?.attributes.get("role")).toBe("menu");
      binding.close();

      // Movement, release, and cancellation each starve a pending long-press.
      root.emit("pointerdown", { target: red, pointerType: "touch", pointerId: 1, clientX: 1, clientY: 1 });
      root.emit("pointermove", { target: red, pointerType: "touch", pointerId: 1, clientX: 20, clientY: 1 });
      await vi.advanceTimersByTimeAsync(100);
      expect(document.body.children).toHaveLength(0);
      root.emit("pointerdown", { target: red, pointerType: "touch", pointerId: 2, clientX: 1, clientY: 1 });
      root.emit("pointerup", { target: red, pointerType: "touch", pointerId: 2, clientX: 1, clientY: 1 });
      await vi.advanceTimersByTimeAsync(100);
      expect(document.body.children).toHaveLength(0);
      root.emit("pointerdown", { target: red, pointerType: "touch", pointerId: 3, clientX: 1, clientY: 1 });
      root.emit("pointercancel", { target: red, pointerType: "touch", pointerId: 3, clientX: 1, clientY: 1 });
      await vi.advanceTimersByTimeAsync(100);
      expect(document.body.children).toHaveLength(0);

      root.emit("pointerdown", { target: red, pointerType: "touch", pointerId: 4, clientX: 3, clientY: 4 });
      await vi.advanceTimersByTimeAsync(100);
      menu = document.body.children[0];
      expect(menu?.attributes.get("role")).toBe("menu");
      menu?.children[1]?.emit("click");
      await vi.runAllTimersAsync();
      await Promise.resolve();
      expect(controller.personalState().mutes).toEqual([
        expect.objectContaining({ enclaveId: "enclave-red", muted: true }),
      ]);
      expect(document.body.children).toHaveLength(0);
      expect(document.activeElement).toBe(red);
      binding.destroy();
      console.log("TASK-6870 dom_menus=4 pointer=1 keyboard=2 touch=1");
      console.log("TASK-6870 touch_cancellations=3 move=1 up=1 cancel=1");
      console.log("TASK-6870 menu_roles=6 menu=1 menuitems=5 focus_returns=2");
    } finally {
      vi.useRealTimers();
    }
  });

  it("executes every exact action through pointer, keyboard and touch with one shared enabled-state model", async () => {
    const test = harness();
    const controller = await EnclaveSidebarController.restore(test.dependencies);
    const target = { enclaveId: "enclave-red", readThroughCursor: 41 } as const;
    const initialPointer = controller.openForPointer(target);
    const initialKeyboard = controller.openForKeyboard(target);
    const initialTouch = controller.openForTouch(target);

    expect(actionProjection(initialPointer)).toEqual(actionProjection(initialKeyboard));
    expect(actionProjection(initialPointer)).toEqual(actionProjection(initialTouch));
    expect(initialPointer.actions).toEqual([
      { id: "pin", label: "Pin", enabled: true, effect: "pin" },
      { id: "mute", label: "Mute", enabled: true, effect: "mute" },
      { id: "mark-read", label: "Mark read", enabled: true, effect: "mark-read" },
      { id: "open-settings", label: "Open settings", enabled: true, effect: "open-settings" },
      { id: "leave", label: "Leave", enabled: true, effect: "leave" },
    ]);

    const exercised = new Set<SidebarContextActionId>();
    expect(await controller.dispatchPointer(initialPointer, "pin")).toEqual({ applied: true, action: "pin" });
    exercised.add("pin");
    expect(controller.personalState().pinOrder.enclaveIds).toEqual(["enclave-red"]);

    const mutedMenu = controller.openForKeyboard(target);
    expect(await controller.dispatchKeyboard(mutedMenu, "mute")).toEqual({ applied: true, action: "mute" });
    exercised.add("mute");
    expect(controller.personalState().mutes.at(0)).toMatchObject({ enclaveId: "enclave-red", muted: true });

    const readMenu = controller.openForTouch(target);
    expect(await controller.dispatchTouch(readMenu, "mark-read")).toEqual({ applied: true, action: "mark-read" });
    exercised.add("mark-read");
    expect(controller.personalState().readCursors.at(0)).toMatchObject({ enclaveId: "enclave-red", cursor: 41 });

    const settingsMenu = controller.openForPointer(target);
    expect(await controller.dispatchPointer(settingsMenu, "open-settings")).toEqual({ applied: true, action: "open-settings" });
    exercised.add("open-settings");
    expect(test.navigator.settings).toEqual(["enclave-red"]);

    const unmuteMenu = controller.openForTouch(target);
    expect(unmuteMenu.actions.at(1)).toEqual({ id: "unmute", label: "Unmute", enabled: true, effect: "unmute" });
    expect(await controller.dispatchTouch(unmuteMenu, "unmute")).toEqual({ applied: true, action: "unmute" });
    exercised.add("unmute");
    expect(controller.personalState().mutes.at(0)).toMatchObject({ enclaveId: "enclave-red", muted: false });

    const unpinMenu = controller.openForKeyboard(target);
    expect(unpinMenu.actions.at(0)).toEqual({ id: "unpin", label: "Unpin", enabled: true, effect: "unpin" });
    expect(await controller.dispatchKeyboard(unpinMenu, "unpin")).toEqual({ applied: true, action: "unpin" });
    exercised.add("unpin");
    expect(controller.personalState().pinOrder.enclaveIds).toEqual([]);

    const leaveMenu = controller.openForPointer(target);
    expect(await controller.dispatchPointer(leaveMenu, "leave")).toEqual({ applied: true, action: "leave" });
    exercised.add("leave");
    expect(test.leaves.commands).toEqual([{
      accountId: "account-alice",
      enclaveId: "enclave-red",
      membershipEpoch: "epoch-1",
      signature: "signed:account-alice:enclave-red:epoch-1",
    }]);

    expect([...exercised].sort()).toEqual([
      "leave", "mark-read", "mute", "open-settings", "pin", "unmute", "unpin",
    ]);
    console.log(`TASK-6870 action_effects=${exercised.size}`);
    console.log("TASK-6870 input_methods=3 pointer=3 keyboard=2 touch=2");
    console.log(`TASK-6870 leave_subsystem_calls=${test.leaves.commands.length}`);
  });

  it("executes the complete seven-action matrix through each of the three input methods", async () => {
    const methods: readonly SidebarInputMethod[] = ["pointer", "keyboard", "touch"];
    const actions: readonly SidebarContextActionId[] = [
      "pin", "mute", "mark-read", "open-settings", "unmute", "unpin", "leave",
    ];
    let effects = 0;

    for (const method of methods) {
      const test = harness(`account-${method}`, `device-${method}`);
      const controller = await EnclaveSidebarController.restore(test.dependencies);
      const target = { enclaveId: "enclave-matrix", readThroughCursor: 73 } as const;
      for (const actionId of actions) {
        const menu = method === "pointer"
          ? controller.openForPointer(target)
          : method === "keyboard"
            ? controller.openForKeyboard(target)
            : controller.openForTouch(target);
        const result = method === "pointer"
          ? await controller.dispatchPointer(menu, actionId)
          : method === "keyboard"
            ? await controller.dispatchKeyboard(menu, actionId)
            : await controller.dispatchTouch(menu, actionId);
        expect(result, `${method}:${actionId}`).toEqual({ applied: true, action: actionId });
        effects += 1;
      }
      expect(test.navigator.settings).toEqual(["enclave-matrix"]);
      expect(test.leaves.commands).toHaveLength(1);
    }

    expect(effects).toBe(methods.length * actions.length);
    console.log("TASK-6870 action_input_effects=21 actions=7 input_methods=3 pointer=7 keyboard=7 touch=7");
  });

  it("encrypts, restarts and sync-merges three personal fields without affecting another account", async () => {
    const sharedProfile = new EncryptedVault();
    const aliceLocalA = new EncryptedVault();
    const aliceA = harness("account-alice", "device-a", sharedProfile, aliceLocalA);
    const controllerA = await EnclaveSidebarController.restore(aliceA.dependencies);
    await controllerA.setPinOrder(["enclave-red", "enclave-blue"]);
    await controllerA.dispatchPointer(controllerA.openForPointer({ enclaveId: "enclave-blue", readThroughCursor: 0 }), "mute");
    await controllerA.dispatchKeyboard(controllerA.openForKeyboard({ enclaveId: "enclave-red", readThroughCursor: 17 }), "mark-read");

    const localCiphertext = aliceLocalA.ciphertextByAccount.get("account-alice") ?? "";
    const profileCiphertext = sharedProfile.ciphertextByAccount.get("account-alice") ?? "";
    expect(localCiphertext).toMatch(/^test-aead-v1:/u);
    expect(profileCiphertext).toMatch(/^test-aead-v1:/u);
    expect(localCiphertext).not.toContain("enclave-red");
    expect(profileCiphertext).not.toContain("enclave-blue");

    const restarted = harness("account-alice", "device-a", sharedProfile, aliceLocalA);
    const afterRestart = await EnclaveSidebarController.restore(restarted.dependencies);
    expect(afterRestart.personalState()).toMatchObject({
      accountId: "account-alice",
      pinOrder: { enclaveIds: ["enclave-red", "enclave-blue"] },
      mutes: [{ enclaveId: "enclave-blue", muted: true }],
      readCursors: [{ enclaveId: "enclave-red", cursor: 17 }],
    });

    const aliceB = harness("account-alice", "device-b", sharedProfile, new EncryptedVault());
    const controllerB = await EnclaveSidebarController.restore(aliceB.dependencies);
    expect(controllerB.personalState()).toEqual(afterRestart.personalState());

    // C starts from the same old profile. B updates the order first; C then
    // commits its independent mute and must merge B's newer profile at commit.
    const aliceC = harness("account-alice", "device-c", sharedProfile, new EncryptedVault());
    const controllerC = await EnclaveSidebarController.restore(aliceC.dependencies);
    await controllerB.setPinOrder(["enclave-blue", "enclave-red"]);
    await controllerC.dispatchTouch(controllerC.openForTouch({ enclaveId: "enclave-red", readThroughCursor: 17 }), "mute");
    const syncedBackA = harness("account-alice", "device-a", sharedProfile, aliceLocalA);
    const mergedA = await EnclaveSidebarController.restore(syncedBackA.dependencies);
    expect(mergedA.personalState().pinOrder.enclaveIds).toEqual(["enclave-blue", "enclave-red"]);
    expect(mergedA.personalState().mutes).toEqual([
      expect.objectContaining({ enclaveId: "enclave-blue", muted: true }),
      expect.objectContaining({ enclaveId: "enclave-red", muted: true }),
    ]);
    expect(mergedA.personalState().readCursors).toEqual([
      expect.objectContaining({ enclaveId: "enclave-red", cursor: 17 }),
    ]);

    const bob = harness("account-bob", "device-bob", sharedProfile, new EncryptedVault());
    const bobController = await EnclaveSidebarController.restore(bob.dependencies);
    expect(bobController.personalState()).toMatchObject({
      accountId: "account-bob", pinOrder: { enclaveIds: [] }, mutes: [], readCursors: [],
    });
    expect(sharedProfile.ciphertextByAccount.size).toBe(2);

    const strictJson = serializeSidebarPersonalState(mergedA.personalState());
    expect(strictJson).not.toMatch(/whitelist|membership/iu);
    expect(() => parseSidebarPersonalState(
      strictJson.replace('"format":', '"whitelist":[],"format":'),
      "account-alice",
    )).toThrow(/fields/u);
    expect(() => parseSidebarPersonalState(strictJson, "account-bob")).toThrow(/account mismatch/u);
    console.log("TASK-6870 restart_restored=3 pin_order=2 mute=1 read_cursor=17");
    console.log("TASK-6870 second_device_synced=3 pin_order=2 mute=1 read_cursor=17");
    console.log("TASK-6870 independent_members_unchanged=1");
    console.log("TASK-6870 encrypted_repositories=2 local=1 profile=1");
    console.log("TASK-6870 forbidden_personal_fields=0 whitelist=0 membership=0");
  });

  it("changes zero state for authorization loss, stale membership, disabled leave and subsystem refusal", async () => {
    const test = harness();
    const controller = await EnclaveSidebarController.restore(test.dependencies);
    const target = { enclaveId: "enclave-red", readThroughCursor: 0 } as const;
    const before = serializeSidebarPersonalState(controller.personalState());

    const unauthorizedMenu = controller.openForPointer(target);
    test.authority.capability = { authorized: false, membershipEpoch: "epoch-1" };
    expect(await controller.dispatchPointer(unauthorizedMenu, "leave")).toEqual({
      applied: false, action: "leave", reason: "unauthorized",
    });
    expect(serializeSidebarPersonalState(controller.personalState())).toBe(before);
    expect(test.leaves.commands).toHaveLength(0);

    test.authority.capability = { authorized: true, membershipEpoch: "epoch-2" };
    const staleMenu = controller.openForKeyboard(target);
    test.authority.capability = { authorized: true, membershipEpoch: "epoch-3" };
    expect(await controller.dispatchKeyboard(staleMenu, "leave")).toEqual({
      applied: false, action: "leave", reason: "stale-membership",
    });
    expect(serializeSidebarPersonalState(controller.personalState())).toBe(before);
    expect(test.leaves.commands).toHaveLength(0);

    test.authority.capability = { authorized: false, membershipEpoch: "epoch-3" };
    const disabledMenu = controller.openForTouch(target);
    expect(disabledMenu.actions.at(-1)).toMatchObject({ id: "leave", enabled: false });
    expect(await controller.dispatchTouch(disabledMenu, "leave")).toEqual({
      applied: false, action: "leave", reason: "disabled",
    });
    expect(serializeSidebarPersonalState(controller.personalState())).toBe(before);
    expect(test.leaves.commands).toHaveLength(0);

    test.authority.capability = { authorized: true, membershipEpoch: "epoch-4" };
    test.leaves.accept = false;
    const refusedMenu = controller.openForPointer(target);
    expect(await controller.dispatchPointer(refusedMenu, "leave")).toEqual({
      applied: false, action: "leave", reason: "leave-refused",
    });
    expect(serializeSidebarPersonalState(controller.personalState())).toBe(before);
    expect(test.leaves.commands).toHaveLength(0);
    console.log("TASK-6870 denials_zero_state=4 unauthorized=1 stale_membership=1 disabled=1 subsystem_refused=1");
  });

  it("rejects a visible enabled label whose machine effect is missing", async () => {
    const test = harness();
    const controller = await EnclaveSidebarController.restore(test.dependencies);
    const valid = controller.openForPointer({ enclaveId: "enclave-red", readThroughCursor: 0 });
    const malformed = {
      ...valid,
      actions: valid.actions.map((entry) => entry.id === "pin" ? { ...entry, effect: undefined } : entry),
    } as unknown as SidebarContextMenu;

    await expect(controller.dispatchPointer(malformed, "pin")).rejects.toThrow(/no matching effect/u);
    expect(controller.personalState().pinOrder.enclaveIds).toEqual([]);
    console.log("TASK-6870 decorative_label_rejected=1 state_changes=0");
  });
});
