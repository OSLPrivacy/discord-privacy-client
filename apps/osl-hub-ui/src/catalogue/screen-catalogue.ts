import packagedCatalogue from "../../../../crates/english-catalogue/catalogues/en-US.v1.json";
import bindingsDocument from "./task-5209-screen-bindings.json";

type Binding = { runtimePath: string; key: string; sourceDigest: string };
type BindingDocument = { surfaces: Array<{ name: string; items: Binding[] }> };
type CatalogueDocument = { entries: Array<{ key: string; value: string }> };

const catalogue = new Map((packagedCatalogue as CatalogueDocument).entries.map(({ key, value }) => [key, value]));
const bindings = new Map((bindingsDocument as BindingDocument).surfaces.map((surface) => [surface.name, new Map(surface.items.map((item) => [item.runtimePath, item]))]));
const personVisibleAttributes = ["aria-label", "placeholder", "title", "alt"] as const;

function digest(value: string): string {
  let hash = 2166136261;
  for (const character of value) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function normalized(value: string): string {
  return value.replace(/\s+/gu, " ").trim();
}

type Replacement = { start: number; end: number; value: string };

/**
 * Resolves every fixed OSL-owned word in a shipping surface through the exact
 * packaged 5205 JSON. Variable person/content data is identified by its
 * pre-migration source digest and remains untouched.
 */
export function catalogueScreenMarkup(surface: string, markup: string): string {
  const surfaceBindings = bindings.get(surface);
  if (!surfaceBindings) throw new Error(`5209 missing packaged surface binding: ${surface}`);
  const replacements: Replacement[] = [];
  let tagOrdinal = 0;
  const tagPattern = /<([a-z][a-z0-9-]*)([^>]*)>([^<]*)/gisu;
  for (const match of markup.matchAll(tagPattern)) {
    const tag = match[1].toLowerCase();
    const attributeSource = match[2];
    tagOrdinal += 1;
    for (const attribute of personVisibleAttributes) {
      const attributePattern = new RegExp(`(${attribute}\\s*=\\s*(["']))(.*?)\\2`, "isu");
      const attributeMatch = attributePattern.exec(attributeSource);
      if (!attributeMatch) continue;
      const copy = normalized(attributeMatch[3]);
      if (!copy) continue;
      const runtimePath = `${surface}/${tag}[${tagOrdinal}]/@${attribute}`;
      const binding = surfaceBindings.get(runtimePath);
      if (!binding || digest(copy) !== binding.sourceDigest) continue;
      const value = catalogue.get(binding.key);
      if (value === undefined) throw new Error(`5209 missing key ${binding.key} at ${runtimePath}`);
      const attributeOffset = match[0].indexOf(attributeSource);
      const valueOffset = attributeMatch.index + attributeMatch[1].length;
      replacements.push({ start: (match.index ?? 0) + attributeOffset + valueOffset, end: (match.index ?? 0) + attributeOffset + valueOffset + attributeMatch[3].length, value });
    }
    const rawCopy = match[3];
    const copy = normalized(rawCopy);
    if (!copy) continue;
    const runtimePath = `${surface}/${tag}[${tagOrdinal}]/text`;
    const binding = surfaceBindings.get(runtimePath);
    if (!binding || digest(copy) !== binding.sourceDigest) continue;
    const value = catalogue.get(binding.key);
    if (value === undefined) throw new Error(`5209 missing key ${binding.key} at ${runtimePath}`);
    const rawOffset = match[0].length - rawCopy.length;
    replacements.push({ start: (match.index ?? 0) + rawOffset, end: (match.index ?? 0) + rawOffset + rawCopy.length, value });
  }
  let resolved = markup;
  for (const replacement of replacements.sort((left, right) => right.start - left.start)) {
    resolved = `${resolved.slice(0, replacement.start)}${replacement.value}${resolved.slice(replacement.end)}`;
  }
  return resolved;
}
