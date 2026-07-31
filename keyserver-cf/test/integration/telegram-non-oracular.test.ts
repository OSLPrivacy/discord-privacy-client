import { env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import { handleTelegramWebhook } from "../../src/endpoints/telegram.js";

const WEBHOOK_SECRET = "telegram-webhook-secret";
const BOT_TOKEN = "1234567890:abcdefghijklmnopqrstuvwxyzABCDE";
const AUTHORIZED_CHAT_ID = "-1001234567890";
const UNAUTHORIZED_CHAT_ID = "99112233";

function configuredEnv(): Env {
  return {
    DB: env.DB,
    TELEGRAM_WEBHOOK_SECRET: WEBHOOK_SECRET,
    TELEGRAM_BOT_TOKEN: BOT_TOKEN,
    TELEGRAM_OPERATOR_CHAT_IDS: AUTHORIZED_CHAT_ID,
  } as Env;
}

function commandRequest(chatId: string): Request {
  return new Request("https://keyserver.test/v1/telegram/webhook", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-telegram-bot-api-secret-token": WEBHOOK_SECRET,
    },
    body: JSON.stringify({
      message: {
        text: "/downloads",
        chat: { id: chatId },
      },
    }),
  });
}

function telegramFetcher() {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string"
      ? input
      : input instanceof URL
        ? input.href
        : input.url;
    expect(url).toBe(`https://api.telegram.org/bot${BOT_TOKEN}/sendMessage`);
    return Response.json({ ok: true });
  }) as unknown as typeof fetch;
}

describe("Telegram webhook non-oracular acknowledgement", () => {
  it("telegram'", async () => {
    const unauthorizedFetcher = telegramFetcher();
    const acceptedFetcher = telegramFetcher();

    const unauthorized = await handleTelegramWebhook(
      commandRequest(UNAUTHORIZED_CHAT_ID),
      configuredEnv(),
      unauthorizedFetcher,
    );
    const accepted = await handleTelegramWebhook(
      commandRequest(AUTHORIZED_CHAT_ID),
      configuredEnv(),
      acceptedFetcher,
    );

    expect(unauthorized.status).toBe(200);
    expect(accepted.status).toBe(200);
    expect(unauthorized.headers.get("content-type")).toBe(
      accepted.headers.get("content-type"),
    );
    const [unauthorizedBody, acceptedBody] = await Promise.all([
      unauthorized.text(),
      accepted.text(),
    ]);
    expect(unauthorizedBody).toBe(acceptedBody);
    expect(JSON.parse(unauthorizedBody)).toEqual({ ok: true });
    expect(unauthorizedFetcher).not.toHaveBeenCalled();
    expect(acceptedFetcher).toHaveBeenCalledTimes(1);
  });
});
