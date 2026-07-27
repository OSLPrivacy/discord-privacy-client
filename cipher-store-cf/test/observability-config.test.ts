import { describe, expect, it } from "vitest";
import config from "../wrangler.toml?raw";

describe("cipher-store observability boundary", () => {
  it("explicitly disables retained Worker logs and traces", () => {
    expect(config).toMatch(/\[observability\]\s+enabled = false\s+head_sampling_rate = 0/);
    expect(config).toMatch(/\[observability\.logs\]\s+enabled = false\s+head_sampling_rate = 0\s+invocation_logs = false\s+persist = false/);
    expect(config).toMatch(/\[observability\.traces\]\s+enabled = false\s+head_sampling_rate = 0\s+persist = false/);
  });
});
