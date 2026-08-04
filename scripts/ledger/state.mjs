#!/usr/bin/env node
// LEDGER 8 -- persisted state writes vs reads that reach behaviour.
//
// This is a conservative static check. It follows concrete storage keys and
// Rust persisted fields far enough to separate "read back into markup" from
// "read into an action boundary"; it is not a full TypeScript/Rust data-flow
// engine. A read is counted as behavioural only when the local consumer reaches
// a native invoke, transport/crypto/storage parameter, destructive/action verb,
// or a non-render decision context. Reads that only feed templates, class names,
// attributes, or test snapshots remain render-only and keep the key RED.
//
//   node scripts/ledger/state.mjs [--root=<dir>]

import { resolve } from "node:path";
import {
  repoRoot,
  read,
  walk,
  uiSources,
  blankComments,
  lineIndex,
  lineOf,
  inputProblems,
  stringConstants,
  resolveArg,
  isTest,
  isDecl,
} from "./lib/io.mjs";
import { report, finish } from "./lib/report.mjs";

const REQUIRED = [
  "apps/osl-hub-ui/src",
  "apps/osl-hub/src",
  "crates/ipc/src",
  "crates/store/src",
];

const TITLE = "persisted state writes vs behavioural reads, ledger 8 of 8";

function addSite(map, id, site, extra = {}) {
  if (!map.has(id)) {
    map.set(id, {
      id,
      key: extra.key ?? id,
      label: extra.label ?? id,
      store: extra.store ?? "state",
      kind: extra.kind ?? "state",
      writeSites: [],
      readSites: [],
      renderSites: [],
      behaviouralSites: [],
      details: new Set(),
    });
  }
  const entry = map.get(id);
  if (extra.key && entry.key === id) entry.key = extra.key;
  if (extra.label && entry.label === id) entry.label = extra.label;
  if (extra.detail) entry.details.add(extra.detail);
  if (extra.write) entry.writeSites.push(site);
  if (extra.read) entry.readSites.push(site);
  if (extra.renderOnly) entry.renderSites.push(site);
  if (extra.behavioural) entry.behaviouralSites.push(site);
  return entry;
}

function identifierLabel(raw, fallback) {
  const trimmed = raw.trim();
  const id = /^[A-Za-z_$][\w$]*$/.test(trimmed) ? trimmed : fallback;
  if (!id) return fallback;
  return id
    .replace(/StorageKey$/u, "")
    .replace(/Key$/u, "")
    .replace(/^browserFirstRun/u, "firstRun")
    .replace(/^browser/u, "")
    .replace(/^oslChat/u, "oslChat");
}

function resolveKey(raw, constants) {
  const arg = resolveArg(raw.trim(), constants);
  if (arg.value) return { value: arg.value, label: identifierLabel(raw, arg.value), resolved: true };
  return { value: raw.trim(), label: `unresolved:${raw.trim().slice(0, 60)}`, resolved: false };
}

