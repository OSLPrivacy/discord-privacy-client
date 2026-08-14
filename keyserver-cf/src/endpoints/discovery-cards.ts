import type { Env } from "../env.js";
import { tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import {
  handleDiscoveryCardPost,
  handleDiscoveryCardRead,
  handleDiscoveryCardsPublish,
  handleDiscoveryCardsTakeBack,
} from "../lib/discovery-card.js";

export const DISCOVERY_CARDS_PUBLISH_MAX_PER_MINUTE = 5;
export const DISCOVERY_CARDS_ASK_MAX_PER_MINUTE = 1200;

export async function handleDiscoveryCardsPost(
  request: Request,
  env: Env,
): Promise<Response> {
  return await handleDiscoveryCardPost(request, env.DB);
}

export async function handleDiscoveryCardsRead(
  request: Request,
  env: Env,
  maxPerMinute = DISCOVERY_CARDS_ASK_MAX_PER_MINUTE,
): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), maxPerMinute, "discovery-card-ask");
  if (!limit.ok) return tooMany(limit.retryAfter);
  return await handleDiscoveryCardRead(request, env.DB);
}

export async function handleDiscoveryCardsPublishPost(
  request: Request,
  env: Env,
  maxPerMinute = DISCOVERY_CARDS_PUBLISH_MAX_PER_MINUTE,
): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), maxPerMinute, "discovery-card-publish");
  if (!limit.ok) return tooMany(limit.retryAfter);
  return await handleDiscoveryCardsPublish(request, env.DB);
}

export async function handleDiscoveryCardsTakeBackPost(
  request: Request,
  env: Env,
): Promise<Response> {
  return await handleDiscoveryCardsTakeBack(request, env.DB);
}
