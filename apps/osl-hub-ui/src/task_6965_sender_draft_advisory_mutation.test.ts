import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { describe, expect, it } from "vitest";

const uiRoot = process.cwd();
const worktreeRoot = join(uiRoot, "..", "..");
const sourceRoot = join(uiRoot, "src");
const gate = "src/task_6964_sender_draft_advisory.test.ts";
const ADVISORY = "Advisory: this is your device checking your own draft. A modified client would not run this check, and nothing prevents the message from arriving.";
const FORBIDDEN_PROMISE_WORDS = ["block", "remove", "owner enforcement", "enforcement"] as const;

const ATTACKS = [
  "warning-refuses-send",
  "advisory-sentence-removed",
  ...FORBIDDEN_PROMISE_WORDS.map((word) => `promise-word-${word.replaceAll(" ", "-")}`),
  "notify-enclave-owner",
  "report-match-to-relay",
  "edit-loses-draft",
] as const;

type Attack = typeof ATTACKS[number];

interface Run {
  readonly status: number | null;
  readonly output: string;
  readonly discarded: boolean;
}

function replaceOnce(source: string, from: string, to: string, attack: Attack): string {
  const count = source.split(from).length - 1;
  if (count !== 1) throw new Error(`TASK6965 mutation=${attack} expected_one_anchor found=${count}`);
  return source.replace(from, to);
}

function mutate(directory: string, attack: Attack): void {
  const main = join(directory, "src/main.ts");
  const catalogue = join(directory, "src/catalogue/en.ts");
  if (attack === "warning-refuses-send") {
    writeFileSync(main, replaceOnce(readFileSync(main, "utf8"), "if (!sendAnyway && advisory) {", "if (advisory) {", attack));
    return;
  }
  if (attack === "advisory-sentence-removed") {
    writeFileSync(catalogue, replaceOnce(readFileSync(catalogue, "utf8"), ADVISORY, "Advisory.", attack));
    return;
  }
  if (attack.startsWith("promise-word-")) {
    const word = attack.slice("promise-word-".length).replaceAll("-", " ");
    writeFileSync(catalogue, replaceOnce(readFileSync(catalogue, "utf8"), ADVISORY, `${ADVISORY} ${word}.`, attack));
    return;
  }
  if (attack === "notify-enclave-owner") {
    writeFileSync(main, replaceOnce(
      readFileSync(main, "utf8"),
      "senderDraftAdvisory = advisory;\n    render();\n    return;",
      "senderDraftAdvisory = advisory;\n    void emitTo(\"enclave-owner-6965\", \"sender-draft-advisory-fired\", { matchedRule: advisory.matchedRule });\n    render();\n    return;",
      attack,
    ));
    return;
  }
  if (attack === "report-match-to-relay") {
    writeFileSync(main, replaceOnce(
      readFileSync(main, "utf8"),
      "senderDraftAdvisory = advisory;\n    render();\n    return;",
      "senderDraftAdvisory = advisory;\n    void invoke(\"report_sender_draft_match\", { matchedRule: advisory.matchedRule });\n    render();\n    return;",
      attack,
    ));
    return;
  }
  writeFileSync(main, replaceOnce(
    readFileSync(main, "utf8"),
    "function editSenderDraft(): void {\n  senderDraftAdvisory = null;\n  render();",
    "function editSenderDraft(): void {\n  senderDraftAdvisory = null;\n  setOslChatDraft(\"\");\n  render();",
    attack,
  ));
}

