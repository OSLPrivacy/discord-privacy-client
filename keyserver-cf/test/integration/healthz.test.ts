import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

describe("GET /v1/healthz", () => {
  it("returns the required schema capability without auth", async () => {
    const res = await SELF.fetch("http://test/v1/healthz");
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({
      ok: true,
      revision: "MAPLE-0439-revision",
      build_time: "2026-08-06T07:43:09Z",
      configuration_name: "production-test",
      capabilities: {
        control_inbox_eviction_signal: 1,
        control_inbox_sender_disposition: 1,
      },
    });
  });

  it("reports live revision metadata from the worker target", async () => {
    const res = await SELF.fetch("http://test/v1/healthz");
    const body = await res.json() as Record<string, unknown>;
    console.log(
      `command=GET /v1/healthz revision=${body.revision} build_time=${body.build_time} configuration_name=${body.configuration_name}`,
    );

    expect(body.revision).toBe("MAPLE-0439-revision");
    expect(body.build_time).toBe("2026-08-06T07:43:09Z");
    expect(body.configuration_name).toBe("production-test");
  });

  it("404s for unknown paths", async () => {
    const res = await SELF.fetch("http://test/nope");
    expect(res.status).toBe(404);
  });
});
