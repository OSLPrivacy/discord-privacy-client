#!/usr/bin/env node
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  BUILD_SWITCH_TEST_METADATA,
  type BuildSwitchTestMetadata,
  formatBuildSwitchTestMetadata,
  type RuntimeSwitchState,
  validateBuildSwitchTestMetadata,
} from "../src/lib/build-switch-metadata.ts";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = path.resolve(SCRIPT_DIR, "..");
const WRANGLER_TOML = path.join(PACKAGE_ROOT, "wrangler.toml");
const UPDATE_MANIFEST_TS = path.join(PACKAGE_ROOT, "src", "endpoints", "update-manifest.ts");

export function activeRuntimeSwitchesFromWranglerToml(toml: string): RuntimeSwitchState[] {
  const switches: RuntimeSwitchState[] = [];
  let inVars = false;
  for (const rawLine of toml.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    if (/^\[.*\]$/.test(line)) {
      inVars = line === "[vars]";
      continue;
    }
    if (!inVars) continue;
    const match = /^([A-Z0-9_]+)\s*=\s*"([^"]*)"/.exec(line);
    if (!match) continue;
    const [, name, value] = match;
    if (name?.endsWith("_ENABLED") && value === "true") {
      switches.push({ name, value });
    }
  }
  return switches;
}

export function oneBuildVersionFromUpdateManifestSource(source: string): string {
  const match = /export\s+const\s+PRODUCTION_VERSION\s*=\s*"([^"]+)"/.exec(source);
  if (!match?.[1]) {
    throw new Error("update-manifest production version is unavailable");
  }
  return match[1];
}

function metadataWithOmissions(omitted: readonly string[]): BuildSwitchTestMetadata {
  return {
    ...BUILD_SWITCH_TEST_METADATA,
    active_runtime_switches: BUILD_SWITCH_TEST_METADATA.active_runtime_switches.filter(
      (sw) => !omitted.includes(sw.name),
    ),
  };
}

function omittedSwitches(args: readonly string[]): string[] {
  const omitted: string[] = [];
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--omit") {
      const value = args[index + 1];
      if (!value) throw new Error("usage: build-switch-metadata.ts [--omit SWITCH]");
      omitted.push(value);
      index += 1;
      continue;
    }
    throw new Error(`unknown argument ${arg}`);
  }
  return omitted;
}

export function runBuildSwitchMetadataCli(args = process.argv.slice(2)): number {
  try {
    const metadata = metadataWithOmissions(omittedSwitches(args));
    const expected = activeRuntimeSwitchesFromWranglerToml(readFileSync(WRANGLER_TOML, "utf8"));
    const expectedVersion = oneBuildVersionFromUpdateManifestSource(
      readFileSync(UPDATE_MANIFEST_TS, "utf8"),
    );
    const errors = validateBuildSwitchTestMetadata(metadata, expected, expectedVersion);
    if (errors.length > 0) {
      for (const error of errors) console.error(error);
      return 1;
    }
    console.log(formatBuildSwitchTestMetadata(metadata));
    return 0;
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    return 1;
  }
}

if (process.argv[1] && import.meta.url === new URL(process.argv[1], "file:").href) {
  process.exitCode = runBuildSwitchMetadataCli();
}
