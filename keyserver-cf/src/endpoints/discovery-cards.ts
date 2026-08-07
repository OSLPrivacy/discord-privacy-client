import type { Env } from "../env.js";
import {
  handleDiscoveryCardPost,
  handleDiscoveryCardRead,
} from "../lib/discovery-card.js";

export async function handleDiscoveryCardsPost(
  request: Request,
  env: Env,
): Promise<Response> {
  return await handleDiscoveryCardPost(request, env.DB);
}

export async function handleDiscoveryCardsRead(
  request: Request,
  env: Env,
): Promise<Response> {
  return await handleDiscoveryCardRead(request, env.DB);
}
