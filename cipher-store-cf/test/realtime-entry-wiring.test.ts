import { describe, expect, it } from "vitest";

import { PushConnection } from "../src/index.js";

describe("T1-51 realtime Durable Object entry wiring", () => {
  it("exports the class named by the shipping Wrangler binding", () => {
    expect(PushConnection.name).toBe("PushConnection");
  });
});
