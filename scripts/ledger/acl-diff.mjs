#!/usr/bin/env node
// LEDGER 3 -- commands ISSUED vs commands GRANTED, including the ones our
// source never mentions.
//
// The point of this ledger is the third issuer class. A ledger built from our
// own `invoke("...")` calls finds nothing wrong with the title bar, because
// nothing in apps/osl-hub-ui contains the string `internal_toggle_maximize`.
// Tauri injects drag.js into the webview; drag.js issues
// `plugin:window|internal_toggle_maximize` on double-click; the ACL rejects it;
// drag.js does not handle the rejection. Double-clicking the title bar is
// silently dead and every source-scanning check scores that file green.
//
// So three issuer classes are collected, not one:
//
//   1. direct   -- `invoke("name")` in our own modules
//   2. api      -- methods on @tauri-apps/api handles. `getCurrentWindow().isMaximized()`
//                  contains no command string either; the string lives in
//                  node_modules/@tauri-apps/api/window.js and is extracted from there.
//   3. internal -- framework-injected scripts, from framework-internals.json,
//                  pinned to the tauri version in apps/osl-hub/Cargo.lock.
//
// Scope: the `main` webview only. Its grants are the union of every capability
// whose `webviews` list contains "main". The other four webview labels are a
// declared blind spot -- see exceptions/acl.json -- because deciding which
// label a given HTML page is loaded under requires modelling runtime window
// creation, and guessing there is how this project produced 34 "P0"s that were
// one real defect.
//
// The set difference is read in every direction, because the other two each
// caused an incident on 2026-08-04 while this ledger was GREEN:
//
//   issued -> granted                     the original: a main-webview issuer
//                                         with no grant. Scoped to `main`.
//   granted -> registered                 D-146: a [[permission]] allowing a
//                                         command that no longer exists.
//                                         tauri-build validates capabilities
//                                         against the registry, so this is the
//                                         direction that stops the app
//                                         compiling at all.
//   registered -> granted anywhere        D-158: a command on the IPC surface
//                                         that NO capability grants to ANY
//                                         webview, so the ACL rejects it before
//                                         its handler runs.
//   defined -> registered                 a #[tauri::command] in no handler
//                                         list: unreachable, and no grant helps.
//
// The last three are webview-agnostic on purpose. "Registered but not granted to
// main" is the naive reading and it is wrong: the overlay capabilities hold 14
// commands `main` must never have, so ungranted-to-main is the correct state for
// them. Only "granted to no webview at all" is a defect.
//
//   node scripts/ledger/acl-diff.mjs [--root=<dir>]

import { readFileSync, existsSync } from "node:fs";
import { dirname, join, normalize, resolve } from "node:path";
import { repoRoot, read, walk, blankComments, lineIndex, lineOf, LEDGER_DIR, inputProblems, stringConstants, resolveArg } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";
import { rustRegistry } from "./lib/rust-registry.mjs";
import { bundleSnapshot, reachableFrom } from "./bundle.mjs";

const MAIN_ENTRIES = ["apps/osl-hub-ui/index.html", "apps/osl-hub-ui/whatsapp-qa.html"];
const EXTRA_MAIN_MODULES = ["apps/osl-hub-ui/src/signal-qa-main.ts"];

/* ------------------------------------------------------------------ grants */

export function grantsForWebview(root, label) {
  const files = walk(root, "apps/osl-hub/capabilities", (r) => r.endsWith(".json"));
  const grants = new Map();
  const unresolvable = [];
  for (const rel of files) {
    const doc = JSON.parse(read(root, rel));
    if (!(doc.webviews ?? []).includes(label) && !(doc.windows ?? []).includes(label)) continue;
    for (const p of doc.permissions ?? []) {
      const id = typeof p === "string" ? p : p.identifier;
      if (!id) continue;
      // A permission SET (`core:default`, `core:window:default`) expands to a
      // list that lives in the tauri crate, not here. Expanding it by guess is
      // exactly the kind of widening that made the previous guard worthless, so
      // an unexpandable set is a hard failure rather than a silent pass.
      if (id.endsWith(":default") || id === "core:default") unresolvable.push(`${rel}: ${id}`);
      grants.set(id, rel);
    }
  }
  return { grants, unresolvable, files };
}

/**
 * Every `[[permission]]` block under apps/osl-hub/permissions, with the line it
 * starts on so a dangling grant can be cited rather than merely named.
 */
