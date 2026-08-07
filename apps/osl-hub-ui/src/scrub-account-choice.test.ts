import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import fixture from "./scrub-account-choice-fixture.json";
import {
  SAVE_SCRUB_ACCOUNT_PERMISSIONS_COMMAND,
  SCRUB_ACCOUNT_CHOICE_BACK_STEP,
  SCRUB_ACCOUNT_CHOICE_NEXT_STEP,
  backFromScrubAccountChoice,
  canContinueFromScrubAccountChoice,
  continueFromScrubAccountChoice,
  initialScrubAccountChoiceState,
  scrubAccountChoiceMarkup,
  scrubAccountPermissionWrite,
  scrubSetupStepMarkup,
  toggleScrubAccountTick,
  type ScrubAccountChoiceInvoke,
  type ScrubAccountPermissionWrite,
} from "./scrub-account-choice";

const accounts = fixture.accounts;
const [first, second] = accounts;
const chosen = fixture.tickedAccountId;

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe("TASK 1402 setup account choice", () => {
  it("lists every account with a tick, a Back and a Continue, and starts with nothing ticked", () => {
    const state = initialScrubAccountChoiceState(accounts);
    const markup = scrubAccountChoiceMarkup(state);

    for (const account of accounts) {
      expect(markup).toContain(`data-account-id="${account.accountId}"`);
      expect(markup).toContain(account.accountLabel);
      expect(markup).toContain(account.appOrBrowserLabel);
    }
    expect(occurrences(markup, 'class="sr-only scrub-account-tick"')).toBe(accounts.length);
    expect(occurrences(markup, 'data-ticked="yes"')).toBe(0);
    expect(occurrences(markup, ">Back<")).toBe(1);
    expect(occurrences(markup, ">Continue<")).toBe(1);
    expect(markup).toContain('data-ticked-count="0"');
    // Continue cannot be pressed until an account is ticked.
    expect(markup).toContain('data-setup-continue="options" disabled aria-disabled="true"');
    expect(canContinueFromScrubAccountChoice(state)).toBe(false);
    expect(scrubSetupStepMarkup("accounts", state)).toBe(markup);
    expect(scrubSetupStepMarkup("intro", state)).toBe("");
    expect(backFromScrubAccountChoice()).toBe(SCRUB_ACCOUNT_CHOICE_BACK_STEP);
  });

  it("ticks one account without ticking the other, and puts only it in the write", () => {
    const state = toggleScrubAccountTick(initialScrubAccountChoiceState(accounts), chosen);
    const markup = scrubAccountChoiceMarkup(state);

    expect(state.tickedAccountIds).toEqual([chosen]);
    expect(occurrences(markup, 'data-ticked="yes"')).toBe(1);
    expect(markup).toContain(`data-account-id="${chosen}" data-service-id="${first.serviceId}" data-ticked="yes"`);
    expect(markup).toContain(`data-account-id="${second.accountId}" data-service-id="${second.serviceId}" data-ticked="no"`);
    expect(markup).toContain('data-ticked-count="1"');
    expect(markup).not.toContain("aria-disabled");

    expect(scrubAccountPermissionWrite(state)).toEqual({
      availableAccountIds: accounts.map((account) => account.accountId),
      selectedAccountIds: [chosen],
    });
  });

  it("refuses to write anything when no account is ticked", async () => {
    const calls: string[] = [];
    const invoke: ScrubAccountChoiceInvoke = async (command) => {
      calls.push(command);
      return { accountIds: [] };
    };

    const result = await continueFromScrubAccountChoice(
      initialScrubAccountChoiceState(accounts),
      invoke,
    );

    expect(result).toEqual({ outcome: "refused", step: "accounts", reason: "no-account-ticked" });
    expect(calls).toEqual([]);
  });

  it("sends exactly one save call naming only the ticked account", async () => {
    const calls: Array<{ command: string; payload: ScrubAccountPermissionWrite }> = [];
    const invoke: ScrubAccountChoiceInvoke = async (command, payload) => {
      calls.push({ command, payload });
      return { accountIds: [...payload.selectedAccountIds] };
    };

    const state = toggleScrubAccountTick(initialScrubAccountChoiceState(accounts), chosen);
    const result = await continueFromScrubAccountChoice(state, invoke);

    // eslint-disable-next-line no-console
    console.log(
      `TASK1402_CONTINUE ticked=${chosen} unticked=${second.accountId} write_calls=${calls.length} command=${calls[0]?.command} available_ids=${calls[0]?.payload.availableAccountIds.join(",")} selected_ids=${calls[0]?.payload.selectedAccountIds.join(",")} selected_count=${calls[0]?.payload.selectedAccountIds.length} unticked_written=${calls[0]?.payload.selectedAccountIds.includes(second.accountId)}`,
    );

    expect(calls).toHaveLength(1);
    expect(calls[0].command).toBe(SAVE_SCRUB_ACCOUNT_PERMISSIONS_COMMAND);
    expect(calls[0].payload.selectedAccountIds).toEqual([chosen]);
    expect(calls[0].payload.selectedAccountIds).not.toContain(second.accountId);
    expect(result).toEqual({
      outcome: "saved",
      step: SCRUB_ACCOUNT_CHOICE_NEXT_STEP,
      write: {
        availableAccountIds: accounts.map((account) => account.accountId),
        selectedAccountIds: [chosen],
      },
      savedAccountIds: [chosen],
    });
  });
});

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..", "..");

