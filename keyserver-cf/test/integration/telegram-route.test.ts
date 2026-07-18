import { env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import { handleTelegramWebhook } from "../../src/endpoints/telegram.js";
import { notifyTelegramForStripeEvent } from "../../src/lib/telegram.js";

const WEBHOOK_SECRET = "telegram-webhook-secret";
const BOT_TOKEN = "1234567890:abcdefghijklmnopqrstuvwxyzABCDE";
const ADMIN_CHAT_ID = "-1001234567890";
const PRIVATE_CHAT_ONE = "1122334455";
const PRIVATE_CHAT_TWO = "5566778899";
const OPERATOR_CHAT_IDS = `${PRIVATE_CHAT_ONE},${PRIVATE_CHAT_TWO},${ADMIN_CHAT_ID}`;

function configuredEnv(overrides: Partial<Env> = {}): Env {
  return {
    DB: env.DB,
    TELEGRAM_WEBHOOK_SECRET: WEBHOOK_SECRET,
    TELEGRAM_BOT_TOKEN: BOT_TOKEN,
    TELEGRAM_OPERATOR_CHAT_IDS: OPERATOR_CHAT_IDS,
    STRIPE_SECRET_KEY: "sk_live_route_test_only",
    ...overrides,
  } as Env;
}

function updateRequest(
  body: unknown,
  secret: string | null = WEBHOOK_SECRET,
): Request {
  const headers = new Headers({ "content-type": "application/json" });
  if (secret !== null) {
    headers.set("x-telegram-bot-api-secret-token", secret);
  }
  return new Request("https://keyserver.test/v1/telegram/webhook", {
    method: "POST",
    headers,
    body: JSON.stringify(body),
  });
}

function commandRequest(
  command: string,
  chatId: string | number = ADMIN_CHAT_ID,
  secret: string | null = WEBHOOK_SECRET,
): Request {
  return updateRequest({
    message: {
      text: command,
      chat: { id: chatId },
    },
  }, secret);
}

function outboundFetcher() {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string"
      ? input
      : input instanceof URL
        ? input.href
        : input.url;
    if (url === "https://api.stripe.com/v1/balance") {
      return Response.json({
        available: [{ amount: 1250, currency: "usd" }],
        pending: [{ amount: 300, currency: "usd" }],
      });
    }
    if (url.startsWith(`https://api.telegram.org/bot${BOT_TOKEN}/sendMessage`)) {
      return Response.json({ ok: true });
    }
    throw new Error(`unexpected outbound request: ${url}`);
  }) as unknown as typeof fetch;
}

async function responseJson(response: Response): Promise<{
  ok?: boolean;
  result?: string;
  error?: string;
}> {
  return await response.json();
}

