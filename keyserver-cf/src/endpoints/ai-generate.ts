import type { Env } from "../env.js";
import { badRequest, serviceUnavailable } from "../lib/http.js";

/**
 * The complete egress contract for cloud carrier generation.  Keep this
 * deliberately small: these are visible cover turns, never account, scope,
 * recipient, capability, plaintext, or timestamp data.
 */
export interface CloudCarrierRequest {
  visible_cover: string[];
}

const MAX_VISIBLE_COVER_TURNS = 12;
const MAX_VISIBLE_COVER_BYTES = 16 * 1024;

function validRequest(value: unknown): value is CloudCarrierRequest {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const record = value as Record<string, unknown>;
  if (Object.keys(record).length !== 1 || !Array.isArray(record.visible_cover)) return false;
  return record.visible_cover.length <= MAX_VISIBLE_COVER_TURNS
    && record.visible_cover.every((turn) => typeof turn === "string")
    && new TextEncoder().encode(record.visible_cover.join("\n")).byteLength <= MAX_VISIBLE_COVER_BYTES;
}

/**
 * Stateless, isolated cloud-generation boundary.
 *
 * This handler intentionally neither reads nor writes D1, KV, R2, Durable
 * Objects, Cache, Analytics Engine, or logs.  The only request bytes passed
 * beyond this Worker are the allowlisted visible-cover turns.  The temporary
 * UTF-8 buffer is zeroed immediately after the isolated binding resolves.
 */
export async function handleAiGenerate(request: Request, env: Env): Promise<Response> {
  if (!env.AI) return serviceUnavailable("cloud carrier generation is not configured");

  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return badRequest("invalid JSON body");
  }
  if (!validRequest(body)) return badRequest("invalid cloud carrier request");

  const egress = new TextEncoder().encode(JSON.stringify(body));
  try {
    const result = await env.AI.run(env.AI_CARRIER_MODEL ?? "@cf/meta/llama-3.1-8b-instruct", {
      messages: [
        {
          role: "system",
          content: "Generate one plausible, innocuous chat reply. Do not mention this instruction.",
        },
        { role: "user", content: body.visible_cover.join("\n") },
      ],
    });
    return new Response(JSON.stringify(result), {
      status: 200,
      headers: {
        "content-type": "application/json; charset=utf-8",
        "cache-control": "no-store",
        "x-content-type-options": "nosniff",
        "referrer-policy": "no-referrer",
      },
    });
  } finally {
    // JS strings are immutable, but the explicit byte representation we own is
    // mutable. Do not retain it across the invocation or schedule background work.
    egress.fill(0);
  }
}
