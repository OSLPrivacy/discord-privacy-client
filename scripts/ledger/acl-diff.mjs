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
//   node scripts/ledger/acl-diff.mjs [--root=<dir>]

import { readFileSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { repoRoot, read, walk, blankComments, lineIndex, lineOf, LEDGER_DIR } from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";
import { bundleSnapshot, reachableFrom } from "./bundle.mjs";

const MAIN_ENTRY = "apps/osl-hub-ui/index.html";

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

/** `plugin:window|is_maximized` -> `core:window:allow-is-maximized`; `foo_bar` -> `allow-foo-bar`. */
export function permissionFor(command) {
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
  return { byFile, version, missing: false };
}

/** Which @tauri-apps/api source file defines the handle a factory hands back. */
const FACTORY_RECEIVER = {
  getCurrentWindow: ["window.js"],
  getAllWindows: ["window.js"],
  getCurrentWebview: ["webview.js"],
  getCurrentWebviewWindow: ["webviewWindow.js", "window.js", "webview.js"],
};

/* ------------------------------------------------------------------ issuers */

const HANDLE_FACTORIES = /\b(getCurrentWindow|getCurrentWebview|getCurrentWebviewWindow|getAllWindows)\s*\(/;

export function scanIssuers(root, modules, apiMethods) {
  const issued = new Map(); // command -> [{site, via}]
  const add = (command, site, via) => {
    if (!issued.has(command)) issued.set(command, []);
    issued.get(command).push({ site, via });
  };
  const unresolved = [];

  for (const rel of modules) {
    if (!rel.endsWith(".ts")) continue;
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);

    // 1. direct invoke("name") / invoke<T>("name") / __TAURI_INTERNALS__.invoke("name")
    for (const m of src.matchAll(/\binvoke\s*(?:<[^>(]*>)?\s*\(\s*([^,)]+)/g)) {
      const arg = m[1].trim();
      const lit = /^"([^"]+)"$|^'([^']+)'$|^`([^`$]+)`$/.exec(arg);
      const site = `${rel}:${lineOf(starts, m.index)}`;
      if (lit) add(lit[1] ?? lit[2] ?? lit[3], site, "direct");
      else unresolved.push({ site, expr: arg.slice(0, 60) });
    }

    // 2. @tauri-apps/api handle methods. Only counted on a handle: either a
    //    direct `getCurrentWindow().m()` chain, or a variable that was assigned
    //    from one of the factory functions in this same file. Counting bare
    //    `.close()` anywhere would collide with our own methods and over-report.
    const handles = new Set();
    for (const m of src.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=\n]+)?=\s*(?:await\s+)?(getCurrentWindow|getCurrentWebview|getCurrentWebviewWindow)\s*\(/g)) {
      handles.add(m[1]);
    }
    const receivers = ["getCurrentWindow\\(\\)", "getCurrentWebview\\(\\)", "getCurrentWebviewWindow\\(\\)", ...handles].join("|");
    const chain = new RegExp(`(?:${receivers})\\s*\\.\\s*([A-Za-z_$][\\w$]*)\\s*\\(`, "g");
    for (const m of src.matchAll(chain)) {
      const method = m[1];
      const site = `${rel}:${lineOf(starts, m.index)}`;
      for (const cmd of apiMethods.get(method) ?? []) add(cmd, site, `@tauri-apps/api ${method}()`);
    }

    // 3. module-scope event helpers imported from @tauri-apps/api/event
    if (/from\s+"@tauri-apps\/api\/event"/.test(src)) {
      const imported = /import\s*\{([^}]*)\}\s*from\s+"@tauri-apps\/api\/event"/.exec(src)?.[1] ?? "";
      for (const name of imported.split(",").map((s) => s.trim().split(/\s+as\s+/)[0])) {
        if (!apiMethods.has(name)) continue;
        const use = new RegExp(`\\b${name}\\s*(?:<[^>(]*>)?\\s*\\(`, "g");
        for (const m of src.matchAll(use)) {
          for (const cmd of apiMethods.get(name)) {
            add(cmd, `${rel}:${lineOf(starts, m.index)}`, `@tauri-apps/api/event ${name}()`);
          }
        }
      }
    }
  }
  return { issued, unresolved };
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
  for (const issuer of doc.issuers) {
    if (issuer.trigger.kind === "never") continue;
    if (issuer.trigger.kind === "markup-contains" && !haystack.includes(issuer.trigger.needle)) continue;
    fired.push(issuer);
  }
  return { fired, staleness, pinned, declared: doc.tauriVersion };
}

/* ---------------------------------------------------------------- assemble */

export async function main(argv = process.argv) {
  const root = repoRoot(argv);
  const snapshot = await bundleSnapshot(root, { cache: !argv.includes("--no-cache") });
  const mainModules = [...reachableFrom(snapshot, MAIN_ENTRY)];
  const api = tauriApiMethodCommands(root);
  const { issued, unresolved } = scanIssuers(root, mainModules, api.map);
  const internals = scanFrameworkInternals(root, mainModules);
  for (const issuer of internals.fired) {
    if (!issued.has(issuer.command)) issued.set(issuer.command, []);
    issued.get(issuer.command).push({ site: issuer.script, via: "framework-injected" });
  }

  const { grants, unresolvable, files } = grantsForWebview(root, "main");
  const violations = [];

  if (internals.staleness) {
    violations.push({ id: "framework-internals-stale", kind: "ledger-input-stale", detail: internals.staleness, sites: ["scripts/ledger/framework-internals.json:1"] });
  }
  for (const u of unresolvable) {
    violations.push({ id: `permission-set:${u}`, kind: "unexpandable-permission-set", detail: "a permission SET is granted; this ledger cannot expand it and refuses to guess", sites: [u] });
  }
  if (api.missing) {
    violations.push({ id: "tauri-api-missing", kind: "ledger-input-stale", detail: "@tauri-apps/api is not installed, so issuer class 2 could not be collected at all", sites: ["apps/osl-hub-ui/package.json:1"] });
  }

  for (const [command, sites] of [...issued.entries()].sort()) {
    const perm = permissionFor(command);
    if (grants.has(perm)) continue;
    const viaInternal = sites.some((s) => s.via === "framework-injected");
    violations.push({
      id: command,
      kind: viaInternal ? "ungranted-framework-internal" : "ungranted",
      detail: `needs "${perm}" in a capability covering the main webview; issued via ${[...new Set(sites.map((s) => s.via))].join(", ")}`,
      sites: sites.map((s) => s.site),
    });
  }

  return finish(
    report({
      id: "acl",
      title: "commands issued vs ACL-granted, including framework-internal, ledger 3 of 7",
      violations,
      stats: {
        "capability files covering webview main": files.length,
        "distinct permissions granted": grants.size,
        "modules reachable from index.html": mainModules.length,
        "distinct commands issued": issued.size,
        "framework-injected issuers that fire here": internals.fired.length,
        "tauri version (Cargo.lock / vendored)": `${internals.pinned} / ${internals.declared}`,
        "@tauri-apps/api version": api.version,
        "invoke() calls with a non-literal command (not analysable)": unresolved.length,
      },
    }),
  );
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) await main();
