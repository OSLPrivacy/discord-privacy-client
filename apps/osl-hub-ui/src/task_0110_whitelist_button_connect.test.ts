import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
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
  parseAllowedPlaceAllowed,
  queryOpenPlaceAllowed,
  type AllowedPlaceCommand,
  type AllowedPlaceCommandRunner,
  type DiscordQaOpenPlace,
} from "./discord-qa-whitelist-place";

// TASK 0110: two direct button actions must change the allowed query from
// false to true to false. The add/remove/allowed commands here are the real
// allowed-place command functions (`crates/ipc/src/allowed_places.rs`), driven
// through the committed `task_0110_allowed_place_cli` example binary against a
// real durable store — not a stand-in that would pass with the wiring absent.

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const targetDir = process.env.CARGO_TARGET_DIR ?? join(repoRoot, "target");
const harnessBin = join(targetDir, "debug", "examples", "task_0110_allowed_place_cli");
const homeCargo = join(homedir(), ".cargo", "bin", "cargo");
const cargoBin = process.env.CARGO ?? (existsSync(homeCargo) ? homeCargo : "cargo");
const BUILD_BUDGET_MS = 600_000;

let storeDir = "";
const commandLog: Array<{ command: AllowedPlaceCommand; stableId: string }> = [];

const cliRunner: AllowedPlaceCommandRunner = async (command, place) => {
  commandLog.push({ command, stableId: place.stableId });
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
  const stdout = execFileSync(harnessBin, args, { encoding: "utf8" });
  return JSON.parse(stdout) as unknown;
};

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

function whitelistNextFromMarkup(scopeApproved: boolean): string {
  const markup = discordQaWhitelistButtonMarkup({
    scopeApproved,
    protectionActive: true,
    verifiedPeer: true,
    busy: false,
  });
  const next = /data-whitelist-next="([^"]+)"/u.exec(markup)?.[1];
  if (!next) throw new Error("whitelist button markup lost data-whitelist-next");
  return next;
}

describe("TASK 0110 single-place whitelist button connects to the allowed-place commands", () => {
  beforeAll(() => {
    execFileSync(
      cargoBin,
      ["build", "--offline", "-p", "ipc", "--example", "task_0110_allowed_place_cli"],
      { cwd: repoRoot, encoding: "utf8", stdio: "pipe", timeout: BUILD_BUDGET_MS },
    );
    storeDir = mkdtempSync(join(tmpdir(), "task-0110-allowed-places-"));
  }, BUILD_BUDGET_MS);

  afterAll(() => {
    if (storeDir) rmSync(storeDir, { recursive: true, force: true });
  });

  it("two direct button actions flip the allowed query false -> true -> false", async () => {
    const place: DiscordQaOpenPlace = discordQaOpenPlace({
      serviceId: "discord",
      accountId: "account-0110",
      personId: "peer-0110",
    });
    expect(place.stableId).toBe("discord:account-0110:direct_message:peer-0110");

    const button = new FakeButton();
    let scopeApproved = false;
    let settle: ((value: { command: "add" | "remove"; allowed: boolean }) => void) | null = null;
    const nextCommand = () =>
      new Promise<{ command: "add" | "remove"; allowed: boolean }>((resolve) => {
        settle = resolve;
      });
    connectDiscordQaWhitelistButton(button, () => place, {
      run: cliRunner,
      onCommand: (command, allowed) => {
        scopeApproved = allowed;
        settle?.({ command, allowed });
      },
    });

    // Open place starts off the list.
    const before = await queryOpenPlaceAllowed(place, cliRunner);
    console.log(`TASK0110_ALLOWED stage=before allowed=${before}`);
    expect(before).toBe(false);

    // First direct button action: the off-list button (data-whitelist-next
    // comes from the real TASK 0109 markup) dispatches the add command.
    button.dataset.whitelistNext = whitelistNextFromMarkup(scopeApproved);
    expect(button.dataset.whitelistNext).toBe("allow");
    let landed = nextCommand();
    button.click();
    const first = await landed;
    expect(first.command).toBe("add");
    const afterAdd = await queryOpenPlaceAllowed(place, cliRunner);
    console.log(`TASK0110_BUTTON_ACTION n=1 next=allow command=${first.command}`);
    console.log(`TASK0110_ALLOWED stage=after_first_click allowed=${afterAdd}`);
    expect(afterAdd).toBe(true);

    // Second direct button action: the button re-rendered on-list, so the same
    // click path dispatches the remove command.
    button.dataset.whitelistNext = whitelistNextFromMarkup(scopeApproved);
    expect(button.dataset.whitelistNext).toBe("revoke");
    landed = nextCommand();
    button.click();
    const second = await landed;
    expect(second.command).toBe("remove");
    const afterRemove = await queryOpenPlaceAllowed(place, cliRunner);
    console.log(`TASK0110_BUTTON_ACTION n=2 next=revoke command=${second.command}`);
    console.log(`TASK0110_ALLOWED stage=after_second_click allowed=${afterRemove}`);
    expect(afterRemove).toBe(false);

    const buttonCommands = commandLog.filter((entry) => entry.command !== "allowed");
    expect(buttonCommands).toEqual([
      { command: "add", stableId: place.stableId },
      { command: "remove", stableId: place.stableId },
    ]);
    console.log(
      `TASK0110_DONE allowed_sequence=${before},${afterAdd},${afterRemove} button_actions=${buttonCommands.length} stable_id=${place.stableId}`,
    );
  });

  it("fails closed when the allowed answer is malformed", () => {
    expect(parseAllowedPlaceAllowed(true)).toBe(true);
    expect(parseAllowedPlaceAllowed({ allowed: true })).toBe(true);
    expect(parseAllowedPlaceAllowed({ allowed: "yes" })).toBe(false);
    expect(parseAllowedPlaceAllowed(null)).toBe(false);
  });
});
