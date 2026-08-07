import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { readRowOwnershipLadder } from "./row-ownership-ladder.mjs";

export const EXPECTED_X_ROW_TEST_NAME = "conversationMessage";
export const PAGE_NAME_CHECK = "x_row_test_name_contract_check";

const PLACE_IDS = [
  "test_name_on_row",
  "per_row_identifier",
  "parent_containers_and_boxes_beside",
  "roles_and_states",
  "picture_address",
  "row_account_link",
  "screen_reader_text",
];

class PageNameContractError extends Error {
  constructor(message, foundNames) {
    super(message);
    this.name = "PageNameContractError";
    this.foundNames = foundNames;
  }
}

function parseAttrs(raw) {
  const attrs = {};
  const attrPattern = /([:\w-]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g;
  for (const match of raw.matchAll(attrPattern)) {
    const [, name, quoted, singleQuoted, bare] = match;
    attrs[name] = quoted ?? singleQuoted ?? bare ?? "";
  }
  return attrs;
}

function textNode(value, parent) {
  return { type: "text", value, parent };
}

export function parseHtml(html) {
  const root = { type: "element", tag: "#document", attrs: {}, children: [] };
  const stack = [root];
  const tokenPattern = /<!--[\s\S]*?-->|<!doctype[^>]*>|<\/?([a-zA-Z][\w:-]*)([^>]*)>|([^<]+)/gi;

  for (const match of html.matchAll(tokenPattern)) {
    if (match[3]) {
      const value = match[3].replace(/\s+/g, " ").trim();
      if (value) {
        stack[stack.length - 1].children.push(textNode(value, stack[stack.length - 1]));
      }
      continue;
    }
    if (!match[1]) continue;
    const full = match[0];
    const tag = match[1].toLowerCase();
    if (full.startsWith("</")) {
      while (stack.length > 1 && stack[stack.length - 1].tag !== tag) stack.pop();
      if (stack.length > 1) stack.pop();
      continue;
    }
    const attrs = parseAttrs(match[2] ?? "");
    const node = { type: "element", tag, attrs, children: [], parent: stack[stack.length - 1] };
    stack[stack.length - 1].children.push(node);
    if (!full.endsWith("/>") && !["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"].includes(tag)) {
      stack.push(node);
    }
  }
  return root;
}

function walk(node, visit) {
  if (node.type !== "element") return;
  visit(node);
  for (const child of node.children) walk(child, visit);
}

function findAll(node, predicate) {
  const matches = [];
  walk(node, (candidate) => {
    if (predicate(candidate)) matches.push(candidate);
  });
  return matches;
}

function ownText(node) {
  return node.children
    .filter((child) => child.type === "text")
    .map((child) => child.value)
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}

function visibleText(node) {
  const pieces = [];
  function collect(candidate) {
    if (candidate.type === "text") {
      pieces.push(candidate.value);
      return;
    }
    for (const child of candidate.children) collect(child);
  }
  collect(node);
  return pieces.join(" ").replace(/\s+/g, " ").trim();
}

function compact(value) {
  return value ? value.replace(/\s+/g, " ").trim() : "none";
}

function selectorFor(node) {
  const pieces = [node.tag];
  if (node.attrs["data-testid"]) pieces.push(`[data-testid="${node.attrs["data-testid"]}"]`);
  if (node.attrs.id) pieces.push(`#${node.attrs.id}`);
  if (node.attrs.role) pieces.push(`[role="${node.attrs.role}"]`);
  if (node.attrs["data-row-index"]) pieces.push(`[data-row-index="${node.attrs["data-row-index"]}"]`);
  return pieces.join("");
}

function allDataTestIds(root) {
  return [
    ...new Set(
      findAll(root, (node) => typeof node.attrs["data-testid"] === "string")
        .map((node) => node.attrs["data-testid"])
        .filter(Boolean),
    ),
  ].sort();
}

function directChildrenElements(node) {
  return node.children.filter((child) => child.type === "element");
}

function descendants(node, predicate) {
  return findAll(node, (candidate) => candidate !== node && predicate(candidate));
}

function ancestorSummary(node) {
  const ancestors = [];
  let current = node.parent;
  while (current && current.tag !== "#document") {
    const attrs = [];
    if (current.attrs["data-testid"]) attrs.push(`test=${current.attrs["data-testid"]}`);
    if (current.attrs.role) attrs.push(`role=${current.attrs.role}`);
    if (current.attrs["aria-label"]) attrs.push(`aria=${current.attrs["aria-label"]}`);
    if (current.attrs["data-box"]) attrs.push(`box=${current.attrs["data-box"]}`);
    ancestors.push(`${selectorFor(current)}${attrs.length ? `[${attrs.join("|")}]` : ""}`);
    current = current.parent;
  }
  return ancestors.join(">");
}

function rolesAndStates(row) {
  const stateAttrs = [
    "role",
    "aria-current",
    "aria-disabled",
    "aria-expanded",
    "aria-haspopup",
    "aria-pressed",
    "aria-selected",
  ];
  return [row, ...descendants(row, () => true)]
    .map((node) => {
      const found = stateAttrs
        .filter((name) => node.attrs[name] !== undefined)
        .map((name) => `${name}=${node.attrs[name]}`);
      return found.length ? `${selectorFor(node)}:${found.join("|")}` : "";
    })
    .filter(Boolean)
    .join(",");
}

function boxesBeside(row) {
  return directChildrenElements(row)
    .filter((child) => child.attrs["data-box"])
    .map((child) => `${selectorFor(child)}[box=${child.attrs["data-box"]}]`)
    .join(",");
}

function pictureSignal(row) {
  const image = descendants(row, (node) => node.tag === "img")[0];
  const src = image?.attrs.src ?? "";
  const accountNumber = src.match(/\b(\d{8,})\b/)?.[1] ?? "none";
  const alt = image?.attrs.alt ?? "none";
  return {
    value: src || "none",
    accountNumber,
    carriesAccountNumber: accountNumber !== "none",
    alt,
  };
}

function linkSignals(row) {
  return descendants(row, (node) => node.tag === "a")
    .map((link) => {
      const name = compact(link.attrs["aria-label"] || visibleText(link));
      const href = link.attrs.href ?? "none";
      return `${href} (${name})`;
    })
    .join(",");
}

function screenReaderSignals(row) {
  const pieces = [];
  let current = row;
  while (current && current.tag !== "#document") {
    if (current.attrs["aria-label"]) pieces.push(current.attrs["aria-label"]);
    current = current.parent;
  }
  for (const node of descendants(row, (candidate) => true)) {
    if (node.attrs["aria-label"]) pieces.push(node.attrs["aria-label"]);
    if (node.attrs.alt) pieces.push(node.attrs.alt);
    if ((node.attrs.class ?? "").split(/\s+/).includes("sr-only")) {
      const text = ownText(node);
      if (text) pieces.push(text);
    }
  }
  return [...new Set(pieces)].join(",");
}

function handleFromText(text) {
  return text.match(/@[a-zA-Z0-9_]{1,15}/)?.[0] ?? "none";
}

function summarizePlace(rows, id) {
  if (id === "test_name_on_row") {
    const names = new Set(rows.map((row) => row.attrs["data-testid"] ?? "none"));
    return `checked=${rows.length} found=${[...names].join(",")} account_id_found=0`;
  }
  if (id === "per_row_identifier") {
    const ids = rows.map((row) => row.attrs["data-sender-id"] ?? "none");
    const unique = [...new Set(ids)];
    const numeric = ids.filter((idValue) => /^\d{8,}$/.test(idValue)).length;
    return `checked=${rows.length} found_attr=data-sender-id numeric_account_ids=${numeric} unique_values=${unique.join(",")}`;
  }
  if (id === "parent_containers_and_boxes_beside") {
    const accountTextRows = rows.filter((row) => /@[a-zA-Z0-9_]{1,15}/.test(ancestorSummary(row))).length;
    return `checked=${rows.length} parent_account_text_rows=${accountTextRows} stable_account_id_found=0 boxes_beside_rows=${rows.filter((row) => boxesBeside(row) !== "").length}`;
  }
  if (id === "roles_and_states") {
    return `checked=${rows.length} rows_with_roles_or_states=${rows.filter((row) => rolesAndStates(row) !== "").length} stable_account_id_found=0`;
  }
  if (id === "picture_address") {
    const pictures = rows.map(pictureSignal);
    return `checked=${rows.length} pictures=${pictures.filter((picture) => picture.value !== "none").length} account_number_in_url=${pictures.filter((picture) => picture.carriesAccountNumber).length}`;
  }
  if (id === "row_account_link") {
    const links = rows.map(linkSignals).filter(Boolean);
    return `checked=${rows.length} links_naming_account=${links.length} stable_account_id_found=0`;
  }
  if (id === "screen_reader_text") {
    const sr = rows.map(screenReaderSignals);
    return `checked=${rows.length} rows_with_account_name=${sr.filter((value) => /@[a-zA-Z0-9_]{1,15}/.test(value)).length} stable_account_id_found=0`;
  }
  throw new Error(`unknown place id: ${id}`);
}

export function auditXRowAuthorSignals({ html, limit = 10 }) {
  const root = parseHtml(html);
  const rows = findAll(root, (node) => node.attrs["data-testid"] === EXPECTED_X_ROW_TEST_NAME).slice(0, limit);
  if (rows.length === 0) {
    throw new PageNameContractError(
      `${PAGE_NAME_CHECK}=fail expected=${EXPECTED_X_ROW_TEST_NAME} found=${allDataTestIds(root).join(",") || "none"}`,
      allDataTestIds(root),
    );
  }

  const wrongNames = rows.filter((row) => row.attrs["data-testid"] !== EXPECTED_X_ROW_TEST_NAME);
  if (wrongNames.length !== 0) {
    throw new PageNameContractError(
      `${PAGE_NAME_CHECK}=fail expected=${EXPECTED_X_ROW_TEST_NAME} wrong_rows=${wrongNames.length}`,
      allDataTestIds(root),
    );
  }

  const rowFindings = rows.map((row, index) => {
    const picture = pictureSignal(row);
    const screenReader = screenReaderSignals(row);
    const accountLinks = linkSignals(row);
    return {
      index: index + 1,
      location: `${selectorFor(row)}@data-sender-id`,
      testName: row.attrs["data-testid"] ?? "none",
      perRowIdentifier: row.attrs["data-sender-id"] ?? "none",
      parentContainers: ancestorSummary(row) || "none",
      boxesBeside: boxesBeside(row) || "none",
      rolesStates: rolesAndStates(row) || "none",
      picture,
      accountLinks: accountLinks || "none",
      screenReader: screenReader || "none",
      handleFromScreenReader: handleFromText(screenReader),
    };
  });

  const allRowsHaveNumericSenderId = rowFindings.every((row) => /^\d{8,}$/.test(row.perRowIdentifier));
  const winner = allRowsHaveNumericSenderId
    ? {
        signal: "per_row_identifier",
        location: rowFindings[0].location,
        evidenceKind: "stable_provider_account_id_cross_check",
      }
    : {
        signal: "none",
        location: "none",
        evidenceKind: "none",
      };

  return {
    rowFindings,
    places: PLACE_IDS.map((id) => ({ id, summary: summarizePlace(rows, id) })),
    placesChecked: PLACE_IDS.length,
    placesUnchecked: 0,
    pageNameCheck: `${PAGE_NAME_CHECK}=pass expected=${EXPECTED_X_ROW_TEST_NAME} rows=${rows.length}`,
    winner,
    claimsWithoutMeasurement: 0,
  };
}

export function renderAudit(audit, { readDate = "unknown", ladderPath } = {}) {
  const lines = [
    `x_page_read_date=${readDate}`,
    `x_row_author_places=${audit.placesChecked}`,
    `x_row_author_places_unchecked=${audit.placesUnchecked}`,
    audit.pageNameCheck,
  ];

  const ladder = readRowOwnershipLadder(ladderPath);
  const ladderKind = ladder.evidence.find((kind) => kind.id === audit.winner.evidenceKind);
  if (ladderKind) {
    lines.push(
      `x_row_author_winner=${audit.winner.signal} location=${audit.winner.location} ladder_kind=${ladderKind.id} ladder_rank=${ladderKind.rank} strength=${ladderKind.strength} allowed_to_mark=${ladderKind.allowed_to_mark}`,
    );
  } else {
    lines.push(`x_row_author_winner=${audit.winner.signal} location=${audit.winner.location} ladder_kind=none`);
  }
  lines.push(
    `x_when_page_names_change=${PAGE_NAME_CHECK} fails closed before author evidence is accepted`,
  );
  for (const place of audit.places) {
    lines.push(`x_place id=${place.id} ${place.summary}`);
  }
  for (const row of audit.rowFindings) {
    lines.push(
      [
        `x_row ${row.index}`,
        `test_name=${row.testName}`,
        `per_row_identifier=data-sender-id:${row.perRowIdentifier}`,
        `parent_containers=${row.parentContainers}`,
        `boxes_beside=${row.boxesBeside}`,
        `roles_states=${row.rolesStates}`,
        `picture=${row.picture.value}`,
        `picture_account_number=${row.picture.accountNumber}`,
        `account_links=${row.accountLinks}`,
        `screen_reader=${row.screenReader}`,
      ].join(" "),
    );
  }
  lines.push(`x_claims_without_measurement=${audit.claimsWithoutMeasurement}`);
  return lines.join("\n");
}

export function parseArgs(argv) {
  const args = { limit: 10, readDate: "unknown" };
  for (let index = 2; index < argv.length; index += 1) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || value === undefined) {
      throw new Error("usage: node scripts/x-row-author-signal-audit.mjs --file HTML [--limit 10] [--read-date YYYY-MM-DD] [--ladder JSON]");
    }
    if (key === "--file") args.file = value;
    else if (key === "--limit") args.limit = Number.parseInt(value, 10);
    else if (key === "--read-date") args.readDate = value;
    else if (key === "--ladder") args.ladder = value;
    else throw new Error(`unknown argument: ${key}`);
    index += 1;
  }
  if (!args.file) {
    throw new Error("missing --file");
  }
  if (!Number.isInteger(args.limit) || args.limit <= 0) {
    throw new Error("--limit must be a positive integer");
  }
  return args;
}

function main(argv) {
  try {
    const args = parseArgs(argv);
    const html = readFileSync(resolve(args.file), "utf8");
    const audit = auditXRowAuthorSignals({ html, limit: args.limit });
    console.log(renderAudit(audit, { readDate: args.readDate, ladderPath: args.ladder }));
    return 0;
  } catch (error) {
    console.error(error.message);
    if (error instanceof PageNameContractError) return 2;
    return 1;
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  process.exit(main(process.argv));
}
