import { parseOslProperties } from "./osl-properties";

export interface OslKnowledgeNote { id: string; title: string; body: string; deletedAt?: number | null; }
export interface OslWikiLink {
  raw: string;
  embed: boolean;
  note: string;
  label: string | null;
  heading: string | null;
  blockId: string | null;
  start: number;
  end: number;
}
export interface OslKnowledgeIndex {
  notesById: ReadonlyMap<string, OslKnowledgeNote>;
  idsByName: ReadonlyMap<string, readonly string[]>;
}

const MAX_BODY = 262_144;
const MAX_LINKS = 512;
const MAX_LINK_INNER = 512;
const DEFAULT_MAX_OUTPUT = 100_000;
const DEFAULT_MAX_DEPTH = 8;

const folded = (value: string): string => value.trim().normalize("NFKC").toLocaleLowerCase();
const safePart = (value: string, maximum: number): boolean => value.length > 0 && value.length <= maximum && !/[\u0000-\u001f\u007f\u202a-\u202e\u2066-\u2069]/u.test(value);

function parseInner(raw: string, embed: boolean, start: number, end: number): OslWikiLink | null {
  if (!safePart(raw, MAX_LINK_INNER)) return null;
  const pipe = raw.indexOf("|");
  if (pipe >= 0 && raw.indexOf("|", pipe + 1) >= 0) return null;
  const target = (pipe < 0 ? raw : raw.slice(0, pipe)).trim();
  const label = pipe < 0 ? null : raw.slice(pipe + 1).trim();
  if (label !== null && !safePart(label, 240)) return null;

  const hash = target.indexOf("#");
  if (hash >= 0 && target.indexOf("#", hash + 1) >= 0) return null;
  const note = (hash < 0 ? target : target.slice(0, hash)).trim();
  const fragment = hash < 0 ? null : target.slice(hash + 1).trim();
  if (!safePart(note, 240) || (fragment !== null && !safePart(fragment, 240))) return null;
  const blockId = fragment?.startsWith("^") ? fragment.slice(1) : null;
  if (blockId !== null && !/^[\p{L}\p{N}_-]{1,80}$/u.test(blockId)) return null;
  const heading = fragment !== null && blockId === null ? fragment : null;

  return { raw: `${embed ? "!" : ""}[[${raw}]]`, embed, note, label, heading, blockId, start, end };
}

/** Parses only complete, bounded wiki references. Malformed references are ignored. */
export function parseOslWikiLinks(body: string): OslWikiLink[] {
  if (body.length > MAX_BODY) return [];
  const links: OslWikiLink[] = [];
  for (let cursor = 0; cursor < body.length && links.length < MAX_LINKS;) {
    const open = body.indexOf("[[", cursor);
    if (open < 0) break;
    const start = open > 0 && body[open - 1] === "!" ? open - 1 : open;
    const close = body.indexOf("]]", open + 2);
    if (close < 0) break;
    const inner = body.slice(open + 2, close);
    const parsed = parseInner(inner, start !== open, start, close + 2);
    if (parsed) links.push(parsed);
    cursor = close + 2;
  }
  return links;
}

function aliasesFor(note: OslKnowledgeNote): string[] {
  const document = parseOslProperties(note.body);
  if (!document) return [];
  const aliasProperties = document.properties.filter((property) => folded(property.name) === "aliases");
  if (aliasProperties.length !== 1 || aliasProperties[0].type !== "list") return [];
  return (aliasProperties[0].value as string[]).filter((alias) => safePart(alias.trim(), 240));
}

export function createOslKnowledgeIndex(notes: readonly OslKnowledgeNote[]): OslKnowledgeIndex {
  const notesById = new Map<string, OslKnowledgeNote>();
  const idsByName = new Map<string, string[]>();
  for (const note of notes.slice(0, 5_000)) {
    if (!note.id || notesById.has(note.id) || note.deletedAt != null || !safePart(note.title.trim(), 240) || note.body.length > MAX_BODY) continue;
    notesById.set(note.id, note);
    const names = new Set([folded(note.title), ...aliasesFor(note).map(folded)]);
    for (const name of names) {
      if (!name) continue;
      const ids = idsByName.get(name) ?? [];
      ids.push(note.id);
      idsByName.set(name, ids);
    }
  }
  return { notesById, idsByName };
}

/** Returns null when a target is missing or matches more than one title/alias. */
export function resolveOslKnowledgeTarget(index: OslKnowledgeIndex, target: string): OslKnowledgeNote | null {
  if (!safePart(target.trim(), 240)) return null;
  const ids = index.idsByName.get(folded(target));
  return ids?.length === 1 ? index.notesById.get(ids[0]) ?? null : null;
}

