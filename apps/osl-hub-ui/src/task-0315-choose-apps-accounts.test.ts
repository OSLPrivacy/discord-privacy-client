import { describe, expect, it } from "vitest";
import { chooseDetectedAccountOpening, detectedOpeningChoiceKey } from "./detected-account-opening";
import { parseDetectedAccounts } from "./services";

describe("TASK 0315 choose-apps detected accounts", () => {
  it("retains one opening choice for each of two detected accounts", () => {
    const detected = parseDetectedAccounts([
      { serviceId: "discord", accountId: "personal-discord", accountLabel: "Personal Discord", openChoices: [{ kind: "windowsApp", label: "Discord" }] },
      { serviceId: "email", accountId: "work-gmail", accountLabel: "Work Gmail", openChoices: [{ kind: "browser", label: "Chrome" }] },
    ]);
    expect(detected).not.toBeNull();
    let choices = chooseDetectedAccountOpening(new Map(), detected![0], "windowsApp");
    choices = chooseDetectedAccountOpening(choices, detected![1], "browser");
    expect([...choices]).toEqual([[detectedOpeningChoiceKey(detected![0]), "windowsApp"], [detectedOpeningChoiceKey(detected![1]), "browser"]]);
    console.log(`task_0315 accounts=${detected!.length} choices=${choices.size} personal=${choices.get("discord:personal-discord")} work=${choices.get("email:work-gmail")}`);
  });
});
