#!/usr/bin/env node
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "../..");
const HUB_CAPABILITY = path.join(REPO_ROOT, "apps/osl-hub/capabilities/hub.json");
const HUB_MAIN = path.join(REPO_ROOT, "apps/osl-hub-ui/src/main.ts");
const TAURI_WINDOW_API = path.join(REPO_ROOT, "apps/osl-hub-ui/node_modules/@tauri-apps/api/window.js");
const HUB_CARGO_LOCK = path.join(REPO_ROOT, "apps/osl-hub/Cargo.lock");

const methodAllowList = new Set([
  "close",
  "isFocused",
  "isFullscreen",
  "isMaximized",
  "minimize",
  "setFocus",
  "setFullscreen",
  "startDragging",
  "toggleMaximize"
]);

function readText(filePath) {
  try {
    return fs.readFileSync(filePath, "utf8");
  } catch (error) {
    throw new Error(`failed to read ${path.relative(REPO_ROOT, filePath)}: ${error.message}`);
  }
}

function lineNumber(source, index) {
  let line = 1;
  for (let cursor = 0; cursor < index; cursor += 1) {
    if (source.charCodeAt(cursor) === 10) line += 1;
  }
  return line;
}

function toPermission(command) {
  return `core:window:allow-${command.replaceAll("_", "-")}`;
}

function relativeOrAbsolute(filePath) {
  const relative = path.relative(REPO_ROOT, filePath);
  return relative.startsWith("..") ? filePath : relative;
}

function parseTauriVersion() {
  const cargoLock = readText(HUB_CARGO_LOCK);
  for (const block of cargoLock.split("[[package]]")) {
    if (!/\bname = "tauri"/.test(block)) continue;
    const match = block.match(/\bversion = "([^"]+)"/);
    if (match) return match[1];
  }
  throw new Error(`failed to find the tauri package version in ${relativeOrAbsolute(HUB_CARGO_LOCK)}`);
}

function findTauriSourceRoot() {
  if (process.env.TAURI_SRC_DIR) return process.env.TAURI_SRC_DIR;

  const version = parseTauriVersion();
  const registrySrc = path.join(process.env.CARGO_HOME || path.join(os.homedir(), ".cargo"), "registry/src");
  const candidates = [];
  for (const registry of fs.readdirSync(registrySrc, { withFileTypes: true })) {
    if (!registry.isDirectory()) continue;
    const candidate = path.join(registrySrc, registry.name, `tauri-${version}`);
    if (fs.existsSync(candidate)) candidates.push(candidate);
  }
  if (candidates.length > 0) return candidates[0];
  throw new Error(`failed to find tauri-${version} under ${registrySrc}; set TAURI_SRC_DIR to the Tauri crate checkout`);
}

