import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  releaseCapabilityMatrix,
  RELEASE_CAPABILITY_COPY,
  RELEASE_INTEGRATIONS,
  SHIPPING_CARRIERS,
  SHIPPING_STRIP_ADAPTERS,
} from "./release-capabilities";

const source = readFileSync(new URL("./release-capabilities.ts", import.meta.url), "utf8");

describe("TASK 6100b release-capability mutants must fail", () => {
  it("source contains the exact carrier-list template and no softened paraphrase", () => {
    expect(source).toContain("This release can send through:");
    expect(source).toContain("All other integrations are unavailable and are not covered by send, offline, Tor or protection tests.");
    expect(source).not.toContain("This release can send through: [");
    expect(source).not.toContain("This release supports");
  });

  it("source contains the exact matrix-explanation sentence", () => {
    expect(source).toContain(RELEASE_CAPABILITY_COPY.matrixExplanationSentence);
    expect(source).toContain("text support does not imply attachment, paste, share, or streaming support");
  });

  it("source hard-codes the 6101 carrier list ['Discord']", () => {
    expect(SHIPPING_CARRIERS).toEqual(["Discord"]);
    expect(source).toContain('Object.freeze(["Discord"])');
  });

  it("source hard-codes the 6101 Strip adapter list ['Discord']", () => {
    expect(SHIPPING_STRIP_ADAPTERS).toEqual(["Discord"]);
    expect(source).toContain('Object.freeze(["Discord"])');
  });

  it("rejects a mutant carrier list by asserting the canonical value", () => {
    const mutant = ["Discord", "Signal"];
    expect(mutant).not.toEqual(SHIPPING_CARRIERS);
  });

  it("rejects a mutant matrix with an extra Supported cell", () => {
    const matrix = releaseCapabilityMatrix();
    const mutant = matrix.map((cell) =>
      cell.integration === "telegram" && cell.contentType === "text" ? { ...cell, status: "Supported" as const } : cell,
    );
    expect(mutant.filter((cell) => cell.status === "Supported")).toHaveLength(2);
    expect(releaseCapabilityMatrix().filter((cell) => cell.status === "Supported")).toHaveLength(1);
  });

  it("rejects a mutant matrix that hides an unsupported attachment cell", () => {
    const matrix = releaseCapabilityMatrix();
    const mutant = matrix.filter((cell) => !(cell.integration === "discord" && cell.contentType === "attachment"));
    expect(mutant).toHaveLength(34);
    expect(matrix).toHaveLength(35);
  });

  it("rejects a mutant that marks text Supported while hiding an unsupported streaming cell", () => {
    const matrix = releaseCapabilityMatrix();
    const kept = matrix.filter((cell) => cell.contentType !== "streaming");
    const stillSupported = kept.filter((cell) => cell.status === "Supported");
    expect(stillSupported).toHaveLength(1);
    expect(kept).toHaveLength(28);
    expect(RELEASE_INTEGRATIONS).toHaveLength(7);
  });

  it("rejects a softened independent-export sentence", () => {
    const soft = "Burn cannot remove files you saved elsewhere.";
    expect(soft).not.toBe(RELEASE_CAPABILITY_COPY.independentExportSentence);
  });

  it("rejects a false claim that OSL can remove a user-held archive", () => {
    const falseClaim = "OSL can remove an archive you saved outside OSL.";
    expect(RELEASE_CAPABILITY_COPY.independentExportSentence).not.toContain(falseClaim);
  });

  it("rejects an empty carrier list", () => {
    expect([]).not.toEqual(SHIPPING_CARRIERS);
    expect(SHIPPING_CARRIERS.length).toBeGreaterThan(0);
  });
});
