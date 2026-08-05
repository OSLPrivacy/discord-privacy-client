import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Master §7.13: "OSL must not read/copy Discord tokens, cookies, LevelDB, or a
 * live profile to create a second session." No 'same account, second window,
 * zero login' implementation may be achieved by token extraction.
 *
 * There was no gate on this. `security.test.ts` already walks
 * `src-tauri/src/` via `readProductionRustTree`, but that helper filters to
 * `entry.name.endsWith(".rs")` (`security.test.ts:20`), so the legacy
 * `Authorization`-header sniffer in `src-tauri/src/injection/boot.js` is
 * invisible to it -- the scanner visits the directory and skips the file on an
 * extension check. This gate closes that blind spot for JavaScript and
 * TypeScript, and refuses the legacy shell as source material too.
 */

const HERE = fileURLToPath(new URL(".", import.meta.url));
const REPO = join(HERE, "..", "..", "..");

function readFile(...parts: string[]): string {
  return readFileSync(join(REPO, ...parts), "utf8");
}

/** Every non-test source file under `root` whose name ends with one of `suffixes`. */
function sourceFiles(root: string, suffixes: readonly string[]): { path: string; text: string }[] {
  const skipped = new Set(["node_modules", "target", "dist", "fixtures", "testdata", "tests", ".git"]);
  const found: { path: string; text: string }[] = [];
  const visit = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const full = join(directory, entry.name);
      if (entry.isDirectory()) {
        if (!skipped.has(entry.name)) visit(full);
      } else if (
        entry.isFile()
        && suffixes.some((suffix) => entry.name.endsWith(suffix))
        && !entry.name.includes(".test.")
      ) {
        found.push({ path: full.slice(REPO.length + 1), text: readFileSync(full, "utf8") });
      }
    }
  };
  visit(join(REPO, root));
  return found;
}

/**
 * The shapes that constitute credential extraction. Each is the mechanism, not
 * a word: a header sniff, a stolen bearer replayed at Discord, a profile store
 * read. The positive control below keeps the patterns calibrated without
 * allowing a live source file to remain as the fixture.
 */
