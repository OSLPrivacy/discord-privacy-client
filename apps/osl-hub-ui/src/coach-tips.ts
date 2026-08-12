import { SecureLocalStore } from "./secure-local-store";

/**
 * Dismissals are UI preference state, not product analytics.
 *
 * The document intentionally has no impression count, last-seen timestamp,
 * route history, control usage, or device identifier. It is written only by
 * the three explicit preference actions below: dismiss, dismiss all, reset.
 */
export const COACH_TIP_STATE_VERSION = 1 as const;
export const COACH_TIP_CATALOG_VERSION = 1 as const;
const ENCRYPTED_PROFILE_UI_STATE_KEY = "ui-state-v1";

export type CoachTipId = "protect-message" | "private-scan" | "switch-profile";
export type CoachTipContext = "protected-workspace" | "privacy-scan" | "profile-picker";

export interface CoachTipDefinition {
  readonly id: CoachTipId;
  readonly revision: number;
  readonly context: CoachTipContext;
  readonly controlSelector: string;
  readonly title: string;
  readonly body: string;
}

export const coachTipCatalog: readonly CoachTipDefinition[] = Object.freeze([
  {
    id: "protect-message",
    revision: 1,
    context: "protected-workspace",
    controlSelector: "#local-protected-toggle",
    title: "Protect a message",
    body: "Protect opens OSL's private composer without changing the app underneath.",
  },
  {
    id: "private-scan",
    revision: 1,
    context: "privacy-scan",
    controlSelector: "[data-scrub-route-scan]",
    title: "Scan on this device",
    body: "Start scan reviews only the accounts and categories you chose. Nothing is uploaded.",
  },
  {
    id: "switch-profile",
    revision: 1,
    context: "profile-picker",
    controlSelector: "[data-switch-identity]",
    title: "Profiles stay separate",
    body: "Switch changes the active OSL profile. Each profile keeps its own coach-tip choices.",
  },
]);

interface CoachTipProfileDocument {
  readonly version: typeof COACH_TIP_STATE_VERSION;
  readonly catalogVersion: typeof COACH_TIP_CATALOG_VERSION;
  readonly dismissed: Partial<Record<CoachTipId, number>>;
}

export type EncryptedProfileStateStore = Pick<SecureLocalStore, "getItem" | "setItem">;
type NativeInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

function emptyDocument(): CoachTipProfileDocument {
  return {
    version: COACH_TIP_STATE_VERSION,
    catalogVersion: COACH_TIP_CATALOG_VERSION,
    dismissed: {},
  };
}

function definition(id: CoachTipId): CoachTipDefinition {
  const found = coachTipCatalog.find((candidate) => candidate.id === id);
  if (!found) throw new Error("unknown coach tip");
  return found;
}

function parseDocument(raw: string | null): CoachTipProfileDocument {
  if (raw === null) return emptyDocument();
  try {
    const value: unknown = JSON.parse(raw);
    if (typeof value !== "object" || value === null) return emptyDocument();
    const record = value as Record<string, unknown>;
    if (
      Object.keys(record).sort().join(",") !== "catalogVersion,dismissed,version"
      || record.version !== COACH_TIP_STATE_VERSION
      || record.catalogVersion !== COACH_TIP_CATALOG_VERSION
      || typeof record.dismissed !== "object"
      || record.dismissed === null
      || Array.isArray(record.dismissed)
    ) return emptyDocument();

    const dismissed: Partial<Record<CoachTipId, number>> = {};
    for (const [id, revision] of Object.entries(record.dismissed)) {
      const candidate = coachTipCatalog.find((tip) => tip.id === id);
      if (candidate && Number.isSafeInteger(revision) && (revision as number) > 0) {
        dismissed[candidate.id] = revision as number;
      }
    }
    return { version: COACH_TIP_STATE_VERSION, catalogVersion: COACH_TIP_CATALOG_VERSION, dismissed };
  } catch {
    return emptyDocument();
  }
}

