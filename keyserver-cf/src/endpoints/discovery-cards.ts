import type { Env } from "../env.js";
import {
  handleDiscoveryCardPost,
  handleDiscoveryCardRead,
  handleDiscoveryCardsPublish,
  handleDiscoveryCardsTakeBack,
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

export async function handleDiscoveryCardsPublishPost(
  request: Request,
  env: Env,
): Promise<Response> {
  return await handleDiscoveryCardsPublish(request, env.DB);
}

export async function handleDiscoveryCardsTakeBackPost(
  request: Request,
  env: Env,
): Promise<Response> {
  return await handleDiscoveryCardsTakeBack(request, env.DB);
}