export function permissionBlocks(root) {
  const blocks = [];
  for (const rel of walk(root, "apps/osl-hub/permissions", (r) => r.endsWith(".toml"))) {
    const src = read(root, rel);
    const starts = lineIndex(src);
    for (const m of src.matchAll(/\[\[permission\]\]([\s\S]*?)(?=\n\[\[permission\]\]|\s*$)/g)) {
      const body = m[1];
      const identifier = /\bidentifier\s*=\s*"([^"]+)"/.exec(body)?.[1] ?? null;
      const commands = /\bcommands\.allow\s*=\s*\[([^\]]*)\]/.exec(body)?.[1] ?? "";
      if (!identifier) continue;
      blocks.push({
        rel,
        identifier,
        site: `${rel}:${lineOf(starts, m.index)}`,
        commands: [...commands.matchAll(/"([^"]+)"/g)].map((c) => c[1]),
      });
    }
  }
  return blocks;
}

export function commandPermissions(root, blocks = permissionBlocks(root)) {
  const out = new Map();
  for (const block of blocks) {
    for (const cmd of block.commands) out.set(cmd, block.identifier);
  }
  return out;
}

/**
 * command -> every permission identifier that allows it. `commandPermissions`
 * is last-wins because `permissionFor` needs one answer; the "granted to no
 * webview" direction needs all of them, or a command allowed by two blocks is
 * reported ungranted whenever the losing block is the granted one.
 */
export function permissionsAllowingCommand(blocks) {
  const out = new Map();
  for (const block of blocks) {
    for (const cmd of block.commands) {
      if (!out.has(cmd)) out.set(cmd, []);
      out.get(cmd).push(block);
    }
  }
  return out;
}

/**
 * Permission identifiers granted by ANY capability file, to any webview.
 *
 * Deliberately not "granted to main". The conductor measured the naive version:
 * 54 violations, almost all noise, because the overlay capabilities legitimately
 * hold commands the main webview must never have -- "ungranted to main" is the
 * correct and expected state for those. The defect D-158 actually was is a
 * command that NO capability grants to ANY webview, so the ACL rejects it before
 * its handler runs no matter which window issues it.
 */
export function grantsAnyWebview(root) {
  const grants = new Map();
  for (const rel of walk(root, "apps/osl-hub/capabilities", (r) => r.endsWith(".json"))) {
    const doc = JSON.parse(read(root, rel));
    for (const p of doc.permissions ?? []) {
      const id = typeof p === "string" ? p : p.identifier;
      if (id && !grants.has(id)) grants.set(id, rel);
    }
  }
  return grants;
}

/* ------------------------------------------------- the Rust command surface */

const COMMAND_SOURCES = ["apps/osl-hub/src/main.rs", "apps/osl-hub/src/hub_command_surface.rs"];
// `#[tauri::command]`, then any further attributes, then the fn it decorates.
// Anchored to the fn on purpose: a loose "attribute, then the next fn within N
// characters" match walks past prose that merely mentions the attribute and
// invents five commands that do not exist (`ai_carrier.rs:107` is a comment
// saying where the wrapper lives; `tor_pref.rs:897` is a test helper).
const COMMAND_ATTRIBUTE = /#\[tauri::command(?:\s*\([^)]*\))?\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-z_][A-Za-z0-9_]*)/g;

function attributedCommands(root, rel) {
  const src = blankComments(read(root, rel));
  const starts = lineIndex(src);
  const found = new Map();
  for (const m of src.matchAll(COMMAND_ATTRIBUTE)) found.set(m[1], `${rel}:${lineOf(starts, m.index)}`);
  return found;
}

/**
 * The commands Tauri actually exposes: the registry INTERSECTED with the names
 * that carry a `#[tauri::command]` attribute.
 *
 * The registry alone is identifier soup -- it is every lowercase word inside the
 * macro body and the generate_handler list, so it contains `assert`, `body`,
 * `command`, `vec`, `mod`. Reporting those as ungranted commands makes the
 * output unusable. The attribute is the authority on what is a command.
 */
export function registeredCommands(root) {
  const { registry, problems } = rustRegistry(root);
  const attributed = new Map();
  for (const rel of COMMAND_SOURCES) {
    for (const [name, site] of attributedCommands(root, rel)) if (!attributed.has(name)) attributed.set(name, site);
  }
  const commands = new Map();
  const unregistered = [];
  for (const [name, site] of attributed) {
    if (registry.has(name)) commands.set(name, { definedAt: site, registeredAt: registry.get(name) });
    else unregistered.push({ name, site });
  }
  // Both halves of the intersection can starve independently, and either one
  // going to zero would make "registered but granted nowhere" silently pass.
  if (attributed.size === 0) {
    problems.push({
      id: "empty-input:tauri-command-attributes",
      detail: `no #[tauri::command] attribute was extracted from ${COMMAND_SOURCES.join(" or ")}; the registry intersection would be empty and the ACL directions below would pass vacuously`,
      sites: COMMAND_SOURCES.map((rel) => `${rel}:1`),
    });
  } else if (commands.size === 0) {
    problems.push({
      id: "empty-input:registered-command-intersection",
      detail: `${attributed.size} #[tauri::command] attributes and ${registry.size} registry identifiers were extracted but they intersect in nothing; one of the two extractors is reading the wrong shape`,
      sites: COMMAND_SOURCES.map((rel) => `${rel}:1`),
    });
  }
  // A command defined outside the two files this ledger reads would be invisible
  // to the intersection, so the assumption starves itself rather than silently
  // narrowing the check.
  const strays = [];
  for (const rel of walk(root, "apps/osl-hub/src", (r) => r.endsWith(".rs") && !COMMAND_SOURCES.includes(r))) {
    for (const [name, site] of attributedCommands(root, rel)) {
      if (registry.has(name) && !commands.has(name)) strays.push({ name, site });
    }
  }
  for (const stray of strays) {
    problems.push({
      id: `stray-command-definition:${stray.name}`,
      detail: `${stray.name} is registered and carries #[tauri::command], but it is defined outside ${COMMAND_SOURCES.join(" / ")}; this ledger reads only those two files, so it would not see this command at all`,
      sites: [stray.site],
    });
  }
  return { commands, registry, problems, unregistered };
}

