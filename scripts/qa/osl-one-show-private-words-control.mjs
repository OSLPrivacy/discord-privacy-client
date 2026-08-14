#!/usr/bin/env node
// TASK 4501 -- bring the four show-private-words controls down to one.
//
// This counts, from source, every control anywhere in the app that turns
// private words (the per-scope "decrypt display") on and off, and refuses a
// tree that has anything other than exactly one.
//
// It does NOT count by name. A control only counts when all three of these are
// true, which is what makes it possible for the check to go red when somebody
// adds a second control under a new name:
//
//   1. a pressable element (<input type="checkbox"> or <button>) carrying an id
//      is emitted somewhere in the shipped source or HTML, and
//   2. that id has an event listener bound to it, and
//   3. the listener reaches a call that WRITES a newly decided value into the
//      per-scope setting -- saveActiveContextSecurity(...) or
//      setNativeDiscordOverlaySecurity(...) with a decrypt-display argument
//      that is not simply the current stored value carried through.
//
// Rule 3 is the one that separates a control from the several places that save
// the setting while changing something else (the expiry control, draft
// preparation): those pass the stored value straight back, and a control does
// not.
//
// Exit 0 = exactly one control, none of them working-but-hidden, exactly one
// pressable in the shipping strip, and no second control appears when every
// `hidden` mark is taken off every box. Exit 1 otherwise, naming what it found.

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "..", "..");
const uiRoot = path.join(repo, "apps", "osl-hub-ui");
const srcRoot = path.join(uiRoot, "src");

const RECORDED_BEFORE_EVIDENCE =
  "/home/liamw/osl-plan/OSL-AUDITS/evidence/4403-eye-control-count.md";

// The two commands that persist the setting. Both take the decrypt-display
// value as their LAST argument.
const PERSISTENCE_CALLS = [
  "saveActiveContextSecurity",
  "setNativeDiscordOverlaySecurity",
];

// A decrypt-display argument that is one of these is the current stored value
// being carried through unchanged; anything else is a newly decided value, i.e.
// a toggle.
const CARRY_THROUGH = new Set([
  "decryptDisplayEnabled",
  "peerProtectedSheet.decryptDisplayEnabled",
  "localProtectedSheet.decryptDisplayEnabled",
  "security.decryptDisplayEnabled",
  "saved.decryptDisplayEnabled",
  "state.decryptDisplayEnabled",
]);

// ---------------------------------------------------------------- file loading

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (/\.ts$/u.test(entry.name)
      && !/\.test\.ts$/u.test(entry.name)
      && !/\.d\.ts$/u.test(entry.name)) out.push(full);
  }
  return out;
}

function loadFiles() {
  const files = new Map();
  for (const f of walk(srcRoot)) files.set(f, fs.readFileSync(f, "utf8"));
  for (const f of fs.readdirSync(uiRoot)) {
    if (f.endsWith(".html")) {
      const full = path.join(uiRoot, f);
      files.set(full, fs.readFileSync(full, "utf8"));
    }
  }
  return files;
}

const rel = (f) => path.relative(repo, f);
const lineOf = (text, index) => text.slice(0, index).split("\n").length;

// ------------------------------------------------------------- tiny TS parsing

/** Return the text inside the parentheses that open at `open`. */
function parenBody(text, open) {
  let depth = 0;
  for (let i = open; i < text.length; i += 1) {
    const c = text[i];
    if (c === "(") depth += 1;
    else if (c === ")") {
      depth -= 1;
      if (depth === 0) return { body: text.slice(open + 1, i), end: i };
    }
  }
  return null;
}

/** Split an argument list on top-level commas. */
function splitArgs(body) {
  const args = [];
  let depth = 0;
  let start = 0;
  let tick = false;
  for (let i = 0; i < body.length; i += 1) {
    const c = body[i];
    if (c === "`" && body[i - 1] !== "\\") tick = !tick;
    if (tick) continue;
    if ("([{".includes(c)) depth += 1;
    else if (")]}".includes(c)) depth -= 1;
    else if (c === "," && depth === 0) {
      args.push(body.slice(start, i));
      start = i + 1;
    }
  }
  args.push(body.slice(start));
  return args.map((a) => a.trim()).filter((a) => a.length > 0);
}

