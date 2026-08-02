import { env } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import type { Env } from "../../src/env.js";
import { handleTelegramWebhook } from "../../src/endpoints/telegram.js";
import {
  notifyTelegramForCryptoSettlement,
  notifyTelegramForStripeEvent,
} from "../../src/lib/telegram.js";

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
  error?: string;
}> {
  return await response.json();
}

async function expectNeutralTelegramAck(response: Response): Promise<void> {
  expect(response.status).toBe(200);
  await expect(responseJson(response)).resolves.toEqual({ ok: true });
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

    await expectNeutralTelegramAck(response);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("acknowledges but ignores a command from the wrong chat", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/payments", "99112233"),
      configuredEnv(),
      fetcher,
    );

    await expectNeutralTelegramAck(response);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("uses the same acknowledgement for accepted and unauthorized handled updates", async () => {
    const acceptedFetcher = outboundFetcher();
    const ignoredFetcher = outboundFetcher();
    const accepted = await handleTelegramWebhook(
      commandRequest("/downloads", ADMIN_CHAT_ID),
      configuredEnv(),
      acceptedFetcher,
    );
    const ignored = await handleTelegramWebhook(
      commandRequest("/downloads", "99112233"),
      configuredEnv(),
      ignoredFetcher,
    );

    expect(accepted.status).toBe(200);
    expect(ignored.status).toBe(200);
    const [acceptedBody, ignoredBody] = await Promise.all([
      accepted.text(),
      ignored.text(),
    ]);
    expect(acceptedBody).toBe(ignoredBody);
    expect(JSON.parse(acceptedBody)).toEqual({ ok: true });
    expect(acceptedFetcher).toHaveBeenCalledTimes(1);
    expect(ignoredFetcher).not.toHaveBeenCalled();
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

      await expectNeutralTelegramAck(response);
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

    await expectNeutralTelegramAck(response);
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string };
    expect(telegramBody.chat_id).toBe(viewerChatId);
  });

  it("lets viewers read aggregate stats without exposing the live Stripe balance", async () => {
    const viewerChatId = "8876204092";
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/stats", viewerChatId),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      fetcher,
    );
    await expectNeutralTelegramAck(response);
    const calls = vi.mocked(fetcher).mock.calls;
    expect(calls).toHaveLength(1);
    expect(String(calls[0]?.[0])).toContain("api.telegram.org");
    const body = JSON.parse(String(calls[0]?.[1]?.body)) as { text: string };
    expect(body.text).toContain("OSL live commerce");
    expect(body.text).not.toContain("Stripe available");
    expect(body.text).not.toContain("Stripe pending");
  });

  it("keeps viewer payments reports aggregate-only without Stripe balance authority", async () => {
    const viewerChatId = "8876204092";
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/payments", viewerChatId),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      fetcher,
    );

    await expectNeutralTelegramAck(response);
    const calls = vi.mocked(fetcher).mock.calls;
    expect(calls).toHaveLength(1);
    expect(String(calls[0]?.[0])).toContain("api.telegram.org");
    const body = JSON.parse(String(calls[0]?.[1]?.body)) as { text: string };
    expect(body.text).toContain("OSL live commerce");
    expect(body.text).toContain("Payments:");
    expect(body.text).not.toContain("Stripe available");
    expect(body.text).not.toContain("Stripe pending");
  });

  it("keyserver-cf/src/lib/telegram.ts", async () => {
    const viewerChatId = "8876204092";
    const viewerFetcher = outboundFetcher();
    const viewerResponse = await handleTelegramWebhook(
      commandRequest("/stats", viewerChatId),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      viewerFetcher,
    );

    await expectNeutralTelegramAck(viewerResponse);
    expect(viewerFetcher).toHaveBeenCalledTimes(1);
    expect(String(vi.mocked(viewerFetcher).mock.calls[0]?.[0])).toContain("api.telegram.org");
    const viewerBody = JSON.parse(
      String(vi.mocked(viewerFetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(viewerBody.chat_id).toBe(viewerChatId);
    expect(viewerBody.text).toContain("OSL live commerce");
    expect(viewerBody.text).not.toContain("Stripe available");
    expect(viewerBody.text).not.toContain("Stripe pending");

    const operatorFetcher = outboundFetcher();
    const operatorResponse = await handleTelegramWebhook(
      commandRequest("/stats", PRIVATE_CHAT_ONE),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      operatorFetcher,
    );

    await expectNeutralTelegramAck(operatorResponse);
    expect(operatorFetcher).toHaveBeenCalledTimes(2);
    expect(String(vi.mocked(operatorFetcher).mock.calls[0]?.[0])).toBe(
      "https://api.stripe.com/v1/balance",
    );
    const operatorBody = JSON.parse(
      String(vi.mocked(operatorFetcher).mock.calls[1]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(operatorBody.chat_id).toBe(PRIVATE_CHAT_ONE);
    expect(operatorBody.text).toContain("Stripe available: $12.50");

    const ignoredFetcher = outboundFetcher();
    const ignoredResponse = await handleTelegramWebhook(
      commandRequest("/stats", "99112233"),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      ignoredFetcher,
    );
    await expectNeutralTelegramAck(ignoredResponse);
    expect(ignoredFetcher).not.toHaveBeenCalled();
  });

  it("Freeze Telegram chat authorization and role separation.", async () => {
    const viewerChatId = "8876204092";

    const unauthorizedFetcher = outboundFetcher();
    const unauthorizedResponse = await handleTelegramWebhook(
      commandRequest("/osl payments", "99112233"),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      unauthorizedFetcher,
    );
    await expectNeutralTelegramAck(unauthorizedResponse);
    expect(unauthorizedFetcher).not.toHaveBeenCalled();

    const viewerFetcher = outboundFetcher();
    const viewerResponse = await handleTelegramWebhook(
      commandRequest("/osl payments", viewerChatId),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      viewerFetcher,
    );
    await expectNeutralTelegramAck(viewerResponse);
    const viewerCalls = vi.mocked(viewerFetcher).mock.calls;
    expect(viewerCalls).toHaveLength(1);
    expect(String(viewerCalls[0]?.[0])).toContain("api.telegram.org");
    const viewerBody = JSON.parse(String(viewerCalls[0]?.[1]?.body)) as {
      chat_id: string;
      text: string;
    };
    expect(viewerBody.chat_id).toBe(viewerChatId);
    expect(viewerBody.text).toContain("OSL live commerce");
    expect(viewerBody.text).not.toContain("Stripe available");
    expect(viewerBody.text).not.toContain("Stripe pending");

    const operatorFetcher = outboundFetcher();
    const operatorResponse = await handleTelegramWebhook(
      commandRequest("/osl payments", PRIVATE_CHAT_ONE),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      operatorFetcher,
    );
    await expectNeutralTelegramAck(operatorResponse);
    const operatorCalls = vi.mocked(operatorFetcher).mock.calls;
    expect(operatorCalls).toHaveLength(2);
    expect(String(operatorCalls[0]?.[0])).toBe("https://api.stripe.com/v1/balance");
    const operatorBody = JSON.parse(String(operatorCalls[1]?.[1]?.body)) as {
      chat_id: string;
      text: string;
    };
    expect(operatorBody.chat_id).toBe(PRIVATE_CHAT_ONE);
    expect(operatorBody.text).toContain("Stripe available: $12.50");

    const malformedFetcher = outboundFetcher();
    const malformedResponse = await handleTelegramWebhook(
      commandRequest("/osl payments", PRIVATE_CHAT_ONE),
      configuredEnv({
        TELEGRAM_OPERATOR_CHAT_IDS: "",
        TELEGRAM_ADMIN_CHAT_ID: ADMIN_CHAT_ID,
        TELEGRAM_VIEWER_CHAT_IDS: viewerChatId,
      }),
      malformedFetcher,
    );
    expect(malformedResponse.status).toBe(503);
    expect(malformedFetcher).not.toHaveBeenCalled();
  });

  it("keeps an operator role when the same chat is also listed as a viewer", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/stats", PRIVATE_CHAT_ONE),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: PRIVATE_CHAT_ONE }),
      fetcher,
    );

    await expectNeutralTelegramAck(response);
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(String(vi.mocked(fetcher).mock.calls[0]?.[0])).toBe(
      "https://api.stripe.com/v1/balance",
    );
    const body = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[1]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(body.chat_id).toBe(PRIVATE_CHAT_ONE);
    expect(body.text).toContain("Stripe available: $12.50");
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

  it("sends crypto settlement alerts only to operators, never additive viewers", async () => {
    const fetcher = outboundFetcher();
    await notifyTelegramForCryptoSettlement(
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: "8876204092" }),
      "xmr",
      500,
      fetcher,
    );
    const destinations = vi.mocked(fetcher).mock.calls.map((call) => {
      const body = JSON.parse(String(call[1]?.body)) as { chat_id: string; text: string };
      expect(body.text).toContain("$5.00 via Monero");
      return body.chat_id;
    });
    expect(destinations).toEqual([PRIVATE_CHAT_ONE, PRIVATE_CHAT_TWO, ADMIN_CHAT_ID]);
    expect(destinations).not.toContain("8876204092");
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

    await expectNeutralTelegramAck(response);
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

    await expectNeutralTelegramAck(response);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("accepts a bot-suffixed stats command and sends live aggregate data", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("  /stats@OSLPrivacyBot extra text  "),
      configuredEnv(),
      fetcher,
    );

    await expectNeutralTelegramAck(response);
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

    await expectNeutralTelegramAck(response);
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

    await expectNeutralTelegramAck(response);
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

  it("serves the /osl hierarchy with honest progress and suggestions", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/osl"),
      configuredEnv(),
      fetcher,
    );

    await expectNeutralTelegramAck(response);
    expect(fetcher).toHaveBeenCalledTimes(1);
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(telegramBody.text).toContain("/osl status: current coordination state");
    expect(telegramBody.text).toContain("/osl progress: project progress block");
    expect(telegramBody.text).toContain("/osl payments: Stripe and Pro license summary");
    expect(telegramBody.text).toContain("OSL progress (internal checklist)");
    expect(telegramBody.text).toContain("Provisional verified progress: 100 / 303 points = 33%");
    expect(telegramBody.text).toContain("Source: docs/design/osl-internal-build-checklist.md#1599e3823ef8");
  });

  it("Implement the Telegram /osl command hierarchy with progress and suggestions.", async () => {
    const helpFetcher = outboundFetcher();
    const helpResponse = await handleTelegramWebhook(
      commandRequest("/osl"),
      configuredEnv(),
      helpFetcher,
    );
    await expectNeutralTelegramAck(helpResponse);
    const helpBody = JSON.parse(
      String(vi.mocked(helpFetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(helpBody.text).toContain("OSL operator commands");
    expect(helpBody.text).toContain("/osl status: current coordination state");
    expect(helpBody.text).toContain("/osl progress: project progress block");
    expect(helpBody.text).toContain("/osl stats: live commerce summary");
    expect(helpBody.text).toContain("/osl payments: Stripe and Pro license summary");
    expect(helpBody.text).toContain("/osl downloads: download requests");
    expect(helpBody.text).toContain("OSL progress (internal checklist)");
    expect(helpBody.text).toContain("Provisional verified progress: 100 / 303 points = 33%");

    const paymentsFetcher = outboundFetcher();
    const paymentsResponse = await handleTelegramWebhook(
      commandRequest("/osl payments"),
      configuredEnv(),
      paymentsFetcher,
    );
    await expectNeutralTelegramAck(paymentsResponse);
    expect(paymentsFetcher).toHaveBeenCalledTimes(2);
    expect(String(vi.mocked(paymentsFetcher).mock.calls[0]?.[0])).toBe(
      "https://api.stripe.com/v1/balance",
    );
    const paymentsBody = JSON.parse(
      String(vi.mocked(paymentsFetcher).mock.calls[1]?.[1]?.body),
    ) as { text: string };
    expect(paymentsBody.text).toContain("Payments:");
    expect(paymentsBody.text).toContain("Stripe available: $12.50");
    expect(paymentsBody.text).toContain("OSL progress (internal checklist)");

    const typoFetcher = outboundFetcher();
    const typoResponse = await handleTelegramWebhook(
      commandRequest("/osl paymnts secret-extra"),
      configuredEnv(),
      typoFetcher,
    );
    await expectNeutralTelegramAck(typoResponse);
    const typoBody = JSON.parse(
      String(vi.mocked(typoFetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(typoBody.text).toContain("Unknown /osl command.");
    expect(typoBody.text).toContain("Suggestion: /osl payments");
    expect(typoBody.text).toContain("OSL progress (internal checklist)");
    expect(typoBody.text).not.toContain("paymnts");
    expect(typoBody.text).not.toContain("secret-extra");
  });

  it("maps /osl report subcommands to existing reports and appends progress", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/osl payments"),
      configuredEnv(),
      fetcher,
    );

    await expectNeutralTelegramAck(response);
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(String(vi.mocked(fetcher).mock.calls[0]?.[0])).toBe(
      "https://api.stripe.com/v1/balance",
    );
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[1]?.[1]?.body),
    ) as { text: string };
    expect(telegramBody.text).toContain("Payments:");
    expect(telegramBody.text).toContain("Stripe available: $12.50");
    expect(telegramBody.text).toContain("OSL progress (internal checklist)");
  });

  it("suggests the closest safe /osl command without echoing unknown text", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/osl paymnts secret-extra"),
      configuredEnv(),
      fetcher,
    );

    expect(response.status).toBe(200);
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(telegramBody.text).toContain("Unknown /osl command.");
    expect(telegramBody.text).toContain("Suggestion: /osl payments");
    expect(telegramBody.text).not.toContain("paymnts");
    expect(telegramBody.text).not.toContain("secret-extra");
  });

  it("refuses /osl coordination controls when owner binding is unavailable", async () => {
    const fetcher = outboundFetcher();
    const response = await handleTelegramWebhook(
      commandRequest("/osl bind tab-7"),
      configuredEnv(),
      fetcher,
    );

    expect(response.status).toBe(200);
    const telegramBody = JSON.parse(
      String(vi.mocked(fetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(telegramBody.text).toContain("Cannot change /osl coordination state here.");
    expect(telegramBody.text).toContain("Owner binding is required");
    expect(telegramBody.text).not.toContain("tab-7");
    expect(telegramBody.text).toContain("OSL progress (internal checklist)");
  });

  it("telegram'", async () => {
    const hierarchyFetcher = outboundFetcher();
    const hierarchyResponse = await handleTelegramWebhook(
      commandRequest("/osl"),
      configuredEnv(),
      hierarchyFetcher,
    );

    await expectNeutralTelegramAck(hierarchyResponse);
    expect(hierarchyFetcher).toHaveBeenCalledTimes(1);
    const hierarchyBody = JSON.parse(
      String(vi.mocked(hierarchyFetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(hierarchyBody.chat_id).toBe(ADMIN_CHAT_ID);
    expect(hierarchyBody.text).toContain("OSL operator commands");
    expect(hierarchyBody.text).toContain("/osl status: current coordination state");
    expect(hierarchyBody.text).toContain("/osl progress: project progress block");
    expect(hierarchyBody.text).toContain("/osl stats: live commerce summary");
    expect(hierarchyBody.text).toContain("/osl payments: Stripe and Pro license summary");
    expect(hierarchyBody.text).toContain("/osl downloads: download requests");
    expect(hierarchyBody.text).toContain("OSL progress (internal checklist)");

    const progressFetcher = outboundFetcher();
    const progressResponse = await handleTelegramWebhook(
      commandRequest("/osl progress"),
      configuredEnv(),
      progressFetcher,
    );

    await expectNeutralTelegramAck(progressResponse);
    expect(progressFetcher).toHaveBeenCalledTimes(1);
    const progressBody = JSON.parse(
      String(vi.mocked(progressFetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(progressBody.chat_id).toBe(ADMIN_CHAT_ID);
    expect(progressBody.text).toContain("OSL progress (internal checklist)");
    expect(progressBody.text).toContain("Provisional verified progress: 100 / 303 points = 33%");
    expect(progressBody.text).toContain("Source: docs/design/osl-internal-build-checklist.md#1599e3823ef8");
    expect(progressBody.text).not.toContain("OSL operator commands");

    const acceptedFetcher = outboundFetcher();
    const badSecretFetcher = outboundFetcher();
    const wrongChatFetcher = outboundFetcher();
    const malformedFetcher = outboundFetcher();
    const invalidJsonFetcher = outboundFetcher();
    const acceptedResponse = await handleTelegramWebhook(
      commandRequest("/downloads", ADMIN_CHAT_ID),
      configuredEnv(),
      acceptedFetcher,
    );
    const badSecretResponse = await handleTelegramWebhook(
      commandRequest("/downloads", ADMIN_CHAT_ID, "wrong-webhook-secret"),
      configuredEnv(),
      badSecretFetcher,
    );
    const wrongChatResponse = await handleTelegramWebhook(
      commandRequest("/downloads", "99112233"),
      configuredEnv(),
      wrongChatFetcher,
    );
    const malformedResponse = await handleTelegramWebhook(
      updateRequest({ update_id: 1234, message: { text: "/downloads" } }),
      configuredEnv(),
      malformedFetcher,
    );
    const invalidJsonResponse = await handleTelegramWebhook(
      new Request("https://keyserver.test/v1/telegram/webhook", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-telegram-bot-api-secret-token": WEBHOOK_SECRET,
        },
        body: "{\"message\":",
      }),
      configuredEnv(),
      invalidJsonFetcher,
    );

    expect([
      acceptedResponse.status,
      badSecretResponse.status,
      wrongChatResponse.status,
      malformedResponse.status,
      invalidJsonResponse.status,
    ]).toEqual([200, 200, 200, 200, 200]);
    const ackBodies = await Promise.all([
      acceptedResponse.text(),
      badSecretResponse.text(),
      wrongChatResponse.text(),
      malformedResponse.text(),
      invalidJsonResponse.text(),
    ]);
    expect(new Set(ackBodies).size).toBe(1);
    expect(JSON.parse(ackBodies[0] ?? "")).toEqual({ ok: true });
    expect(acceptedFetcher).toHaveBeenCalledTimes(1);
    expect(badSecretFetcher).not.toHaveBeenCalled();
    expect(wrongChatFetcher).not.toHaveBeenCalled();
    expect(malformedFetcher).not.toHaveBeenCalled();
    expect(invalidJsonFetcher).not.toHaveBeenCalled();

    const typoFetcher = outboundFetcher();
    const typoResponse = await handleTelegramWebhook(
      commandRequest("/osl paymnts private-note"),
      configuredEnv(),
      typoFetcher,
    );

    await expectNeutralTelegramAck(typoResponse);
    expect(typoFetcher).toHaveBeenCalledTimes(1);
    const typoBody = JSON.parse(
      String(vi.mocked(typoFetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(typoBody.chat_id).toBe(ADMIN_CHAT_ID);
    expect(typoBody.text).toContain("Unknown /osl command.");
    expect(typoBody.text).toContain("Suggestion: /osl payments");
    expect(typoBody.text).toContain("OSL progress (internal checklist)");
    expect(typoBody.text).not.toContain("paymnts");
    expect(typoBody.text).not.toContain("private-note");

    const viewerChatId = "8876204092";
    const controlFetcher = outboundFetcher();
    const controlResponse = await handleTelegramWebhook(
      commandRequest("/osl on owner-secret", viewerChatId),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      controlFetcher,
    );

    await expectNeutralTelegramAck(controlResponse);
    expect(controlFetcher).toHaveBeenCalledTimes(1);
    const controlBody = JSON.parse(
      String(vi.mocked(controlFetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(controlBody.chat_id).toBe(viewerChatId);
    expect(controlBody.text).toContain("Cannot change /osl coordination state from this chat.");
    expect(controlBody.text).toContain("Try /osl status");
    expect(controlBody.text).toContain("OSL progress (internal checklist)");
    expect(controlBody.text).not.toContain("owner-secret");
  });

  it("keeps /osl payments viewer reports aggregate-only while operators retain balance authority", async () => {
    const viewerChatId = "8876204092";
    const viewerFetcher = outboundFetcher();
    const viewerResponse = await handleTelegramWebhook(
      commandRequest("/osl payments", viewerChatId),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      viewerFetcher,
    );

    await expectNeutralTelegramAck(viewerResponse);
    expect(viewerFetcher).toHaveBeenCalledTimes(1);
    expect(String(vi.mocked(viewerFetcher).mock.calls[0]?.[0])).toContain(
      "api.telegram.org/bot",
    );
    const viewerBody = JSON.parse(
      String(vi.mocked(viewerFetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(viewerBody.chat_id).toBe(viewerChatId);
    expect(viewerBody.text).toContain("OSL live commerce");
    expect(viewerBody.text).not.toContain("Stripe available");
    expect(viewerBody.text).not.toContain("Stripe pending");

    const statusFetcher = outboundFetcher();
    const statusResponse = await handleTelegramWebhook(
      commandRequest("/osl status", viewerChatId),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      statusFetcher,
    );

    await expectNeutralTelegramAck(statusResponse);
    const statusBody = JSON.parse(
      String(vi.mocked(statusFetcher).mock.calls[0]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(statusBody.chat_id).toBe(viewerChatId);
    expect(statusBody.text).toContain("Authorized chat role: viewer");
    expect(statusBody.text).toContain("Coordination controls: unavailable");

    const operatorFetcher = outboundFetcher();
    const operatorResponse = await handleTelegramWebhook(
      commandRequest("/osl payments", PRIVATE_CHAT_ONE),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      operatorFetcher,
    );

    await expectNeutralTelegramAck(operatorResponse);
    expect(operatorFetcher).toHaveBeenCalledTimes(2);
    expect(String(vi.mocked(operatorFetcher).mock.calls[0]?.[0])).toBe(
      "https://api.stripe.com/v1/balance",
    );
    const operatorBody = JSON.parse(
      String(vi.mocked(operatorFetcher).mock.calls[1]?.[1]?.body),
    ) as { chat_id: string; text: string };
    expect(operatorBody.chat_id).toBe(PRIVATE_CHAT_ONE);
    expect(operatorBody.text).toContain("Stripe available: $12.50");

    const ignoredFetcher = outboundFetcher();
    const ignoredResponse = await handleTelegramWebhook(
      commandRequest("/osl payments", "99112233"),
      configuredEnv({ TELEGRAM_VIEWER_CHAT_IDS: viewerChatId }),
      ignoredFetcher,
    );

    await expectNeutralTelegramAck(ignoredResponse);
    expect(ignoredFetcher).not.toHaveBeenCalled();
  });

  it("keeps /osl help and owner-binding refusals free of user-supplied text", async () => {
    const helpFetcher = outboundFetcher();
    const helpResponse = await handleTelegramWebhook(
      commandRequest("/osl"),
      configuredEnv(),
      helpFetcher,
    );

    await expectNeutralTelegramAck(helpResponse);
    const helpBody = JSON.parse(
      String(vi.mocked(helpFetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(helpBody.text).toContain("OSL operator commands");
    expect(helpBody.text).toContain("/osl status: current coordination state");
    expect(helpBody.text).toContain("/osl progress: project progress block");
    expect(helpBody.text).toContain("/osl on|off|quiet|bind|unbind");
    expect(helpBody.text).toContain("OSL progress (internal checklist)");
    expect(helpBody.text).toContain("Provisional verified progress: 100 / 303 points = 33%");

    const typoFetcher = outboundFetcher();
    const typoResponse = await handleTelegramWebhook(
      commandRequest("/osl paymnts secret-extra"),
      configuredEnv(),
      typoFetcher,
    );

    await expectNeutralTelegramAck(typoResponse);
    const typoBody = JSON.parse(
      String(vi.mocked(typoFetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(typoBody.text).toContain("Unknown /osl command.");
    expect(typoBody.text).toContain("Suggestion: /osl payments");
    expect(typoBody.text).not.toContain("paymnts");
    expect(typoBody.text).not.toContain("secret-extra");

    const controlFetcher = outboundFetcher();
    const controlResponse = await handleTelegramWebhook(
      commandRequest("/osl quiet target-chat"),
      configuredEnv(),
      controlFetcher,
    );

    await expectNeutralTelegramAck(controlResponse);
    const controlBody = JSON.parse(
      String(vi.mocked(controlFetcher).mock.calls[0]?.[1]?.body),
    ) as { text: string };
    expect(controlBody.text).toContain("Cannot change /osl coordination state here.");
    expect(controlBody.text).toContain("Owner binding is required");
    expect(controlBody.text).not.toContain("target-chat");
    expect(controlBody.text).toContain("Try /osl status");
  });
});
