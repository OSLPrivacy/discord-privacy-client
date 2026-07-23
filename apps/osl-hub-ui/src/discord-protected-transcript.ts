import "./discord-protected-transcript.css";

export type DiscordTranscriptTheme = "discord-dark" | "discord-light" | "system";
export type DiscordTranscriptDensity = "cozy" | "compact";

export interface VerifiedOslTranscriptIdentity {
  id: string;
  displayName: string;
  avatarUrl?: string;
  avatarFallback?: string;
  provenance: "verified-osl";
}

export interface DiscordTranscriptTimestamp {
  epochMs: number;
  label: string;
}

export interface DiscordTranscriptMediaCard {
  key: string;
  kind: "image" | "video" | "audio" | "file";
  label: string;
  detail?: string;
  state: "available" | "pending" | "expired";
  action?: DiscordTranscriptAction;
}

export interface DiscordTranscriptAction {
  id: string;
  label: string;
  disabled?: boolean;
}

interface DiscordTranscriptRowBase {
  key: string;
  author: VerifiedOslTranscriptIdentity;
  timestamp: DiscordTranscriptTimestamp;
  direction?: "incoming" | "outgoing";
}

export interface DiscordTranscriptTextRow extends DiscordTranscriptRowBase {
  kind: "text";
  plaintext: string;
  plaintextHidden?: boolean;
  receipt?: { status: "sent" | "received" | "opened" | "expired"; label: string };
  media?: readonly DiscordTranscriptMediaCard[];
}

export interface DiscordTranscriptReplyRow extends DiscordTranscriptRowBase {
  kind: "reply";
  plaintext: string;
  plaintextHidden?: boolean;
  receipt?: { status: "sent" | "received" | "opened" | "expired"; label: string };
  replyTo: {
    author: VerifiedOslTranscriptIdentity;
    excerpt: string;
  };
  media?: readonly DiscordTranscriptMediaCard[];
}

export interface DiscordTranscriptReceiptRow extends DiscordTranscriptRowBase {
  kind: "receipt";
  status: "sent" | "received" | "opened" | "expired";
  label: string;
  action?: DiscordTranscriptAction;
}

export type DiscordProtectedTranscriptRow =
  | DiscordTranscriptTextRow
  | DiscordTranscriptReplyRow
  | DiscordTranscriptReceiptRow;

export interface DiscordProtectedTranscriptWindow {
  /** The already-virtualized slice. This renderer never reads Discord history. */
  rows: readonly DiscordProtectedTranscriptRow[];
  startIndex: number;
  totalRowCount: number;
  beforePx?: number;
  afterPx?: number;
}

export interface DiscordProtectedTranscriptPreferences {
  theme: DiscordTranscriptTheme;
  density: DiscordTranscriptDensity;
  zoom: number;
}

export interface DiscordProtectedTranscriptOptions {
  window: DiscordProtectedTranscriptWindow;
  preferences: DiscordProtectedTranscriptPreferences;
  ariaLabel?: string;
  onAction?: (actionId: string, rowKey: string) => void;
  document?: Document;
}

export interface DiscordProtectedTranscriptController {
  readonly root: HTMLElement;
  updateWindow(window: DiscordProtectedTranscriptWindow): void;
  updatePreferences(preferences: DiscordProtectedTranscriptPreferences): void;
  focusRow(key: string): boolean;
  destroy(): void;
}

const MAX_VISIBLE_ROWS = 500;
const MAX_TEXT_LENGTH = 100_000;
const MAX_LABEL_LENGTH = 512;

function assertBoundedText(value: string, label: string, maximum: number): void {
  if (typeof value !== "string" || value.length > maximum || /[\u0000]/u.test(value)) {
    throw new Error(`Invalid ${label}`);
  }
}