describe("Telegram operator webhook route", () => {
  it("fails closed when Telegram reporting is not configured", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/stats"),
      configuredEnv({ TELEGRAM_BOT_TOKEN: undefined }),
      fetcher,
    );

    expect(response.status).toBe(503);
    await expect(responseJson(response)).resolves.toEqual({
      error: "Telegram reporting is not configured",
    });
    expect(fetcher).not.toHaveBeenCalled();
  });

  it.each([
    ["missing", null],
    ["invalid", "wrong-webhook-secret"],
  ])("acknowledges but ignores a %s webhook secret", async (_label, secret) => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/stats", ADMIN_CHAT_ID, secret),
      configuredEnv(),
      fetcher,
    );

    expect(response.status).toBe(200);
    await expect(responseJson(response)).resolves.toEqual({
      ok: true,
      result: "ignored",
    });
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("acknowledges but ignores a command from the wrong chat", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/payments", "99112233"),
      configuredEnv(),
      fetcher,
    );

    expect(response.status).toBe(200);
    await expect(responseJson(response)).resolves.toEqual({
      ok: true,
      result: "ignored",
    });
    expect(fetcher).not.toHaveBeenCalled();
  });

  it.each([PRIVATE_CHAT_ONE, PRIVATE_CHAT_TWO])(
    "allows configured private operator %s and replies only to that matched ID",
    async (operatorChatId) => {
      const fetcher = outboundFetcher();
      const response = await handleTelegramWebhook(
        commandRequest("/downloads", operatorChatId),
        configuredEnv(),
        fetcher,
      );

      expect(response.status).toBe(200);
      await expect(responseJson(response)).resolves.toEqual({
        ok: true,
        result: "accepted",
      });
      const telegramBody = JSON.parse(
        String(vi.mocked(fetcher).mock.calls[0]?.[1]?.body),
      ) as { chat_id: string; text: string };
      expect(telegramBody.chat_id).toBe(operatorChatId);
      expect(telegramBody.text).toContain("OSL download requests");
    },
  );

  it("adds private viewers without replacing the operator allowlist", async () => {
    const viewerChatId = "8876204092";
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/downloads", viewerChatId),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      fetcher,
    );

    expect(response.status).toBe(200);
    await expect(responseJson(response)).resolves.toEqual({
      ok: true,
      result: "accepted",
    });
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string };
    expect(telegramBody.chat_id).toBe(viewerChatId);
  });

  it("fails closed when the additive viewer allowlist is malformed", async () => {
    for (const malformed of ["", "8876204092,", "@coworker", "-1001234567890"]) {
      const fetcher = outboundFetcher();
      const response = await handleTelegramWebhook(
        commandRequest("/stats", PRIVATE_CHAT_ONE),
        configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: malformed }),
        fetcher,
      );

      expect(response.status).toBe(503);
      expect(fetcher).not.toHaveBeenCalled();
    }
  });

  it("fails closed on an explicit malformed allowlist even when legacy config is valid", async () => {
    for (const malformed of ["", "1122334455,", "abc", "-1001,-1002"]) {
      const fetcher = outboundFetcher();
      const response = await handleTelegramWebhook(
        commandRequest("/stats", PRIVATE_CHAT_ONE),
        configuredEnv({
          TELEGRAM_OPERATOR_CHAT_IDS: malformed,
          TELEGRAM_ADMIN_CHAT_ID: ADMIN_CHAT_ID,
        }),
        fetcher,
      );

      expect(response.status).toBe(503);
      await expect(responseJson(response)).resolves.toEqual({
        error: "Telegram reporting is not configured",
      });
      expect(fetcher).not.toHaveBeenCalled();
    }
  });

  it("deduplicates IDs and fans payment alerts out to every operator", async () => {
    const fetcher = outboundFetcher();
    await notifyTelegramForStripeEvent(
      configuredEnv({
        TELEGRAM_OPERATOR_CHAT_IDS:
          `${PRIVATE_CHAT_ONE},${PRIVATE_CHAT_ONE},${PRIVATE_CHAT_TWO},${ADMIN_CHAT_ID}`,
      }),
      {
        id: "evt_operator_fanout",
        type: "checkout.session.completed",
        livemode: true,
        data: {
          object: {
            mode: "payment",
            payment_status: "paid",
            amount_total: 500,
            currency: "usd",
          },
        },
      },
      fetcher,
    );

    const destinations = vi.mocked(fetcher).mock.calls.map((call) => {
      const body = JSON.parse(String(call[1]?.body)) as {
        chat_id: string;
        text: string;
      };
      expect(body.text).toContain("OSL payment verified");
      expect(body.text).toContain("$5.00");
      return body.chat_id;
    });
    expect(destinations).toEqual([PRIVATE_CHAT_ONE, PRIVATE_CHAT_TWO, ADMIN_CHAT_ID]);
  });

  it("keeps the legacy single-chat setting as a migration fallback", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/downloads", ADMIN_CHAT_ID),
      configuredEnv({
        TELEGRAM_OPERATOR_CHAT_IDS: undefined,
        TELEGRAM_ADMIN_CHAT_ID: ADMIN_CHAT_ID,
      }),
      fetcher,
    );

    expect(response.status).toBe(200);
    await expect(responseJson(response)).resolves.toEqual({
      ok: true,
      result: "accepted",
    });
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string };
    expect(telegramBody.chat_id).toBe(ADMIN_CHAT_ID);
  });

  it("acknowledges but ignores a malformed update", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      updateRequest({ update_id: 1234, message: { text: "/stats" } }),
      configuredEnv(),
      fetcher,
    );

    expect(response.status).toBe(200);
    await expect(responseJson(response)).resolves.toEqual({
      ok: true,
      result: "ignored",
    });
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("accepts a bot-suffixed stats command and sends live aggregate data", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("  /stats@OSLPrivacyBot extra text  "),
      configuredEnv(),
      fetcher,
    );

    expect(response.status).toBe(200);
    await expect(responseJson(response)).resolves.toEqual({
      ok: true,
      result: "accepted",
    });
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(fetcher).toHaveBeenNthCalledWith(
      1,
      "https://api.stripe.com/v1/balance",
      expect.objectContaining({
        headers: { authorization: "Bearer sk_live_route_test_only" },
      }),
    );
    const telegramCall = vi.mocked(fetcher).mock.calls[1];
    expect(String(telegramCall?.[0])).toContain("api.telegram.org/bot");
    const telegramBody = JSON.parse(String(telegramCall?.[1]?.body)) as {
      chat_id: string;
      text: string;
    };
    expect(telegramBody.chat_id).toBe(ADMIN_CHAT_ID);
    expect(telegramBody.text).toContain("OSL live commerce");
    expect(telegramBody.text).toContain("Stripe available: $12.50");
    expect(telegramBody.text).toContain("Mode: LIVE");
  });

  it("accepts payments and sends the Stripe-backed Pro summary", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/payments"),
      configuredEnv(),
      fetcher,
    );

    expect(response.status).toBe(200);
    await expect(responseJson(response)).resolves.toEqual({
      ok: true,
      result: "accepted",
    });
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(String(vi.mocked(fetcher).mock.calls[0]?.[0])).toBe(
      "https://api.stripe.com/v1/balance",
    );
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[1]?.[1]?.body),
    ) as { text: string };
    expect(telegramBody.text).toContain("Payments:");
    expect(telegramBody.text).toContain("Active Pro:");
  });

  it("accepts downloads without querying Stripe", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/downloads"),
      configuredEnv(),
      fetcher,
    );

    expect(response.status).toBe(200);
    await expect(responseJson(response)).resolves.toEqual({
      ok: true,
      result: "accepted",
    });
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(String(vi.mocked(fetcher).mock.calls[0]?.[0])).toContain(
      "api.telegram.org/bot",
    );
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(telegramBody.text).toContain("OSL download requests");
    expect(telegramBody.text).toContain("All time:");
    expect(telegramBody.text).toContain("Last 24h:");
  });
});
