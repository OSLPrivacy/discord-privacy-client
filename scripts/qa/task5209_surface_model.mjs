import { createHash } from "node:crypto";

const PERSON_VISIBLE_ATTRIBUTES = ["aria-label", "placeholder", "title", "alt"];

export function surfaceSlug(surface) {
  return surface.replace(/[^a-z0-9]+/gu, ".").replace(/^\.|\.$/gu, "");
}

export function sourceDigest(value) {
  let hash = 2166136261;
  for (const character of value) {
    hash ^= character.codePointAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

function attributesOf(source) {
  const attributes = new Map();
  for (const match of source.matchAll(/([a-zA-Z_:][-a-zA-Z0-9_:.]*)\s*=\s*(["'])(.*?)\2/gsu)) {
    attributes.set(match[1], match[3]);
  }
  return attributes;
}

function normalizeCopy(copy) {
  return copy.replace(/\s+/gu, " ").trim();
}

function semanticClass(copy, action) {
  const value = `${copy} ${action}`.toLowerCase();
  if (/burn|delete|erase|remove|revoke|clear activation|cannot be undone/u.test(value)) return "destructive";
  if (/password|encrypt|protect|privacy|security|verify|verification|key|recovery|trust/u.test(value)) return "security";
  if (/cancel|close|back|not now|keep/u.test(value)) return "safe";
  return "ordinary";
}

function invokedAction(tag, attributes, copy, ordinal) {
  if (!["button", "a", "input", "select", "textarea"].includes(tag)) return "display";
  if (attributes.has("id")) return `invoke:${attributes.get("id")}`;
  const handler = [...attributes.keys()].find((name) => name.startsWith("data-") && !name.endsWith("-state"));
  if (handler) return `invoke:${handler}`;
  if (attributes.get("type") === "submit") return "invoke:form-submit";
  return `invoke:${tag}-${ordinal}:${normalizeCopy(copy).toLowerCase().replace(/[^a-z0-9]+/gu, "-") || "unnamed"}`;
}

export function visibleItems(surface, markup) {
  const items = [];
  let tagOrdinal = 0;
  for (const match of markup.matchAll(/<([a-z][a-z0-9-]*)([^>]*)>([^<]*)/gisu)) {
    const tag = match[1].toLowerCase();
    const attributes = attributesOf(match[2]);
    tagOrdinal += 1;
    for (const attribute of PERSON_VISIBLE_ATTRIBUTES) {
      const copy = normalizeCopy(attributes.get(attribute) ?? "");
      if (!copy) continue;
      const action = invokedAction(tag, attributes, copy, tagOrdinal);
      items.push({
        surface,
        runtimePath: `${surface}/${tag}[${tagOrdinal}]/@${attribute}`,
        control: attributes.get("id") ?? [...attributes.keys()].find((name) => name.startsWith("data-")) ?? `${tag}[${tagOrdinal}]`,
        copy,
        action,
        class: semanticClass(copy, action),
        kind: attribute.replace("-", "_"),
      });
    }
    const copy = normalizeCopy(match[3]);
    if (copy) {
      const action = invokedAction(tag, attributes, copy, tagOrdinal);
      items.push({
        surface,
        runtimePath: `${surface}/${tag}[${tagOrdinal}]/text`,
        control: attributes.get("id") ?? [...attributes.keys()].find((name) => name.startsWith("data-")) ?? `${tag}[${tagOrdinal}]`,
        copy,
        action,
        class: semanticClass(copy, action),
        kind: "text",
      });
    }
  }
  return items.map((item, ordinal) => ({
    ...item,
    key: `screen.${surfaceSlug(surface)}.${item.kind}.n${String(ordinal + 1).padStart(3, "0")}`,
    meaning: `5209.${surfaceSlug(surface)}.${item.kind}.${String(ordinal + 1).padStart(3, "0")}`,
    digest: createHash("sha256").update(item.copy).digest("hex"),
    sourceDigest: sourceDigest(item.copy),
  }));
}

export function sortedSurfaceNames(surfaces) {
  return surfaces.map(({ name }) => name).sort();
}
