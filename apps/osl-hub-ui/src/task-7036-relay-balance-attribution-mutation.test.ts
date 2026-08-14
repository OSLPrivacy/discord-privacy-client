import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const uiRoot = process.cwd();
const worktreeRoot = join(uiRoot, "..", "..");
const gate = "src/task-7035-device-voucher-balance.test.ts";

const ATTACKS = [
  "account-id-in-store",
  "email-hash-beside-grant",
  "stable-device-balance-handle",
  "keyserver-served-balance",
  "restore-my-balance-endpoint",
  "shared-issue-order-join",
] as const;

type Attack = typeof ATTACKS[number];

interface GateRun {
  readonly status: number | null;
  readonly output: string;
  readonly discarded: boolean;
}

function append(directory: string, relativePath: string, text: string): void {
  const path = join(directory, relativePath);
  // The isolated packages contain only copied source files, so appending cannot
  // alter the restored worktree and yields a single-purpose malicious surface.
  writeFileSync(path, `${readFileSync(path, "utf8")}\n${text}\n`);
}

function mutate(directory: string, attack: Attack): void {
  if (attack === "account-id-in-store") {
    append(directory, "cipher-store-cf/src/lib/voucher-balance-grant.ts", [
      "export const deployedBalanceState = Object.freeze({",
      '  accountId: "alice-account-7036",',
      "  balance: { stored: 89, moved: 103 },",
      "});",
    ].join("\n"));
    return;
  }
  if (attack === "email-hash-beside-grant") {
    append(directory, "keyserver-cf/src/lib/voucher-balance-grant.ts", [
      "export const relayGrantWithEmailHash = Object.freeze({",
      '  aud: "osl-capacity-voucher", exp: 2_000_000_000, jti: "emailHashVoucher_0001",',
      '  emailHash: "sha256:alice-7036",',
      "});",
    ].join("\n"));
    return;
  }
  if (attack === "stable-device-balance-handle") {
    append(directory, "services/voucher-redemption/src/voucher_balance_grant.rs", [
      "pub struct RelayIndexedBalance {",
      "    pub deviceBalanceHandle: String,",
      "    pub redemption_jti: String,",
      "}",
    ].join("\n"));
    return;
  }
  if (attack === "keyserver-served-balance") {
    append(directory, "apps/osl-hub-ui/src/device-voucher-balance.ts", [
      "export async function keyserverServedBalance(): Promise<VoucherBalance> {",
      '  return fetch("/v1/voucher-balance").then((response) => response.json() as Promise<VoucherBalance>);',
      "}",
    ].join("\n"));
    return;
  }
  if (attack === "restore-my-balance-endpoint") {
    append(directory, "keyserver-cf/src/lib/voucher-balance-grant.ts", [
      "export function restoreMyBalance(accountId: string): RelayVoucherGrant {",
      "  throw new Error(`restore-my-balance for ${accountId}`);",
      "}",
    ].join("\n"));
    append(directory, "apps/osl-hub-ui/src/device-voucher-balance.ts", [
      "export async function restoreMyBalance(): Promise<void> {",
      '  await fetch("/v1/restore-my-balance");',
      "}",
    ].join("\n"));
    return;
  }
  append(directory, "keyserver-cf/src/lib/voucher-balance-grant.ts", [
    "export const redemptionsJoinedByIssueOrder = Object.freeze({",
    '  issueOrder: "shared-issue-order-7036",',
    '  firstJti: "issueOrderVoucher_0001", secondJti: "issueOrderVoucher_0002",',
    "});",
  ].join("\n"));
}

function prepare(directory: string): string {
  const packageRoot = join(directory, "apps", "osl-hub-ui");
  mkdirSync(packageRoot, { recursive: true });
  cpSync(join(uiRoot, "src"), join(packageRoot, "src"), { recursive: true });
  const relaySources = [
    "keyserver-cf/src/lib/voucher-balance-grant.ts",
    "services/voucher-redemption/src/voucher_balance_grant.rs",
    "cipher-store-cf/src/lib/voucher-balance-grant.ts",
  ] as const;
  for (const relativePath of relaySources) {
    const target = join(directory, relativePath);
    mkdirSync(join(target, ".."), { recursive: true });
    cpSync(join(worktreeRoot, relativePath), target);
  }
  writeFileSync(join(packageRoot, "package.json"), '{"private":true,"type":"module"}\n');
  symlinkSync(join(uiRoot, "node_modules"), join(packageRoot, "node_modules"), "dir");
  return packageRoot;
}