function assertIdentity(identity: VerifiedOslTranscriptIdentity): void {
  if (identity.provenance !== "verified-osl") throw new Error("Transcript identity is not verified by OSL");
  assertBoundedText(identity.id, "identity id", 256);
  assertBoundedText(identity.displayName, "display name", 256);
  if (!identity.id || !identity.displayName) throw new Error("Transcript identity is incomplete");
  if (identity.avatarFallback !== undefined) assertBoundedText(identity.avatarFallback, "avatar fallback", 8);
  if (identity.avatarUrl !== undefined) {
    assertBoundedText(identity.avatarUrl, "avatar URL", 8_192);
    if (!/^data:image\/(?:png|jpeg|webp);base64,[A-Za-z0-9+/]+={0,2}$/u.test(identity.avatarUrl)) {
      throw new Error("Transcript avatar must be an embedded verified OSL image");
    }
  }
}

function assertTimestamp(timestamp: DiscordTranscriptTimestamp): void {
  if (!Number.isFinite(timestamp.epochMs) || Math.abs(timestamp.epochMs) > 8_640_000_000_000_000) {
    throw new Error("Invalid transcript timestamp");
  }
  assertBoundedText(timestamp.label, "timestamp label", 128);
}

function assertRow(row: DiscordProtectedTranscriptRow): void {
  assertBoundedText(row.key, "row key", 256);
  if (!row.key) throw new Error("Transcript row key is required");
  assertIdentity(row.author);
  assertTimestamp(row.timestamp);
  if (row.kind === "receipt") {
    assertBoundedText(row.label, "receipt label", MAX_LABEL_LENGTH);
    return;
  }
  assertBoundedText(row.plaintext, "plaintext", MAX_TEXT_LENGTH);
  if (row.kind === "reply") {
    assertIdentity(row.replyTo.author);
    assertBoundedText(row.replyTo.excerpt, "reply excerpt", 2_000);
  }
  for (const card of row.media ?? []) {
    assertBoundedText(card.key, "media key", 256);
    assertBoundedText(card.label, "media label", MAX_LABEL_LENGTH);
    if (card.detail !== undefined) assertBoundedText(card.detail, "media detail", MAX_LABEL_LENGTH);
    if (card.action) {
      assertBoundedText(card.action.id, "media action id", 256);
      assertBoundedText(card.action.label, "media action label", 128);
    }
  }
}

export function normalizeDiscordTranscriptPreferences(
  preferences: DiscordProtectedTranscriptPreferences,
): DiscordProtectedTranscriptPreferences {
  const themes: readonly DiscordTranscriptTheme[] = ["discord-dark", "discord-light", "system"];
  const densities: readonly DiscordTranscriptDensity[] = ["cozy", "compact"];
  if (!themes.includes(preferences.theme) || !densities.includes(preferences.density)
    || !Number.isFinite(preferences.zoom)) throw new Error("Invalid transcript preferences");
  return { ...preferences, zoom: Math.min(2, Math.max(0.75, preferences.zoom)) };
}

export function validateDiscordTranscriptWindow(window: DiscordProtectedTranscriptWindow): void {
  if (!Number.isSafeInteger(window.startIndex) || window.startIndex < 0
    || !Number.isSafeInteger(window.totalRowCount) || window.totalRowCount < window.rows.length
    || window.startIndex + window.rows.length > window.totalRowCount
    || window.rows.length > MAX_VISIBLE_ROWS
    || !Number.isFinite(window.beforePx ?? 0) || (window.beforePx ?? 0) < 0
    || !Number.isFinite(window.afterPx ?? 0) || (window.afterPx ?? 0) < 0) {
    throw new Error("Invalid transcript window");
  }
  const keys = new Set<string>();
  for (const row of window.rows) {
    assertRow(row);
    if (keys.has(row.key)) throw new Error("Duplicate transcript row key");
    keys.add(row.key);
  }
}

function element(document: Document, tag: string, className?: string): HTMLElement {
  const result = document.createElement(tag);
  if (className) result.className = className;
  return result;
}

function appendText(document: Document, parent: Element, tag: string, className: string, value: string): HTMLElement {
  const result = element(document, tag, className);
  // Deliberately literal: markdown and HTML-like text never become markup.
  result.textContent = value;
  parent.append(result);
  return result;
}

