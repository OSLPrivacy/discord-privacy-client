import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, expect, it } from "vitest";

const ATTACKS = [
  "twelve-digits-three-groups-of-four",
  "regroup-sixty-digits-into-fours",
  "truncate-to-first-forty-bits",
  "drop-scannable-code",
  "code-disagrees-with-rendered-digits",
  "derive-from-one-identity-key",
  "second-renderer-beside-5068-panel",
  "accept-verifies-different-pair",
] as const;

type Attack = typeof ATTACKS[number];
type Sources = Record<string, string>;

const uiRoot = process.cwd();
const worktreeRoot = join(uiRoot, "..", "..");
const PATHS = [
  "apps/osl-hub-ui/src/safety-number-panel.ts",
  "apps/osl-hub-ui/src/main.ts",
  "apps/osl-hub-ui/src/task_6919_verify_dialog.test.ts",
  "apps/osl-hub/src/security.rs",
  "crates/ipc/src/tofu.rs",
] as const;

const shippingSources: Sources = Object.fromEntries(PATHS.map((path) => [path, readFileSync(join(worktreeRoot, path), "utf8")])) as Sources;

function replaceOnce(text: string, from: string, to: string, attack: Attack): string {
  const count = text.split(from).length - 1;
  if (count !== 1) throw new Error(`TASK6920 mutation=${attack} expected one target found=${count}`);
  return text.replace(from, to);
}

function changed(sources: Sources, path: string, from: string, to: string, attack: Attack): Sources {
  return { ...sources, [path]: replaceOnce(sources[path], from, to, attack) };
}

function mutate(attack: Attack): Sources {
  const panel = "apps/osl-hub-ui/src/safety-number-panel.ts";
  const main = "apps/osl-hub-ui/src/main.ts";
  const security = "apps/osl-hub/src/security.rs";
  switch (attack) {
    case "twelve-digits-three-groups-of-four":
      return changed(shippingSources, panel, "/^(?:\\d{5})(?: \\d{5}){11}$/u", "/^\\d{4} \\d{4} \\d{4}$/u", attack);
    case "regroup-sixty-digits-into-fours":
      return changed(shippingSources, panel, "/^(?:\\d{5})(?: \\d{5}){11}$/u", "/^(?:\\d{4})(?: \\d{4}){14}$/u", attack);
    case "truncate-to-first-forty-bits": {
      const shortened = changed(shippingSources, panel, "const digits = safetyNumber.replaceAll(/\\s/g, \"\");", "const digits = safetyNumber.replaceAll(/\\s/g, \"\").slice(0, 12);", attack);
      return changed(shortened, panel, "return /^\\d{60}$/u.test(digits) ? digits : null;", "return /^\\d{12}$/u.test(digits) ? digits : null;", attack);
    }
    case "drop-scannable-code":
      return changed(shippingSources, panel, '<div class="safety-number-scannable" data-safety-number-payload="${payload}">${qrModulesSvg(payload)}</div>', "", attack);
    case "code-disagrees-with-rendered-digits":
      return changed(
        shippingSources,
        panel,
        '<div class="safety-number-scannable" data-safety-number-payload="${payload}">${qrModulesSvg(payload)}</div>',
        '<div class="safety-number-scannable" data-safety-number-payload="${payload[0] === "0" ? `1${payload.slice(1)}` : `0${payload.slice(1)}`}">${qrModulesSvg(payload[0] === "0" ? `1${payload.slice(1)}` : `0${payload.slice(1)}`)}</div>',
        attack,
      );
    case "derive-from-one-identity-key":
      return changed(shippingSources, security, "ipc::tofu::safety_number_pair(&mine, peer)", "ipc::tofu::safety_number(&mine)", attack);
    case "second-renderer-beside-5068-panel":
      return changed(shippingSources, main, "${safetyNumberPanelMarkup(copy.code)}<label", "${safetyNumberPanelMarkup(copy.code)}<code class=\"verification-code\">${copy.code}</code><label", attack);
    case "accept-verifies-different-pair":
      return changed(shippingSources, main, "verifyHubPersonAndRefresh(request.personId, typedVerificationCode)", "verifyHubPersonAndRefresh(\"different-person\", typedVerificationCode)", attack);
  }
}

function writeSources(directory: string, sources: Sources): void {
  for (const [relative, contents] of Object.entries(sources)) {
    const target = join(directory, relative);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, contents);
  }
  const packageRoot = join(directory, "apps", "osl-hub-ui");
  writeFileSync(join(packageRoot, "package.json"), '{"private":true,"type":"module"}\n');
  symlinkSync(join(uiRoot, "node_modules"), join(packageRoot, "node_modules"), "dir");
}