function parseWindowApiCommands() {
  const source = readText(TAURI_WINDOW_API);
  const methodToCommand = new Map();
  const methodPattern = /async\s+([A-Za-z_$][\w$]*)\s*\([^)]*\)\s*\{/g;
  let methodMatch;
  while ((methodMatch = methodPattern.exec(source))) {
    const method = methodMatch[1];
    if (!methodAllowList.has(method)) continue;
    const bodyStart = methodMatch.index;
    const bodyEnd = source.indexOf("\n    }", bodyStart);
    const body = source.slice(bodyStart, bodyEnd === -1 ? bodyStart + 1200 : bodyEnd);
    const commandMatch = body.match(/invoke\('plugin:window\|([a-z0-9_]+)'/);
    if (commandMatch) methodToCommand.set(method, commandMatch[1]);
  }
  return methodToCommand;
}

function extractFrontendCommands(methodToCommand) {
  const source = readText(HUB_MAIN);
  const receiverNames = new Set();
  const receiverPattern = /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*getCurrentWindow\(\)/g;
  let receiverMatch;
  while ((receiverMatch = receiverPattern.exec(source))) {
    receiverNames.add(receiverMatch[1]);
  }

  const sources = [];
  const directPattern = /getCurrentWindow\(\)\.([A-Za-z_$][\w$]*)\s*\(/g;
  let directMatch;
  while ((directMatch = directPattern.exec(source))) {
    const method = directMatch[1];
    const command = methodToCommand.get(method);
    if (!command) continue;
    sources.push({
      command,
      origin: `${relativeOrAbsolute(HUB_MAIN)}:${lineNumber(source, directMatch.index)} getCurrentWindow().${method}()`
    });
  }

  if (receiverNames.size > 0) {
    const receiverPatternText = [...receiverNames].map((name) => name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|");
    const methodPattern = new RegExp(`\\b(?:${receiverPatternText})\\.([A-Za-z_$][\\w$]*)\\s*\\(`, "g");
    let methodMatch;
    while ((methodMatch = methodPattern.exec(source))) {
      const method = methodMatch[1];
      const command = methodToCommand.get(method);
      if (!command) continue;
      sources.push({
        command,
        origin: `${relativeOrAbsolute(HUB_MAIN)}:${lineNumber(source, methodMatch.index)} ${source.slice(methodMatch.index, source.indexOf("(", methodMatch.index) + 1)}...)`
      });
    }
  }

  return sources;
}

function extractFrameworkDragCommands(tauriRoot) {
  const hubSource = readText(HUB_MAIN);
  if (!hubSource.includes("data-tauri-drag-region")) return [];

  const dragPath = path.join(tauriRoot, "src/window/scripts/drag.js");
  const dragSource = readText(dragPath);
  const commands = [];
  const ternaryPattern = /const\s+cmd\s*=\s*[^?]+\?\s*'([a-z0-9_]+)'\s*:\s*'([a-z0-9_]+)'/g;
  let ternaryMatch;
  while ((ternaryMatch = ternaryPattern.exec(dragSource))) {
    for (const command of ternaryMatch.slice(1)) {
      commands.push({
        command,
        origin: `${relativeOrAbsolute(dragPath)}:${lineNumber(dragSource, ternaryMatch.index)} Tauri drag-region script`
      });
    }
  }

  const invokePattern = /plugin:window\|([a-z0-9_]+)/g;
  let invokeMatch;
  while ((invokeMatch = invokePattern.exec(dragSource))) {
    commands.push({
      command: invokeMatch[1],
      origin: `${relativeOrAbsolute(dragPath)}:${lineNumber(dragSource, invokeMatch.index)} Tauri drag-region invoke`
    });
  }
  return commands;
}

function parseDefaultWindowPermissions(tauriRoot) {
  const referencePath = path.join(tauriRoot, "permissions/window/autogenerated/reference.md");
  const source = readText(referencePath);
  const defaultSection = source.slice(0, source.indexOf("## Permission Table"));
  return new Set([...defaultSection.matchAll(/- `allow-([a-z0-9-]+)`/g)].map((match) => `core:window:allow-${match[1]}`));
}

function parseGrantedWindowPermissions(tauriRoot) {
  const capability = JSON.parse(readText(HUB_CAPABILITY));
  const permissions = Array.isArray(capability.permissions) ? capability.permissions : [];
  const granted = new Set();
  for (const permission of permissions) {
    if (permission === "core:window:default") {
      for (const defaultPermission of parseDefaultWindowPermissions(tauriRoot)) granted.add(defaultPermission);
      continue;
    }
    if (typeof permission === "string" && permission.startsWith("core:window:allow-")) {
      granted.add(permission);
    }
  }
  return granted;
}

function uniqueSources(sources) {
  const seen = new Set();
  return sources.filter((source) => {
    const key = `${source.command}\0${source.origin}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function main() {
  const tauriRoot = findTauriSourceRoot();
  const methodToCommand = parseWindowApiCommands();
  const commandSources = uniqueSources([
    ...extractFrontendCommands(methodToCommand),
    ...extractFrameworkDragCommands(tauriRoot)
  ]).sort((a, b) => a.command.localeCompare(b.command) || a.origin.localeCompare(b.origin));
  const granted = parseGrantedWindowPermissions(tauriRoot);
  const requiredByCommand = new Map();
  for (const source of commandSources) {
    if (!requiredByCommand.has(source.command)) requiredByCommand.set(source.command, []);
    requiredByCommand.get(source.command).push(source.origin);
  }

  const missing = [...requiredByCommand.keys()]
    .map((command) => ({ command, permission: toPermission(command), origins: requiredByCommand.get(command) }))
    .filter(({ permission }) => !granted.has(permission))
    .sort((a, b) => a.permission.localeCompare(b.permission));

  console.log(`ACL diff for ${relativeOrAbsolute(HUB_CAPABILITY)}`);
  console.log(`Tauri source: ${relativeOrAbsolute(tauriRoot)}`);
  console.log("Required window commands:");
  for (const [command, origins] of [...requiredByCommand.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    console.log(`  ${command} -> ${toPermission(command)}`);
    for (const origin of origins) console.log(`    ${origin}`);
  }
  console.log("Granted window permissions:");
  for (const permission of [...granted].sort()) console.log(`  ${permission}`);

  if (missing.length > 0) {
    console.log("ACL diff: RED");
    for (const { command, permission, origins } of missing) {
      console.log(`  missing ${permission} for plugin:window|${command}`);
      for (const origin of origins) console.log(`    ${origin}`);
    }
    process.exitCode = 1;
    return;
  }

  console.log("ACL diff: GREEN");
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 2;
}
