export type ChannelPermissionAnswer = "allow" | "deny";

export interface EnclaveChannelIdentity {
  readonly channelId: string;
  readonly name: string;
  readonly topic: string;
  readonly position: number;
  readonly categoryId: string;
  readonly categoryName: string;
}

export interface EnclaveChannelPermission {
  readonly key: string;
  readonly label: string;
  readonly categoryAnswer: ChannelPermissionAnswer;
}

export interface EnclaveChannelPermissionEditorSnapshot {
  readonly channel: EnclaveChannelIdentity;
  readonly permissions: readonly EnclaveChannelPermission[];
  readonly overrides: Readonly<Record<string, ChannelPermissionAnswer>>;
}

export interface EnclaveChannelPermissionSyncState {
  readonly kind: "synced" | "changed";
  readonly overrideCount: number;
  readonly label: string;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function validOverrides(
  permissions: readonly EnclaveChannelPermission[],
  overrides: Readonly<Record<string, ChannelPermissionAnswer>>,
): Record<string, ChannelPermissionAnswer> {
  const permissionByKey = new Map(permissions.map((permission) => [permission.key, permission]));
  const result: Record<string, ChannelPermissionAnswer> = {};

  for (const [key, answer] of Object.entries(overrides)) {
    const permission = permissionByKey.get(key);
    if (permission && answer !== permission.categoryAnswer) result[key] = answer;
  }

  return result;
}

export function channelPermissionSyncState(
  snapshot: EnclaveChannelPermissionEditorSnapshot,
): EnclaveChannelPermissionSyncState {
  const overrideCount = Object.keys(validOverrides(snapshot.permissions, snapshot.overrides)).length;
  if (overrideCount === 0) {
    return { kind: "synced", overrideCount, label: "Synced with category" };
  }

  const noun = overrideCount === 1 ? "permission" : "permissions";
  return {
    kind: "changed",
    overrideCount,
    label: `Changed from category · ${overrideCount} ${noun}`,
  };
}

/**
 * Owns only the relationship between a category's answers and one channel's
 * explicit answers. Channel identity is deliberately copied through every
 * permission operation unchanged.
 */
export class EnclaveChannelPermissionEditor {
  readonly #channel: EnclaveChannelIdentity;
  #permissions: EnclaveChannelPermission[];
  #overrides: Record<string, ChannelPermissionAnswer>;

  constructor(snapshot: EnclaveChannelPermissionEditorSnapshot) {
    this.#channel = { ...snapshot.channel };
    this.#permissions = snapshot.permissions.map((permission) => ({ ...permission }));
    this.#overrides = validOverrides(this.#permissions, snapshot.overrides);
  }

  snapshot(): EnclaveChannelPermissionEditorSnapshot {
    return {
      channel: { ...this.#channel },
      permissions: this.#permissions.map((permission) => ({ ...permission })),
      overrides: { ...this.#overrides },
    };
  }

  syncState(): EnclaveChannelPermissionSyncState {
    return channelPermissionSyncState(this.snapshot());
  }

  setChannelAnswer(key: string, answer: ChannelPermissionAnswer): void {
    const permission = this.#permissions.find((candidate) => candidate.key === key);
    if (!permission) throw new Error(`Unknown channel permission: ${key}`);

    if (answer === permission.categoryAnswer) delete this.#overrides[key];
    else this.#overrides[key] = answer;
  }

  matchCategory(): void {
    this.#overrides = {};
  }

  updateCategoryAnswers(answers: Readonly<Record<string, ChannelPermissionAnswer>>): void {
    this.#permissions = this.#permissions.map((permission) => ({
      ...permission,
      categoryAnswer: answers[permission.key] ?? permission.categoryAnswer,
    }));
    this.#overrides = validOverrides(this.#permissions, this.#overrides);
  }
}

export function enclaveChannelPermissionEditorMarkup(
  snapshot: EnclaveChannelPermissionEditorSnapshot,
): string {
  const editor = new EnclaveChannelPermissionEditor(snapshot);
  const normalized = editor.snapshot();
  const sync = editor.syncState();
  const rows = normalized.permissions.map((permission) => {
    const answer = normalized.overrides[permission.key] ?? permission.categoryAnswer;
    const source = normalized.overrides[permission.key] ? "Channel override" : "Category";
    return `<li class="enclave-channel-permission-row" data-permission-key="${escapeHtml(permission.key)}" data-permission-source="${normalized.overrides[permission.key] ? "channel" : "category"}"><span><strong>${escapeHtml(permission.label)}</strong><small>${source}: ${answer === "allow" ? "Allow" : "Deny"}</small></span><span class="enclave-channel-permission-choices" role="group" aria-label="${escapeHtml(permission.label)}"><button class="text-button" type="button" data-permission-answer="allow" aria-pressed="${answer === "allow"}">Allow</button><button class="text-button" type="button" data-permission-answer="deny" aria-pressed="${answer === "deny"}">Deny</button></span></li>`;
  }).join("");

  return `<section class="enclave-channel-permissions" data-channel-id="${escapeHtml(normalized.channel.channelId)}" data-category-id="${escapeHtml(normalized.channel.categoryId)}" data-channel-position="${normalized.channel.position}" data-permission-sync="${sync.kind}" data-override-count="${sync.overrideCount}" aria-labelledby="channel-permissions-heading"><header><span><h2 id="channel-permissions-heading">#${escapeHtml(normalized.channel.name)} permissions</h2><p>${escapeHtml(normalized.channel.topic)}</p><small>Category: ${escapeHtml(normalized.channel.categoryName)} · Position ${normalized.channel.position}</small></span><span class="enclave-channel-sync-actions"><strong class="enclave-channel-sync-label" role="status" aria-live="polite">${sync.label}</strong><button class="button secondary" type="button" data-match-category ${sync.overrideCount === 0 ? "disabled" : ""}>Match category</button></span></header><ul class="enclave-channel-permission-list">${rows}</ul></section>`;
}

export interface ChannelPermissionEditorRoot {
  innerHTML: string;
  addEventListener(type: "click", listener: (event: Event) => void): void;
}

/** Mount the editor with event delegation so each state change is announced immediately. */
export function mountEnclaveChannelPermissionEditor(
  root: ChannelPermissionEditorRoot,
  initial: EnclaveChannelPermissionEditorSnapshot,
): EnclaveChannelPermissionEditor {
  const editor = new EnclaveChannelPermissionEditor(initial);
  const render = () => { root.innerHTML = enclaveChannelPermissionEditorMarkup(editor.snapshot()); };

  root.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;

    if (target.closest("[data-match-category]")) {
      editor.matchCategory();
      render();
      return;
    }

    const answerButton = target.closest<HTMLElement>("[data-permission-answer]");
    const permissionRow = answerButton?.closest<HTMLElement>("[data-permission-key]");
    const answer = answerButton?.dataset.permissionAnswer;
    const key = permissionRow?.dataset.permissionKey;
    if (key && (answer === "allow" || answer === "deny")) {
      editor.setChannelAnswer(key, answer);
      render();
    }
  });

  render();
  return editor;
}
