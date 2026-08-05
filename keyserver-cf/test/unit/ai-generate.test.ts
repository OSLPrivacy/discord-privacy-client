import { describe, expect, it, vi } from "vitest";
import { handleAiGenerate } from "../../src/endpoints/ai-generate.js";
import type { Env } from "../../src/env.js";

/**
 * The shape `handleAiGenerate` actually passes to the isolated AI binding:
 * `env.AI.run(model, { messages })`.
 *
 * This type exists because the mocks below were declared as `vi.fn(async () =>
 * ...)` -- zero parameters. That made `run.mock.calls[0]` the empty tuple `[]`,
 * so `run.mock.calls[0]?.[1]` -- the model request that carries the egress
 * bytes, and the ONLY thing the egress-allowlist assertions in this file
 * inspect -- was statically `undefined` and had to be cast to be read at all.
 * The egress boundary is the point of these tests, so the argument they read
 * has to be a typed argument, not a cast off the end of an empty tuple.
 */
type CarrierModelRequest = {
  messages: Array<{ role: string; content: string }>;
};
type AiCarrierRun = (
  model: string,
  request: CarrierModelRequest,
) => Promise<{ response: string }>;

describe("T13-TE5 cloud generation retention boundary", () => {
  it("T13-TE2 serializes only the explicit visible-cover egress allowlist", async () => {
    const run = vi.fn<AiCarrierRun>(async () => ({ response: "ordinary cover text" }));
    const env = { AI: { run } } as unknown as Env;
    const response = await handleAiGenerate(
      new Request("https://keyserver.test/v1/ai/generate", {
        method: "POST",
        body: JSON.stringify({ visible_cover: ["earlier visible carrier", "latest visible carrier"] }),
      }),
      env,
    );

    expect(response.status).toBe(200);
    expect(run).toHaveBeenCalledOnce();
    const modelRequest = run.mock.calls[0]?.[1];
    if (modelRequest === undefined) throw new Error("the AI binding received no model request");
    const egressBytes = JSON.stringify(modelRequest.messages);
    expect(egressBytes).toContain("earlier visible carrier");
    expect(egressBytes).toContain("latest visible carrier");
    for (const forbidden of ["user_id", "scope", "capability", "contact", "plaintext", "timestamp"]) {
      expect(egressBytes).not.toContain(forbidden);
    }
  });

  it("uses only the isolated AI binding and leaves no D1 row after a generate call", async () => {
    const run = vi.fn<AiCarrierRun>(async () => ({ response: "ordinary cover text" }));
    const prepare = vi.fn(() => {
      throw new Error("a generation request must never touch D1");
    });
    const env = {
      AI: { run },
      DB: { prepare },
    } as unknown as Env;

    const response = await handleAiGenerate(
      new Request("https://keyserver.test/v1/ai/generate", {
        method: "POST",
        body: JSON.stringify({ visible_cover: ["Did you see the game?"] }),
      }),
      env,
    );

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ response: "ordinary cover text" });
    expect(prepare).not.toHaveBeenCalled();
    expect(run).toHaveBeenCalledOnce();
    expect(run.mock.calls[0]?.[1]).toEqual({
      messages: expect.arrayContaining([
        expect.objectContaining({ content: "Did you see the game?" }),
      ]),
    });
  });

  it("rejects fields outside the egress allowlist before the model boundary", async () => {
    const run = vi.fn();
    const env = { AI: { run } } as unknown as Env;
    const response = await handleAiGenerate(
      new Request("https://keyserver.test/v1/ai/generate", {
        method: "POST",
        body: JSON.stringify({ visible_cover: ["cover"], user_id: "must-not-leave" }),
      }),
      env,
    );
    expect(response.status).toBe(400);
    expect(run).not.toHaveBeenCalled();
  });
});
