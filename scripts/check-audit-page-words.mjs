#!/usr/bin/env node

// TASK 1523 — grades the audit page.
//
// The audit page has one job the rest of the site does not: it must name every
// problem OSL already knows about itself. That is only checkable if "every
// problem" is a record rather than an opinion, so this check re-derives the
// record from its two authorities before it looks at the page at all:
//
//   * data/pricing.json          — every capability whose status is not
//                                  `Available` is a known problem.
//   * docs/status/support-matrix.json — every entry with `claim_allowed: false`
//                                  is a known problem.
//
// A capability downgraded tomorrow becomes a missing entry today. The page
// count and the record count are printed side by side and compared; dropping a
// problem from the page fails this check instead of quietly shrinking the list.
//
// Usage: node scripts/check-audit-page-words.mjs [audit page] [limits page]

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const DEFAULT_AUDIT_PAGE = "audit.html";
const DEFAULT_LIMITS_PAGE = "tested-limits.html";
const RECORD_PATH = "data/tested-limits.json";
const PRICING_PATH = "data/pricing.json";
const MATRIX_PATH = "docs/status/support-matrix.json";

// Sentences the audit page may not lose. Each one is here because a page that
// drops it stops answering one of the five things the page exists to state:
// protections, limits, the inspection route, the Scrub risk, and Pro consent.
const REQUIRED_SENTENCES = [
  // Protections, each shipped with its own limit.
  ["protections", "Message contents are encrypted with a hybrid scheme combining X25519 and ML-KEM-768."],
  ["protections", "Discord still sees who you talk to, when, and how often. We cannot hide that."],
  ["protections", "Decrypted messages cached on your device are encrypted at rest. This does not cover attachments."],
  ["protections", "the key server refuses Discord's numeric account IDs outright"],
  // Pinned in data/pricing.json required_phrases for audit.html.
  ["licences", "The only exception is the optional AutoScrub module"],
  ["pro-consent", "No redemption record exists yet"],
  // Inspection route.
  ["inspection", "node scripts/check-audit-page-words.mjs"],
  ["inspection", "docs/design/osl-public-claim-allowlist.md"],
  ["inspection", "docs/THREAT_MODEL.md"],
  ["inspection", "docs/status/support-matrix.json"],
  // Scrub risk.
  ["scrub-risk", "This may break that service's rules and could terminate your account."],
  ["scrub-risk", "AutoScrub is not installed by default"],
  ["scrub-risk", "Cloud AutoScrub would send selected data to a server and is not end-to-end private."],
  ["scrub-risk", "Scrub itself does not delete anything for you."],
  // Pro consent.
  ["pro-consent", "$5 is the intended price for one month of Pro."],
  ["pro-consent", "Nothing renews automatically, and OSL never stores your payment details."],
  ["pro-consent", "Free stays free. Paying is never required for the basic encryption path to keep working."],
  ["pro-consent", "Pro is not consent to cloud processing."],
  ["pro-consent", "cloud-consent-required"],
];

const REQUIRED_LINKS = [
  ["tested-limits page", DEFAULT_LIMITS_PAGE],
  ["rebuild guide", "docs/release/verify-the-build-yourself.md"],
];

function repoPath(entry, label) {
  if (path.isAbsolute(entry) || entry.includes("..")) throw new Error(`unsafe ${label} path: ${entry}`);
  const fullPath = path.join(REPO_ROOT, entry);
  if (!existsSync(fullPath)) throw new Error(`${label} is missing: ${entry}`);
  return fullPath;
}

function readJson(entry, label) {
  return JSON.parse(readFileSync(repoPath(entry, label), "utf8"));
}

function decodeEntities(text) {
  return text
    .replaceAll("&nbsp;", " ")
    .replaceAll("&quot;", '"')
    .replaceAll("&#39;", "'")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
}

function stripHtml(markup) {
  return decodeEntities(
    markup
      .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ")
      .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ")
      .replace(/<!--[\s\S]*?-->/g, " ")
      .replace(/<[^>]+>/g, " "),
  )
    .replace(/\s+/g, " ")
    .trim();
}

// Each known problem is one <li data-limit="id"> … </li>. Returned in page
// order with its own plain text, so a problem can be checked for its record
// wording rather than for merely being mentioned somewhere on the page.
function limitItems(markup) {
  const items = [];
  const regex = /<li\b[^>]*\bdata-limit="([^"]+)"[^>]*>([\s\S]*?)<\/li>/gi;
  for (const match of markup.matchAll(regex)) {
    items.push({ id: decodeEntities(match[1]), text: stripHtml(match[2]) });
  }
  return items;
}