/** Name of the nearest enclosing `function NAME(` before `index`. */
function enclosingFunction(text, index) {
  const head = text.slice(0, index);
  const matches = [...head.matchAll(/(?:^|\s)function\s+([A-Za-z0-9_$]+)\s*\(/gu)];
  return matches.length ? matches[matches.length - 1][1] : "<module>";
}

/**
 * Every function that writes a NEWLY DECIDED show-private-words value.
 * Returns Map<functionName, [{file, line, call, arg}]>.
 */
function findToggleWriters(files) {
  const writers = new Map();
  const carried = [];
  for (const [file, text] of files) {
    if (!file.endsWith(".ts")) continue;
    for (const call of PERSISTENCE_CALLS) {
      const re = new RegExp(`\\b${call}\\s*\\(`, "gu");
      for (const m of text.matchAll(re)) {
        // `function saveActiveContextSecurity(...)` is where the command is
        // DECLARED, not a place that decides a value. Skip declarations.
        if (/\bfunction\s+$/u.test(text.slice(Math.max(0, m.index - 40), m.index))) continue;
        const open = m.index + m[0].length - 1;
        const parsed = parenBody(text, open);
        if (!parsed) continue;
        const args = splitArgs(parsed.body);
        if (args.length === 0) continue;
        const arg = args[args.length - 1].replace(/\s+as\s+[A-Za-z0-9_$<>\[\]]+$/u, "").trim();
        // A `name: type` parameter is a signature, never an argument.
        if (/^[A-Za-z0-9_$]+\s*:\s*[A-Za-z]/u.test(arg)) continue;
        const record = { file, line: lineOf(text, m.index), call, arg };
        if (CARRY_THROUGH.has(arg)) { carried.push(record); continue; }
        const fn = enclosingFunction(text, m.index);
        if (!writers.has(fn)) writers.set(fn, []);
        writers.get(fn).push(record);
      }
    }
  }
  return { writers, carried };
}

// ------------------------------------------------------------ element scanning

/** Every pressable element carrying an id, with its own tag text. */
function findPressableElements(files) {
  const byId = new Map();
  for (const [file, text] of files) {
    const re = /<(input|button)\b[^>]*>/giu;
    for (const m of text.matchAll(re)) {
      const tag = m[0];
      const idMatch = /\sid="([^"]+)"/u.exec(tag);
      if (!idMatch) continue;
      const id = idMatch[1];
      if (!byId.has(id)) byId.set(id, []);
      byId.get(id).push({
        file,
        line: lineOf(text, m.index),
        tag,
        ownHidden: /\shidden(?=[\s/>])/u.test(tag),
        boxHidden: enclosedByHiddenBox(text, m.index),
      });
    }
  }
  return byId;
}

/**
 * Is this element inside a box carrying a `hidden` mark?
 *
 * Runs a tag stack backwards over the markup that precedes the element. A
 * backtick resets the stack, so a fragment in one template literal is never
 * judged by tags in another. Plain .html files have no backticks, so the whole
 * file is one region and real ancestry is used.
 */
function enclosedByHiddenBox(text, index) {
  const head = text.slice(0, index);
  const regionStart = Math.max(0, head.lastIndexOf("`") + 1);
  const region = head.slice(regionStart);
  const stack = [];
  const re = /<\/?([a-zA-Z][a-zA-Z0-9-]*)\b([^>]*)>/gu;
  for (const m of region.matchAll(re)) {
    const [whole, name, attrs] = m;
    if (whole.startsWith("</")) {
      for (let i = stack.length - 1; i >= 0; i -= 1) {
        if (stack[i].name === name) { stack.splice(i); break; }
      }
    } else if (!whole.endsWith("/>") && !["input", "img", "br", "hr", "meta", "link"].includes(name)) {
      // `attrs` stops before the closing `>`, so a box whose LAST attribute is
      // the bare `hidden` mark -- which is exactly how the paint-over box was
      // written -- needs end-of-string to count as a boundary too.
      stack.push({ name, hidden: /\shidden(?=[\s/>=]|$)/u.test(attrs) });
    }
  }
  const box = stack.find((entry) => entry.hidden);
  return box ? box.name : null;
}

