import { describe, expect, it } from "vitest";
import { createOslKnowledgeIndex, getOslLocalGraphIds, parseOslWikiLinks, resolveOslKnowledgeTarget, transcludeOslNote, type OslKnowledgeNote } from "./osl-knowledge";

const note = (id: string, title: string, body: string): OslKnowledgeNote => ({ id, title, body });

describe("bounded local knowledge references", () => {
  it("parses labelled, heading, block, and embedded references structurally", () => {
    const links = parseOslWikiLinks("[[Note]] [[Note|Label]] [[Note#Heading]] [[Note#^block-1]] ![[Note]]");
    expect(links.map(({ embed, note, label, heading, blockId }) => ({ embed, note, label, heading, blockId }))).toEqual([
      { embed: false, note: "Note", label: null, heading: null, blockId: null },
      { embed: false, note: "Note", label: "Label", heading: null, blockId: null },
      { embed: false, note: "Note", label: null, heading: "Heading", blockId: null },
      { embed: false, note: "Note", label: null, heading: null, blockId: "block-1" },
      { embed: true, note: "Note", label: null, heading: null, blockId: null },
    ]);
    expect(parseOslWikiLinks(`[[${"x".repeat(513)}]]`)).toEqual([]);
    expect(parseOslWikiLinks("[[bad|two|labels]] [[bad\nlink]]")).toEqual([]);
  });

  it("resolves portable Aliases lists and fails closed on ambiguity", () => {
    const notes = [
      note("a", "Alpha", '---\nAliases: ["A","First"]\n---\nBody'),
      note("b", "Beta", "Body"),
    ];
    const index = createOslKnowledgeIndex(notes);
    expect(resolveOslKnowledgeTarget(index, " first ")?.id).toBe("a");
    expect(resolveOslKnowledgeTarget(index, "BETA")?.id).toBe("b");
    const ambiguous = createOslKnowledgeIndex([...notes, note("c", "A", "Body")]);
    expect(resolveOslKnowledgeTarget(ambiguous, "a")).toBeNull();
  });

  it("transcludes whole notes, headings, and blocks as bounded plain text", () => {
    const notes = [
      note("root", "Root", "# Root\n![[Details#Install]]\n![[Details#^answer]]"),
      note("details", "Details", "# Intro\nSkip\n## Install\n**Run** setup.\n### Child\nThen test.\n## Other\nAnswer here ^answer"),
    ];
    expect(transcludeOslNote("root", notes)).toBe("Root\nRun setup.\nChild\nThen test.\nAnswer here");
    expect(transcludeOslNote("root", notes, { maxChars: 12 })).toHaveLength(12);
  });

  it("is cycle-safe and leaves unresolved or ambiguous embeds visible", () => {
    const cycle = [note("a", "A", "Before ![[B]]"), note("b", "B", "Inside ![[A]]")];
    expect(transcludeOslNote("a", cycle)).toBe("Before Inside ![[A]]");
    const ambiguous = [note("root", "Root", "![[Same]]"), note("x", "Same", "x"), note("y", "Same", "y")];
    expect(transcludeOslNote("root", ambiguous)).toBe("![[Same]]");
  });

  it("returns deterministic directionless one-hop and two-hop local graph IDs", () => {
    const notes = [note("a", "A", "[[B]]"), note("b", "B", "[[C]]"), note("c", "C", "[[D]]"), note("d", "D", "")];
    expect(getOslLocalGraphIds("b", notes, 1)).toEqual(["b", "a", "c"]);
    expect(getOslLocalGraphIds("b", notes, 2)).toEqual(["b", "a", "c", "d"]);
  });
});
