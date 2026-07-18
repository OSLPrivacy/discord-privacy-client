import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const config = readFileSync(new URL("../wrangler.toml", import.meta.url), "utf8");

describe("cipher-store observability boundary", () => {
  it("explicitly disables retained Worker logs and traces", () => {
    expect(config).toMatch(/\[observability\]\s+enabled = false\s+head_sampling_rate = 0/);
    expect(config).toMatch(/\[observability\.logs\]\s+enabled = false\s+head_sampling_rate = 0\s+invocation_logs = false\s+persist = false/);
    expect(config).toMatch(/\[observability\.traces\]\s+enabled = false\s+head_sampling_rate = 0\s+persist = false/);
  });
});