// ------------------------------------------------------------ binding scanning

/** const NAME = requireElement<...>("#id") -> NAME maps to id. */
function elementVariables(text) {
  const map = new Map();
  const re = /\b(?:const|let)\s+([A-Za-z0-9_$]+)\s*=\s*(?:requireElement|document\.querySelector)\s*(?:<[^>]*>)?\s*\(\s*"#([^"]+)"/gu;
  for (const m of text.matchAll(re)) map.set(m[1], m[2]);
  return map;
}

/**
 * Every id that has a listener bound to it whose callback reaches a toggle
 * writer. This is what "a control that turns private words on and off" means
 * here: something a person acts on, wired to something that decides the value.
 */
function findControls(files, writerNames) {
  const controls = new Map();
  for (const [file, text] of files) {
    if (!file.endsWith(".ts")) continue;
    const vars = elementVariables(text);
    for (const m of text.matchAll(/\.addEventListener\s*\(/gu)) {
      const open = m.index + m[0].length - 1;
      const parsed = parenBody(text, open);
      if (!parsed) continue;
      const args = splitArgs(parsed.body);
      if (args.length < 2) continue;
      const callback = args.slice(1).join(",");

      // Does this listener reach a decision about the setting?
      const reaches = writerNames.some((name) => new RegExp(`\\b${name}\\b`, "u").test(callback));
      if (!reaches) continue;

      // What is it bound to? Either a literal "#id" in the same statement, or a
      // variable that requireElement/querySelector resolved from one.
      const stmtStart = Math.max(
        text.lastIndexOf(";", m.index),
        text.lastIndexOf("\n", m.index),
      ) + 1;
      const target = text.slice(stmtStart, m.index);
      let id = null;
      const literal = [...target.matchAll(/"#([^"]+)"/gu)].pop();
      if (literal) id = literal[1];
      if (!id) {
        const ident = /([A-Za-z0-9_$]+)\s*$/u.exec(target);
        if (ident && vars.has(ident[1])) id = vars.get(ident[1]);
      }
      if (!id) continue;
      if (!controls.has(id)) controls.set(id, []);
      controls.get(id).push({ file, line: lineOf(text, m.index) });
    }
  }
  return controls;
}

// ------------------------------------------------------------------- the count

function countControls(files) {
  const { writers, carried } = findToggleWriters(files);
  const writerNames = [...writers.keys()];
  const bound = findControls(files, writerNames);
  const elements = findPressableElements(files);

  const found = [];
  for (const [id, bindings] of bound) {
    const els = elements.get(id) ?? [];
    if (els.length === 0) continue; // bound, but no element a person could press
    for (const el of els) {
      found.push({
        id,
        element: `${rel(el.file)}:${el.line}`,
        boundAt: bindings.map((b) => `${rel(b.file)}:${b.line}`).join(", "),
        ownHidden: el.ownHidden,
        boxHidden: el.boxHidden,
        hidden: el.ownHidden || Boolean(el.boxHidden),
      });
    }
  }
  return {
    found: found.sort((a, b) => a.id.localeCompare(b.id)),
    writers,
    writerNames,
    carried,
    orphanBindings: [...bound.keys()].filter((id) => !elements.has(id)),
  };
}

// ------------------------------------------- what the shipping strip actually shows

function region(text, startNeedle, endNeedle) {
  const start = text.indexOf(startNeedle);
  if (start < 0) throw new Error(`not found: ${startNeedle}`);
  const end = text.indexOf(endNeedle, start + startNeedle.length);
  if (end < 0) throw new Error(`not found after start: ${endNeedle}`);
  return text.slice(start, end);
}

/**
 * Render the SHIPPING Discord strip from the real template in main.ts (the
 * branch taken when the test switch is off) and count the controls in it a
 * person could actually press.
 */
function renderShippingStrip(mainSource) {
  const eyeBlock = region(
    mainSource,
    '  const transcriptMode = transcriptVisible ? "plaintext" : "flagtext";',
    "\n  const lock =",
  ).replace(": DiscordQaTranscriptVisibilityOutcome", "");

  const shippingBranch = region(
    mainSource,
    "  if (!discordQaShell) {",
    "\n  return `<div class=\"native-discord-header-controls discord-qa-header-controls\"",
  );
  const returnStart = shippingBranch.indexOf("return `");
  const returnEnd = shippingBranch.lastIndexOf("`;");
  const template = shippingBranch.slice(returnStart + "return ".length, returnEnd + 1);

  const build = new Function(
    "transcriptVisible",
    "verifiedPeer",
    "visibilityBusy",
    "discordQaTranscriptVisibilityOutcome",
    "eye",
    "composerUnreachableNotice",
    "inactive",
    "nativeDiscordCovertextEnabled",
    "inDomTooltipMarkup",
    `${eyeBlock}\nreturn ${template};`,
  );

  return build(
    true,                       // transcript currently showing decrypted text
    { personId: "person-1" },   // a verified friend is present
    false,                      // not mid-write
    "applied",
    "<svg></svg>",
    "",                         // no composer-unreachable notice
    "",                         // protection active, so nothing disabled
    false,
    () => "",
  );
}

/** Controls in rendered HTML that a person could press. */
function pressableInMarkup(html, controlIds) {
  const out = [];
  for (const m of html.matchAll(/<(input|button)\b[^>]*>/giu)) {
    const tag = m[0];
    const idMatch = /\sid="([^"]+)"/u.exec(tag);
    if (!idMatch || !controlIds.has(idMatch[1])) continue;
    const disabled = /\sdisabled(?=[\s/>])/u.test(tag);
    const hidden = /\shidden(?=[\s/>])/u.test(tag)
      || Boolean(enclosedByHiddenBox(html, m.index));
    out.push({ id: idMatch[1], disabled, hidden, pressable: !disabled && !hidden });
  }
  return out;
}