function contentFor(note: OslKnowledgeNote): string {
  return parseOslProperties(note.body)?.content ?? note.body;
}

function selectedMarkdown(note: OslKnowledgeNote, link: OslWikiLink): string | null {
  const body = contentFor(note);
  const lines = body.split(/\r?\n/u);
  if (link.blockId) {
    const marker = new RegExp(`(?:^|\\s)\\^${link.blockId.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&")}\\s*$`, "u");
    const line = lines.find((candidate) => marker.test(candidate));
    return line === undefined ? null : line.replace(marker, "").trimEnd();
  }
  if (link.heading) {
    const wanted = folded(link.heading);
    const start = lines.findIndex((line) => { const match = /^(#{1,6})\s+(.+?)\s*#*\s*$/u.exec(line); return match !== null && folded(match[2]) === wanted; });
    if (start < 0) return null;
    const level = /^(#{1,6})/u.exec(lines[start])![1].length;
    let end = lines.length;
    for (let index = start + 1; index < lines.length; index += 1) { const next = /^(#{1,6})\s+/u.exec(lines[index]); if (next && next[1].length <= level) { end = index; break; } }
    return lines.slice(start + 1, end).join("\n").trim();
  }
  return body;
}

function markdownToPlainText(value: string): string {
  return value
    .replace(/^```[^\n]*\n?|```$/gmu, "")
    .replace(/^#{1,6}\s+/gmu, "")
    .replace(/^\s*>\s?/gmu, "")
    .replace(/^\s*[-*+]\s+/gmu, "")
    .replace(/!\[([^\]]*)\]\([^)]*\)/gu, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/gu, "$1")
    .replace(/[*_~=`]/gu, "")
    .trim();
}

export interface OslTransclusionOptions { maxDepth?: number; maxChars?: number; }

/** Expands embeds from unlocked in-memory notes only and returns bounded plain text. */
export function transcludeOslNote(noteId: string, notes: readonly OslKnowledgeNote[], options: OslTransclusionOptions = {}): string | null {
  const index = createOslKnowledgeIndex(notes);
  const root = index.notesById.get(noteId);
  if (!root) return null;
  const maxDepth = Math.max(0, Math.min(DEFAULT_MAX_DEPTH, Math.trunc(options.maxDepth ?? DEFAULT_MAX_DEPTH)));
  const maxChars = Math.max(1, Math.min(DEFAULT_MAX_OUTPUT, Math.trunc(options.maxChars ?? DEFAULT_MAX_OUTPUT)));
  let expansions = 0;

  const expand = (markdown: string, path: ReadonlySet<string>, depth: number): string => {
    let output = ""; let cursor = 0;
    for (const link of parseOslWikiLinks(markdown)) {
      if (!link.embed) continue;
      output += markdown.slice(cursor, link.start);
      const target = resolveOslKnowledgeTarget(index, link.note);
      if (!target || path.has(target.id) || depth >= maxDepth || expansions >= MAX_LINKS) output += link.raw;
      else {
        const selection = selectedMarkdown(target, link);
        if (selection === null) output += link.raw;
        else { expansions += 1; output += expand(selection, new Set([...path, target.id]), depth + 1); }
      }
      cursor = link.end;
      if (output.length >= maxChars * 2) break;
    }
    return output + markdown.slice(cursor);
  };
  return markdownToPlainText(expand(contentFor(root), new Set([root.id]), 0)).slice(0, maxChars);
}

/** Returns the center and all directionless neighbours within one or two hops. */
export function getOslLocalGraphIds(centerId: string, notes: readonly OslKnowledgeNote[], hops: 1 | 2 = 1): string[] {
  const index = createOslKnowledgeIndex(notes);
  if (!index.notesById.has(centerId)) return [];
  const adjacency = new Map<string, Set<string>>([...index.notesById.keys()].map((id) => [id, new Set()]));
  for (const note of index.notesById.values()) for (const link of parseOslWikiLinks(note.body)) {
    const target = resolveOslKnowledgeTarget(index, link.note);
    if (!target || target.id === note.id) continue;
    adjacency.get(note.id)!.add(target.id); adjacency.get(target.id)!.add(note.id);
  }
  const seen = new Set([centerId]); let frontier = [centerId];
  for (let depth = 0; depth < hops; depth += 1) { const next: string[] = []; for (const id of frontier) for (const neighbour of adjacency.get(id) ?? []) if (!seen.has(neighbour)) { seen.add(neighbour); next.push(neighbour); } frontier = next; }
  return [...seen];
}
