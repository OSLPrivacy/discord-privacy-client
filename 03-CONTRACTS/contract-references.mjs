//! Resolver for cross-document references made by a contract in `03-CONTRACTS/`.
//!
//! W3-8 / D-272. `spaces.md` deferred its space-event envelope and capability
//! rules to ``[`transport.md`](transport.md) §6b``, and the gate that was meant
//! to hold that deferral honest asserted only that the **link text** was
//! present. The link's target did not exist, so the gate passed over a void and
//! the deferred rules were never written.
//!
//! The durable fix is here, not in the prose: a contract reference is only a
//! reference if it RESOLVES. This module extracts every typed reference a
//! contract makes and reports the ones whose referent is absent, so a gate can
//! check the target rather than the link.
//!
//! A "typed reference" is a reference written in a form that promises
//! resolution. Two forms are recognised, because contracts in this directory
//! use both:
//!
//!   1. a Markdown link            `[text](target.md)` / `[text](target.md#anchor)`
//!   2. a backticked file name     `` `target.md` ``
//!
//! Either form may be followed by a section token (`§6b`), which is checked
//! against the target's headings. Prose that merely names a document without
//! using one of these forms is not a typed reference and is not checked --
//! this module reports voids, it does not police vocabulary.
//!
//! Deliberately NOT supported: an escape hatch for a reference whose target
//! lives outside this repository. A contract in this repository may not make a
//! typed reference to a document this repository does not contain -- that is
//! precisely the condition that produced D-272. Cite an external document in
//! prose, naming the repository that holds it.

import { readFileSync, existsSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";

const MARKDOWN_LINK = /\[((?:[^[\]]|\[[^[\]]*\])*)\]\(([^()\s]+)\)/g;
const BACKTICKED_FILE = /`([A-Za-z0-9._/-]+\.md)`/g;
const TRAILING_SECTION = /^[\s,;]*(§{1,2}\s*[0-9]+[0-9A-Za-z.]*)/;
const EXTERNAL_SCHEME = /^(?:https?|mailto|tel|data|ftp):/i;

/** GitHub-flavoured heading slug, so `#some-heading` can be checked. */
export function headingSlug(text) {
  return text
    .replace(/\[([^\]]*)\]\([^()]*\)/g, "$1") // link -> its text
    .replace(/[`*_~]/g, "")
    .toLowerCase()
    .replace(/[^\w\- ]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

function headingsOf(source) {
  const headings = [];
  for (const match of source.matchAll(/^[ \t]{0,3}(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*$/gm)) {
    headings.push(match[2]);
  }
  return headings;
}

function anchorsOf(source) {
  const anchors = new Set(headingsOf(source).map(headingSlug));
  for (const match of source.matchAll(/<a\s+(?:name|id)="([^"]+)"/gi)) {
    anchors.add(match[1].toLowerCase());
  }
  // An explicit `Anchor: Name` line, the convention used by
  // `docs/reports/telegram-adapter-verdict.md`.
  for (const match of source.matchAll(/^Anchor:\s*(\S+)\s*$/gim)) {
    anchors.add(match[1].toLowerCase());
  }
  return anchors;
}

/**
 * Does `source` contain a heading that opens the section named by `token`?
 *
 * `§6b` must be matched by a heading such as `### 6b. \`delivery_tag\` ...`.
 * The boundary check is load-bearing: `§1` must NOT be satisfied by a heading
 * `## 1b. Group sends`, or the check would accept a neighbouring section and
 * wave through exactly the kind of near-miss it exists to catch.
 */
export function hasSection(source, token) {
  const number = token.replace(/^§+/, "").replace(/\s+/g, "").replace(/\.$/, "");
  if (number === "") return false;
  const escaped = number.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`^\\s*(?:§\\s*)?${escaped}(?![0-9A-Za-z])`);
  return headingsOf(source).some((heading) => pattern.test(heading));
}

/** Every typed reference `contractPath` makes, in source order. */
export function extractReferences(source) {
  const references = [];
  const lines = source.split("\n");

  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const seen = new Set();

    for (const match of line.matchAll(MARKDOWN_LINK)) {
      const [path, anchor] = match[2].split("#");
      if (EXTERNAL_SCHEME.test(match[2]) || match[2].startsWith("#") || path === "") continue;
      const tail = line.slice(match.index + match[0].length).match(TRAILING_SECTION);
      seen.add(path);
      references.push({
        form: "markdown-link",
        text: match[0],
        target: path,
        anchor: anchor ?? null,
        section: tail ? tail[1].replace(/\s+/g, "") : null,
        line: index + 1,
      });
    }

    for (const match of line.matchAll(BACKTICKED_FILE)) {
      // A backticked name that is already the text of a Markdown link on this
      // line has been recorded once; do not report the same void twice.
      if (seen.has(match[1])) continue;
      const tail = line.slice(match.index + match[0].length).match(TRAILING_SECTION);
      references.push({
        form: "backticked-file",
        text: match[0],
        target: match[1],
        anchor: null,
        section: tail ? tail[1].replace(/\s+/g, "") : null,
        line: index + 1,
      });
    }
  }

  return references;
}

/**
 * Resolve every typed reference `contractPath` makes.
 *
 * Returns one entry per reference whose referent is absent, each naming the
 * citing line, the target, and which half is missing -- the file, the anchor,
 * or the section. An empty array means every reference resolves.
 */
export function unresolvedReferences(contractPath) {
  const source = readFileSync(contractPath, "utf8");
  const base = dirname(contractPath);
  const problems = [];

  for (const reference of extractReferences(source)) {
    const targetPath = resolve(base, reference.target);
    const where = `${contractPath}:${reference.line}`;

    if (!existsSync(targetPath) || !statSync(targetPath).isFile()) {
      problems.push({
        ...reference,
        problem: "missing-file",
        detail: `${where} cites ${reference.text} but ${reference.target} does not exist`,
      });
      continue;
    }

    const targetSource = readFileSync(targetPath, "utf8");

    if (reference.anchor && !anchorsOf(targetSource).has(decodeURIComponent(reference.anchor).toLowerCase())) {
      problems.push({
        ...reference,
        problem: "missing-anchor",
        detail: `${where} cites ${reference.target}#${reference.anchor} but ${reference.target} has no such anchor`,
      });
    }

    if (reference.section && !hasSection(targetSource, reference.section)) {
      problems.push({
        ...reference,
        problem: "missing-section",
        detail: `${where} cites ${reference.target} ${reference.section} but ${reference.target} has no section ${reference.section}`,
      });
    }
  }

  return problems;
}

/** One assertion-ready message for a set of unresolved references. */
export function describeUnresolved(problems) {
  return [
    `${problems.length} contract reference(s) do not resolve:`,
    ...problems.map((problem) => `  - [${problem.problem}] ${problem.detail}`),
  ].join("\n");
}
