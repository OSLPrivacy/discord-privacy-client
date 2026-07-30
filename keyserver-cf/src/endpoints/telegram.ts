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
  await handleTelegramCommand(request, env, fetcher);
  // Always acknowledge handled updates without revealing whether any command
  // was authorized. Telegram will not retry an acknowledged update.
  return json({ ok: true });
}
