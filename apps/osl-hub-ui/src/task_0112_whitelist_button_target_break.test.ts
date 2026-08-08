import { execFile, execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

// The module under test imports the Tauri invoke entrypoint; keep it inert so
// this test only ever talks to the real command harness below.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { discordQaWhitelistButtonMarkup } from "./discord-qa-whitelist-button";
import {
  connectDiscordQaWhitelistButton,
  discordQaOpenPlace,
  queryOpenPlaceAllowed,
  type AllowedPlaceCommandRunner,
  type DiscordQaOpenPlace,
} from "./discord-qa-whitelist-place";

// TASK 0112: break the whitelist button target. Seed allowed record
// CEDAR-0112, press the button for valid place PLACE-0112, then press a copy
// whose only changed field is its missing stable ID. The target must refuse
// the broken press as "stable ID required", the count must stay 2, and both
// saved records must stay byte-for-byte unchanged. Every add / allowed / read
// / count / dump here is the real allowed-place store
// (`crates/ipc/src/allowed_places.rs`) driven through the committed
// `task_0112_allowed_place_cli` example binary — the same target the TASK 0110
// button wiring proved.

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const targetDir = process.env.CARGO_TARGET_DIR ?? join(repoRoot, "target");
const harnessBin = join(targetDir, "debug", "examples", "task_0112_allowed_place_cli");
const homeCargo = join(homedir(), ".cargo", "bin", "cargo");
const cargoBin = process.env.CARGO ?? (existsSync(homeCargo) ? homeCargo : "cargo");
const BUILD_BUDGET_MS = 600_000;

let storeDir = "";
let lastRefusal = "";

function buildHarness(): Promise<void> {
  return new Promise((resolve, reject) => {
    execFile(
      cargoBin,
      ["build", "--locked", "--offline", "-p", "ipc", "--example", "task_0112_allowed_place_cli"],
      { cwd: repoRoot, encoding: "utf8", timeout: BUILD_BUDGET_MS },
      (error) => error ? reject(error) : resolve(),
    );
  });
}

function runHarness(args: string[]): unknown {
  const stdout = execFileSync(harnessBin, args, { encoding: "utf8" });
  return JSON.parse(stdout) as unknown;
}

const cliRunner: AllowedPlaceCommandRunner = async (command, place) => {
  const args = command === "remove"
    ? ["remove", "--store", storeDir, "--stable-id", place.stableId]
    : [
        command,
        "--store", storeDir,
        "--app", place.app,
        "--account", place.account,
        "--kind", place.kind,
        "--stable-id", place.stableId,
      ];
  try {
    return runHarness(args);
  } catch (error) {
    const stdout = (error as { stdout?: string }).stdout ?? "";
    lastRefusal = (JSON.parse(stdout) as { error?: string }).error ?? "";
    throw error;
  }
};

function readRecord(stableId: string): { stableId: string } {
  const raw = runHarness(["read", "--store", storeDir, "--stable-id", stableId]) as {
    ok: boolean;
    record: { stableId: string };
  };
  expect(raw.ok).toBe(true);
  return raw.record;
}

function countRecords(): number {
  const raw = runHarness(["count", "--store", storeDir]) as { ok: boolean; count: number };
  expect(raw.ok).toBe(true);
  return raw.count;
}

// The stored row's raw column bytes, for byte-for-byte comparison of a saved
// record across presses.
function dumpRecordBytes(stableId: string): Buffer {
  return execFileSync(harnessBin, ["dump", "--store", storeDir, "--stable-id", stableId]);
}

function storeFileBytes(): Buffer {
  return readFileSync(join(storeDir, "allowed_places.sqlite"));
}

class FakeButton {
  readonly dataset: { whitelistNext?: string } = {};
  private readonly listeners: Array<(event: { currentTarget: FakeButton }) => void> = [];

  addEventListener(_type: "click", listener: (event: { currentTarget: FakeButton }) => void): void {
    this.listeners.push(listener);
  }

  click(): void {
    for (const listener of this.listeners) listener({ currentTarget: this });
  }
}

function markupFor(scopeApproved: boolean): string {
  return discordQaWhitelistButtonMarkup({
    scopeApproved,
    protectionActive: true,
    verifiedPeer: true,
    busy: false,
  });
}

function whitelistNextFromMarkup(scopeApproved: boolean): string {
  const next = /data-whitelist-next="([^"]+)"/u.exec(markupFor(scopeApproved))?.[1];
  if (!next) throw new Error("whitelist button markup lost data-whitelist-next");
  return next;
}

function whitelistLabelFromMarkup(scopeApproved: boolean): string {
  const label = /class="discord-qa-whitelist-label">([^<]+)</u.exec(markupFor(scopeApproved))?.[1];
  if (!label) throw new Error("whitelist button markup lost its label");
  return label;
}

