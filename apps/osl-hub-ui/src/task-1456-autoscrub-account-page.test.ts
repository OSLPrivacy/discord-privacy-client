import { describe, expect, it } from "vitest";
import {
  autoScrubAccountPageInitialState,
  autoScrubAccountPageLoad,
  autoScrubAccountPagePresentCode,
  autoScrubAccountPageToggleSwitch,
  autoScrubAccountPageMarkup,
  autoScrubProExplanationMarkup,
  autoScrubUnlockProMarkup,
  autoScrubEnterProCodeFormMarkup,
  type AutoScrubAccountCommandReply,
} from "./autoscrub-account-page";

/**
 * TASK 1456: connect the AutoScrub account page.
 *
 * The fake below mirrors the exact contract
 * `crates/ipc/src/autoscrub_account_switches.rs` documents and Task 1455
 * tests against: three fixture accounts, a Pro gate that starts locked, and a
 * consent set naming which accounts normal Scrub approved. Driving the page
 * state functions through it proves the wiring -- not just the markup --
 * reaches the same fail-closed backend.
 */

const ACTIVE_PRO_CODE = "OSL-ACTV-ACTV-ACTV-ACTV";
const ACCOUNTS = [
  { accountId: "discord-maple", label: "Discord app" },
  { accountId: "discord-pine", label: "Discord app" },
  { accountId: "telegram-pine", label: "Telegram app" },
];

function fakeBackend(approved: string[]) {
  let unlocked = false;
  const approvedSet = new Set(approved);
  const records: { accountId: string; label: string; on: boolean }[] = [];

  const switchesFor = () =>
    ACCOUNTS.map((account) => {
      const record = records.find((r) => r.accountId === account.accountId);
      if (!unlocked) {
        return {
          accountId: account.accountId,
          label: account.label,
          available: false,
          on: Boolean(record),
          refusal: {
            command: "autoscrub_account_switch",
            accountId: account.accountId,
            reason: "pro_code_required",
            message: "AutoScrub account switches need an active Pro code.",
          },
        };
      }
      if (!approvedSet.has(account.accountId)) {
        return {
          accountId: account.accountId,
          label: account.label,
          available: false,
          on: Boolean(record),
          refusal: {
            command: "autoscrub_account_switch",
            accountId: account.accountId,
            reason: "account_not_approved_for_autoscrub",
            message: `${account.accountId} is not approved for AutoScrub.`,
          },
        };
      }
      return { accountId: account.accountId, label: account.label, available: true, on: Boolean(record) };
    });

  const nativeInvoke = (async (command: string, args: Record<string, unknown>) => {
    if (command === "autoscrub_account_switches") {
      return { ok: true, command, result: { switches: switchesFor(), recordCount: records.length } };
    }
    if (command === "autoscrub_account_switch") {
      const request = (args as { request: { accountId: string; on: boolean } }).request;
      const row = switchesFor().find((r) => r.accountId === request.accountId);
      if (!row?.available) {
        return {
          ok: false,
          command,
          accountId: request.accountId,
          errorCode: row?.refusal?.reason ?? "unknown_account",
          error: row?.refusal?.message ?? "unknown account",
        };
      }
      const account = ACCOUNTS.find((a) => a.accountId === request.accountId)!;
      const existingIndex = records.findIndex((r) => r.accountId === request.accountId);
      if (request.on) {
        if (existingIndex === -1) records.push({ accountId: account.accountId, label: account.label, on: true });
      } else if (existingIndex !== -1) {
        records.splice(existingIndex, 1);
      }
      return {
        ok: true,
        command,
        result: { accountId: request.accountId, on: request.on, records: [...records], recordCount: records.length },
      };
    }
    if (command === "autoscrub_present_pro_code") {
      const request = (args as { request: { code: string } }).request;
      if (request.code === ACTIVE_PRO_CODE) unlocked = true;
      return {
        ok: true,
        command,
        result: { verdict: unlocked ? "active" : "unknown", unlocked, switches: switchesFor(), recordCount: records.length },
      };
    }
    throw new Error(`unexpected command ${command}`);
  }) as unknown as Parameters<typeof autoScrubAccountPageLoad>[1];

  return { nativeInvoke, records };
}