function surroundingFunctionName(source, index) {
  const before = source.slice(0, index);
  const matches = [...before.matchAll(/\b(?:async\s+)?function\s+([A-Za-z_$][\w$]*)\s*\(|\b(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\([^)]*\)\s*=>/g)];
  const last = matches.at(-1);
  return last ? (last[1] ?? last[2]) : "";
}

function contextWindow(source, index, before = 900, after = 1300) {
  return source.slice(Math.max(0, index - before), Math.min(source.length, index + after));
}

function isRenderOnlyContext(context) {
  return /return\s*`|innerHTML|insertAdjacentHTML|textContent|className|classList|dataset\.|aria-|role=|<section\b|<div\b|<label\b|<button\b|<input\b|statusTag\(|Markup\(|Content\(/u.test(context);
}

function isBehaviourContext(context, functionName = "") {
  const actionBoundary = /\binvoke\s*\(|\bfetch\s*\(|\bqueue\b|\btransport\b|\bdeliver\b|\bupload\b|\bdownload\b|\baead\b|\bseal\b|\bopen_record\b|\bdecrypt\b|\bencrypt\b|\bcrypto\b|\bset_window|\bcapture\b|\bwhitelist\b|\bfriend\b|\bscope\b|\bburn\b|\bdelete\b|\brevoke\b|\bscrub\b|\btor\b|\bmullvad\b|\bwrite_[A-Za-z0-9_]+\b|\bread_[A-Za-z0-9_]+\b|\.put\s*\(|\.get\s*\(|\.save\s*\(|\.load\s*\(/u;
  const decision = /\bif\s*\(|\bswitch\s*\(|\bmatch\s+|\bwhile\s*\(|\?[^:]+:/u;
  const renderOnly = isRenderOnlyContext(context);
  if (actionBoundary.test(context) && (!renderOnly || decision.test(context))) return true;
  if (/Consent|Authorization|Policy|Preference|Protection|Capture|Send|Queue|Transport|Decrypt|Encrypt|Whitelist|Burn|Scrub|Tor|Mullvad/u.test(functionName) && decision.test(context) && !renderOnly) {
    return true;
  }
  return false;
}

function classifyUse(source, index) {
  const context = contextWindow(source, index);
  const functionName = surroundingFunctionName(source, index);
  if (isBehaviourContext(context, functionName)) return "behavioural";
  return "render-only";
}

function jsSourceFiles(root) {
  return uiSources(root).filter((rel) => !rel.endsWith(".test.ts"));
}

function rustSourceFiles(root) {
  return [
    ...walk(root, "apps/osl-hub/src", (r) => r.endsWith(".rs") && !isTest(r)),
    ...walk(root, "crates/ipc/src", (r) => r.endsWith(".rs") && !isTest(r)),
    ...walk(root, "crates/store/src", (r) => r.endsWith(".rs") && !isTest(r)),
  ].filter((rel) => !isDecl(rel));
}

function collectFrontendStorage(root, files, constants) {
  const state = new Map();
  const unresolved = [];
  let secureStoreInstantiationSites = [];

  for (const rel of files) {
    const raw = read(root, rel);
    const src = blankComments(raw);
    const starts = lineIndex(src);
    if (rel !== "apps/osl-hub-ui/src/secure-local-store.ts") {
      for (const m of src.matchAll(/\bnew\s+SecureLocalStore\s*\(/g)) {
        secureStoreInstantiationSites.push(`${rel}:${lineOf(starts, m.index)}`);
      }
    }

    if (rel === "apps/osl-hub-ui/src/secure-local-store.ts") {
      continue;
    }

    for (const m of src.matchAll(/\b(localStorage|sessionStorage|storage)\.setItem\s*\(\s*([^,\n)]+)/g)) {
      const key = resolveKey(m[2], constants);
      const site = `${rel}:${lineOf(starts, m.index)}`;
      if (!key.resolved) {
        unresolved.push({ id: `unresolved-write:${site}`, site, expr: m[2].trim() });
        continue;
      }
      addSite(state, `frontend-localStorage:${key.label}`, site, {
        write: true,
        store: "frontend-localStorage",
        key: key.value,
        label: key.label,
        detail: `writes browser storage key ${JSON.stringify(key.value)}`,
      });
    }

    for (const m of src.matchAll(/\b(localStorage|sessionStorage|storage)\.getItem\s*\(\s*([^,\n)]+)/g)) {
      const key = resolveKey(m[2], constants);
      const site = `${rel}:${lineOf(starts, m.index)}`;
      const id = `frontend-localStorage:${key.label}`;
      const classification = classifyUse(src, m.index);
      addSite(state, id, site, {
        read: true,
        [classification === "behavioural" ? "behavioural" : "renderOnly"]: true,
        store: "frontend-localStorage",
        key: key.value,
        label: key.label,
      });
    }

    if (rel.endsWith("/main.ts")) {
      for (const m of src.matchAll(/\bpersistSensitiveOslChatJson\s*\(\s*([^,\n)]+)/g)) {
        const key = resolveKey(m[1], constants);
        if (!key.resolved || m[1].includes(":")) continue;
        const site = `${rel}:${lineOf(starts, m.index)}`;
        addSite(state, `frontend-secureLocalStore:${key.value}`, site, {
          write: true,
          store: "frontend-secureLocalStore",
          key: key.value,
          label: key.label,
          detail: `writes SecureLocalStore logical key ${JSON.stringify(key.value)}`,
        });
      }
      for (const m of src.matchAll(/\b(?:store|oslChatSecureStore)\.setItem\s*\(\s*([^,\n)]+)/g)) {
        const key = resolveKey(m[1], constants);
        if (!key.resolved) continue;
        if (!key.value.startsWith("osl-chat-") && !key.label.startsWith("oslChat")) continue;
        const site = `${rel}:${lineOf(starts, m.index)}`;
        addSite(state, `frontend-secureLocalStore:${key.value}`, site, {
          write: true,
          store: "frontend-secureLocalStore",
          key: key.value,
          label: key.label,
          detail: `writes SecureLocalStore logical key ${JSON.stringify(key.value)}`,
        });
      }
      for (const m of src.matchAll(/\bsecureOrLegacyOslChatPreference\s*\([^)]*,\s*[^)]*,\s*([^,\n)]+)/g)) {
        const key = resolveKey(m[1], constants);
        if (!key.resolved) continue;
        const site = `${rel}:${lineOf(starts, m.index)}`;
        const classification = classifyUse(src, m.index);
        addSite(state, `frontend-secureLocalStore:${key.value}`, site, {
          read: true,
          [classification === "behavioural" ? "behavioural" : "renderOnly"]: true,
          store: "frontend-secureLocalStore",
          key: key.value,
          label: key.label,
        });
      }
    }
  }

  if (secureStoreInstantiationSites.length === 0) {
    for (const entry of state.values()) {
      if (entry.store !== "frontend-secureLocalStore" || entry.writeSites.length === 0) continue;
      entry.details.add("SecureLocalStore is typed/configurable but no production source instantiates it");
    }
  }

  return { state, unresolved, secureStoreInstantiationSites };
}

const PREFERENCE_FIELD_NAMES = new Set([
  "onboarding_complete",
  "send_mode",
  "placement_mode",
  "show_plaintext_preview",
  "window_capture_enabled",
  "acknowledge_experimental_send_risk",
  "forward_secrecy_mode",
]);

function collectRustPreferences(root, files) {
  const state = new Map();
  for (const rel of files) {
    const raw = read(root, rel);
    const src = blankComments(raw);
    const starts = lineIndex(src);
    for (const field of PREFERENCE_FIELD_NAMES) {
      const fieldRe = new RegExp(`\\b${field}\\b`, "g");
      for (const m of src.matchAll(fieldRe)) {
        const site = `${rel}:${lineOf(starts, m.index)}`;
        const id = `rust-preferences:${field}`;
        const isDeclaration = /pub\s+[a-z_]+:|[a-z_]+:/u.test(src.slice(Math.max(0, m.index - 20), m.index + field.length + 2));
        if (isDeclaration || /write_preferences|serde_json::to_vec|save\(/u.test(contextWindow(src, m.index, 350, 450))) {
          addSite(state, id, site, {
            write: true,
            store: "rust-preferences",
            key: field,
            label: field,
            detail: "persisted through preview/onboarding preferences JSON",
          });
        } else {
          const classification = classifyUse(src, m.index);
          addSite(state, id, site, {
            read: true,
            [classification === "behavioural" ? "behavioural" : "renderOnly"]: true,
            store: "rust-preferences",
            key: field,
            label: field,
          });
        }
      }
    }
  }
  return state;
}

function collectRustSecureLocalStore(root, files, constants) {
  const state = new Map();
  const recordFactories = new Map();

  for (const rel of files) {
    const raw = read(root, rel);
    const src = blankComments(raw);
    const starts = lineIndex(src);
    for (const fnMatch of src.matchAll(/\bfn\s+([A-Za-z0-9_]*record_id[A-Za-z0-9_]*)\s*\([^)]*\)[\s\S]{0,500}?RecordId::new\s*\(\s*([^,\n)]+)\s*,\s*([^,\n)]+)\s*\)/g)) {
      const namespace = resolveKey(fnMatch[2], constants);
      const key = resolveKey(fnMatch[3], constants);
      const label = `${namespace.value}:${key.resolved ? key.value : "*"}`;
      if (!recordFactories.has(rel)) recordFactories.set(rel, []);
      recordFactories.get(rel).push({ name: fnMatch[1], id: `rust-secureLocalStore:${label}`, label, key: label, site: `${rel}:${lineOf(starts, fnMatch.index)}` });
      addSite(state, `rust-secureLocalStore:${label}`, `${rel}:${lineOf(starts, fnMatch.index)}`, {
        write: true,
        store: "rust-secureLocalStore",
        key: label,
        label,
        detail: "RecordId factory declares a SecureLocalStore persisted record",
      });
    }
  }

  for (const rel of files) {
    const raw = read(root, rel);
    const src = blankComments(raw);
    const starts = lineIndex(src);
    for (const record of recordFactories.get(rel) ?? []) {
      const { name } = record;
      const callRe = new RegExp(`\\.\\s*(put|get|delete)\\s*\\(\\s*&(?:Self::)?${name}\\s*\\(`, "g");
      for (const m of src.matchAll(callRe)) {
        const site = `${rel}:${lineOf(starts, m.index)}`;
        const classification = m[1] === "get" ? classifyUse(src, m.index) : "behavioural";
        addSite(state, record.id, site, {
          read: m[1] === "get",
          write: m[1] !== "get",
          behavioural: classification === "behavioural",
          renderOnly: classification !== "behavioural" && m[1] === "get",
          store: "rust-secureLocalStore",
          key: record.key,
          label: record.label,
        });
      }
      const aliasNames = new Set();
      const aliasRe = new RegExp(`\\blet\\s+([A-Za-z_][A-Za-z0-9_]*)\\s*=\\s*[^;\\n]*(?:Self::|\\.)${name}\\s*\\(`, "g");
      for (const m of src.matchAll(aliasRe)) aliasNames.add(m[1]);
      for (const alias of aliasNames) {
        const aliasCallRe = new RegExp(`\\.\\s*(put|get|delete)\\s*\\(\\s*&${alias}\\b`, "g");
        for (const m of src.matchAll(aliasCallRe)) {
          const site = `${rel}:${lineOf(starts, m.index)}`;
          const classification = m[1] === "get" ? classifyUse(src, m.index) : "behavioural";
          addSite(state, record.id, site, {
            read: m[1] === "get",
            write: m[1] !== "get",
            behavioural: classification === "behavioural",
            renderOnly: classification !== "behavioural" && m[1] === "get",
            store: "rust-secureLocalStore",
            key: record.key,
            label: record.label,
          });
        }
      }
      const dynamicCallRe = new RegExp(`\\.\\s*(put|get|delete)\\s*\\(\\s*&[^,)]*\\.${name}\\s*\\(`, "g");
      for (const m of src.matchAll(dynamicCallRe)) {
        const site = `${rel}:${lineOf(starts, m.index)}`;
        const classification = m[1] === "get" ? classifyUse(src, m.index) : "behavioural";
        addSite(state, record.id, site, {
          read: m[1] === "get",
          write: m[1] !== "get",
          behavioural: classification === "behavioural",
          renderOnly: classification !== "behavioural" && m[1] === "get",
          store: "rust-secureLocalStore",
          key: record.key,
          label: record.label,
        });
      }
    }
  }
  return state;
}

function collectSqlState(root, files) {
  const state = new Map();
  for (const rel of files.filter((f) => f.startsWith("crates/store/src/"))) {
    const src = blankComments(read(root, rel));
    const starts = lineIndex(src);
    for (const m of src.matchAll(/INSERT\s+INTO\s+([a-zA-Z0-9_]+)\s*\(([^)]+)\)/g)) {
      const table = m[1];
      const cols = m[2].split(",").map((c) => c.trim().split(/\s+/u)[0]).filter((c) => /^[a-z_][a-z0-9_]*$/u.test(c));
      for (const col of cols) {
        addSite(state, `rust-sql:${table}.${col}`, `${rel}:${lineOf(starts, m.index)}`, {
          write: true,
          store: "rust-sql",
          key: `${table}.${col}`,
          label: `${table}.${col}`,
          detail: "SQLite schema/write column in crates/store",
        });
      }
    }
    for (const m of src.matchAll(/UPDATE\s+([a-zA-Z0-9_]+)\s+SET\s+([^;"']+)/g)) {
      const table = m[1];
      const cols = [...m[2].matchAll(/\b([a-z_][a-z0-9_]*)\s*=/g)].map((c) => c[1]);
      for (const col of cols) {
        addSite(state, `rust-sql:${table}.${col}`, `${rel}:${lineOf(starts, m.index)}`, {
          write: true,
          store: "rust-sql",
          key: `${table}.${col}`,
          label: `${table}.${col}`,
          detail: "SQLite update column in crates/store",
        });
      }
    }
    for (const m of src.matchAll(/SELECT\s+([^;"']+)\s+FROM\s+([a-zA-Z0-9_]+)/g)) {
      const table = m[2];
      const cols = m[1].split(",").map((c) => c.trim().replace(/\s+AS\s+.+$/iu, "")).filter((c) => /^[a-z_][a-z0-9_]*$/u.test(c));
      for (const col of cols) {
        const classification = classifyUse(src, m.index);
        addSite(state, `rust-sql:${table}.${col}`, `${rel}:${lineOf(starts, m.index)}`, {
          read: true,
          [classification === "behavioural" ? "behavioural" : "renderOnly"]: true,
          store: "rust-sql",
          key: `${table}.${col}`,
          label: `${table}.${col}`,
        });
      }
    }
  }
  return state;
}

function mergeMaps(...maps) {
  const out = new Map();
  for (const map of maps) {
    for (const entry of map.values()) {
      const merged = addSite(out, entry.id, entry.writeSites[0] ?? entry.readSites[0] ?? "scripts/ledger/state.mjs:1", {
        store: entry.store,
        key: entry.key,
        label: entry.label,
      });
      merged.writeSites.push(...entry.writeSites);
      merged.readSites.push(...entry.readSites);
      merged.renderSites.push(...entry.renderSites);
      merged.behaviouralSites.push(...entry.behaviouralSites);
      for (const detail of entry.details) merged.details.add(detail);
    }
  }
  return out;
}

function analyse(entries, unresolved) {
  const violations = [];
  for (const u of unresolved) {
    violations.push({
      id: u.id,
      kind: "unresolved-persisted-state-key",
      detail: `storage write key is not a literal or known string constant: ${u.expr}`,
      sites: [u.site],
    });
  }

  for (const entry of [...entries.values()].sort((a, b) => a.id.localeCompare(b.id))) {
    if (entry.writeSites.length === 0) continue;
    const sites = [...new Set([...entry.writeSites, ...entry.readSites, ...entry.renderSites, ...entry.behaviouralSites])];
    if (entry.store === "frontend-secureLocalStore" && [...entry.details].some((d) => d.includes("no production source instantiates"))) {
      violations.push({
        id: entry.id,
        kind: "secure-local-store-never-instantiated",
        detail: `${entry.label} (${entry.key}) is written through the SecureLocalStore hook, but production code never constructs the hook`,
        sites,
      });
      continue;
    }
    if (entry.behaviouralSites.length > 0) continue;
    if (entry.readSites.length === 0 && entry.renderSites.length === 0) {
      violations.push({
        id: entry.id,
        kind: "persisted-state-never-read",
        detail: `${entry.label} is persisted in ${entry.store}, but no scanned production read consumes it`,
        sites: entry.writeSites,
      });
      continue;
    }
    violations.push({
      id: entry.id,
      kind: "persisted-state-render-only",
      detail: `${entry.label} is read after persistence, but every observed consumer is render/display/validation-only rather than an action boundary`,
      sites,
    });
  }
  return violations;
}

export function collect(root) {
  const jsFiles = jsSourceFiles(root);
  const rustFiles = rustSourceFiles(root);
  const constants = stringConstants([...jsFiles, ...rustFiles], (rel) => read(root, rel));
  const frontend = collectFrontendStorage(root, jsFiles, constants);
  const rustPreferences = collectRustPreferences(root, rustFiles);
  const rustSecure = collectRustSecureLocalStore(root, rustFiles, constants);
  const sql = collectSqlState(root, rustFiles);
  const entries = mergeMaps(frontend.state, rustPreferences, rustSecure, sql);
  return {
    entries,
    unresolved: frontend.unresolved,
    stats: {
      "frontend production files scanned": jsFiles.length,
      "rust production files scanned": rustFiles.length,
      "persisted state entries found": entries.size,
      "unresolved persisted write keys": frontend.unresolved.length,
      "production SecureLocalStore instantiations": frontend.secureStoreInstantiationSites.length,
    },
  };
}

export function main(argv = process.argv) {
  const root = repoRoot(argv);
  const input = inputProblems(root, REQUIRED).map((p) => ({ ...p, kind: "ledger-input-missing" }));
  if (input.length) {
    return finish(report({ id: "state", title: TITLE, violations: input }));
  }
  const collected = collect(root);
  return finish(report({
    id: "state",
    title: TITLE,
    violations: analyse(collected.entries, collected.unresolved),
    stats: collected.stats,
  }));
}

if (resolve(process.argv[1] ?? "") === resolve(new URL(import.meta.url).pathname)) main();
