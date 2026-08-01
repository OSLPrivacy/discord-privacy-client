import { describe, expect, it } from "vitest";
import { unavailableOhttpKeyConfig } from "../src/endpoints/ohttp-keyconfig.js";

describe("OHTTP key configuration", () => {
  it("fails closed until an independently operated gateway is provisioned", async () => {
    const response = unavailableOhttpKeyConfig();

    expect(response.status).toBe(404);
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(new Uint8Array(await response.arrayBuffer())).toHaveLength(0);
  });
});