function avatar(document: Document, identity: VerifiedOslTranscriptIdentity): HTMLElement {
  const shell = element(document, "span", "osl-discord-transcript__avatar");
  shell.setAttribute("aria-hidden", "true");
  if (identity.avatarUrl) {
    const image = document.createElement("img");
    image.alt = "";
    image.decoding = "async";
    image.loading = "lazy";
    image.referrerPolicy = "no-referrer";
    image.src = identity.avatarUrl;
    shell.append(image);
  } else {
    shell.textContent = identity.avatarFallback || Array.from(identity.displayName.trim())[0] || "?";
  }
  return shell;
}

function timestamp(document: Document, value: DiscordTranscriptTimestamp): HTMLTimeElement {
  const time = document.createElement("time");
  time.className = "osl-discord-transcript__time";
  time.dateTime = new Date(value.epochMs).toISOString();
  time.textContent = value.label;
  return time;
}

function mediaCards(document: Document, cards: readonly DiscordTranscriptMediaCard[]): HTMLElement {
  const list = element(document, "ul", "osl-discord-transcript__media");
  list.setAttribute("aria-label", "Protected attachments");
  for (const card of cards) {
    const item = element(document, "li", "osl-discord-transcript__media-card");
    item.dataset.mediaKey = card.key;
    item.dataset.mediaKind = card.kind;
    item.dataset.mediaState = card.state;
    appendText(document, item, "strong", "osl-discord-transcript__media-label", card.label);
    if (card.detail) appendText(document, item, "span", "osl-discord-transcript__media-detail", card.detail);
    appendText(document, item, "span", "osl-discord-transcript__media-state", card.state);
    if (card.action) {
      const action = document.createElement("button");
      action.type = "button";
      action.className = "osl-discord-transcript__action";
      action.dataset.transcriptAction = card.action.id;
      action.disabled = card.action.disabled ?? false;
      action.textContent = card.action.label;
      item.append(action);
    }
    list.append(item);
  }
  return list;
}

function renderRow(document: Document, row: DiscordProtectedTranscriptRow, position: number, total: number): HTMLLIElement {
  const item = document.createElement("li");
  item.className = `osl-discord-transcript__row osl-discord-transcript__row--${row.kind}`;
  item.dataset.rowKey = row.key;
  item.dataset.rowKind = row.kind;
  if (row.direction) item.dataset.direction = row.direction;
  item.tabIndex = -1;
  item.setAttribute("aria-posinset", String(position));
  item.setAttribute("aria-setsize", String(total));
  item.append(avatar(document, row.author));

  const content = element(document, "article", "osl-discord-transcript__content");
  content.setAttribute("aria-label", `${row.author.displayName}, ${row.timestamp.label}`);
  if (row.kind === "reply") {
    const reply = element(document, "div", "osl-discord-transcript__reply");
    appendText(document, reply, "strong", "osl-discord-transcript__reply-author", row.replyTo.author.displayName);
    appendText(document, reply, "span", "osl-discord-transcript__reply-excerpt", row.replyTo.excerpt);
    content.append(reply);
  }
  const meta = element(document, "div", "osl-discord-transcript__meta");
  appendText(document, meta, "strong", "osl-discord-transcript__author", row.author.displayName);
  meta.append(timestamp(document, row.timestamp));
  content.append(meta);
  if (row.kind === "receipt") {
    const receipt = appendText(document, content, "p", "osl-discord-transcript__receipt", row.label);
    receipt.dataset.receiptStatus = row.status;
    if (row.action) {
      const action = document.createElement("button");
      action.type = "button";
      action.className = "osl-discord-transcript__action";
      action.dataset.transcriptAction = row.action.id;
      action.disabled = row.action.disabled ?? false;
      action.textContent = row.action.label;
      content.append(action);
    }
  } else {
    const plaintext = appendText(document, content, "p", "osl-discord-transcript__plaintext", row.plaintext);
    plaintext.hidden = row.plaintextHidden ?? false;
    if (row.receipt) {
      const receipt = appendText(document, content, "p", "osl-discord-transcript__receipt", row.receipt.label);
      receipt.dataset.receiptStatus = row.receipt.status;
    }
    if (row.media?.length) content.append(mediaCards(document, row.media));
  }
  item.append(content);
  return item;
}

