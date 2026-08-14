/**
 * TASK 7051 — derive copy semantics from the page tree, never a vocabulary.
 *
 * Design files contain made-up people and messages.  Those values are not a
 * product promise, but the headings, controls, and prose around them are.  A
 * semantic list is the boundary: its repeated rows carry demo content; all
 * other visible text is shipping copy.  No person, address, message, date, or
 * filename appears in this classifier.
 */

const ignoredTags = new Set(["script", "style", "template", "helmet"]);
const headingTags = new Set(["h1", "h2", "h3", "h4", "h5", "h6"]);
const controlTags = new Set(["button", "label", "option", "select", "textarea", "input"]);
const listTags = new Set(["ul", "ol", "table", "tbody", "thead"]);
const rowTags = new Set(["li", "tr"]);

function cleanText(value) {
  return value
    .replace(/&nbsp;/giu, " ")
    .replace(/&amp;/giu, "&")
    .replace(/&lt;/giu, "<")
    .replace(/&gt;/giu, ">")
    .replace(/&#39;/giu, "'")
    .replace(/&quot;/giu, '"')
    .replace(/\s+/gu, " ")
    .trim();
}

function attrs(source) {
  const result = {};
  for (const match of source.matchAll(/([:\w-]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/gu)) {
    const name = match[1].toLowerCase();
    if (name) result[name] = match[2] ?? match[3] ?? match[4] ?? "";
  }
  return result;
}

function element(tag, attributes, parent) {
  return { type: "element", tag, attributes, parent, children: [], path: "" };
}

/** A deliberately small, dependency-free HTML tree for captured D27 markup. */
export function parsePage(markup) {
  if (typeof markup !== "string") throw new TypeError("page markup must be a string");
  const root = element("root", {}, null);
  const stack = [root];
  const voidTags = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);
  for (const token of markup.matchAll(/<!--[\s\S]*?-->|<[^>]*>|[^<]+/gu)) {
    const value = token[0];
    if (value.startsWith("<!--")) continue;
    if (value.startsWith("</")) {
      const tag = value.slice(2, -1).trim().toLowerCase();
      for (let index = stack.length - 1; index > 0; index -= 1) {
        if (stack[index].tag === tag) {
          stack.length = index;
          break;
        }
      }
      continue;
    }
    if (value.startsWith("<")) {
      if (value.startsWith("<!") || value.startsWith("<?")) continue;
      const match = /^<\s*([\w-]+)/u.exec(value);
      if (!match) continue;
      const tag = match[1].toLowerCase();
      const node = element(tag, attrs(value.slice(match[0].length, value.endsWith(">") ? -1 : undefined)), stack.at(-1));
      stack.at(-1).children.push(node);
      if (!voidTags.has(tag) && !/\/\s*>$/u.test(value)) stack.push(node);
      continue;
    }
    stack.at(-1).children.push({ type: "text", value, parent: stack.at(-1), path: "" });
  }
  const assignPaths = (node) => {
    const counts = new Map();
    for (const child of node.children) {
      const key = child.type === "text" ? "#text" : child.tag;
      const count = (counts.get(key) ?? 0) + 1;
      counts.set(key, count);
      child.path = `${node.path}/${key}[${count}]`;
      if (child.type === "element") assignPaths(child);
    }
  };
  root.path = "";
  assignPaths(root);
  return root;
}

function walk(node, visit) {
  visit(node);
  if (node.type === "element") for (const child of node.children) walk(child, visit);
}

function hasRole(node, role) {
  return node.attributes.role?.split(/\s+/u).includes(role) ?? false;
}

function hasData(node, suffix) {
  return Object.keys(node.attributes).some((name) => name === `data-${suffix}` || name.endsWith(`-${suffix}`));
}

function isList(node) {
  return node.type === "element" && (listTags.has(node.tag) || hasRole(node, "list") || hasRole(node, "listbox") || hasRole(node, "grid") || hasData(node, "list"));
}

function isRow(node) {
  return node.type === "element" && (rowTags.has(node.tag) || hasRole(node, "listitem") || hasRole(node, "row") || hasData(node, "row"));
}

function descendants(node) {
  const found = [];
  walk(node, (candidate) => found.push(candidate));
  return found;
}

function rowShape(node) {
  if (node.type === "text") return "#text";
  const children = node.children.filter((child) => child.type === "element" && !ignoredTags.has(child.tag));
  return `${node.tag}[${children.map(rowShape).join(",")}]`;
}

function listName(node) {
  return cleanText(node.attributes["aria-label"] || node.attributes["data-list-name"] || node.attributes.id || "") || `list at ${node.path}`;
}

function listRows(node) {
  const direct = node.children.filter((child) => child.type === "element" && isRow(child));
  if (direct.length) return direct;
  // A role/data list may use neutral div rows.  Its direct children are still
  // the repeated structural unit; no text content informs this decision.
  if (hasRole(node, "list") || hasRole(node, "listbox") || hasRole(node, "grid") || hasData(node, "list")) {
    return node.children.filter((child) => child.type === "element");
  }
  return [];
}

function textKind(node) {
  let current = node.parent;
  while (current) {
    if (controlTags.has(current.tag)) return "control label";
    if (headingTags.has(current.tag)) return "heading";
    if (hasRole(current, "alert") || hasRole(current, "alertdialog") || hasData(current, "warning") || hasData(current, "error")) return "warning or error wording";
    if (hasRole(current, "dialog") && hasData(current, "consent") || hasData(current, "consent")) return "consent wording";
    current = current.parent;
  }
  return "fixed sentence";
}

function dataField(node, field) {
  return Object.keys(node.attributes).some((name) => name === `data-${field}` || name.endsWith(`-${field}`));
}

/** Dynamic fields are identified by their element/attribute position, not text. */
function standaloneDemoKind(node) {
  let current = node.parent;
  while (current) {
    if (current.tag === "time") return "timestamp";
    if (current.tag === "img" || hasRole(current, "img")) return "avatar";
    if (dataField(current, "demo-content")) return "demo-content field";
    if (dataField(current, "count")) return "count field";
    if (dataField(current, "timestamp")) return "timestamp field";
    if (dataField(current, "avatar")) return "avatar field";
    if (dataField(current, "sample-file")) return "sample-file field";
    current = current.parent;
  }
  return null;
}

function textKey(node, kind) {
  return `${kind}:${node.path}`;
}

/**
 * Returns an exact shipping-copy inventory and content-blind demo-list shape.
 * Slots intentionally have no literal value in either side of the comparison.
 */
export function classifyPageText(markup) {
  const root = parsePage(markup);
  const demoNodes = new Set();
  const demoLists = [];
  const demo = [];
  walk(root, (node) => {
    if (!isList(node)) return;
    const rows = listRows(node);
    if (!rows.length) return;
    const shapes = new Set();
    for (const row of rows) {
      for (const child of descendants(row)) demoNodes.add(child);
      const shape = rowShape(row);
      shapes.add(shape);
    }
    demoLists.push({ key: node.path, name: listName(node), rows: rows.length, repeats: [...shapes].sort((left, right) => left.localeCompare(right)) });
  });
  const shipping = [];
  walk(root, (node) => {
    if (node.type !== "text") return;
    let parent = node.parent;
    while (parent && ignoredTags.has(parent.tag)) parent = parent.parent;
    if (!parent || ignoredTags.has(parent.tag)) return;
    const text = cleanText(node.value);
    if (!text) return;
    if (/^\{\{[\s\S]*\}\}$/u.test(text)) {
      demo.push({ key: textKey(node, "slot"), kind: "slot value" });
      return;
    }
    const kind = textKind(node);
    // A repeated row makes its body dynamic, never its headings, controls, or
    // explicit warning/error/consent copy.
    if (demoNodes.has(node) && kind === "fixed sentence") {
      demo.push({ key: textKey(node, "row"), kind: "repeated-row text" });
      return;
    }
    const field = standaloneDemoKind(node);
    if (field && kind === "fixed sentence") {
      demo.push({ key: textKey(node, "field"), kind: field });
      return;
    }
    shipping.push({ key: textKey(node, kind), kind, text });
  });
  // Placeholders are control labels even though they live in attributes.
  walk(root, (node) => {
    if (node.type !== "element" || !controlTags.has(node.tag)) return;
    for (const field of ["placeholder", "value", "aria-label", "title"]) {
      const text = cleanText(node.attributes[field] ?? "");
      if (text && !/^\{\{[\s\S]*\}\}$/u.test(text)) shipping.push({ key: `control label:${node.path}@${field}`, kind: "control label", text });
    }
  });
  walk(root, (node) => {
    if (node.type === "element" && node.tag === "img" && demoNodes.has(node)) demo.push({ key: `${node.path}@img`, kind: "repeated-row avatar" });
  });
  return { shipping, demo, demoLists };
}

function quoted(value) { return JSON.stringify(value); }

/** Compare exact shipping wording plus demo-list existence, repeat shape, and count. */
export function comparePageText({ page, route, designMarkup, buildMarkup }) {
  let design;
  let build;
  try {
    design = classifyPageText(designMarkup);
    build = classifyPageText(buildMarkup);
  } catch (error) {
    return { ok: false, findings: [`design page ${quoted(page)}: build route ${quoted(route)} cannot compare text inventory (${error.message}).`] };
  }
  const findings = [];
  const designCopy = new Map(design.shipping.map((entry) => [entry.key, entry]));
  const buildCopy = new Map(build.shipping.map((entry) => [entry.key, entry]));
  for (const [key, entry] of designCopy) {
    const actual = buildCopy.get(key);
    if (!actual) findings.push(`design page ${quoted(page)}: shipping ${entry.kind} ${quoted(entry.text)} is missing from build route ${quoted(route)}.`);
    else if (actual.text !== entry.text) findings.push(`design page ${quoted(page)}: shipping ${entry.kind} differs at ${quoted(entry.text)} — build route ${quoted(route)} says ${quoted(actual.text)}.`);
  }
  for (const [key, entry] of buildCopy) if (!designCopy.has(key)) findings.push(`design page ${quoted(page)}: build route ${quoted(route)} adds shipping ${entry.kind} ${quoted(entry.text)}.`);

  const designLists = new Map(design.demoLists.map((list) => [list.key, list]));
  const buildLists = new Map(build.demoLists.map((list) => [list.key, list]));
  for (const [key, list] of designLists) {
    const actual = buildLists.get(key);
    if (!actual) {
      findings.push(`design page ${quoted(page)}: demo list ${quoted(list.name)} is missing from build route ${quoted(route)}.`);
      continue;
    }
    if (JSON.stringify(actual.repeats) !== JSON.stringify(list.repeats)) findings.push(`design page ${quoted(page)}: demo list ${quoted(list.name)} repeats a different row shape on build route ${quoted(route)}.`);
    if (actual.rows !== list.rows) findings.push(`design page ${quoted(page)}: demo list ${quoted(list.name)} has different row count — design has ${list.rows} rows and build route ${quoted(route)} has ${actual.rows}.`);
  }
  for (const [key, list] of buildLists) if (!designLists.has(key)) findings.push(`design page ${quoted(page)}: build route ${quoted(route)} adds demo list ${quoted(list.name)}.`);
  return { ok: findings.length === 0, findings };
}