describe("TASK 0112 the whitelist button target refuses a missing stable ID", () => {
  beforeAll(async () => {
    await buildHarness();
    storeDir = mkdtempSync(join(tmpdir(), "task-0112-allowed-places-"));
  }, BUILD_BUDGET_MS);

  afterAll(() => {
    if (storeDir) rmSync(storeDir, { recursive: true, force: true });
  });

  it("valid press lands On list, missing-ID press is refused and changes nothing", async () => {
    // Seed allowed record CEDAR-0112 through the real add command.
    const cedarId = "discord:account-0112:direct_message:CEDAR-0112";
    const seeded = runHarness([
      "add",
      "--store", storeDir,
      "--app", "discord",
      "--account", "account-0112",
      "--kind", "direct_message",
      "--stable-id", cedarId,
    ]) as { ok: boolean };
    expect(seeded.ok).toBe(true);

    // CEDAR-0112 is readable and the count is 1 before.
    const cedar = readRecord(cedarId);
    expect(cedar.stableId).toBe(cedarId);
    const countBefore = countRecords();
    console.log(`TASK0112_SEED cedar_readable=${cedar.stableId} count_before=${countBefore}`);
    expect(countBefore).toBe(1);

    // Press the button for valid place PLACE-0112 through the real TASK 0110
    // wiring, with the action taken from the real TASK 0109 markup.
    const place: DiscordQaOpenPlace = discordQaOpenPlace({
      serviceId: "discord",
      accountId: "account-0112",
      personId: "PLACE-0112",
    });
    expect(place.stableId).toBe("discord:account-0112:direct_message:PLACE-0112");

    const validButton = new FakeButton();
    let landed: ((value: { command: "add" | "remove"; allowed: boolean }) => void) | null = null;
    const nextCommand = new Promise<{ command: "add" | "remove"; allowed: boolean }>((resolve) => {
      landed = resolve;
    });
    connectDiscordQaWhitelistButton(validButton, () => place, {
      run: cliRunner,
      onCommand: (command, allowed) => landed?.({ command, allowed }),
    });
    validButton.dataset.whitelistNext = whitelistNextFromMarkup(false);
    expect(validButton.dataset.whitelistNext).toBe("allow");
    validButton.click();
    const validPress = await nextCommand;
    expect(validPress.command).toBe("add");

    // The valid press shows On list for PLACE-0112 and makes the count 2.
    const placeAllowed = await queryOpenPlaceAllowed(place, cliRunner);
    expect(placeAllowed).toBe(true);
    const label = whitelistLabelFromMarkup(placeAllowed);
    expect(label).toBe("On list");
    const countAfterValid = countRecords();
    console.log(
      `TASK0112_VALID_PRESS stable_id=${place.stableId} allowed=${placeAllowed} label=${label} count=${countAfterValid}`,
    );
    expect(countAfterValid).toBe(2);

    // Snapshot both saved records byte for byte before the broken press.
    const cedarBytesBefore = dumpRecordBytes(cedarId);
    const placeBytesBefore = dumpRecordBytes(place.stableId);
    const storeBytesBefore = storeFileBytes();

    // Press a copy whose only changed field is its missing stable ID.
    const brokenCopy: DiscordQaOpenPlace = { ...place, stableId: "" };
    expect(brokenCopy).toEqual({ ...place, stableId: "" });
    expect(brokenCopy.app).toBe(place.app);
    expect(brokenCopy.account).toBe(place.account);
    expect(brokenCopy.kind).toBe(place.kind);
    expect(brokenCopy.stableId).toBe("");

    const brokenButton = new FakeButton();
    let refusedSettle: (() => void) | null = null;
    const refused = new Promise<void>((resolve) => {
      refusedSettle = resolve;
    });
    let brokenCommandLanded = false;
    connectDiscordQaWhitelistButton(brokenButton, () => brokenCopy, {
      run: cliRunner,
      // Settle on either outcome: a refusal is the pass path, a landed
      // command is the red path the assertions below catch.
      onCommand: () => {
        brokenCommandLanded = true;
        refusedSettle?.();
      },
      onError: () => refusedSettle?.(),
    });
    brokenButton.dataset.whitelistNext = whitelistNextFromMarkup(false);
    lastRefusal = "";
    brokenButton.click();
    await refused;

    // The missing-ID press is refused as stable ID required.
    expect(brokenCommandLanded).toBe(false);
    console.log(`TASK0112_BROKEN_PRESS refusal=${JSON.stringify(lastRefusal)}`);
    expect(lastRefusal).toBe("stable ID required");

    // The count stays 2.
    const countAfterBroken = countRecords();
    console.log(`TASK0112_COUNT_AFTER_BROKEN count=${countAfterBroken}`);
    expect(countAfterBroken).toBe(2);

    // Both saved records stay byte-for-byte unchanged.
    const cedarBytesAfter = dumpRecordBytes(cedarId);
    const placeBytesAfter = dumpRecordBytes(place.stableId);
    const storeBytesAfter = storeFileBytes();
    expect(cedarBytesAfter.equals(cedarBytesBefore)).toBe(true);
    expect(placeBytesAfter.equals(placeBytesBefore)).toBe(true);
    expect(storeBytesAfter.equals(storeBytesBefore)).toBe(true);
    console.log(
      `TASK0112_RECORDS_UNCHANGED cedar_bytes=${cedarBytesBefore.length}/${cedarBytesAfter.length} place_bytes=${placeBytesBefore.length}/${placeBytesAfter.length} store_bytes=${storeBytesBefore.length}/${storeBytesAfter.length}`,
    );

    console.log(
      `TASK0112_DONE count_sequence=${countBefore},${countAfterValid},${countAfterBroken} refusal="stable ID required" records_unchanged=true`,
    );
  });
});
