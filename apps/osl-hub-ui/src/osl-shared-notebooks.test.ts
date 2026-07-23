import { describe, expect, it } from "vitest";
import { parseSharedNotebookInvite, sharedNotebooksAvailability } from "./osl-shared-notebooks";

describe("shared notebook boundary", () => {
  it("strictly parses an opaque invite and remains fail-closed without transport", () => {
    expect(parseSharedNotebookInvite({ version: 1, notebookId: "a".repeat(32), capability: "A".repeat(43) })).not.toBeNull();
    expect(parseSharedNotebookInvite({ version: 1, notebookId: "a".repeat(32), capability: "A".repeat(43), endpoint: "https://example.com" })).toBeNull();
    expect(sharedNotebooksAvailability.available).toBe(false);
  });
});