function serializeDocument(document: CoachTipProfileDocument): string {
  const dismissed = Object.fromEntries(
    coachTipCatalog
      .filter((tip) => document.dismissed[tip.id] !== undefined)
      .map((tip) => [tip.id, document.dismissed[tip.id]]),
  );
  return JSON.stringify({
    version: COACH_TIP_STATE_VERSION,
    catalogVersion: COACH_TIP_CATALOG_VERSION,
    dismissed,
  });
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** One profile-bound controller. Creating a new instance models a restart. */
export class CoachTipController {
  readonly #store: EncryptedProfileStateStore;
  #document: CoachTipProfileDocument = emptyDocument();
  #loaded = false;

  constructor(store: EncryptedProfileStateStore) {
    this.#store = store;
  }

  async load(): Promise<void> {
    this.#document = parseDocument(await this.#store.getItem(ENCRYPTED_PROFILE_UI_STATE_KEY));
    this.#loaded = true;
  }

  isLoaded(): boolean {
    return this.#loaded;
  }

  isEligible(id: CoachTipId, context: CoachTipContext, availableControls: readonly string[]): boolean {
    if (!this.#loaded) return false;
    const tip = definition(id);
    return tip.context === context
      && availableControls.includes(tip.controlSelector)
      && (this.#document.dismissed[id] ?? 0) < tip.revision;
  }

  markup(id: CoachTipId, context: CoachTipContext, availableControls: readonly string[]): string {
    if (!this.isEligible(id, context, availableControls)) return "";
    const tip = definition(id);
    const titleId = `coach-tip-${tip.id}-title`;
    return `<aside class="coach-tip" data-coach-tip="${tip.id}" role="note" aria-labelledby="${titleId}"><div><strong id="${titleId}">${escapeHtml(tip.title)}</strong><p>${escapeHtml(tip.body)}</p></div><div class="coach-tip-actions"><button class="text-button" type="button" data-coach-tip-dismiss="${tip.id}">Got it</button><button class="text-button" type="button" data-coach-tip-dismiss-all>Dismiss all tips</button></div></aside>`;
  }

  settingsMarkup(): string {
    const remaining = coachTipCatalog.filter((tip) => (this.#document.dismissed[tip.id] ?? 0) < tip.revision).length;
    return `<details class="settings-disclosure coach-tip-settings"><summary><span><strong>Coach tips</strong><small>${remaining === 0 ? "Hidden" : `${remaining} available`}</small></span></summary><div><p>Tips appear only beside a relevant control. OSL stores only your explicit dismissals in the encrypted profile.</p><div class="settings-actions"><button class="button" type="button" data-coach-tip-dismiss-all${remaining === 0 ? " disabled" : ""}>Dismiss all tips</button><button class="button" type="button" data-coach-tip-reset>Reset coach tips</button></div></div></details>`;
  }

  async dismiss(id: CoachTipId): Promise<void> {
    const tip = definition(id);
    const next: CoachTipProfileDocument = {
      ...this.#document,
      dismissed: { ...this.#document.dismissed, [id]: tip.revision },
    };
    await this.#persist(next);
    this.#document = next;
  }

  async dismissAll(): Promise<void> {
    const next: CoachTipProfileDocument = {
      ...this.#document,
      dismissed: Object.fromEntries(coachTipCatalog.map((tip) => [tip.id, tip.revision])),
    };
    await this.#persist(next);
    this.#document = next;
  }

  async reset(): Promise<void> {
    const next = emptyDocument();
    await this.#persist(next);
    this.#document = next;
  }

  async #persist(document: CoachTipProfileDocument): Promise<void> {
    await this.#store.setItem(ENCRYPTED_PROFILE_UI_STATE_KEY, serializeDocument(document));
  }
}

/**
 * Build a profile store over the app's AES-256-GCM encrypted profile backend.
 * The generic logical key avoids exposing coach-tip vocabulary to local or
 * sync observers; only the authenticated ciphertext envelope is transported.
 */
export async function encryptedCoachTipProfileStore(
  storage: Pick<Storage, "getItem" | "setItem">,
  rawProfileKey: Uint8Array,
  randomBytes?: (bytes: Uint8Array) => Uint8Array,
): Promise<EncryptedProfileStateStore> {
  const key = await SecureLocalStore.importRawKey(rawProfileKey);
  return new SecureLocalStore({
    storage,
    key,
    namespace: "osl-encrypted-profile-v1",
    randomBytes,
  });
}

/** Native profile storage. Tauri IPC stays on-device; the native side seals
 * this document before disk or profile-transfer bytes are produced. */
export function nativeCoachTipProfileStore(invoke: NativeInvoke): EncryptedProfileStateStore {
  return {
    getItem: async () => JSON.stringify(await invoke<unknown>("get_coach_tip_state")),
    setItem: async (_logicalKey, value) => {
      const state: unknown = JSON.parse(value);
      await invoke<unknown>("save_coach_tip_state", { state });
    },
  };
}

let activeController: CoachTipController | null = null;

export async function configureCoachTips(store: EncryptedProfileStateStore): Promise<void> {
  const controller = new CoachTipController(store);
  await controller.load();
  activeController = controller;
}

export function coachTipMarkup(
  id: CoachTipId,
  context: CoachTipContext,
  availableControls: readonly string[],
): string {
  return activeController?.markup(id, context, availableControls) ?? "";
}

export function coachTipSettingsMarkup(): string {
  return activeController?.settingsMarkup() ?? "";
}

export function bindCoachTipControls(root: Pick<Document, "querySelectorAll">, rerender: () => void): void {
  root.querySelectorAll<HTMLButtonElement>("[data-coach-tip-dismiss]").forEach((button) => {
    button.addEventListener("click", () => {
      const id = button.dataset.coachTipDismiss as CoachTipId;
      void activeController?.dismiss(id).then(rerender).catch(() => undefined);
    });
  });
  root.querySelectorAll<HTMLButtonElement>("[data-coach-tip-dismiss-all]").forEach((button) => {
    button.addEventListener("click", () => { void activeController?.dismissAll().then(rerender).catch(() => undefined); });
  });
  root.querySelectorAll<HTMLButtonElement>("[data-coach-tip-reset]").forEach((button) => {
    button.addEventListener("click", () => { void activeController?.reset().then(rerender).catch(() => undefined); });
  });
}
