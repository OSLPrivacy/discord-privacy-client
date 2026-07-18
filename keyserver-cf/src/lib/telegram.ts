import type { Env } from "../env.js";
import { getCommerceSummary, type CommerceSummary } from "./commerce-metrics.js";
import {
  getDonationSummary,
  type VerifiedDonation,
} from "./donations.js";
import { isLiveStripeSecretKey, type StripeEvent } from "./stripe.js";

interface TelegramUpdate {
  message?: {
    text?: string;
    chat?: { id?: number | string };
  };
}

interface StripeBalance {
  available?: Array<{ amount?: number; currency?: string }>;
  pending?: Array<{ amount?: number; currency?: string }>;
}

type TelegramChatConfiguration =
  | { status: "configured"; chatIds: string[] }
  | { status: "invalid" | "unconfigured"; chatIds: [] };

const MAX_TELEGRAM_CHAT_ID = 9_007_199_254_740_991n;
const MAX_OPERATOR_CHAT_IDS = 32;

function normalizeTelegramChatId(value: unknown): string | null {
  if (typeof value === "number") {
    return Number.isSafeInteger(value) && value !== 0 ? String(value) : null;
  }
  if (typeof value !== "string") return null;
  const normalized = value.trim();
  if (!/^-?[1-9]\d{0,15}$/.test(normalized)) return null;
  const magnitude = BigInt(normalized.startsWith("-") ? normalized.slice(1) : normalized);
  return magnitude <= MAX_TELEGRAM_CHAT_ID ? normalized : null;
}

/**
 * Resolve deployment-owned authorization only. If the new allowlist exists it
 * always wins, including when malformed, so a bad migration cannot silently
 * reopen access through the legacy value.
 */
function telegramChatConfiguration(env: Env): TelegramChatConfiguration {
  let operatorChatIds: string[];
  if (env.TELEGRAM_OPERATOR_CHAT_IDS !== undefined) {
    const rawIds = env.TELEGRAM_OPERATOR_CHAT_IDS.split(",");
    if (rawIds.length === 0 || rawIds.length > MAX_OPERATOR_CHAT_IDS) {
      return { status: "invalid", chatIds: [] };
    }
    const uniqueIds = new Set<string>();
    for (const rawId of rawIds) {
      const normalized = normalizeTelegramChatId(rawId);
      if (normalized === null) return { status: "invalid", chatIds: [] };
      uniqueIds.add(normalized);
    }
    const chatIds = [...uniqueIds];
    if (chatIds.length === 0 || chatIds.filter((chatId) => chatId.startsWith("-")).length > 1) {
      return { status: "invalid", chatIds: [] };
    }
    operatorChatIds = chatIds;
  } else if (env.TELEGRAM_ADMIN_CHAT_ID !== undefined) {
    const legacyChatId = normalizeTelegramChatId(env.TELEGRAM_ADMIN_CHAT_ID);
    if (legacyChatId === null) return { status: "invalid", chatIds: [] };
    operatorChatIds = [legacyChatId];
  } else {
    operatorChatIds = [];
  }

  const viewerChatIds: string[] = [];
  if (env.TELEGRAM_VIEWER_CHAT_IDS !== undefined) {
    const rawIds = env.TELEGRAM_VIEWER_CHAT_IDS.split(",");
    if (rawIds.length === 0 || rawIds.length > MAX_OPERATOR_CHAT_IDS) {
      return { status: "invalid", chatIds: [] };
    }
    for (const rawId of rawIds) {
      const normalized = normalizeTelegramChatId(rawId);
      if (normalized === null || normalized.startsWith("-")) {
        return { status: "invalid", chatIds: [] };
      }
      viewerChatIds.push(normalized);
    }
  }

  const chatIds = [...new Set([...operatorChatIds, ...viewerChatIds])];
  return chatIds.length === 0
    ? { status: "unconfigured", chatIds: [] }
    : { status: "configured", chatIds };
}

export function telegramReportingIsConfigured(env: Env): boolean {
  return Boolean(env.TELEGRAM_WEBHOOK_SECRET && env.TELEGRAM_BOT_TOKEN) &&
    telegramChatConfiguration(env).status === "configured";
}

