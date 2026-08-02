import { describe, expect, it, vi } from "vitest";
import { handleAiGenerate } from "../../src/endpoints/ai-generate.js";
import type { Env } from "../../src/env.js";

describe("T13-TE5 cloud generation retention boundary", () => {
  it("uses only the isolated AI binding and leaves no D1 row after a generate call", async () => {
    const run = vi.fn(async () => ({ response: "ordinary cover text" }));
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
