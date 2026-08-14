import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SOURCE_ROOT = fileURLToPath(new URL(".", import.meta.url));
const ENTRYPOINT = join(SOURCE_ROOT, "main.ts");

// This is an intentional, temporary baseline: these modules are implemented but
// not yet in the shipping graph. Stage R/I must replace entries here with a
// reachable module, rather than letting the deletion lane disappear unnoticed.
//
// `scrub-review-list.ts` and `scrub-scope-fingerprint.ts` left this list when
// the Scrub route started rendering grouped owner-review rows and the scope
// digest, so the list is now shorter by exactly the two modules that became
// reachable -- not by any that merely stopped being checked.
//
// Everything still listed is deliberately unreachable. v1 Scrub is DISCOVERY:
// it shows the owner what an export exposes and gives manual directions. Every
// remaining entry is part of the live-deletion product -- IMAP delete ports,
// hosted-session ports and preloads, typed irreversible confirmation, dry-run
// and receipt views for actions this build never performs -- and must stay out
// of the shipping graph until the native authority, consent spend, and
// provider-specific reviewed-run path exist.
//
// `scrub-what-to-find.ts` (TASK 1413) is here for the opposite reason: it is
// not deletion, it is the Scrub SETUP page that asks what counts as a bad
// message. It has no shipping entry point yet because the setup host that
// walks accounts (TASK 1402) → consent (TASK 1408) → this page → ready to scan
// (TASK 1418) does not exist in this tree. It leaves this list when that host
// renders it, not when anything about the page itself changes.
//
// `autoscrub-progress.ts` stays for a narrower reason: its completion counter
// only advances through `runNext(remove)`, the destructive step. Nothing in
// this build can start a run, so the only ways to render it would be to call
// the destructive path with a no-op -- recording deletions that never happened
// -- or to widen its API so a caller can assert a completed count it did not
// earn. Both would make the UI claim more than the build does.
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
  "scrub-what-to-find.ts",
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