const CREDENTIAL_CAPTURE = [
  { name: "setRequestHeader interception", pattern: /XMLHttpRequest\.prototype\.setRequestHeader\s*=/u },
  { name: "Authorization header capture into a variable", pattern: /=\s*[A-Za-z_$][\w$]*\.(?:get|headers)\s*\(\s*["'`]authorization["'`]/iu },
  { name: "Authorization header written into an outbound Discord request", pattern: /["'`]?Authorization["'`]?\s*[:=]\s*(?!["'`]Bearer\s|["'`]\$\{)[A-Za-z_$][\w$.[\]"']*(?:Token|token|auth|Auth)/u },
  { name: "discord.com/api call carrying a caller-supplied bearer", pattern: /\/api\/v\d+\/channels\/[^"'`\s]*["'`][\s\S]{0,400}?Authorization/u },
  // Mechanism-precise on purpose. An earlier draft matched `localStorage` and
  // the prose "Discord profile" and produced 13 hits, every one of them a doc
  // comment stating that OSL does NOT do this -- the exact way an inverse-grep
  // manufactures a finding. These two now require a credential *store* by name
  // or a Discord profile *path*, not the words.
  { name: "LevelDB credential-store read", pattern: /leveldb|\.ldb\b/iu },
  { name: "Discord profile credential-store path", pattern: /discord[\\/][^"'`\n]{0,120}(?:Local\s?Storage|leveldb|Cookies|Login\s?Data)/iu },
] as const;

/** The shipping product: the hub binary, the shared crates, and the hub renderer. */
const SHIPPING_TREES = [
  { root: "apps/osl-hub/src", suffixes: [".rs"] },
  { root: "apps/osl-hub-ui/src", suffixes: [".ts", ".js"] },
  { root: "crates", suffixes: [".rs"] },
] as const;

describe("§7.13 · OSL never extracts a Discord credential", () => {
  it("finds no credential-capture mechanism anywhere in the shipping product", () => {
    const offences: string[] = [];
    let scanned = 0;

    for (const tree of SHIPPING_TREES) {
      const files = sourceFiles(tree.root, tree.suffixes);
      // Positive control: an empty or mis-rooted scan must not pass vacuously.
      expect(files.length, `no sources scanned under ${tree.root}`).toBeGreaterThan(10);
      scanned += files.length;

      for (const file of files) {
        for (const rule of CREDENTIAL_CAPTURE) {
          const match = rule.pattern.exec(file.text);
          if (match) offences.push(`${file.path}: ${rule.name} -> ${match[0].slice(0, 120).replace(/\s+/gu, " ")}`);
        }
      }
    }

    expect(scanned).toBeGreaterThan(200);
    expect(offences).toEqual([]);
  });

  it("detects a synthetic sniffer, proving the patterns are not decoration", () => {
    const syntheticSniffer = String.raw`
      XMLHttpRequest.prototype.setRequestHeader = function (name, value) {
        if (name.toLowerCase() === "authorization") editOverlayAuthToken = value;
      };
      fetch("/api/v9/channels/1/messages/2", {
        method: "PATCH",
        headers: { Authorization: editOverlayAuthToken }
      });
    `;
    const matched = CREDENTIAL_CAPTURE.filter((rule) => rule.pattern.test(syntheticSniffer)).map((rule) => rule.name);

    expect(matched, "patterns no longer detect the known sniffer").toContain("setRequestHeader interception");
    expect(matched, "patterns no longer detect the known sniffer").toContain("Authorization header written into an outbound Discord request");
    expect(matched, "patterns no longer detect the known sniffer").toContain("discord.com/api call carrying a caller-supplied bearer");
  });

  it("finds no credential-capture mechanism in the legacy shell source either", () => {
    const legacy = readFile("src-tauri", "src", "injection", "boot.js");
    const matched = CREDENTIAL_CAPTURE.filter((rule) => rule.pattern.test(legacy)).map((rule) => rule.name);

    expect(legacy).not.toContain("editOverlayAuthToken");
    expect(matched).toEqual([]);
  });

  it("keeps the legacy shell structurally unreachable from the shipping build", () => {
    // Root workspace excludes it, so no `cargo --workspace` gate builds it.
    const cargo = readFile("Cargo.toml");
    expect(cargo.split("exclude = [")[1]?.split("]")[0]).toContain('"src-tauri"');

    // The two apps are separate bundles; the shipping one never points its
    // frontend or identifier at the legacy shell.
    const hub = JSON.parse(readFile("apps", "osl-hub", "tauri.conf.json")) as { identifier: string; build: { frontendDist: string } };
    const legacy = JSON.parse(readFile("src-tauri", "tauri.conf.json")) as { identifier: string; build: { frontendDist: string } };
    expect(hub.identifier).toBe("org.oslprivacy.hub");
    expect(hub.build.frontendDist).toBe("../osl-hub-ui/dist");
    expect(hub.identifier).not.toBe(legacy.identifier);
    expect(hub.build.frontendDist).not.toBe(legacy.build.frontendDist);

    // CI may syntax-check the legacy file, but must never build or bundle it.
    const workflows = sourceFiles(join(".github", "workflows"), [".yml", ".yaml"]);
    expect(workflows.length).toBeGreaterThan(0);
    for (const workflow of workflows) {
      for (const line of workflow.text.split("\n")) {
        if (!line.includes("src-tauri")) continue;
        // A YAML comment cannot build anything. D-159 replaced the deleted
        // audit_capabilities.py step with a comment explaining that it had been
        // auditing src-tauri -- the excluded legacy shell -- rather than the
        // shipping app, and that explanation tripped this guard. The prohibition
        // is "CI must never BUILD or BUNDLE the legacy shell"; skipping comments
        // keeps exactly that and stops the guard from policing prose.
        if (/^\s*#/u.test(line)) continue;
        expect(line, `${workflow.path} does more than syntax-check the legacy shell`).toMatch(/node --check/u);
      }
    }
  });
});
