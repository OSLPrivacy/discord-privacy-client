import type { Env } from "../env.js";
import { json, serviceUnavailable } from "../lib/http.js";
import {
  handleTelegramCommand,
  telegramReportingIsConfigured,
} from "../lib/telegram.js";

type TelegramVitest = typeof import("vitest");

declare global {
  interface ImportMeta {
    vitest?: Pick<TelegramVitest, "describe" | "expect" | "it" | "vi">;
  }
}

const TELEGRAM_NEUTRAL_ACK = Object.freeze({ ok: true });

export async function handleTelegramWebhook(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  if (!telegramReportingIsConfigured(env)) {
    return serviceUnavailable("Telegram reporting is not configured");
  }
  try {
    await handleTelegramCommand(request, env, fetcher);
  } catch (error) {
    if (!(error instanceof SyntaxError)) throw error;
  }
  // Always acknowledge handled updates without revealing whether any command
  // was authorized. Telegram will not retry an acknowledged update.
  return json(TELEGRAM_NEUTRAL_ACK);
}

if (import.meta.vitest) {
  const { expect, it, vi } = import.meta.vitest;
  const WEBHOOK_SECRET = "telegram-webhook-secret";
  const BOT_TOKEN = "1234567890:abcdefghijklmnopqrstuvwxyzABCDE";
  const OPERATOR_CHAT_ID = "-1001234567890";

  const configuredEnv = (): Env => ({
    DB: {} as D1Database,
    MAILBOX: {} as DurableObjectNamespace<import("../mail/mailbox.js").Mailbox>,
    RATE_LIMIT_5: {} as RateLimit,
    RATE_LIMIT_10: {} as RateLimit,
    RATE_LIMIT_120: {} as RateLimit,
    RATE_LIMIT_1200: {} as RateLimit,
    RATE_LIMIT_3600: {} as RateLimit,
    TELEGRAM_WEBHOOK_SECRET: WEBHOOK_SECRET,
    TELEGRAM_BOT_TOKEN: BOT_TOKEN,
    TELEGRAM_OPERATOR_CHAT_IDS: OPERATOR_CHAT_ID,
  });

  const updateRequest = (
    chatId: string,
    secret: string | null = WEBHOOK_SECRET,
  ): Request => {
    const headers = new Headers({ "content-type": "application/json" });
    if (secret !== null) {
      headers.set("x-telegram-bot-api-secret-token", secret);
    }
    return new Request("https://keyserver.test/v1/telegram/webhook", {
      method: "POST",
      headers,
      body: JSON.stringify({
        message: {
          text: "/unknown",
          chat: { id: chatId },
        },
      }),
    });
  };

  const outboundFetcher = () =>
    vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
      if (url.startsWith(`https://api.telegram.org/bot${BOT_TOKEN}/sendMessage`)) {
        return Response.json({ ok: true });
      }
      throw new Error(`unexpected outbound request: ${url}`);
    }) as unknown as typeof fetch;

  const installNodeTimingSafeEqual = (): (() => void) => {
    if ("timingSafeEqual" in crypto.subtle) {
      return () => {};
    }
    Object.defineProperty(crypto.subtle, "timingSafeEqual", {
      configurable: true,
      value: (left: ArrayBuffer, right: ArrayBuffer): boolean => {
        const leftBytes = new Uint8Array(left);
        const rightBytes = new Uint8Array(right);
        if (leftBytes.byteLength !== rightBytes.byteLength) return false;
        let difference = 0;
        for (let index = 0; index < leftBytes.byteLength; index += 1) {
          difference |= leftBytes[index]! ^ rightBytes[index]!;
        }
        return difference === 0;
      },
    });
    return () => {
      Reflect.deleteProperty(crypto.subtle, "timingSafeEqual");
    };
  };

  it("telegram'", async () => {
    const restoreTimingSafeEqual = installNodeTimingSafeEqual();
    const acceptedFetcher = outboundFetcher();
    const wrongChatFetcher = outboundFetcher();
    const wrongSecretFetcher = outboundFetcher();
    const env = configuredEnv();

    try {
      const [accepted, wrongChat, wrongSecret] = await Promise.all([
        handleTelegramWebhook(updateRequest(OPERATOR_CHAT_ID), env, acceptedFetcher),
        handleTelegramWebhook(updateRequest("99112233"), env, wrongChatFetcher),
        handleTelegramWebhook(
          updateRequest(OPERATOR_CHAT_ID, "wrong-webhook-secret"),
          env,
          wrongSecretFetcher,
        ),
      ]);

      expect(accepted.status).toBe(200);
      expect(wrongChat.status).toBe(200);
      expect(wrongSecret.status).toBe(200);

      const [acceptedBody, wrongChatBody, wrongSecretBody] = await Promise.all([
        accepted.text(),
        wrongChat.text(),
        wrongSecret.text(),
      ]);
      expect(JSON.parse(acceptedBody)).toEqual({ ok: true });
      expect(wrongChatBody).toBe(acceptedBody);
      expect(wrongSecretBody).toBe(acceptedBody);
      expect(acceptedFetcher).toHaveBeenCalledTimes(1);
      expect(wrongChatFetcher).not.toHaveBeenCalled();
      expect(wrongSecretFetcher).not.toHaveBeenCalled();
    } finally {
      restoreTimingSafeEqual();
    }
  });
}
