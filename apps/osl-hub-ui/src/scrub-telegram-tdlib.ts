import type { NarrowTdlibClient, TelegramMessage } from "./scrub-telegram-adapter";

export const TELEGRAM_TDLIB_CAPABILITY = Object.freeze({
  deletionEnabledByDefault: false,
  clientBinaryPackaged: false,
  phoneSessionAuthenticationRequired: true,
  verification: "live-account-and-packaged-tdlib-required" as const,
  historyCoverage: "bounded-to-1000-newest-messages-per-chat" as const,
});

type TdlibRequest =
  | { "@type": "getAuthorizationState" }
  | { "@type": "getChatHistory"; chat_id: string; from_message_id: string; offset: 0; limit: number; only_local: false }
  | { "@type": "getMessage"; chat_id: string; message_id: string }
  | { "@type": "deleteMessages"; chat_id: string; message_ids: readonly string[]; revoke: true };

/**
 * A packaged TDLib owner implements this with td_json_client_send/receive (or
 * an equivalent maintained binding). The union deliberately excludes auth
 * credential submission and every non-Scrub TDLib method.
 */
export interface TdlibDeleteOnlyJsonClient {
  send(request: TdlibRequest): Promise<unknown>;
}

export interface TdlibMessageJson {
  "@type": "message";
  id: string | number;
  chat_id: string | number;
  date: number;
  is_outgoing: boolean;
  can_be_deleted_for_all_users: boolean;
  content: unknown;
}

export type TdlibContentFingerprint = (message: TdlibMessageJson) => string;

function object(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function int64(value: unknown): string | null {
  if (typeof value === "string" && /^-?[0-9]{1,20}$/.test(value)) return value;
  if (typeof value === "number" && Number.isSafeInteger(value)) return String(value);
  return null;
}

function tdError(value: unknown): Error | null {
  const item = object(value);
  if (item?.["@type"] !== "error") return null;
  const code = typeof item.code === "number" ? item.code : 0;
  const message = typeof item.message === "string" ? item.message : "TDLib returned an unstructured error";
  return new Error(`TDLib ${code}: ${message}`);
}

export class TdlibDeleteOnlyClient implements NarrowTdlibClient {
  readonly #client: TdlibDeleteOnlyJsonClient;
  readonly #fingerprint: TdlibContentFingerprint;

  constructor(client: TdlibDeleteOnlyJsonClient, fingerprint: TdlibContentFingerprint) {
    this.#client = client;
    this.#fingerprint = fingerprint;
  }

  async #send(request: TdlibRequest): Promise<unknown> {
    const result = await this.#client.send(request);
    const error = tdError(result);
    if (error !== null) throw error;
    return result;
  }

  async #requireReady(): Promise<void> {
    const state = object(await this.#send({ "@type": "getAuthorizationState" }));
    if (state?.["@type"] !== "authorizationStateReady") throw new Error("TDLib phone session is not authorizationStateReady");
  }

  #message(value: unknown, expectedChatId: string, expectedMessageId?: string): TelegramMessage {
    const item = object(value);
    const id = int64(item?.id), chatId = int64(item?.chat_id);
    if (item?.["@type"] !== "message" || id === null || chatId !== expectedChatId || (expectedMessageId !== undefined && id !== expectedMessageId)
      || !Number.isSafeInteger(item.date) || (item.date as number) < 0
      || typeof item.is_outgoing !== "boolean"
      || typeof item.can_be_deleted_for_all_users !== "boolean") throw new Error("TDLib message schema drift");
    const fingerprint = this.#fingerprint(item as unknown as TdlibMessageJson);
    if (fingerprint.length === 0 || fingerprint.length > 512) throw new Error("invalid local TDLib content fingerprint");
    return { id, chatId, authoredBySelf: item.is_outgoing, createdAtUnixMs: (item.date as number) * 1_000, contentFingerprint: fingerprint, canBeDeletedForAllUsers: item.can_be_deleted_for_all_users };
  }

  async getChatHistory(chatId: string, beforeUnixMs: number): Promise<readonly TelegramMessage[]> {
    await this.#requireReady();
    const messages: TelegramMessage[] = [];
    const seen = new Set<string>();
    let cursor = "0";
    for (let page = 0; page < 10; page += 1) {
      const result = object(await this.#send({ "@type": "getChatHistory", chat_id: chatId, from_message_id: cursor, offset: 0, limit: 100, only_local: false }));
      if (result?.["@type"] !== "messages" || !Array.isArray(result.messages)) throw new Error("TDLib getChatHistory schema drift");
      const parsed = result.messages.filter((item) => item !== null).map((item) => this.#message(item, chatId));
      if (parsed.length === 0) break;
      let added = 0;
      for (const item of parsed) {
        if (seen.has(item.id)) continue;
        seen.add(item.id);
        messages.push(item);
        added += 1;
      }
      const nextCursor = parsed.at(-1)?.id;
      if (added === 0 || nextCursor === undefined || nextCursor === cursor) break;
      cursor = nextCursor;
    }
    return messages.filter((item) => item.createdAtUnixMs < beforeUnixMs);
  }

  async getMessage(chatId: string, messageId: string): Promise<TelegramMessage | null> {
    await this.#requireReady();
    return this.#message(await this.#send({ "@type": "getMessage", chat_id: chatId, message_id: messageId }), chatId, messageId);
  }

  async deleteMessages(chatId: string, messageIds: readonly string[], revoke: true): Promise<void> {
    await this.#requireReady();
    const result = object(await this.#send({ "@type": "deleteMessages", chat_id: chatId, message_ids: messageIds, revoke }));
    if (result?.["@type"] !== "ok") throw new Error("TDLib deleteMessages schema drift");
  }
}
