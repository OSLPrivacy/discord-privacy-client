import { describe, expect, it } from "vitest";
import { destructStatusMarkup } from "./destruct-status";

function renderedText(markup: string): string {
  return markup.replace(/<[^>]*>/gu, " ").replace(/\s+/gu, " ").trim();
}

describe("T2-81 two-tier destructive status", () => {
  it("keeps an offline burn's completed local deletion separate from its pending server deletion", () => {
    const output = renderedText(destructStatusMarkup({
      action: "burn",
      local: "complete",
      server: "pending",
    }));

    expect(output).toContain("On this device: Deleted from this device.");
    expect(output).toContain("On the server: Removing from the server when you're back online.");
    expect(output).not.toMatch(/burning|burn complete|finished/iu);
  });

  it("states expiry's device and server enforcement as separate facts", () => {
    const output = renderedText(destructStatusMarkup({
      action: "expiry",
      local: "complete",
      server: "confirmed",
    }));

    expect(output).toContain("On this device: Expired on this device.");
    expect(output).toContain("On the server: The server confirmed that it stops being downloadable.");
    expect(output).not.toMatch(/expiry complete|finished/iu);
  });

  it("never presents an unconfirmed server outcome as a completed action", () => {
    const output = renderedText(destructStatusMarkup({
      action: "expiry",
      local: "complete",
      server: "not-confirmed",
    }));

    expect(output).toContain("On this device: Expired on this device.");
    expect(output).toContain("On the server: Server expiry was not confirmed.");
    expect(output).not.toContain("The server confirmed");
  });
});
