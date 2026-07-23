import { describe, expect, it } from "vitest";
import { parseOslNote, parseOslNotes } from "./osl-notes";

const note = {
  id: "a".repeat(32),
  title: "Private",
  body: "Local only",
  createdAt: 1,
  updatedAt: 2,
};

describe("OSL Notes renderer boundary", () => {
  it("strictly parses bounded native notes", () => {
    expect(parseOslNote(note)).toEqual(note);
    expect(parseOslNote({ ...note, plaintextServerCopy: true })).toBeNull();
    expect(parseOslNotes([note, note])).toBeNull();
  });
});