/** `plugin:window|is_maximized` -> `core:window:allow-is-maximized`; `foo_bar` -> `allow-foo-bar`. */
export function permissionFor(command, permissionDefs = new Map()) {
  const configured = permissionDefs.get(command);
  if (configured) return configured;
  const m = /^plugin:([a-z]+)\|(.+)$/.exec(command);
  const kebab = (s) => s.replace(/_/g, "-");
  if (m) {
    const [, plugin, cmd] = m;
    const core = ["window", "webview", "event", "app", "path", "image", "resources", "menu", "tray", "webviewWindow"];
    return core.includes(plugin) ? `core:${plugin}:allow-${kebab(cmd)}` : `${plugin}:allow-${kebab(cmd)}`;
  }
  return `allow-${kebab(command)}`;
}

/* ------------------------------------------------- @tauri-apps/api command map */

/**
 * Extract `method name -> commands it invokes` from the shipped
 * @tauri-apps/api sources. Derived from the package, never hand-listed: a
 * hand-listed map is a search pattern that silently rots at the next upgrade.
 */
export function tauriApiMethodCommands(root) {
  const pkgDir = join(root, "apps/osl-hub-ui/node_modules/@tauri-apps/api");
  if (!existsSync(pkgDir)) return { byFile: new Map(), version: null, missing: true };
  const version = JSON.parse(readFileSync(join(pkgDir, "package.json"), "utf8")).version;
  const byFile = new Map();
  // Kept per FILE, i.e. per receiver type. `close()` on a Window is
  // `plugin:window|close` (granted here); `close()` on a Webview is
  // `plugin:webview|webview_close` (not). One flat name->commands map reports
  // both for every `getCurrentWindow().close()` and invents two defects that do
  // not exist. This project has already turned 34 such "P0"s into one real one.
  for (const file of ["window.js", "webview.js", "webviewWindow.js", "event.js", "app.js"]) {
    const p = join(pkgDir, file);
    if (!existsSync(p)) continue;
    const src = blankComments(readFileSync(p, "utf8"));
    const map = new Map();
    // Method or function headers, and every literal command until the next one.
    const heads = [...src.matchAll(/^\s*(?:async\s+)?(?:function\s+)?([A-Za-z_$][\w$]*)\s*\([^)\n]*\)\s*\{/gm)];
    const cmds = [...src.matchAll(/invoke\(\s*'(plugin:[a-z]+\|[a-z_]+)'/g)];
    for (const c of cmds) {
      let owner = null;
      for (const h of heads) {
        if (h.index < c.index) owner = h[1];
        else break;
      }
      if (!owner) continue;
      if (!map.has(owner)) map.set(owner, new Set());
      map.get(owner).add(c[1]);
    }
    byFile.set(file, map);
  }
  const extracted = [...byFile.values()].reduce((sum, map) => sum + map.size, 0);
  return { byFile, version, missing: false, extracted };
}

/** Which @tauri-apps/api source file defines the handle a factory hands back. */
const FACTORY_RECEIVER = {
  getCurrentWindow: ["window.js"],
  getAllWindows: ["window.js"],
  getCurrentWebview: ["webview.js"],
  getCurrentWebviewWindow: ["webviewWindow.js", "window.js", "webview.js"],
};

const REQUIRED_API_METHODS = {
  "window.js": ["close", "isFocused", "isFullscreen", "isMaximized", "minimize", "setFocus", "setFullscreen", "toggleMaximize"],
  "event.js": ["emitTo", "listen"],
};

/* ------------------------------------------------------------------ issuers */

function relativeModule(from, spec, root) {
  if (!spec.startsWith(".")) return null;
  const base = normalize(join(dirname(from), spec)).split("\\").join("/");
  const candidates = [base, `${base}.ts`, `${base}.tsx`, `${base}.js`, `${base}.css`, join(base, "index.ts").split("\\").join("/")];
  return candidates.find((rel) => existsSync(join(root, rel))) ?? null;
}

export function entryReachability(snapshot) {
  const byEntry = new Map();
  const byModule = new Map();
  for (const entry of snapshot.entries ?? []) {
    const modules = reachableFrom(snapshot, entry);
    byEntry.set(entry, modules);
    for (const rel of modules) {
      if (!byModule.has(rel)) byModule.set(rel, new Set());
      byModule.get(rel).add(entry);
    }
  }
  return { byEntry, byModule };
}

export function importedValuesByEntry(root, byEntry) {
  const byEntryImport = new Map();
  for (const [entry, modules] of byEntry) {
    const imports = new Map();
    for (const rel of modules) {
      if (!rel.endsWith(".ts")) continue;
      const src = blankComments(read(root, rel));
      for (const m of src.matchAll(/\bimport\s+([\s\S]*?)\s+from\s+["']([^"']+)["']/g)) {
        const target = relativeModule(rel, m[2], root);
        if (!target) continue;
        if (!imports.has(target)) imports.set(target, new Set());
        const names = imports.get(target);
        const clause = m[1].trim();
        const named = /\{([\s\S]*?)\}/.exec(clause)?.[1] ?? "";
        if (/\*\s+as\s+/.test(clause)) names.add("*");
        for (const raw of named.split(",")) {
          const part = raw.trim();
          if (!part || part.startsWith("type ")) continue;
          names.add(part.split(/\s+as\s+/)[0].trim());
        }
        const withoutNamed = clause.replace(/\{[\s\S]*?\}/g, "").trim();
        if (withoutNamed && !withoutNamed.startsWith("type ") && !withoutNamed.startsWith("*") && !withoutNamed.startsWith(",")) names.add("default");
      }
    }
    byEntryImport.set(entry, imports);
  }
  return byEntryImport;
}

function matchingBrace(src, open) {
  let depth = 0;
  for (let i = open; i < src.length; i += 1) {
    if (src[i] === "{") depth += 1;
    else if (src[i] === "}") {
      depth -= 1;
      if (depth === 0) return i + 1;
    }
  }
  return src.length;
}

export function functionSpans(src) {
  const spans = [];
  for (const m of src.matchAll(/\b(export\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)\s*(?:<[^>{}]+>)?\s*\(/g)) {
    const open = src.indexOf("{", m.index);
    if (open !== -1) spans.push({ name: m[2], exported: Boolean(m[1]), start: m.index, end: matchingBrace(src, open) });
  }
  for (const m of src.matchAll(/\b(export\s+)?const\s+([A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>/g)) {
    const semi = src.indexOf(";", m.index);
    const open = src.indexOf("{", m.index);
    const end = open !== -1 && (semi === -1 || open < semi) ? matchingBrace(src, open) : semi === -1 ? src.length : semi + 1;
    spans.push({ name: m[2], exported: Boolean(m[1]), start: m.index, end });
  }
  return spans.sort((a, b) => a.start - b.start);
}

export function enclosingFunction(spans, index) {
  return spans
    .filter((span) => span.start <= index && index < span.end)
    .sort((a, b) => (a.end - a.start) - (b.end - b.start))[0] ?? null;
}

function entriesForExport(rel, exportName, importsByEntry) {
  const out = new Set();
  for (const [entry, imports] of importsByEntry) {
    const names = imports.get(rel);
    if (names?.has(exportName) || names?.has("*")) out.add(entry);
  }
  return out;
}

export function localFunctionEntryResolver(rel, spans, src, moduleEntries, importsByEntry) {
  const byName = new Map(spans.map((span) => [span.name, span]));
  const resolveFunction = (name, seen = new Set()) => {
    if (seen.has(name)) return new Set();
    seen.add(name);
    const span = byName.get(name);
    if (!span) return new Set(moduleEntries.get(rel) ?? []);
    if (span.exported) return entriesForExport(rel, name, importsByEntry);
    const out = new Set();
    const call = new RegExp(`\\b${name}\\s*(?:<[^>(]*>)?\\s*\\(`, "g");
    for (const m of src.matchAll(call)) {
      if (m.index >= span.start && m.index < span.end) continue;
      const caller = enclosingFunction(spans, m.index);
      const entries = caller ? resolveFunction(caller.name, new Set(seen)) : new Set(moduleEntries.get(rel) ?? []);
      for (const entry of entries) out.add(entry);
    }
    return out;
  };
  return (fn) => fn ? resolveFunction(fn.name) : new Set(moduleEntries.get(rel) ?? []);
}

function parseParamNames(params) {
  return params.split(",").map((p) => p.trim().split(/[:?=]/)[0]?.trim()).filter(Boolean);
}

function invokeWrappers(src, spans) {
  const wrappers = new Map();
  const add = (name, params, bodyStart, bodyEnd) => {
    const first = parseParamNames(params)[0];
    if (!first) return;
    if (!/^[A-Za-z_$][\w$]*$/.test(first)) return;
    const body = src.slice(bodyStart, bodyEnd);
    const re = new RegExp(`\\binvoke\\s*(?:<[^>(]*>)?\\s*\\(\\s*${first}\\b`);
    if (re.test(body)) wrappers.set(name, { param: first, start: bodyStart, end: bodyEnd });
  };
  for (const span of spans) {
    const header = src.slice(span.start, src.indexOf("{", span.start) === -1 ? span.end : src.indexOf("{", span.start));
    const params = /\(([\s\S]*)\)/.exec(header)?.[1] ?? "";
    add(span.name, params, span.start, span.end);
  }
  return wrappers;
}

function parseCommandExpression(text) {
  const lit = /^"((?:[^"\\]|\\.)*)"$|^'((?:[^'\\]|\\.)*)'$|^`([^`$\\]*)`$/.exec(text.trim());
  if (lit) return [lit[1] ?? lit[2] ?? lit[3]];
  const ternary = /\?\s*"((?:[^"\\]|\\.)*)"\s*:\s*"((?:[^"\\]|\\.)*)"|\?\s*'((?:[^'\\]|\\.)*)'\s*:\s*'((?:[^'\\]|\\.)*)'/.exec(text);
  if (ternary) return [ternary[1] ?? ternary[3], ternary[2] ?? ternary[4]];
  return [];
}

function resolveCommands(arg, constants, src, beforeIndex) {
  const direct = resolveArg(arg, constants);
  if (direct.value) return [direct.value];
  const text = arg.trim();
  const parsed = parseCommandExpression(text);
  if (parsed.length) return parsed;
  if (/^[A-Za-z_$][\w$]*$/.test(text)) {
    const prefix = src.slice(0, beforeIndex);
    const defs = [...prefix.matchAll(new RegExp(`\\bconst\\s+${text}\\s*(?::[^=]+)?=\\s*([^;]+);`, "g"))];
    const last = defs.at(-1)?.[1];
    if (last) return parseCommandExpression(last);
  }
  return [];
}

export function scanIssuers(root, modules, apiMethods, scope) {
  const issued = new Map(); // command -> [{site, via}]
  const dead = [];
  const add = (command, site, via) => {
    const entries = site.entries ?? new Set();
    const entryNames = [...entries].sort();
    if (entryNames.length === 0) {
      dead.push({
        id: `dead-issuer:${command}:${site.text}`,
        kind: "dead-issuer",
        command,
        detail: `${command} is in an exported issuer that no production entry imports; adding this exception admits the issuer does not ship`,
        sites: [site.text],
      });
      return;
    }
    if (scope?.mainEntries && !entryNames.some((entry) => scope.mainEntries.has(entry))) return;
    if (!issued.has(command)) issued.set(command, []);
    issued.get(command).push({ site: site.text, via });
  };
  const unresolved = [];
  const constants = stringConstants(modules, (rel) => read(root, rel));
  const commandsFor = (receiverFiles, method) => {
    const out = new Set();
    for (const file of receiverFiles) {
      for (const cmd of apiMethods.byFile.get(file)?.get(method) ?? []) out.add(cmd);
    }
    return out;
  };

  for (const rel of modules) {
    if (!rel.endsWith(".ts")) continue;
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    const spans = functionSpans(src);
    const localEntries = localFunctionEntryResolver(rel, spans, src, scope?.moduleEntries ?? new Map(), scope?.importsByEntry ?? new Map());
    const wrappers = invokeWrappers(src, spans);
    const site = (index) => ({
      rel,
      text: `${rel}:${lineOf(starts, index)}`,
      entries: localEntries(enclosingFunction(spans, index)),
    });

    for (const [wrapperName, wrapper] of wrappers) {
      const calls = new RegExp(`\\b${wrapperName}\\s*(?:<[^>(]*>)?\\s*\\(\\s*([^,)]+)`, "g");
      for (const m of src.matchAll(calls)) {
        if (m.index >= wrapper.start && m.index < wrapper.end) continue;
        for (const command of resolveCommands(m[1], constants, src, m.index)) add(command, site(m.index), `direct ${wrapperName}()`);
      }
    }

    // 1. direct invoke("name") / invoke<T>("name") / __TAURI_INTERNALS__.invoke("name")
    for (const m of src.matchAll(/\binvoke\s*(?:<[^>(]*>)?\s*\(\s*([^,)]+)/g)) {
      const arg = m[1].trim();
      if (/^[A-Za-z_$][\w$]*\s*:/.test(arg)) continue;
      if (arg === "command" && /=>\s*invoke\s*(?:<[^>(]*>)?\s*\(\s*command\b/.test(src.slice(Math.max(0, m.index - 80), m.index + 80))) continue;
      const wrapped = [...wrappers.values()].some((wrapper) => m.index >= wrapper.start && m.index < wrapper.end && arg === wrapper.param);
      if (wrapped) continue;
      const commands = resolveCommands(arg, constants, src, m.index);
      if (commands.length) for (const command of commands) add(command, site(m.index), "direct");
      else unresolved.push({ site: site(m.index).text, expr: arg.slice(0, 60), via: "direct invoke()" });
    }

    if (rel === "apps/osl-hub-ui/src/core.ts") {
      for (const m of src.matchAll(/\bnativeInvoke\s*\(\s*([^,)]+)/g)) {
        for (const command of resolveCommands(m[1].trim(), constants, src, m.index)) add(command, site(m.index), "direct nativeInvoke()");
      }
    }

    // 2. @tauri-apps/api handle methods. Only counted on a handle: either a
    //    direct `getCurrentWindow().m()` chain, or a variable that was assigned
    //    from one of the factory functions in this same file. Counting bare
    //    `.close()` anywhere would collide with our own methods and over-report.
    const handles = new Map();
    for (const m of src.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=\n]+)?=\s*(?:await\s+)?(getCurrentWindow|getCurrentWebview|getCurrentWebviewWindow)\s*\(/g)) {
      handles.set(m[1], FACTORY_RECEIVER[m[2]]);
    }
    const directFactory = /\b(getCurrentWindow|getCurrentWebview|getCurrentWebviewWindow)\s*\(\s*\)\s*\.\s*([A-Za-z_$][\w$]*)\s*\(/g;
    for (const m of src.matchAll(directFactory)) {
      const [, factory, method] = m;
      for (const cmd of commandsFor(FACTORY_RECEIVER[factory], method)) add(cmd, site(m.index), `@tauri-apps/api ${factory}().${method}()`);
    }
    const receivers = [...handles.keys()].join("|");
    if (receivers) {
      const chain = new RegExp(`\\b(${receivers})\\s*\\.\\s*([A-Za-z_$][\\w$]*)\\s*\\(`, "g");
      for (const m of src.matchAll(chain)) {
        const [, receiver, method] = m;
        for (const cmd of commandsFor(handles.get(receiver) ?? [], method)) add(cmd, site(m.index), `@tauri-apps/api ${receiver}.${method}()`);
      }
    }

    // 3. module-scope event helpers imported from @tauri-apps/api/event
    if (/from\s+"@tauri-apps\/api\/event"/.test(src)) {
      const imported = /import\s*\{([^}]*)\}\s*from\s+"@tauri-apps\/api\/event"/.exec(src)?.[1] ?? "";
      for (const name of imported.split(",").map((s) => s.trim().split(/\s+as\s+/)[0])) {
        if (!apiMethods.byFile.get("event.js")?.has(name)) continue;
        const use = new RegExp(`\\b${name}\\s*(?:<[^>(]*>)?\\s*\\(`, "g");
        for (const m of src.matchAll(use)) {
          for (const cmd of commandsFor(["event.js"], name)) {
            add(cmd, site(m.index), `@tauri-apps/api/event ${name}()`);
          }
        }
      }
    }
  }
  return { issued, unresolved, dead };
}

function tauriApiExtractionProblems(api) {
  const problems = [];
  if (api.missing || api.extracted === 0) return problems;
  for (const [file, methods] of Object.entries(REQUIRED_API_METHODS)) {
    const map = api.byFile.get(file);
    for (const method of methods) {
      if (map?.has(method)) continue;
      problems.push(`${file} no longer exposes an extracted command binding for ${method}(); this UI calls that API, so the ACL ledger refuses partial extraction`);
    }
  }
  return problems;
}

/** Framework-injected scripts, gated on the trigger actually being present. */
export function scanFrameworkInternals(root, modules) {
  const doc = JSON.parse(readFileSync(join(LEDGER_DIR, "framework-internals.json"), "utf8"));
  const lock = read(root, "apps/osl-hub/Cargo.lock");
  const pinned = /name = "tauri"\nversion = "([^"]+)"/.exec(lock)?.[1] ?? null;
  const staleness =
    pinned && pinned !== doc.tauriVersion
      ? `framework-internals.json was read against tauri ${doc.tauriVersion} but apps/osl-hub/Cargo.lock now pins ${pinned}. Re-read the injected scripts before trusting this ledger.`
      : null;

  const markup = modules
    .filter((r) => r.endsWith(".ts") || r.endsWith(".html"))
    .map((r) => read(root, r))
    .join("\n");
  const html = walk(root, "apps/osl-hub-ui", (r) => r.endsWith(".html") && !r.includes("node_modules"))
    .map((r) => read(root, r))
    .join("\n");
  const haystack = markup + html;

  const fired = [];
  const inputProblems = [];
  if (!doc.tauriVersion) inputProblems.push("framework-internals.json has no tauriVersion pin");
  if (!Array.isArray(doc.issuers) || doc.issuers.length === 0) inputProblems.push("framework-internals.json has no issuers");
  if (Array.isArray(doc.issuers) && !doc.issuers.some((issuer) => issuer.command && issuer.trigger?.kind !== "never")) {
    inputProblems.push("framework-internals.json has no releasable command issuer; this would silently green if extraction returned zero commands");
  }
  for (const issuer of doc.issuers) {
    if (issuer.trigger.kind === "never") continue;
    if (issuer.trigger.kind === "markup-contains" && !haystack.includes(issuer.trigger.needle)) continue;
    fired.push(issuer);
  }
  return { fired, staleness, pinned, declared: doc.tauriVersion, inputProblems };
}

/* ---------------------------------------------------------------- assemble */

export async function main(argv = process.argv) {
  const root = repoRoot(argv);
  const input = inputProblems(root, [
    "apps/osl-hub-ui/src/main.ts",
    "apps/osl-hub-ui/whatsapp-qa.html",
    "apps/osl-hub-ui/vite.config.ts",
    "apps/osl-hub/capabilities",
    "apps/osl-hub/Cargo.lock",
    "scripts/ledger/framework-internals.json",
  ]).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "acl", title: "commands issued vs ACL-granted, including framework-internal, ledger 3 of 7", violations: input }));
  }
  let snapshot;
  const noCache = argv.includes("--no-cache");
  try {
    snapshot = await bundleSnapshot(root, { cache: !noCache, writeCache: !noCache });
  } catch (error) {
    if (error.ledgerViolation) {
      return finish(report({ id: "acl", title: "commands issued vs ACL-granted, including framework-internal, ledger 3 of 7", violations: [error.ledgerViolation] }));
    }
    return finish(report({
      id: "acl",
      title: "commands issued vs ACL-granted, including framework-internal, ledger 3 of 7",
      violations: [{
        id: "bundle-reachability-unavailable",
        kind: "ledger-input-missing",
        detail: `bundle reachability could not be collected, so main-webview issuer scope is not trustworthy: ${error.message}`,
        sites: ["apps/osl-hub-ui/vite.config.ts:1"],
      }],
    }));
  }
  const mainModules = [
    ...new Set([
      ...MAIN_ENTRIES.flatMap((entry) => [...reachableFrom(snapshot, entry)]),
      ...EXTRA_MAIN_MODULES,
    ]),
  ];
  const reachability = entryReachability(snapshot);
  const importsByEntry = importedValuesByEntry(root, reachability.byEntry);
  const mainEntries = new Set(MAIN_ENTRIES);
  const api = tauriApiMethodCommands(root);
  const { issued, unresolved, dead } = scanIssuers(root, mainModules, api, { mainEntries, moduleEntries: reachability.byModule, importsByEntry });
  const internals = scanFrameworkInternals(root, mainModules);
  for (const issuer of internals.fired) {
    if (!issued.has(issuer.command)) issued.set(issuer.command, []);
    issued.get(issuer.command).push({ site: issuer.script, via: "framework-injected" });
  }

  const { grants, unresolvable, files } = grantsForWebview(root, "main");
  const blocks = permissionBlocks(root);
  const permissionDefs = commandPermissions(root, blocks);
  const allowedBy = permissionsAllowingCommand(blocks);
  const anyGrant = grantsAnyWebview(root);
  // Direction 1 is checked against the RAW registry, not the attribute-filtered
  // intersection: existence is what tauri-build checks, and the raw list is a
  // superset, so a grant is only reported dangling when nothing in either
  // handler list mentions the name at all.
  const { commands: registered, registry: registeredRegistry, problems: registryProblems, unregistered } = registeredCommands(root);
  const violations = [];

  for (const problem of registryProblems) {
    violations.push({ ...problem, kind: "ledger-input-stale" });
  }

  if (internals.staleness) {
    violations.push({ id: "framework-internals-stale", kind: "ledger-input-stale", detail: internals.staleness, sites: ["scripts/ledger/framework-internals.json:1"] });
  }
  for (const u of unresolvable) {
    violations.push({ id: `permission-set:${u}`, kind: "unexpandable-permission-set", detail: "a permission SET is granted; this ledger cannot expand it and refuses to guess", sites: [u] });
  }
  if (api.missing) {
    violations.push({ id: "tauri-api-missing", kind: "ledger-input-stale", detail: "@tauri-apps/api is not installed, so issuer class 2 could not be collected at all", sites: ["apps/osl-hub-ui/package.json:1"] });
  } else if (api.extracted === 0) {
    violations.push({ id: "tauri-api-empty", kind: "ledger-input-stale", detail: "@tauri-apps/api was present but no method-to-command bindings were extracted", sites: ["apps/osl-hub-ui/node_modules/@tauri-apps/api/package.json:1"] });
  }
  for (const problem of internals.inputProblems) {
    violations.push({ id: `framework-internals-input:${problem}`, kind: "ledger-input-stale", detail: problem, sites: ["scripts/ledger/framework-internals.json:1"] });
  }
  for (const problem of tauriApiExtractionProblems(api)) {
    violations.push({ id: `tauri-api-partial:${problem}`, kind: "ledger-input-stale", detail: problem, sites: ["apps/osl-hub-ui/node_modules/@tauri-apps/api/package.json:1"] });
  }

  for (const [command, sites] of [...issued.entries()].sort()) {
    const perm = permissionFor(command, permissionDefs);
    if (grants.has(perm)) continue;
    const viaInternal = sites.some((s) => s.via === "framework-injected");
    violations.push({
      id: command,
      kind: viaInternal ? "ungranted-framework-internal" : "ungranted",
      detail: `needs "${perm}" in a capability covering the main webview; issued via ${[...new Set(sites.map((s) => s.via))].join(", ")}`,
      sites: sites.map((s) => s.site),
    });
  }
  // Inverse direction 1 -- a grant naming a command that does not exist. This is
  // the direction that breaks the BUILD: tauri-build validates every capability
  // against the command registry, so D-146 took the shipping app from compiling
  // to not compiling while all three test suites stayed green.
  for (const block of blocks) {
    for (const command of block.commands) {
      if (registeredRegistry.has(command)) continue;
      violations.push({
        id: `granted-command-does-not-exist:${block.identifier}:${command}`,
        kind: "granted-command-does-not-exist",
        detail: `permission "${block.identifier}" allows command "${command}", which no Rust invoke_handler registers${anyGrant.has(block.identifier) ? `; ${anyGrant.get(block.identifier)} grants this permission, so tauri-build validates it against the registry and fails` : "; no capability grants this permission today, so it is a dead definition waiting to break the build the moment one does"}`,
        sites: [block.site],
      });
    }
  }

  // Inverse direction 2 -- a registered command that NO capability grants to ANY
  // webview, so the ACL rejects it before its handler runs. Not "ungranted to
  // main": the overlay capabilities legitimately hold commands main must never
  // have, and scoring those is the 54-false-positive version of this check.
  for (const [command, where] of [...registered.entries()].sort()) {
    const candidates = allowedBy.get(command) ?? [];
    const identifiers = candidates.length ? candidates.map((b) => b.identifier) : [permissionFor(command, permissionDefs)];
    if (identifiers.some((id) => anyGrant.has(id))) continue;
    violations.push({
      id: `registered-but-granted-to-no-webview:${command}`,
      kind: "registered-but-granted-to-no-webview",
      detail: candidates.length
        ? `registered on the IPC surface, and permission ${identifiers.map((i) => `"${i}"`).join(" / ")} exists, but no capability file grants it to any webview; the ACL rejects this command before its handler runs`
        : `registered on the IPC surface with no [[permission]] block and no capability grant anywhere; the ACL rejects this command before its handler runs`,
      sites: [where.definedAt, ...where.registeredAt, ...candidates.map((b) => b.site)],
    });
  }

  // The fifth artifact scripts/audit_capabilities.py used to check, on the tree
  // that actually ships: a fn carrying #[tauri::command] that no handler list
  // registers is unreachable from every webview no matter what the ACL says.
  for (const u of unregistered) {
    violations.push({
      id: `defined-command-not-registered:${u.name}`,
      kind: "defined-command-not-registered",
      detail: `${u.name} carries #[tauri::command] but no invoke_handler list registers it, so no webview can reach it and no grant can help`,
      sites: [u.site],
    });
  }

  for (const u of unresolved) {
    violations.push({
      id: `unresolved-command:${u.site}`,
      kind: "unresolved-command-name",
      detail: `${u.via} command is not a literal or known string constant: ${u.expr}`,
      sites: [u.site],
    });
  }
  const liveDead = dead.filter((d) => !grants.has(permissionFor(d.command, permissionDefs)));
  violations.push(...liveDead);

  return finish(
    report({
      id: "acl",
      title: "commands issued vs ACL-granted, including framework-internal, ledger 3 of 7",
      violations,
      stats: {
        "capability files covering webview main": files.length,
        "distinct permissions granted": grants.size,
        "modules reachable from main-window entries": mainModules.length,
        "distinct commands issued": issued.size,
        "permission identifiers granted to any webview": anyGrant.size,
        "[[permission]] blocks defined": blocks.length,
        "registered Rust commands (registry n #[tauri::command])": `${registered.size} (of ${registeredRegistry.size} registry identifiers)`,
        "framework-injected issuers that fire here": internals.fired.length,
        "tauri version (Cargo.lock / vendored)": `${internals.pinned} / ${internals.declared}`,
        "@tauri-apps/api version": api.version,
        "invoke() calls with a non-literal command (not analysable)": unresolved.length,
        "dead exported command issuers excepted": liveDead.length,
      },
    }),
  );
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) await main();