function run7035(name: string, attack?: Attack): GateRun {
  const directory = mkdtempSync(join(tmpdir(), `osl-task-7036-${name}-`));
  let status: number | null = null;
  let output = "";
  try {
    const packageRoot = prepare(directory);
    if (attack) mutate(directory, attack);
    const vitest = join(uiRoot, "node_modules", "vitest", "vitest.mjs");
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

function required(present: boolean, caseName: string): void {
  expect(present, `TASK7036 absent case=${caseName}`).toBe(true);
}

function assertCoverage(): void {
  const omitted = process.env.TASK7036_OMIT_MUTANT as Attack | undefined;
  for (const attack of ATTACKS) required(attack !== omitted, `mutant=${attack}`);
  required(process.env.TASK7036_SKIP_RELAY_OBSERVER !== "1", "relay_observer");
  required(process.env.TASK7036_SKIP_DEVICE_A !== "1", "device_a");
  required(process.env.TASK7036_SKIP_DEVICE_B !== "1", "device_b");
  required(process.env.TASK7036_SKIP_STORED_METER !== "1", "stored_meter");
  required(process.env.TASK7036_SKIP_MOVED_METER !== "1", "moved_meter");
  required(process.env.TASK7036_SKIP_RESTORATION !== "1", "restoration");
}

function expectedObservation(attack: Attack): string {
  if (attack === "account-id-in-store") return "surface=deployed_store field=accountId attribution=person:account-holder";
  if (attack === "email-hash-beside-grant") return "surface=keyserver field=emailHash attribution=person:email-hash-holder";
  if (attack === "stable-device-balance-handle") return "surface=mixing_queue field=deviceBalanceHandle attribution=device:stable-balance-handle";
  if (attack === "keyserver-served-balance") return "surface=client field=keyserver-served-balance attribution=person:keyserver-balance-requester";
  if (attack === "restore-my-balance-endpoint") return "surface=keyserver field=restore-my-balance attribution=person:balance-restoration-requester";
  return "surface=keyserver field=issueOrder attribution=balance:two-redemptions-joined";
}

function expectedNetworkObservation(attack: Attack): string | undefined {
  if (attack === "keyserver-served-balance") return "surface=client field=network-request=fetch:/v1/voucher-balance";
  if (attack === "restore-my-balance-endpoint") return "surface=client field=network-request=fetch:/v1/restore-my-balance";
  return undefined;
}

describe("TASK 7036 — relay balance attribution mutation proof", () => {
  it("makes each person, device, remote-balance, restoration, and redemption-join leak turn 7035 red", () => {
    assertCoverage();
    const selected = process.env.TASK7036_ATTACK as Attack | undefined;
    if (selected) required(ATTACKS.includes(selected), `mutant=${selected}`);
    const attacks = selected ? [selected] : ATTACKS;
    const observed: Attack[] = [];
    for (const attack of attacks) {
      const red = run7035(attack, attack);
      expect(red.discarded, `TASK7036 mutation=${attack} copy discarded`).toBe(true);
      expect(red.status, `TASK7036 mutation=${attack} must make 7035 exit 1`).toBe(1);
      expect(red.output, `TASK7036 mutation=${attack} names leaked field and surface`).toContain(expectedObservation(attack));
      const network = expectedNetworkObservation(attack);
      if (network) expect(red.output, `TASK7036 mutation=${attack} names needless network request`).toContain(network);
      observed.push(attack);
      console.info(`TASK7036 mutation=${attack} exit=${red.status} discarded=${red.discarded} observer=${expectedObservation(attack)}${network ? ` ${network}` : ""}`);
    }
    for (const attack of attacks) required(observed.includes(attack), `mutant=${attack}`);
    console.info(`TASK7036 mutations=${observed.length} copies_discarded=${observed.length}`);
  }, 180_000);

  it("runs 7035 against the restored build after every throwaway mutation copy is discarded", () => {
    assertCoverage();
    const green = run7035("restored");
    expect(green.discarded, "TASK7036 restored copy discarded").toBe(true);
    expect(green.status, "TASK7036 restored 7035 gate").toBe(0);
    expect(green.output).toContain("TASK7035 RELAY_OBSERVER person_attributions=0 device_attributions=0 joined_balances=0");
    expect(green.output).toContain("TASK7035 wallets=2 local_a_stored=59 local_a_moved=40 local_b_stored=30 local_b_moved=63 cross_wallet_alterations=0");
    expect(green.output).toContain("TASK7035 redemption=2 spends=2 restart=1 secure_local_persistence=1 stored=89 moved=103 independent_stored=89 independent_moved=103 network_requests=0");
    console.info(`TASK7036 restored_exit=${green.status} discarded=${green.discarded} person_attributions=0 device_attributions=0 offline_stored=89 offline_moved=103 devices=2`);
  }, 180_000);

  it("fails closed when any mutant, observer, device, meter, or restoration case is starved", () => {
    assertCoverage();
    console.info(`TASK7036_COVERAGE mutants=${ATTACKS.length} relay_observer=present device_a=present device_b=present stored_meter=present moved_meter=present restoration=present`);
  });
});
