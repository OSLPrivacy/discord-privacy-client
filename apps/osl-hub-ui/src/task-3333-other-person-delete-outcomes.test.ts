import { describe, expect, it } from "vitest";
import {
  firstTimedDeleteWarningMarkup,
  SIGNAL_QUOTED_REPLY_OUTCOME,
  TIMED_DELETE_OTHER_PERSON_OUTCOMES,
} from "./onboarding-delete";

const warning = firstTimedDeleteWarningMarkup(false);

describe("TASK 3333 other-person timed-delete outcomes", () => {
  it("renders exactly seven apps in the three required outcome groups", () => {
    const groups = TIMED_DELETE_OTHER_PERSON_OUTCOMES;
    expect(groups.nothingLeft.apps).toEqual(["Discord", "Telegram", "Instagram"]);
    expect(groups.deletionNote.apps).toEqual(["WhatsApp", "Signal", "Messenger"]);
    expect(groups.originalRemains.apps).toEqual(["Email"]);

    const apps = Object.values(groups).flatMap((group) => group.apps);
    const appOutcomes = Object.values(groups).flatMap((group) =>
      group.apps.map((app) => `${app}: ${group.outcome}`),
    );
    expect(Object.values(groups).map((group) => group.apps.length)).toEqual([3, 3, 1]);
    expect(apps).toHaveLength(7);
    expect(new Set(apps).size).toBe(7);
    expect(warning.match(/data-timed-delete-outcome(?=\s|>)/gu)).toHaveLength(7);
    expect(warning.match(/data-timed-delete-outcome-group=/gu)).toHaveLength(3);

    for (const app of apps) {
      expect(warning.match(new RegExp(`<strong>${app}</strong>`, "gu"))).toHaveLength(1);
    }

    expect(warning).toContain("Nothing left in the chat");
    expect(warning).toContain("A “message was deleted” note remains");
    expect(warning).toContain("The original remains — delivered email cannot be recalled");

    console.log(`TASK3333_APP_OUTCOMES=${appOutcomes.join(" | ")}`);
  });

  it("names Signal's quoted-reply problem on the visible Signal outcome", () => {
    const signalOutcome = warning.match(/<li[^>]*data-timed-delete-app="Signal"[^>]*>(.*?)<\/li>/u)?.[1] ?? "";
    expect(signalOutcome).toContain("Quoted-reply problem");
    expect(signalOutcome).toContain(SIGNAL_QUOTED_REPLY_OUTCOME);
    console.log(`TASK3333_SIGNAL_QUOTED_REPLY=${SIGNAL_QUOTED_REPLY_OUTCOME}`);
  });
});
