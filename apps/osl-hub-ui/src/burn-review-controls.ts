import type { BurnReviewSide } from "./burn-review-screen";

export interface BurnReviewCommandPort {
  save(selectedScope: string, selectedChat: string, hideOtherPeople: boolean): Promise<boolean>;
  back(): Promise<boolean>;
}

export async function saveBurnReviewControl(
  port: BurnReviewCommandPort,
  side: BurnReviewSide,
  selectedChat: string,
  hideOtherPeople: boolean,
): Promise<boolean> {
  return port.save(side, selectedChat, hideOtherPeople);
}

export async function backBurnReviewControl(port: BurnReviewCommandPort): Promise<boolean> {
  return port.back();
}
