import releaseScopeNote from "virtual:release-scope-note";

export type ReleaseExclusionId = "code-signing" | "osl-notes" | "osl-mail";

export interface ReleaseExclusion {
  id: ReleaseExclusionId;
  name: string;
  ruling_date: string;
  owner_words: string;
  release_note: string;
  tile_label?: string;
}

interface ReleaseExclusionNote {
  owner: string;
  items: ReleaseExclusion[];
}

const NOTE_BLOCK = /```json release-exclusions\s*\n([\s\S]*?)\n```/u;
const REQUIRED_IDS: readonly ReleaseExclusionId[] = ["code-signing", "osl-notes", "osl-mail"];

export function parseReleaseScopeNote(note: string): ReleaseExclusionNote {
  const block = note.match(NOTE_BLOCK)?.[1];
  if (!block) throw new Error("release scope note has no release-exclusions block");

  const parsed = JSON.parse(block) as Partial<ReleaseExclusionNote>;
  if (parsed.owner !== "Liam" || !Array.isArray(parsed.items)) {
    throw new Error("release scope note must name Liam and contain items");
  }
  const ids = parsed.items.map((item) => item.id);
  if (ids.length !== REQUIRED_IDS.length || REQUIRED_IDS.some((id) => ids.filter((candidate) => candidate === id).length !== 1)) {
    throw new Error("release scope note must name code-signing, osl-notes, and osl-mail exactly once");
  }
  for (const item of parsed.items) {
    if (!item.name || !/^\d{4}-\d{2}-\d{2}$/u.test(item.ruling_date) || !item.owner_words || !item.release_note) {
      throw new Error(`release scope exclusion ${item.id} is incomplete`);
    }
    if ((item.id === "osl-notes" || item.id === "osl-mail") && !item.tile_label) {
      throw new Error(`release scope exclusion ${item.id} has no honest tile label`);
    }
  }
  return parsed as ReleaseExclusionNote;
}

export const releaseScopeExclusions = parseReleaseScopeNote(releaseScopeNote);

export function releaseScopeExclusion(id: ReleaseExclusionId): ReleaseExclusion {
  const item = releaseScopeExclusions.items.find((candidate) => candidate.id === id);
  if (!item) throw new Error(`release scope exclusion ${id} is missing`);
  return item;
}