describe("TASK1456 connect the AutoScrub account page", () => {
  it("shows the Pro explanation, Unlock Pro action, and Enter Pro code form while locked", () => {
    expect(autoScrubProExplanationMarkup(false)).toContain("AutoScrub");
    expect(autoScrubUnlockProMarkup(false)).toContain('id="autoscrub-unlock-pro"');
    expect(autoScrubUnlockProMarkup(false)).toContain("Unlock Pro");
    expect(autoScrubEnterProCodeFormMarkup()).toContain('id="autoscrub-pro-code-form"');
    const page = autoScrubAccountPageMarkup({ switches: [], recordCount: 0 }, false);
    expect(page).toContain('id="autoscrub-unlock-pro"');
    expect(page).toContain('id="autoscrub-pro-code-form"');
  });

  it("hides the Unlock Pro action and the code form once Pro is unlocked", () => {
    const page = autoScrubAccountPageMarkup({ switches: [], recordCount: 0 }, true);
    expect(page).not.toContain('id="autoscrub-unlock-pro"');
    expect(page).not.toContain('id="autoscrub-pro-code-form"');
  });

  it("entering the Pro code unlocks the page state and reveals the approved account switch", async () => {
    const { nativeInvoke } = fakeBackend(["discord-maple"]);
    let state = autoScrubAccountPageInitialState();
    state = await autoScrubAccountPageLoad(state, nativeInvoke);
    expect(state.unlocked).toBe(false);
    expect(state.listing.recordCount).toBe(0);

    state = await autoScrubAccountPagePresentCode(state, ACTIVE_PRO_CODE, nativeInvoke);
    expect(state.unlocked).toBe(true);
    const row = state.listing.switches.find((r) => r.accountId === "discord-maple");
    expect(row?.available).toBe(true);
    expect(row?.on).toBe(false);
  });

  it("turning the switch on saves exactly 1 AutoScrub record naming that approved account", async () => {
    const { nativeInvoke, records } = fakeBackend(["discord-maple"]);
    let state = autoScrubAccountPageInitialState();
    state = await autoScrubAccountPageLoad(state, nativeInvoke);
    state = await autoScrubAccountPagePresentCode(state, ACTIVE_PRO_CODE, nativeInvoke);

    state = await autoScrubAccountPageToggleSwitch(state, "discord-maple", true, nativeInvoke);

    expect(state.listing.recordCount).toBe(1);
    expect(records).toHaveLength(1);
    expect(records[0].accountId).toBe("discord-maple");
    const row = state.listing.switches.find((r) => r.accountId === "discord-maple");
    expect(row?.on).toBe(true);
  });

  it("turning the switch off leaves 0 AutoScrub records", async () => {
    const { nativeInvoke, records } = fakeBackend(["discord-maple"]);
    let state = autoScrubAccountPageInitialState();
    state = await autoScrubAccountPageLoad(state, nativeInvoke);
    state = await autoScrubAccountPagePresentCode(state, ACTIVE_PRO_CODE, nativeInvoke);
    state = await autoScrubAccountPageToggleSwitch(state, "discord-maple", true, nativeInvoke);
    expect(state.listing.recordCount).toBe(1);

    state = await autoScrubAccountPageToggleSwitch(state, "discord-maple", false, nativeInvoke);

    expect(state.listing.recordCount).toBe(0);
    expect(records).toHaveLength(0);
    const row = state.listing.switches.find((r) => r.accountId === "discord-maple");
    expect(row?.on).toBe(false);
  });

  it("a sibling account that was never approved is refused and never adds a record", async () => {
    const { nativeInvoke, records } = fakeBackend(["discord-maple"]);
    let state = autoScrubAccountPageInitialState();
    state = await autoScrubAccountPageLoad(state, nativeInvoke);
    state = await autoScrubAccountPagePresentCode(state, ACTIVE_PRO_CODE, nativeInvoke);

    const next = await autoScrubAccountPageToggleSwitch(state, "discord-pine", true, nativeInvoke);

    expect(next.error).toBeDefined();
    expect(next.listing.recordCount).toBe(state.listing.recordCount);
    expect(records).toHaveLength(0);
  });
});
