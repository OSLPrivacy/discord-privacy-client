import type { Env } from "../env.js";
import { json, serviceUnavailable } from "../lib/http.js";
import {
  handleTelegramCommand,
  telegramReportingIsConfigured,
} from "../lib/telegram.js";

export async function handleTelegramWebhook(
  request: Request,
  env: Env,
  fetcher: typeof fetch = fetch,
): Promise<Response> {
  if (!telegramReportingIsConfigured(env)) {
    return serviceUnavailable("Telegram reporting is not configured");
  }
  const result = await handleTelegramCommand(request, env, fetcher);
  // Always acknowledge unauthorized chat IDs and bad secret tokens without
  // revealing which part was wrong. Telegram will not retry a handled update.
  return json({ ok: true, result });
}
