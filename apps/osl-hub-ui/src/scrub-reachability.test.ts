import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SOURCE_ROOT = fileURLToPath(new URL(".", import.meta.url));
const ENTRYPOINT = join(SOURCE_ROOT, "main.ts");

// This is an intentional, temporary baseline: these modules are implemented but
// not yet in the shipping graph. Stage R/I must replace entries here with a
// reachable module, rather than letting the deletion lane disappear unnoticed.
const KNOWN_ORPHANS = [
  "autoscrub-progress.ts",
  "scrub-attended-imap-run.ts",
  "scrub-confirm.ts",
  "scrub-coverage-view.ts",
  "scrub-dryrun-view.ts",
  "scrub-hosted-session-assisted.ts",
  "scrub-hosted-session-channel.ts",
  "scrub-hosted-session-port.ts",
  "scrub-hosted-session-scan.ts",
  "scrub-imap-adapter.ts",
  "scrub-imap-ipc.ts",
  "scrub-imap.ts",
  "scrub-protected.ts",
  "scrub-provider-policy.ts",
  "scrub-provider-preloads.ts",
  "scrub-receipt-view.ts",
  "scrub-review-list.ts",
  "scrub-scope-fingerprint.ts",
] as const;

function scrubModules(): string[] {
  return readdirSync(SOURCE_ROOT)
    .filter((name) => /^(?:scrub-|autoscrub-)/u.test(name))
    .filter((name) => extname(name) === ".ts" && !name.endsWith(".test.ts"))
    .sort();
}

function localImports(file: string): string[] {
  const source = readFileSync(file, "utf8");
  const imports = new Set<string>();
  const expression = /^\s*(?:import|export)\s+(?!type\b)(?:[\s\S]*?\s+from\s+)?["'](\.[^"']+)["'];?/gmu;

  for (const match of source.matchAll(expression)) {
    const specifier = match[1];
    const candidate = normalize(join(dirname(file), specifier));
    const resolved = extname(candidate) ? candidate : `${candidate}.ts`;
    if (existsSync(resolved)) imports.add(resolved);
  }

  return [...imports];
}

function reachableFromEntrypoint(): Set<string> {
  const reachable = new Set<string>();
  const pending = [ENTRYPOINT];

  while (pending.length) {
    const file = pending.pop();
    if (!file || reachable.has(file)) continue;
    reachable.add(file);
    pending.push(...localImports(file));
  }

  return reachable;
}

describe("Scrub deletion lane reachability", () => {
  it("records every scrub/autoscrub module outside the shipping import graph", () => {
    const reachable = reachableFromEntrypoint();
    const orphaned = scrubModules().filter((name) => !reachable.has(join(SOURCE_ROOT, name)));

    expect(orphaned).toEqual(KNOWN_ORPHANS);
  });
});
