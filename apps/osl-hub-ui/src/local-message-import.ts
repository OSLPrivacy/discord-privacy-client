import type { LocalMessageCandidate } from "./adapters";

export const LOCAL_MESSAGE_IMPORT_MAX_BYTES = 8 * 1024 * 1024;
export const LOCAL_MESSAGE_IMPORT_MAX_CANDIDATES = 2_000;
export const LOCAL_MESSAGE_IMPORT_MAX_TEXT_BYTES = 8 * 1024;

export interface LocalMessageImportContext {
  serviceId: string;
  accountId: string;
  conversationId?: string;
}

/**
 * Parse a user-selected, in-memory export for the device-local privacy scan.
 *
 * This helper is deliberately pure: it does not persist the export, invoke the
 * native broker, or perform network requests. `null` means the export or its
 * context failed closed validation.
 */
export function importLocalMessageExport(
  input: string,
  context: LocalMessageImportContext,
): LocalMessageCandidate[] | null {
  if (!boundedUtf8(input, LOCAL_MESSAGE_IMPORT_MAX_BYTES, true)
    || !validServiceId(context.serviceId)
    || !validIdentifier(context.accountId, 128)) return null;

  const defaultConversationId = context.conversationId ?? "local-import";
  if (!validIdentifier(defaultConversationId, 256)) return null;

  const trimmed = input.trim();
  if (trimmed.startsWith("[")) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(trimmed);
    } catch {
      return null;
    }
    return importJsonArray(parsed, context.serviceId, context.accountId, defaultConversationId);
  }

  const lines = input.split(/\r?\n/u).filter((line) => line.trim().length > 0);
  if (lines.length > LOCAL_MESSAGE_IMPORT_MAX_CANDIDATES) return null;

  const candidates: LocalMessageCandidate[] = [];
  for (let index = 0; index < lines.length; index += 1) {
    const text = lines[index];
    if (!boundedMessageText(text)) return null;
    candidates.push(candidate(
      context.serviceId,
      context.accountId,
      defaultConversationId,
      generatedLocator(index),
      false,
      null,
      text,
    ));
  }
  return candidates;
}

function importJsonArray(
  parsed: unknown,
  serviceId: string,
  accountId: string,
  defaultConversationId: string,
): LocalMessageCandidate[] | null {
  if (!Array.isArray(parsed) || parsed.length > LOCAL_MESSAGE_IMPORT_MAX_CANDIDATES) return null;

  const candidates: LocalMessageCandidate[] = [];
  for (let index = 0; index < parsed.length; index += 1) {
    const item = parsed[index];
    if (typeof item === "string") {
      if (!boundedMessageText(item)) return null;
      candidates.push(candidate(
        serviceId,
        accountId,
        defaultConversationId,
        generatedLocator(index),
        false,
        null,
        item,
      ));
      continue;
    }
    if (!isRecord(item)) return null;

    const text = firstText(item);
    const conversationId = item.conversationId ?? defaultConversationId;
    const messageLocator = item.messageLocator ?? generatedLocator(index);
    if (!boundedMessageText(text)
      || !validIdentifier(conversationId, 256)
      || !validIdentifier(messageLocator, 256)
      || !(item.authoredBySelf === undefined || typeof item.authoredBySelf === "boolean")
      || !validCreatedAt(item.createdAtUnixMs)) return null;

    candidates.push(candidate(
      serviceId,
      accountId,
      conversationId,
      messageLocator,
      item.authoredBySelf === true,
      typeof item.createdAtUnixMs === "number" ? item.createdAtUnixMs : null,
      text,
    ));
  }
  return candidates;
}

function candidate(
  serviceId: string,
  accountId: string,
  conversationId: string,
  messageLocator: string,
  authoredBySelf: boolean,
  createdAtUnixMs: number | null,
  text: string,
): LocalMessageCandidate {
  return { serviceId, accountId, conversationId, messageLocator, authoredBySelf, createdAtUnixMs, text };
}

function firstText(item: Record<string, unknown>): unknown {
  if (typeof item.text === "string") return item.text;
  if (typeof item.content === "string") return item.content;
  return item.message;
}

function generatedLocator(index: number): string {
  return `local-import-${index + 1}`;
}

function validCreatedAt(value: unknown): boolean {
  return value === undefined
    || (typeof value === "number" && Number.isSafeInteger(value) && value >= 0);
}

function boundedMessageText(value: unknown): value is string {
  return typeof value === "string"
    && value.trim().length > 0
    && boundedUtf8(value, LOCAL_MESSAGE_IMPORT_MAX_TEXT_BYTES, false);
}

function validServiceId(value: unknown): value is string {
  return typeof value === "string" && value.length <= 32 && /^[a-z0-9_-]+$/u.test(value);
}

function validIdentifier(value: unknown, maxLength: number): value is string {
  return typeof value === "string"
    && value.length > 0
    && value.length <= maxLength
    && !/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/u.test(value);
}

function boundedUtf8(value: string, maxBytes: number, allowEmpty: boolean): boolean {
  return (allowEmpty || value.length > 0) && new TextEncoder().encode(value).length <= maxBytes;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