async function constantTimeEqual(left: string, right: string): Promise<boolean> {
  const encoder = new TextEncoder();
  const [leftDigest, rightDigest] = await Promise.all([
    crypto.subtle.digest("SHA-256", encoder.encode(left)),
    crypto.subtle.digest("SHA-256", encoder.encode(right)),
  ]);
  return crypto.subtle.timingSafeEqual(leftDigest, rightDigest);
}

async function authorizedTelegramChatId(
  supplied: unknown,
  allowedChatIds: string[],
): Promise<string | null> {
  const normalized = normalizeTelegramChatId(supplied);
  if (normalized === null) return null;
  const encoder = new TextEncoder();
  const suppliedDigest = await crypto.subtle.digest("SHA-256", encoder.encode(normalized));
  const allowedDigests = await Promise.all(allowedChatIds.map(
    (chatId) => crypto.subtle.digest("SHA-256", encoder.encode(chatId)),
  ));
  let matched: string | null = null;
  for (let index = 0; index < allowedDigests.length; index += 1) {
    // Check every entry without an early exit. Returning the deployment-owned
    // value also ensures the public update never chooses a send destination.
    if (crypto.subtle.timingSafeEqual(suppliedDigest, allowedDigests[index]!)) {
      matched = allowedChatIds[index]!;
    }
  }
  return matched;
}

export async function verifyTelegramWebhook(
  request: Request,
  expected: string,
): Promise<boolean> {
  const supplied = request.headers.get("x-telegram-bot-api-secret-token");
  return supplied !== null && await constantTimeEqual(supplied, expected);
}

export async function sendTelegramMessage(
  botToken: string,
  chatId: string,
  text: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  if (!/^[A-Za-z0-9:_-]{20,128}$/.test(botToken)) {
    throw new Error("Telegram bot token is malformed");
  }
  const response = await fetcher(`https://api.telegram.org/bot${botToken}/sendMessage`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      chat_id: chatId,
      text,
      disable_web_page_preview: true,
    }),
    signal: AbortSignal.timeout(5_000),
  });
  if (!response.ok) throw new Error(`Telegram sendMessage returned ${response.status}`);
}

export async function sendTelegramOperatorMessage(
  env: Env,
  text: string,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  const botToken = env.TELEGRAM_BOT_TOKEN;
  if (!botToken) return;
  const configuration = telegramChatConfiguration(env);
  if (configuration.status === "unconfigured") return;
  if (configuration.status === "invalid") {
    throw new Error("Telegram operator chat allowlist is malformed");
  }
  const results = await Promise.allSettled(configuration.chatIds.map(
    (chatId) => sendTelegramMessage(botToken, chatId, text, fetcher),
  ));
  if (results.some((result) => result.status === "rejected")) {
    throw new Error("one or more Telegram operator messages failed");
  }
}

export async function getLiveStripeBalance(
  secretKey: string,
  fetcher: typeof fetch = fetch,
): Promise<StripeBalance> {
  if (!isLiveStripeSecretKey(secretKey)) throw new Error("live Stripe key is unavailable");
  const response = await fetcher("https://api.stripe.com/v1/balance", {
    headers: { authorization: `Bearer ${secretKey}` },
    signal: AbortSignal.timeout(5_000),
  });
  if (!response.ok) throw new Error(`Stripe balance returned ${response.status}`);
  return (await response.json()) as StripeBalance;
}

function money(cents: number, currency = "usd"): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: currency.toUpperCase(),
  }).format(cents / 100);
}

function balanceLine(label: string, rows: StripeBalance["available"]): string {
  if (!rows?.length) return `${label}: ${money(0)}`;
  return `${label}: ${rows.map((row) => money(
    Number.isSafeInteger(row.amount) ? row.amount as number : 0,
    typeof row.currency === "string" ? row.currency : "usd",
  )).join(", ")}`;
}