function run6919(name: string, sources: Sources): { readonly status: number | null; readonly output: string; readonly discarded: boolean } {
  const directory = mkdtempSync(join(uiRoot, `.task-6920-${name}-`));
  let status: number | null = null;
  let output = "";
  try {
    writeSources(directory, sources);
    const packageRoot = join(directory, "apps", "osl-hub-ui");
    const vitest = join(uiRoot, "node_modules", "vitest", "vitest.mjs");
    const result = spawnSync(process.execPath, [vitest, "run", "src/task_6919_verify_dialog.test.ts", "--reporter=verbose"], {
      cwd: packageRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        NO_COLOR: "1",
        TASK6919_UI_ROOT: packageRoot,
        TASK6919_WORKTREE_ROOT: directory,
      },
    });
    status = result.status;
    output = `${result.stdout}${result.stderr}`;
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
  return { status, output, discarded: !existsSync(directory) };
}

function requireCase(present: boolean, name: string): void {
  expect(present, `TASK6920 absent case=${name}`).toBe(true);
}

function starvedRequiredCase(): string | null {
  if (process.env.TASK6920_SKIP_IDENTITY_A === "1") return "identity=alice";
  if (process.env.TASK6920_SKIP_IDENTITY_B === "1") return "identity=bob";
  if (process.env.TASK6920_SKIP_REFERENCE === "1") return "reference-implementation";
  if (process.env.TASK6920_SKIP_DECODE === "1") return "decode";
  if (process.env.TASK6920_SKIP_RESTORATION === "1") return "restoration";
  return null;
}

describe("TASK 6920 verify-dialog mutation proof", () => {
  it("runs restored 6919 for two identities, reference derivation, decode, and restoration", () => {
    requireCase(process.env.TASK6920_SKIP_IDENTITY_A !== "1", "identity=alice");
    requireCase(process.env.TASK6920_SKIP_IDENTITY_B !== "1", "identity=bob");
    requireCase(process.env.TASK6920_SKIP_REFERENCE !== "1", "reference-implementation");
    requireCase(process.env.TASK6920_SKIP_DECODE !== "1", "decode");
    requireCase(process.env.TASK6920_SKIP_RESTORATION !== "1", "restoration");
    const green = run6919("restored", shippingSources);
    expect(green.discarded, "TASK6920 restored copy must be discarded").toBe(true);
    expect(green.status, "TASK6920 restored 6919 gate").toBe(0);
    expect(green.output, "TASK6920 restored reference").toContain("TASK6919_DIALOG clients=2 digits=60 groups=12 group_size=5");
    expect(green.output, "TASK6920 restored binding").toContain("TASK6919_BINDING renderer=1 pair=personId reference=both-identity-keys accept=request.personId");
    console.info(`TASK6920 restored_exit=${green.status} discarded=true identities=alice,bob reference=present decode=present`);
  }, 60_000);

  it("makes every short, regrouped, truncated, absent-code, disagreeing-code, single-key, second-renderer, and mismatched-accept copy red", () => {
    const starved = starvedRequiredCase();
    if (starved) {
      requireCase(false, starved);
      return;
    }
    const omitted = process.env.TASK6920_OMIT_MUTANT as Attack | undefined;
    if (omitted) {
      requireCase(false, `mutant=${omitted}`);
      return;
    }
    const observed: Attack[] = [];
    for (const attack of ATTACKS) {
      const red = run6919(attack, mutate(attack));
      expect(red.status, `TASK6920 mutation=${attack} must make 6919 exit 1`).toBe(1);
      expect(red.output, `TASK6920 mutation=${attack} must name digit count, group, or key`).toMatch(/TASK6919_(?:DIGIT_COUNT|GROUP|KEY)/u);
      observed.push(attack);
      console.info(`TASK6920 mutation=${attack} exit=${red.status} discarded=true guard=${red.output.match(/TASK6919_(?:DIGIT_COUNT|GROUP|KEY)[^\n]*/u)?.[0] ?? "missing"}`);
    }
    for (const attack of ATTACKS) requireCase(observed.includes(attack), `mutant=${attack}`);
    console.info(`TASK6920 mutations=${observed.length} copies_discarded=${observed.length}`);
  }, 120_000);

  it("fails closed when a mutant, identity, reference, decode, or restoration is starved", () => {
    const omitted = process.env.TASK6920_OMIT_MUTANT as Attack | undefined;
    for (const attack of ATTACKS) requireCase(attack !== omitted, `mutant=${attack}`);
    requireCase(process.env.TASK6920_SKIP_IDENTITY_A !== "1", "identity=alice");
    requireCase(process.env.TASK6920_SKIP_IDENTITY_B !== "1", "identity=bob");
    requireCase(process.env.TASK6920_SKIP_REFERENCE !== "1", "reference-implementation");
    requireCase(process.env.TASK6920_SKIP_DECODE !== "1", "decode");
    requireCase(process.env.TASK6920_SKIP_RESTORATION !== "1", "restoration");
    console.info(`TASK6920_COVERAGE mutants=${ATTACKS.length} identity_a=present identity_b=present reference=present decode=present restoration=present`);
  });
});
