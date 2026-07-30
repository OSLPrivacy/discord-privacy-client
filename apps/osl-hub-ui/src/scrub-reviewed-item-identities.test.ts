import { describe, expect, it } from "vitest";
import { bindReviewedItemIdentities, type ReviewedItemIdentity } from "./adapters";

const reviewed: ReviewedItemIdentity[] = [
  {
    reviewId: "review-a",
    serviceId: "email",
    accountId: "account-a",
    conversationId: "thread-a",
    messageLocator: "message-a",
  },
  {
    reviewId: "review-b",
    serviceId: "email",
    accountId: "account-b",
    conversationId: "thread-a",
    messageLocator: "message-a",
  },
  {
    reviewId: "review-c",
    serviceId: "email",
    accountId: "account-a",
    conversationId: "thread-b",
    messageLocator: "message-a",
  },
];

describe("reviewed scrub item identity binding", () => {
  it("bindReviewedItemIdentities binds exact reviewed item identities, not overbroad account or conversation matches", () => {
    expect(bindReviewedItemIdentities(reviewed, ["review-a"])).toEqual([{
      ...reviewed[0],
      selected: true,
    }]);

    expect(bindReviewedItemIdentities(reviewed, ["review-missing"])).toBeNull();
    expect(bindReviewedItemIdentities(reviewed, ["review-a", "review-a"])).toBeNull();
    expect(bindReviewedItemIdentities([
      reviewed[0],
      { ...reviewed[1], reviewId: "review-a" },
    ], ["review-a"])).toBeNull();
  });
});