export function formatOperatorStats(
  summary: CommerceSummary,
  balance?: StripeBalance,
): string {
  const lines = [
    "OSL live commerce",
    `Payments: ${summary.successful_payments}`,
    `Gross verified: ${money(summary.gross_cents)}`,
    `Refunds/disputes: ${money(summary.refunds_and_disputes_cents)}`,
    `Donations: ${summary.verified_donations} (${money(summary.donation_gross_cents)})`,
    `Active Pro: ${summary.active_subscriptions}`,
    `Download requests: ${summary.download_starts} (${summary.download_starts_24h} in 24h)`,
  ];
  if (balance) {
    lines.push(balanceLine("Stripe available", balance.available));
    lines.push(balanceLine("Stripe pending", balance.pending));
  }
  lines.push("Mode: LIVE");
  return lines.join("\n");
}

export async function telegramStatsMessage(
  env: Env,
  includeBalance: boolean,
  fetcher: typeof fetch = fetch,
): Promise<string> {
  const summary = await getCommerceSummary(env.DB);
  const balance = includeBalance && env.STRIPE_SECRET_KEY
    ? await getLiveStripeBalance(env.STRIPE_SECRET_KEY, fetcher)
    : undefined;
  return formatOperatorStats(summary, balance);
}

export async function notifyTelegramForStripeEvent(
  env: Env,
  event: StripeEvent,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  if (!env.TELEGRAM_BOT_TOKEN) return;
  if (
    event.type !== "invoice.paid" &&
    event.type !== "checkout.session.completed" &&
    event.type !== "checkout.session.async_payment_succeeded"
  ) return;
  const object = event.data.object as {
    mode?: unknown;
    payment_status?: unknown;
    amount_paid?: unknown;
    amount_total?: unknown;
    currency?: unknown;
  };
  const oneTime = event.type !== "invoice.paid";
  if (oneTime && (object.mode !== "payment" || object.payment_status !== "paid")) return;
  const rawAmount = oneTime ? object.amount_total : object.amount_paid;
  const amount = Number.isSafeInteger(rawAmount) ? rawAmount as number : 0;
  const currency = typeof object.currency === "string" ? object.currency : "usd";
  await sendTelegramOperatorMessage(
    env,
    `OSL payment verified\n${money(amount, currency)}\nMode: LIVE`,
    fetcher,
  );
}

export async function notifyTelegramForDonation(
  env: Env,
  donation: VerifiedDonation,
  fetcher: typeof fetch = fetch,
): Promise<void> {
  if (!env.TELEGRAM_BOT_TOKEN) return;
  const summary = await getDonationSummary(env.DB);
  await sendTelegramOperatorMessage(
    env,
    [
      "OSL donation verified",
      `${money(donation.amountUsdCents)} via Stripe`,
      `Verified donations: ${summary.verified} / ${money(summary.grossUsdCents)}`,
      "Mode: LIVE",
    ].join("\n"),
    fetcher,
  );
}

export async function handleTelegramCommand(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<"accepted" | "ignored"> {
  const configuration = telegramChatConfiguration(env);
  if (
    !env.TELEGRAM_WEBHOOK_SECRET ||
    !env.TELEGRAM_BOT_TOKEN ||
    configuration.status !== "configured"
  ) {
    throw new Error("Telegram reporting is not configured");
  }
  if (!(await verifyTelegramWebhook(request, env.TELEGRAM_WEBHOOK_SECRET))) {
    return "ignored";
  }
  const update = (await request.json()) as TelegramUpdate;
  const chatId = update.message?.chat?.id;
  const authorizedChatId = await authorizedTelegramChatId(chatId, configuration.chatIds);
  if (authorizedChatId === null) return "ignored";
  const command = update.message?.text?.trim().split(/\s+/, 1)[0]?.split("@", 1)[0] ?? "";
  let message: string;
  if (command === "/stats" || command === "/payments") {
    message = await telegramStatsMessage(env, true, fetcher);
  } else if (command === "/downloads") {
    const summary = await getCommerceSummary(env.DB);
    message = `OSL download requests\nAll time: ${summary.download_starts}\nLast 24h: ${summary.download_starts_24h}`;
  } else {
    message = "OSL operator commands\n/stats: live commerce summary\n/payments: Stripe and Pro license summary\n/downloads: download requests";
  }
  await sendTelegramMessage(env.TELEGRAM_BOT_TOKEN, authorizedChatId, message, fetcher);
  return "accepted";
}