// ------------------------------------------------------------------------ main

function recordedBefore() {
  try {
    const text = fs.readFileSync(RECORDED_BEFORE_EVIDENCE, "utf8");
    const m = /Total controls found that can change the setting in source:\s*\*\*(\d+)\*\*/u.exec(text);
    if (m) return { count: Number(m[1]), source: RECORDED_BEFORE_EVIDENCE };
  } catch { /* fall through */ }
  return { count: 4, source: "TASK 4403 recorded figure (evidence file unreadable here)" };
}

const failures = [];
const files = loadFiles();
const before = recordedBefore();
const result = countControls(files);

console.log("TASK 4501 -- show-private-words controls");
console.log("=".repeat(72));
console.log(`scanned: ${files.size} files under apps/osl-hub-ui (src/*.ts + *.html)`);
console.log("");
console.log("Functions that decide a NEW show-private-words value:");
for (const [fn, calls] of result.writers) {
  for (const c of calls) console.log(`  ${fn}  ->  ${c.call}(..., ${c.arg})  at ${rel(c.file)}:${c.line}`);
}
if (result.writers.size === 0) console.log("  (none)");
console.log("");
console.log(`Calls that only carry the stored value through (not controls): ${result.carried.length}`);
for (const c of result.carried) console.log(`  ${rel(c.file)}:${c.line}  ${c.call}(..., ${c.arg})`);
console.log("");
console.log("CONTROLS FOUND");
console.log("-".repeat(72));
for (const c of result.found) {
  const where = c.hidden
    ? `HIDDEN (${c.ownHidden ? "own hidden mark" : `inside <${c.boxHidden} hidden>`})`
    : "reachable";
  console.log(`  #${c.id}`);
  console.log(`      element: ${c.element}`);
  console.log(`      bound:   ${c.boundAt}`);
  console.log(`      state:   ${where}`);
}
if (result.found.length === 0) console.log("  (none)");
console.log("");

