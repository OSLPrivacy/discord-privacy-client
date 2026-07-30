import { describe, expect, it } from "vitest";
import type { OslNote } from "./osl-notes";
import { OSL_PROPERTY_QUERY_LIMITS, parseOslPropertyQuery, runOslPropertyQuery } from "./osl-property-query";

const note = (id: string, body: string): OslNote => ({ id: id.repeat(32), kind: "note", title: id, body, folder: "", tags: [], favorite: false, pinned: false, createdAt: 1, updatedAt: 1, deletedAt: null });
const query = (overrides: Record<string, unknown> = {}) => ({ version: 1, predicates: [], sorts: [], columns: ["Status"], ...overrides });

describe("bounded local property queries", () => {
  const notes = [
    note("a", '---\nStatus: "Doing"\nEstimate: 3\nDone: false\nDue: 2026-08-03\nPeople: ["Liam","Sam"]\n---\nA'),
    note("b", '---\nStatus: "Done"\nEstimate: 8\nDone: true\nDue: 2026-07-01\nPeople: ["Sam"]\n---\nB'),
    note("c", '---\nStatus: "Doing"\nEstimate: 3\nDone: true\nDue: 2026-08-01\nPeople: []\n---\nC'),
  ];

  it("filters with typed operators and projects only requested columns", () => {
    const rows = runOslPropertyQuery(notes, query({ predicates: [
      { property: "Status", type: "text", operator: "contains", value: "oin" },
      { property: "Estimate", type: "number", operator: "gte", value: 3 },
      { property: "People", type: "list", operator: "contains", value: "Liam" },
      { property: "Due", type: "date", operator: "lt", value: "2026-09-01" },
      { property: "Done", type: "checkbox", operator: "not-eq", value: true },
    ], columns: ["Status", "Estimate"] }));
    expect(rows?.map((row) => row.note.id)).toEqual(["a".repeat(32)]);
    expect(rows?.[0].columns).toEqual([{ name: "Status", property: { name: "Status", type: "text", value: "Doing" } }, { name: "Estimate", property: { name: "Estimate", type: "number", value: 3 } }]);
  });

  it("treats missing and genuinely empty values explicitly without coercion", () => {
    expect(runOslPropertyQuery(notes, query({ predicates: [{ property: "People", type: "list", operator: "is-empty" }] }))?.map((row) => row.note.title)).toEqual(["c"]);
    expect(runOslPropertyQuery(notes, query({ predicates: [{ property: "Unknown", type: "text", operator: "is-empty" }] }))?.length).toBe(3);
    expect(runOslPropertyQuery(notes, query({ predicates: [{ property: "Estimate", type: "text", operator: "eq", value: "3" }] }))).toEqual([]);
  });

  it("sorts by typed values, puts missing values last, and preserves input order for ties", () => {
    const extra = note("d", "No frontmatter");
    const rows = runOslPropertyQuery([notes[0], notes[2], notes[1], extra], query({ sorts: [{ property: "Estimate", type: "number", direction: "asc" }] }));
    expect(rows?.map((row) => row.note.title)).toEqual(["a", "c", "b", "d"]);
    const descending = runOslPropertyQuery([notes[0], extra, notes[2], notes[1]], query({ sorts: [{ property: "Estimate", type: "number", direction: "desc" }] }));
    expect(descending?.map((row) => row.note.title)).toEqual(["b", "a", "c", "d"]);
  });

  it("rejects unknown keys, invalid operator shapes, control/bidi text, and unsafe bounds", () => {
    expect(parseOslPropertyQuery({ ...query(), surprise: true })).toBeNull();
    expect(parseOslPropertyQuery(query({ predicates: [{ property: "Due", type: "date", operator: "gt", value: "tomorrow" }] }))).toBeNull();
    expect(parseOslPropertyQuery(query({ predicates: [{ property: "Status", type: "text", operator: "is-empty", value: "" }] }))).toBeNull();
    expect(parseOslPropertyQuery(query({ columns: ["Safe\u202eName"] }))).toBeNull();
    expect(parseOslPropertyQuery(query({ predicates: Array.from({ length: OSL_PROPERTY_QUERY_LIMITS.predicates + 1 }, () => ({ property: "Status", type: "text", operator: "eq", value: "Doing" })) }))).toBeNull();
    expect(parseOslPropertyQuery(query({ columns: ["x".repeat(41)] }))).toBeNull();
  });

  it("fails closed for malformed note frontmatter and oversized note sets", () => {
    expect(runOslPropertyQuery([note("f", "---\nDue: 2026-02-30\n---\nBody")], query())).toBeNull();
    expect(runOslPropertyQuery(Array.from({ length: OSL_PROPERTY_QUERY_LIMITS.notes + 1 }, () => notes[0]), query())).toBeNull();
  });
});
