import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  connectedAppsPhrase,
  expectedSafeStep,
  homeProtectionSummaryFacts,
  homeProtectionSummaryMarkup,
  homeProtectionSummarySentence,
  homeSafeStepAction,
  homeSummaryFactErrors,
  trustedPeoplePhrase,
  type HomeProtectionSummary,
} from "./home-protection-summary";

/**
 * TASK 0821 - the rules behind the Home summary screen.
 *
 * The fixtures loaded here are the direct output of
 * `cmd_osl_home_protection_summary`; see
 * crates/ipc/tests/task_0821_home_summary_action_route.rs, which fails if either
 * file stops being what the backend returns.
 */

const FIXTURE_DIR = path.resolve(import.meta.dirname, "..", "screenshots", "fixtures");

function fixture(name: string): HomeProtectionSummary {
  return JSON.parse(readFileSync(path.join(FIXTURE_DIR, name), "utf8")) as HomeProtectionSummary;
}

const direct = fixture("task-0821-home-summary-direct.json");
const empty = fixture("task-0821-home-summary-empty.json");

describe("the action route comes from the direct summary", () => {
  it("routes the ready account to a protected conversation", () => {
    expect(direct.next_safe_step).toBe("Open a protected conversation");
    const action = homeSafeStepAction(direct);
    expect(action.step).toBe(direct.next_safe_step);
    expect(action.route).toBe("osl-chat");
    expect(action.homeModule).toBe("osl-chats");
    expect(action.label).toBe("Open OSL Chat");
  });

  it("routes an account with nothing saved to finishing protection", () => {
    expect(empty.next_safe_step).toBe("Finish account protection");
    const action = homeSafeStepAction(empty);
    expect(action.step).toBe(empty.next_safe_step);
    expect(action.route).toBe("settings");
    expect(action.settingsSection).toBe("account");
  });

  it("covers every step the backend can return", () => {
    const steps = [
      { ...empty, protection_state: "needs-attention", next_safe_step: "Finish account protection" },
      {
        ...empty,
        protection_state: "protected",
        connected_app_count: 0,
        apps: "0 connected apps",
        next_safe_step: "Connect an app",
      },
      {
        ...empty,
        protection_state: "protected",
        connected_app_count: 1,
        apps: "1 connected app",
        trusted_people_count: 0,
        trusted_people: "0 trusted people",
        next_safe_step: "Add a trusted person",
      },
      direct,
    ] as HomeProtectionSummary[];
    expect(steps.map((summary) => homeSafeStepAction(summary).route)).toEqual([
      "settings",
      "connections",
      "people",
      "osl-chat",
    ]);
    for (const summary of steps) {
      expect(homeSummaryFactErrors(summary)).toEqual([]);
      expect(expectedSafeStep(summary)).toBe(summary.next_safe_step);
    }
  });

  it("refuses a step this build does not know rather than guessing a route", () => {
    expect(() => homeSafeStepAction({ ...direct, next_safe_step: "Open settings" }))
      .toThrow(/unknown next safe step/u);
  });
});

describe("the counted facts are rebuilt, not repeated", () => {
  it("names one person and one app in the singular", () => {
    expect(trustedPeoplePhrase(1)).toBe("1 trusted person");
    expect(trustedPeoplePhrase(0)).toBe("0 trusted people");
    expect(connectedAppsPhrase(1)).toBe("1 connected app");
    expect(connectedAppsPhrase(3)).toBe("3 connected apps");
  });

  it("states the saved facts of the direct summary", () => {
    expect(homeProtectionSummaryFacts(direct).map((fact) => fact.value)).toEqual([
      "Protected",
      "2 trusted people",
      "2 connected apps",
      "Open a protected conversation",
    ]);
    expect(homeProtectionSummarySentence(direct))
      .toBe("Your account is protected · 2 trusted people · 2 connected apps");
    expect(homeProtectionSummarySentence(empty))
      .toBe("Your account still needs attention · 0 trusted people · 0 connected apps");
  });

  it("catches prose that disagrees with its own count", () => {
    const lying = { ...direct, trusted_people: "2 trusted people", trusted_people_count: 0 };
    expect(homeSummaryFactErrors(lying)).toEqual([
      'trusted people reads "2 trusted people" beside a count of 0',
    ]);
    expect(() => homeProtectionSummaryMarkup(lying)).toThrow(/not usable/u);
  });

  it("catches a missing trusted-person count", () => {
    const missing = { ...direct, trusted_people_count: undefined } as unknown as HomeProtectionSummary;
    expect(homeSummaryFactErrors(missing)[0]).toMatch(/missing trusted-person count/u);
    expect(() => homeProtectionSummaryMarkup(missing)).toThrow(/not usable/u);
  });

  it("catches a next step that does not follow from the facts", () => {
    const wrong = { ...direct, next_safe_step: "Connect an app" };
    expect(homeSummaryFactErrors(wrong)[0]).toMatch(/does not follow from the saved facts/u);
    expect(() => homeProtectionSummaryMarkup(wrong)).toThrow(/not usable/u);
  });
});

describe("the markup carries both named elements", () => {
  const markup = homeProtectionSummaryMarkup(direct);

  it("draws the summary", () => {
    expect(markup).toContain("data-home-summary-sentence");
    expect(markup).toContain("Your account is protected · 2 trusted people · 2 connected apps");
    expect(markup).toContain('data-summary-fact="next-safe-step"');
    expect(markup).toContain("Open a protected conversation");
  });

  it("draws the action route", () => {
    expect(markup).toContain('data-safe-step="Open a protected conversation"');
    expect(markup).toContain('data-route="osl-chat"');
    expect(markup).toContain('data-home-module="osl-chats"');
    expect(markup).toContain(">Open OSL Chat</button>");
  });

  it("routes the empty state somewhere else", () => {
    const emptyMarkup = homeProtectionSummaryMarkup(empty);
    expect(emptyMarkup).toContain('data-route="settings"');
    expect(emptyMarkup).toContain('data-settings="account"');
    expect(emptyMarkup).not.toContain('data-route="osl-chat"');
  });
});
