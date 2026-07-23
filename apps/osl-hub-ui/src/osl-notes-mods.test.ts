import { describe, expect, it } from "vitest";
import { parseNotesExtensionManifest } from "./osl-notes-mods";

describe("Notes extension manifests", () => {
  it("rejects ambient or unknown permissions", () => {
    expect(parseNotesExtensionManifest({ manifestVersion: 1, id: "example.mod", name: "Example", version: "1.0.0", permissions: ["notes:create"] })).not.toBeNull();
    expect(parseNotesExtensionManifest({ manifestVersion: 1, id: "example.mod", name: "Example", version: "1.0.0", permissions: ["network"] })).toBeNull();
  });
});
