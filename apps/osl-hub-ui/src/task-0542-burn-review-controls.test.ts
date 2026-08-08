import { describe, expect, it } from "vitest";
import { backBurnReviewControl, saveBurnReviewControl, type BurnReviewCommandPort } from "./burn-review-controls";
import { burnReviewScreenMarkup, initialBurnReviewScreenState } from "./burn-review-screen";

describe("TASK 0542 burn review controls", () => {
  it("emits exactly one matching command for each review control and limits server choices", async () => {
    const actions: string[] = [];
    const port: BurnReviewCommandPort = {
      async save(scope, chat, hidden) {
        actions.push(`save_burn_review_state scope=${scope} chat=${chat} hidden=${hidden}`);
        return true;
      },
      async back() {
        actions.push("back_burn_review");
        return true;
      },
    };
    for (const side of ["your_side", "their_side", "both_sides"] as const) {
      expect(await saveBurnReviewControl(port, side, "chat:task0542", false)).toBe(true);
    }
    expect(await saveBurnReviewControl(port, "both_sides", "chat:task0542", true)).toBe(true);
    expect(await backBurnReviewControl(port)).toBe(true);

    const directMarkup = burnReviewScreenMarkup(initialBurnReviewScreenState(false));
    const serverMarkup = burnReviewScreenMarkup(initialBurnReviewScreenState(true));
    expect(directMarkup).not.toContain("data-burn-review-server-choice");
    expect(serverMarkup.match(/data-burn-review-server-choice/g)).toHaveLength(2);
    expect(actions).toEqual([
      "save_burn_review_state scope=your_side chat=chat:task0542 hidden=false",
      "save_burn_review_state scope=their_side chat=chat:task0542 hidden=false",
      "save_burn_review_state scope=both_sides chat=chat:task0542 hidden=false",
      "save_burn_review_state scope=both_sides chat=chat:task0542 hidden=true",
      "back_burn_review",
    ]);
    for (const action of actions) console.log(`TASK0542 ACTION ${action} count=1`);
    console.log("TASK0542 SERVER_CHOICES direct=0 server_channel=2");
  });
});