function run6964(name: string, attack?: Attack): Run {
  const directory = mkdtempSync(join(tmpdir(), `osl-task-6965-${name}-`));
  let status: number | null = null;
  let output = "";
  try {
    const packageRoot = join(directory, "apps", "osl-hub-ui");
    mkdirSync(packageRoot, { recursive: true });
    cpSync(sourceRoot, join(packageRoot, "src"), { recursive: true });
    writeFileSync(join(packageRoot, "package.json"), '{"private":true,"type":"module"}\n');
    symlinkSync(join(uiRoot, "node_modules"), join(packageRoot, "node_modules"), "dir");
    symlinkSync(join(worktreeRoot, "crates"), join(directory, "crates"), "dir");
    if (attack) mutate(packageRoot, attack);
    const vitest = join(uiRoot, "node_modules/vitest/vitest.mjs");
    const result = spawnSync(process.execPath, [vitest, "run", gate, "--reporter=verbose", "--pool=threads", "--poolOptions.threads.singleThread=true"], {
      cwd: packageRoot,
      encoding: "utf8",
      env: { ...process.env, NO_COLOR: "1" },
    });
    status = result.status;
    output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
  return { status, output, discarded: !existsSync(directory) };
}

function required(present: boolean, name: string): void {
  expect(present, `TASK6965 absent case=${name}`).toBe(true);
}

function expectedSurface(attack: Attack): string {
  if (attack === "warning-refuses-send") return "exact original draft: porn stays verbatim";
  if (attack === "advisory-sentence-removed") return "TASK6964_ADVISORY sentence";
  if (attack.startsWith("promise-word-")) return attack.slice("promise-word-".length).replaceAll("-", " ");
  if (attack === "notify-enclave-owner") return "TASK6964_OWNER_NOTIFICATION surface=enclave-owner";
  if (attack === "report-match-to-relay") return "TASK6964_RELAY_REPORT surface=relay";
  return "exact original draft: porn stays verbatim";
}

function assertCoverage(): void {
  const omitted = process.env.TASK6965_OMIT_MUTANT as Attack | undefined;
  for (const attack of ATTACKS) required(attack !== omitted, `mutant=${attack}`);
  required(process.env.TASK6965_SKIP_SECOND_IDENTITY !== "1", "second_identity=receiver-6964");
  required(process.env.TASK6965_SKIP_FORBIDDEN_LIST !== "1", "forbidden_list");
  required(process.env.TASK6965_SKIP_RESTORATION !== "1", "restoration");
}

describe("TASK 6965 sender advisory mutation proof", () => {
  it("runs restored 6964 for the second identity and discards the copy", () => {
    assertCoverage();
    if (process.env.TASK6965_ATTACK) return;
    const green = run6964("restored");
    expect(green.discarded, "TASK6965 restored copy discarded").toBe(true);
    expect(green.status, "TASK6965 restored 6964 gate").toBe(0);
    expect(green.output, "TASK6965 second identity").toContain("second_identity=receiver-6964");
    expect(green.output, "TASK6965 forbidden list").toContain("promise_words=0");
    console.info(`TASK6965 restored_exit=${green.status} discarded=${green.discarded} second_identity=receiver-6964 forbidden_words=${FORBIDDEN_PROMISE_WORDS.length}`);
  }, 180_000);

  it("makes every blocking, wording, notification, report, and draft mutation make 6964 red", () => {
    assertCoverage();
    const selected = process.env.TASK6965_ATTACK as Attack | undefined;
    if (selected) required(ATTACKS.includes(selected), `mutant=${selected}`);
    const attacks = selected ? [selected] : ATTACKS;
    const observed: Attack[] = [];
    for (const attack of attacks) {
      const red = run6964(attack, attack);
      expect(red.discarded, `TASK6965 mutation=${attack} copy discarded`).toBe(true);
      expect(red.status, `TASK6965 mutation=${attack} must make 6964 exit 1`).toBe(1);
      expect(red.output, `TASK6965 mutation=${attack} names word or surface`).toContain(expectedSurface(attack));
      observed.push(attack);
      console.info(`TASK6965 mutation=${attack} exit=${red.status} discarded=${red.discarded} named=${expectedSurface(attack)}`);
    }
    for (const attack of attacks) required(observed.includes(attack), `mutant=${attack}`);
    console.info(`TASK6965 mutations=${observed.length} copies_discarded=${observed.length}`);
  }, 180_000);

  it("fails closed naming any absent mutant, second identity, forbidden list, or restoration", () => {
    assertCoverage();
    console.info(`TASK6965_COVERAGE mutants=${ATTACKS.length} second_identity=receiver-6964 forbidden_list=present restoration=present`);
  });
});
