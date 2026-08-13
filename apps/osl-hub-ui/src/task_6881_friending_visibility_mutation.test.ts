import { copyFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { describe, expect, it } from "vitest";

const SOURCE_FILES = [
  "friending-visibility.ts",
  "onboarding-controls.ts",
  "main.ts",
  "task_6880_friending_visibility.test.ts",
] as const;

const ATTACKS = [
  "detach-findableByPublicUsername",
  "detach-friendRequestsAllowed",
  "detach-messageRequestsAllowed",
  "detach-profileViewable",
  "rename-profileViewable",
  "add-fifth-row",
  "remove-messageRequestsAllowed",
  "silent-leaves-profileViewable-on",
  "visible-leaves-profileViewable-off",
  "visible-chip-lit-at-1110",
  "onboarding-only-store",
] as const;

type Attack = typeof ATTACKS[number];
type PackageSources = Record<(typeof SOURCE_FILES)[number], string>;

const sourceRoot = join(process.cwd(), "src");

function source(name: (typeof SOURCE_FILES)[number]): string {
  return readFileSync(join(sourceRoot, name), "utf8");
}

const shippingSources: PackageSources = Object.fromEntries(
  SOURCE_FILES.map((name) => [name, source(name)]),
) as PackageSources;

function replaceOnce(sourceText: string, from: string, to: string, attack: Attack): string {
  const count = sourceText.split(from).length - 1;
  if (count !== 1) throw new Error(`TASK6881 mutation=${attack} expected one target, found ${count}`);
  return sourceText.replace(from, to);
}

function detachedSwitch(attack: Attack, row: string, switchName: string): PackageSources {
  return {
    ...shippingSources,
    "friending-visibility.ts": replaceOnce(
      shippingSources["friending-visibility.ts"],
      'data-friending-visibility-switch="${row.key}"',
      `data-friending-visibility-switch="${row.key === "${row}" ? "${switchName}" : row.key}"`,
      attack,
    ),
  };
}

function mutate(attack: Attack): PackageSources {
  switch (attack) {
    case "detach-findableByPublicUsername":
      return detachedSwitch(attack, "findableByPublicUsername", "friendRequestsAllowed");
    case "detach-friendRequestsAllowed":
      return detachedSwitch(attack, "friendRequestsAllowed", "messageRequestsAllowed");
    case "detach-messageRequestsAllowed":
      return detachedSwitch(attack, "messageRequestsAllowed", "profileViewable");
    case "detach-profileViewable":
      return detachedSwitch(attack, "profileViewable", "findableByPublicUsername");
    case "rename-profileViewable":
      return {
        ...shippingSources,
        "friending-visibility.ts": replaceOnce(
          shippingSources["friending-visibility.ts"],
          'label: "profile viewable"',
          'label: "profile discoverable"',
          attack,
        ),
      };
    case "add-fifth-row":
      return {
        ...shippingSources,
        "friending-visibility.ts": replaceOnce(
          shippingSources["friending-visibility.ts"],
          '  { key: "profileViewable", label: "profile viewable", detail: "Lets someone who reaches your profile see the profile details you chose to publish." },',
          '  { key: "profileViewable", label: "profile viewable", detail: "Lets someone who reaches your profile see the profile details you chose to publish." },\n  { key: "profileViewable", label: "onboarding-only extra", detail: "This fifth row must not ship." },',
          attack,
        ),
      };
    case "remove-messageRequestsAllowed":
      return {
        ...shippingSources,
        "friending-visibility.ts": replaceOnce(
          shippingSources["friending-visibility.ts"],
          '  { key: "messageRequestsAllowed", label: "message requests allowed", detail: "Lets someone who finds you send an OSL Chat request; it does not open a chat automatically." },\n',
          "",
          attack,
        ),
      };
    case "silent-leaves-profileViewable-on":
      return {
        ...shippingSources,
        "friending-visibility.ts": replaceOnce(
          shippingSources["friending-visibility.ts"],
          "return FRIENDING_VISIBILITY_SWITCHES.reduce(\n    (next, key) => setFriendingVisibilitySwitch(next, key, enabled),\n    state,\n  );",
          "return { ...FRIENDING_VISIBILITY_SWITCHES.reduce(\n    (next, key) => setFriendingVisibilitySwitch(next, key, enabled),\n    state,\n  ), profileViewable: preset === \"SILENT\" ? true : enabled };",
          attack,
        ),
      };
    case "visible-leaves-profileViewable-off":
      return {
        ...shippingSources,
        "friending-visibility.ts": replaceOnce(
          shippingSources["friending-visibility.ts"],
          "return FRIENDING_VISIBILITY_SWITCHES.reduce(\n    (next, key) => setFriendingVisibilitySwitch(next, key, enabled),\n    state,\n  );",
          "return { ...FRIENDING_VISIBILITY_SWITCHES.reduce(\n    (next, key) => setFriendingVisibilitySwitch(next, key, enabled),\n    state,\n  ), profileViewable: preset === \"VISIBLE\" ? false : enabled };",
          attack,
        ),
      };
    case "visible-chip-lit-at-1110":
      return {
        ...shippingSources,
        "friending-visibility.ts": replaceOnce(
          shippingSources["friending-visibility.ts"],
          "if (enabled === FRIENDING_VISIBILITY_SWITCHES.length) return \"VISIBLE\";",
          "if (enabled === FRIENDING_VISIBILITY_SWITCHES.length - 1) return \"VISIBLE\";",
          attack,
        ),
      };
    case "onboarding-only-store":
      return {
        ...shippingSources,
        "main.ts": replaceOnce(
          shippingSources["main.ts"],
          'if (onboardingRoute === "visibility") return friendingVisibilityMarkup(friendingVisibility, "onboarding");',
          'if (onboardingRoute === "visibility") return friendingVisibilityMarkup(loadFriendingVisibility(onboardingVisibilityStore), "onboarding");',
          attack,
        ),
      };
  }
}

function run6880(name: string, sources: PackageSources): { status: number | null; output: string; discarded: boolean } {
  const directory = mkdtempSync(join(process.cwd(), ".task-6881-"));
  let status: number | null = null;
  let output = "";
  try {
    mkdirSync(join(directory, "src"));
    for (const sourceFile of SOURCE_FILES) writeFileSync(join(directory, "src", sourceFile), sources[sourceFile]);
    writeFileSync(join(directory, "package.json"), '{"private":true,"type":"module"}\n');
    const vitest = join(process.cwd(), "node_modules", "vitest", "vitest.mjs");
    const result = spawnSync(process.execPath, [vitest, "run", "src/task_6880_friending_visibility.test.ts", "--reporter=verbose"], {
      cwd: directory,
      encoding: "utf8",
      env: { ...process.env, NO_COLOR: "1" },
    });
    status = result.status;
    output = `${result.stdout}${result.stderr}`;
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
  return { status, output, discarded: !existsSync(directory) };
}

function requireCase(enabled: boolean, caseName: string): void {
  expect(enabled, `TASK6881 absent case=${caseName}`).toBe(true);
}

describe("TASK 6881 friending visibility mutation proof", () => {
  it("runs the restored 6880 gate and records its restart and second-identity evidence", () => {
    const green = run6880("restored", shippingSources);
    expect(green.discarded, "TASK6881 restored package must be discarded").toBe(true);
    expect(green.status, "TASK6881 restored 6880 gate").toBe(0);
    requireCase(process.env.TASK6881_SKIP_RESTORATION !== "1", "restoration");
    requireCase(green.output.includes("restart=true"), "restart");
    requireCase(green.output.includes("second_identity_effect="), "second-running-identity");
    console.info(`TASK6881 restored_exit=${green.status} discarded=${green.discarded} restart=present second_identity=present`);
  });

  it("makes every detached, renamed, extra, missing, incomplete, wrongly-lit, and second-store package red", () => {
    const omitted = process.env.TASK6881_OMIT_MUTANT as Attack | undefined;
    const attacks = ATTACKS.filter((attack) => attack !== omitted);
    const observed: Attack[] = [];
    for (const attack of attacks) {
      const red = run6880(attack, mutate(attack));
      expect(red.discarded, `TASK6881 mutation=${attack} package must be discarded`).toBe(true);
      expect(red.status, `TASK6881 mutation=${attack} must make 6880 exit 1`).toBe(1);
      expect(red.output, `TASK6881 mutation=${attack} must name its failed 6880 guard`).toContain("TASK6880_");
      if (attack.startsWith("detach-")) {
        const row = attack.slice("detach-".length);
        expect(red.output).toContain(`switch=${row}`);
      }
      if (attack === "rename-profileViewable") expect(red.output).toContain("row=profile viewable switch=profileViewable");
      if (attack === "remove-messageRequestsAllowed") expect(red.output).toContain("row=message requests allowed switch=messageRequestsAllowed");
      if (attack === "silent-leaves-profileViewable-on") expect(red.output).toContain("preset=SILENT row=profileViewable switch=profileViewable");
      if (attack === "visible-leaves-profileViewable-off") expect(red.output).toContain("preset=VISIBLE row=profileViewable switch=profileViewable");
      if (attack === "visible-chip-lit-at-1110") expect(red.output).toContain("TASK6880_CHIP preset=VISIBLE checkboxes=1111");
      if (attack === "onboarding-only-store") expect(red.output).toContain("TASK6880_STORE surface=onboarding row=findable by public username switch=findableByPublicUsername");
      observed.push(attack);
      console.info(`TASK6881 mutation=${attack} exit=${red.status} discarded=${red.discarded}`);
    }
    for (const attack of ATTACKS) requireCase(observed.includes(attack), `mutant=${attack}`);
    console.info(`TASK6881 attacks=${observed.join(",")} count=${observed.length} packages_discarded=${observed.length}`);
  });
});