const after = result.found.length;
const hiddenCount = result.found.filter((c) => c.hidden).length;

console.log("BEFORE vs AFTER");
console.log("-".repeat(72));
console.log(`  before: ${before.count}    after: ${after}`);
console.log(`  (before figure read from ${before.source})`);
console.log("");

if (after !== 1) {
  failures.push(
    `expected exactly 1 show-private-words control, found ${after}: `
    + result.found.map((c) => `#${c.id} (${c.element}${c.hidden ? ", hidden" : ""})`).join(", "),
  );
}

console.log(`working-but-hidden controls: ${hiddenCount}`);
if (hiddenCount !== 0) {
  failures.push(
    `expected 0 working-but-hidden controls, found ${hiddenCount}: `
    + result.found.filter((c) => c.hidden).map((c) => `#${c.id} (${c.element})`).join(", "),
  );
}

if (result.orphanBindings.length > 0) {
  failures.push(
    "handler bound to an id with no element to press -- it becomes a control the "
    + `moment anybody adds that element: ${result.orphanBindings.map((id) => `#${id}`).join(", ")}`,
  );
}
console.log(`handlers bound to a missing element: ${result.orphanBindings.length}`);
console.log("");

// --- the shipping strip, rendered from its real template ---------------------
const controlIds = new Set(result.found.map((c) => c.id));
const mainSource = files.get(path.join(srcRoot, "main.ts"));
let shippingPressable = 0;
try {
  const stripHtml = renderShippingStrip(mainSource);
  const seen = pressableInMarkup(stripHtml, controlIds);
  shippingPressable = seen.filter((s) => s.pressable).length;
  console.log("SHIPPING BUILD (test switch off), Discord strip rendered from main.ts");
  console.log("-".repeat(72));
  for (const s of seen) {
    console.log(`  #${s.id}  disabled=${s.disabled}  hidden=${s.hidden}  pressable=${s.pressable}`);
  }
  if (seen.length === 0) console.log("  (no show-private-words control in the shipping strip)");
  console.log(`  pressable in the shipping build: ${shippingPressable}`);
  if (shippingPressable !== 1) {
    failures.push(`expected exactly 1 pressable control in the shipping build, found ${shippingPressable}`);
  }
} catch (error) {
  failures.push(`could not render the shipping strip: ${error.message}`);
}
console.log("");

// --- take every hidden mark off every box ------------------------------------
const unhidden = new Map();
for (const [file, text] of files) {
  unhidden.set(file, text.replace(/\shidden(?=[\s/>])/gu, " "));
}
const afterUnhide = countControls(unhidden);
console.log("WITH EVERY `hidden` MARK STRIPPED FROM EVERY BOX");
console.log("-".repeat(72));
console.log(`  controls: ${afterUnhide.found.length}`);
for (const c of afterUnhide.found) console.log(`    #${c.id} (${c.element})`);
if (afterUnhide.found.length !== after) {
  failures.push(
    `taking the hidden marks off made the count change from ${after} to `
    + `${afterUnhide.found.length}: `
    + afterUnhide.found.map((c) => `#${c.id} (${c.element})`).join(", "),
  );
}
if (afterUnhide.found.length !== 1) {
  failures.push(`expected 1 control with every hidden mark stripped, found ${afterUnhide.found.length}`);
}
console.log("");

console.log("=".repeat(72));
if (failures.length > 0) {
  console.log("FAIL");
  for (const f of failures) console.log(`  - ${f}`);
  process.exit(1);
}
console.log("PASS");
console.log(`  exactly 1 control (before ${before.count}, after ${after})`);
console.log("  0 working-but-hidden");
console.log(`  ${shippingPressable} pressable in the shipping build`);
console.log("  stripping every hidden mark surfaces no second control");
process.exit(0);