function signature(row: DiscordProtectedTranscriptRow): string {
  return JSON.stringify(row);
}

export function createDiscordProtectedTranscript(
  options: DiscordProtectedTranscriptOptions,
): DiscordProtectedTranscriptController {
  const document = options.document ?? globalThis.document;
  if (!document) throw new Error("A document is required to render the transcript");
  validateDiscordTranscriptWindow(options.window);
  const root = element(document, "section", "osl-discord-transcript");
  root.setAttribute("aria-label", options.ariaLabel || "OSL protected Discord transcript");
  const viewport = element(document, "div", "osl-discord-transcript__viewport");
  viewport.setAttribute("role", "log");
  viewport.setAttribute("aria-live", "polite");
  viewport.setAttribute("aria-relevant", "additions text");
  const list = document.createElement("ol");
  list.className = "osl-discord-transcript__list";
  viewport.append(list);
  root.append(viewport);
  root.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const action = target.closest<HTMLElement>("[data-transcript-action]");
    const row = action?.closest<HTMLElement>("[data-row-key]");
    if (action?.dataset.transcriptAction && row?.dataset.rowKey && !action.matches(":disabled")) {
      options.onAction?.(action.dataset.transcriptAction, row.dataset.rowKey);
    }
  });

  const nodes = new Map<string, { node: HTMLLIElement; signature: string }>();

  function updatePreferences(preferences: DiscordProtectedTranscriptPreferences): void {
    const normalized = normalizeDiscordTranscriptPreferences(preferences);
    root.dataset.theme = normalized.theme;
    root.dataset.density = normalized.density;
    root.style.setProperty("--osl-discord-transcript-zoom", String(normalized.zoom));
  }

  function spacer(className: string, pixels: number): HTMLLIElement | null {
    if (pixels <= 0) return null;
    const node = document.createElement("li");
    node.className = className;
    node.style.blockSize = `${pixels}px`;
    node.setAttribute("aria-hidden", "true");
    return node;
  }

  function updateWindow(window: DiscordProtectedTranscriptWindow): void {
    validateDiscordTranscriptWindow(window);
    const nextKeys = new Set(window.rows.map((row) => row.key));
    for (const key of nodes.keys()) if (!nextKeys.has(key)) nodes.delete(key);
    const children: Node[] = [];
    const before = spacer("osl-discord-transcript__spacer", window.beforePx ?? 0);
    if (before) children.push(before);
    window.rows.forEach((row, offset) => {
      const nextSignature = signature(row);
      const current = nodes.get(row.key);
      let entry = current;
      if (!entry) {
        entry = { node: renderRow(document, row, window.startIndex + offset + 1, window.totalRowCount), signature: nextSignature };
      } else if (entry.signature !== nextSignature) {
        const updated = renderRow(document, row, window.startIndex + offset + 1, window.totalRowCount);
        entry.node.className = updated.className;
        for (const name of [...entry.node.getAttributeNames()]) entry.node.removeAttribute(name);
        for (const name of updated.getAttributeNames()) entry.node.setAttribute(name, updated.getAttribute(name) ?? "");
        entry.node.replaceChildren(...updated.childNodes);
        entry.signature = nextSignature;
      }
      entry.node.setAttribute("aria-posinset", String(window.startIndex + offset + 1));
      entry.node.setAttribute("aria-setsize", String(window.totalRowCount));
      nodes.set(row.key, entry);
      children.push(entry.node);
    });
    const after = spacer("osl-discord-transcript__spacer", window.afterPx ?? 0);
    if (after) children.push(after);
    list.replaceChildren(...children);
  }

  updatePreferences(options.preferences);
  updateWindow(options.window);
  return {
    root,
    updateWindow,
    updatePreferences,
    focusRow(key: string): boolean {
      const node = nodes.get(key)?.node;
      if (!node) return false;
      node.focus({ preventScroll: true });
      node.scrollIntoView({ block: "nearest" });
      return true;
    },
    destroy(): void {
      nodes.clear();
      root.replaceChildren();
      root.remove();
    },
  };
}
