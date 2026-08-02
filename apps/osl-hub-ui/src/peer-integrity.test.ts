import { describe, expect, it } from "vitest";
import { peerIntegrityMarkup } from "./peer-integrity";

describe("T10-T13 peer integrity disclosure", () => {
  it("keeps reported matching, unpublished, and not-reported states visibly distinct", () => {
    const matching = peerIntegrityMarkup("matching");
    const unpublished = peerIntegrityMarkup("unpublished");
    const unknown = peerIntegrityMarkup("unknown");
    expect(matching).toContain("peer-integrity--matching");
    expect(unpublished).toContain("peer-integrity--unpublished");
    expect(unknown).toContain("peer-integrity--unknown");
    expect(unknown).toContain("has not confirmed screenshot protection");
    expect(new Set([matching, unpublished, unknown]).size).toBe(3);
  });
});