function countAttribute(markup, which) {
  const match = markup.match(new RegExp(`<strong\\s+data-limit-count="${which}"[^>]*>([^<]*)</strong>`, "i"));
  if (!match) return null;
  const value = Number.parseInt(match[1].trim(), 10);
  return Number.isNaN(value) ? null : value;
}

function linkedHrefs(markup) {
  return [...markup.matchAll(/<a\s+[^>]*href="([^"]+)"/gi)].map((match) => decodeEntities(match[1]));
}

function expectedRecordIds(pricing, matrix) {
  const ids = new Set();
  for (const row of pricing.capability_registry ?? []) {
    if (row.status !== "Available") ids.add(`cap:${row.id}`);
  }
  for (const row of matrix.versioned_public_support_matrix?.entries ?? []) {
    if (row.claim_allowed === false) ids.add(`matrix:${row.id}`);
  }
  return ids;
}

function checkRecord(record, pricing, matrix, errors) {
  const byId = new Map(record.limits.map((limit) => [limit.id, limit]));
  if (byId.size !== record.limits.length) errors.push("limits record contains duplicate ids");

  for (const id of expectedRecordIds(pricing, matrix)) {
    if (!byId.has(id)) errors.push(`limits record is missing a known problem its authority requires: ${id}`);
  }

  const capabilities = new Map((pricing.capability_registry ?? []).map((row) => [row.id, row]));
  const matrixRows = new Map((matrix.versioned_public_support_matrix?.entries ?? []).map((row) => [row.id, row]));

  for (const limit of record.limits) {
    for (const field of ["id", "title", "status", "problem", "source"]) {
      if (typeof limit[field] !== "string" || limit[field].trim() === "") {
        errors.push(`limits record entry ${limit.id} has no ${field}`);
      }
    }
    if (!Array.isArray(limit.evidence) || limit.evidence.length === 0) {
      errors.push(`limits record entry ${limit.id} names no evidence file`);
      continue;
    }
    for (const file of limit.evidence) {
      if (!existsSync(path.join(REPO_ROOT, file))) {
        errors.push(`limits record entry ${limit.id} cites a file that does not exist: ${file}`);
      }
    }
    if (limit.id.startsWith("cap:")) {
      const row = capabilities.get(limit.id.slice(4));
      if (!row) errors.push(`limits record entry ${limit.id} has no capability registry row`);
      else {
        if (row.status !== limit.status) errors.push(`${limit.id} status ${limit.status} does not match registry status ${row.status}`);
        if (row.public_note !== limit.problem) errors.push(`${limit.id} problem text does not match the registry public_note`);
      }
    }
    if (limit.id.startsWith("matrix:")) {
      const row = matrixRows.get(limit.id.slice(7));
      if (!row) errors.push(`limits record entry ${limit.id} has no support matrix row`);
      else if (row.support_boundary !== limit.problem) {
        errors.push(`${limit.id} problem text does not match the support matrix support_boundary`);
      }
    }
  }
}

function checkPageNamesEveryProblem(label, markup, record, errors) {
  const items = limitItems(markup);
  const seen = new Map();
  for (const item of items) seen.set(item.id, (seen.get(item.id) ?? 0) + 1);

  for (const limit of record.limits) {
    const count = seen.get(limit.id) ?? 0;
    if (count === 0) {
      errors.push(`${label} does not name known problem ${limit.id} (${limit.title})`);
      continue;
    }
    if (count > 1) errors.push(`${label} names ${limit.id} ${count} times`);
    const item = items.find((candidate) => candidate.id === limit.id);
    if (!item.text.includes(limit.problem)) {
      errors.push(`${label} states ${limit.id} in words other than the record's: ${item.text.slice(0, 120)}`);
    }
    if (!item.text.includes(limit.title)) errors.push(`${label} does not give ${limit.id} its recorded title`);
  }

  for (const id of seen.keys()) {
    if (!record.limits.some((limit) => limit.id === id)) errors.push(`${label} names ${id}, which is not in the limits record`);
  }

  return items.length;
}