/**
 * The write has to land on the command that actually exists. These read the
 * Rust source rather than a copy of it, so renaming the command or a field of
 * `ScrubAccountPermissionInput` fails here instead of at runtime in a build
 * nobody reruns.
 */
describe("TASK 1402 the write matches the real Scrub permission command", () => {
  const preferences = readFileSync(
    path.join(REPO_ROOT, "apps/osl-hub/src/preferences.rs"),
    "utf8",
  );
  const commandSurface = readFileSync(
    path.join(REPO_ROOT, "apps/osl-hub/src/hub_command_surface.rs"),
    "utf8",
  );

  function camel(field: string): string {
    return field.replace(/_([a-z])/g, (_match, letter: string) => letter.toUpperCase());
  }

  it("sends every field of ScrubAccountPermissionInput and nothing else", () => {
    const struct = preferences.match(
      /pub struct ScrubAccountPermissionInput \{([^}]*)\}/,
    );
    expect(struct, "ScrubAccountPermissionInput must exist in preferences.rs").not.toBeNull();
    const fields = [...(struct as RegExpMatchArray)[1].matchAll(/pub ([a-z_]+):/g)]
      .map((match) => camel(match[1]))
      .sort();

    const state = toggleScrubAccountTick(initialScrubAccountChoiceState(accounts), chosen);
    expect(Object.keys(scrubAccountPermissionWrite(state)).sort()).toEqual(fields);
    expect(fields).toEqual(["availableAccountIds", "selectedAccountIds"]);
  });

  it("names a command the desktop build registers", () => {
    expect(commandSurface).toContain(`pub fn ${SAVE_SCRUB_ACCOUNT_PERMISSIONS_COMMAND}_command(`);
    expect(commandSurface).toMatch(
      new RegExp(`^\\s+${SAVE_SCRUB_ACCOUNT_PERMISSIONS_COMMAND},$`, "m"),
    );
  });
});

/**
 * The finish line, against the real command rather than a stand-in: Continue's
 * payload goes to `save_scrub_account_permissions` in a separate process, and
 * `get_scrub_account_permissions` reads the store back in a third one.
 *
 * Build the transport first:
 *   cargo build --manifest-path apps/osl-hub/Cargo.toml -p osl-hub \
 *     --no-default-features --features core --example task_1402_scrub_account_choice
 */
const BRIDGE = process.env.OSL_1402_BRIDGE_BIN
  ?? path.join(
    process.env.CARGO_TARGET_DIR ?? path.join(REPO_ROOT, "target"),
    "debug",
    "examples",
    "task_1402_scrub_account_choice",
  );

function bridge(args: string[]): { accountIds: string[]; stdout: string } {
  const stdout = execFileSync(BRIDGE, args, { encoding: "utf8" });
  const line = stdout.split("\n").find((entry) => entry.startsWith("TASK1402_JSON="));
  if (!line) throw new Error(`native permission command printed no result:\n${stdout}`);
  return { accountIds: JSON.parse(line.slice("TASK1402_JSON=".length)).accountIds, stdout };
}

describe.runIf(existsSync(BRIDGE))("TASK 1402 setup account choice writes the real permission", () => {
  it("writes only the ticked account through the native command", async () => {
    const directory = mkdtempSync(path.join(tmpdir(), "osl-1402-ui-"));
    const store = path.join(directory, "preferences.json");
    const owner = fixture.ownerUserId;
    try {
      const before = bridge(["read", store, owner]);
      expect(before.accountIds).toEqual([]);

      const invoke: ScrubAccountChoiceInvoke = async (command, payload) => {
        expect(command).toBe(SAVE_SCRUB_ACCOUNT_PERMISSIONS_COMMAND);
        return { accountIds: bridge(["save", store, owner, JSON.stringify(payload)]).accountIds };
      };

      const state = toggleScrubAccountTick(initialScrubAccountChoiceState(accounts), chosen);
      const result = await continueFromScrubAccountChoice(state, invoke);
      expect(result.outcome).toBe("saved");

      const after = bridge(["read", store, owner]);
      // eslint-disable-next-line no-console
      console.log(
        `TASK1402_UI ticked=${chosen} unticked=${second.accountId} before_count=${before.accountIds.length} saved_ids=${result.outcome === "saved" ? result.savedAccountIds.join(",") : ""} read_ids=${after.accountIds.join(",")} read_count=${after.accountIds.length} unticked_saved=${after.accountIds.includes(second.accountId)}`,
      );

      expect(after.accountIds).toEqual([chosen]);
      expect(after.accountIds).toHaveLength(1);
      expect(after.accountIds).not.toContain(second.accountId);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
});