function checkAuditPage(auditEntry, limitsEntry) {
  const errors = [];
  const record = readJson(RECORD_PATH, "limits record");
  const pricing = readJson(PRICING_PATH, "pricing manifest");
  const matrix = readJson(MATRIX_PATH, "support matrix");
  const auditMarkup = readFileSync(repoPath(auditEntry, "audit page"), "utf8");
  const limitsMarkup = readFileSync(repoPath(limitsEntry, "tested-limits page"), "utf8");
  const auditText = stripHtml(auditMarkup);

  if (record.schema !== "osl-tested-limits-v1") errors.push(`limits record schema is ${record.schema}`);
  if (!Array.isArray(record.limits) || record.limits.length === 0) {
    console.error("check-audit-page-words: the limits record holds no entries");
    process.exit(1);
  }

  checkRecord(record, pricing, matrix, errors);

  const recordCount = record.limits.length;
  const auditNamed = checkPageNamesEveryProblem("audit page", auditMarkup, record, errors);
  const limitsNamed = checkPageNamesEveryProblem("tested-limits page", limitsMarkup, record, errors);

  const auditPrinted = countAttribute(auditMarkup, "page");
  const auditPrintedRecord = countAttribute(auditMarkup, "record");
  if (auditPrinted !== auditNamed) errors.push(`audit page prints ${auditPrinted} known problems but names ${auditNamed}`);
  if (auditPrintedRecord !== recordCount) errors.push(`audit page prints a record count of ${auditPrintedRecord}; the record holds ${recordCount}`);
  if (auditNamed !== recordCount) errors.push(`audit page names ${auditNamed} known problems; the limits record holds ${recordCount}`);
  if (limitsNamed !== recordCount) errors.push(`tested-limits page names ${limitsNamed} known problems; the limits record holds ${recordCount}`);

  const hrefs = linkedHrefs(auditMarkup);
  for (const [label, href] of REQUIRED_LINKS) {
    if (!hrefs.includes(href)) errors.push(`audit page does not link to the ${label} (${href})`);
    else if (!existsSync(path.join(REPO_ROOT, href))) errors.push(`audit page links to ${href}, which does not exist`);
  }
  if (!linkedHrefs(limitsMarkup).includes(DEFAULT_AUDIT_PAGE)) {
    errors.push(`tested-limits page does not link back to ${DEFAULT_AUDIT_PAGE}`);
  }

  const missingSentences = [];
  for (const [section, sentence] of REQUIRED_SENTENCES) {
    if (!auditText.includes(sentence)) missingSentences.push(`${section}: ${sentence}`);
  }

  const pinned = (pricing.required_phrases ?? []).filter((row) => row.file === DEFAULT_AUDIT_PAGE);
  for (const row of pinned) {
    if (!auditText.includes(row.phrase)) missingSentences.push(`pricing required_phrases: ${row.phrase}`);
  }

  const forbiddenHits = [];
  const forbidden = [
    ...(pricing.forbidden_phrases ?? []).map((row) => row.phrase),
    ...(pricing.forbidden_claims ?? []).filter((row) => row.scope === "public-claims").map((row) => row.pattern ?? row.phrase),
  ].filter((phrase) => typeof phrase === "string" && phrase.length > 0);
  const haystack = auditText.toLowerCase();
  for (const phrase of forbidden) {
    if (haystack.includes(phrase.toLowerCase())) forbiddenHits.push(phrase);
  }

  console.log(`check-audit-page-words: ${auditEntry} + ${limitsEntry}`);
  console.log(`limits record entries: ${recordCount}`);
  console.log(`known problems named on the audit page: ${auditNamed}`);
  console.log(`known problems named on the tested-limits page: ${limitsNamed}`);
  console.log(`audit page printed count: ${auditPrinted}`);
  console.log(`count on the page equals the count in the limits record: ${auditNamed === recordCount && auditPrinted === recordCount}`);
  console.log(`tested-limits link: ${hrefs.includes(DEFAULT_LIMITS_PAGE) ? DEFAULT_LIMITS_PAGE : "MISSING"}`);
  console.log(`required sentences: ${REQUIRED_SENTENCES.length + pinned.length - missingSentences.length}/${REQUIRED_SENTENCES.length + pinned.length}`);
  console.log(`forbidden phrases found: ${forbiddenHits.length}`);

  for (const sentence of missingSentences) errors.push(`audit page is missing required wording — ${sentence}`);
  for (const phrase of forbiddenHits) errors.push(`audit page uses a forbidden phrase: ${phrase}`);

  if (errors.length > 0) throw new Error(errors.join("\n"));
  console.log("check-audit-page-words: complete.");
}

try {
  checkAuditPage(process.argv[2] || DEFAULT_AUDIT_PAGE, process.argv[3] || DEFAULT_LIMITS_PAGE);
} catch (error) {
  console.error(`check-audit-page-words: fatal error: ${error.message}`);
  process.exit(1);
}
