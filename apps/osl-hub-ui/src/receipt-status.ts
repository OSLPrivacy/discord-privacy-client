/**
 * Sender-facing wording for recipient-controlled privacy receipts.
 *
 * A missing receipt is deliberately ambiguous: the recipient may not have
 * received the message yet, or may have disabled receipts. It must therefore
 * never be represented as evidence that a person did not read the message.
 */
export type SenderReceiptState = "Prepared" | "RelayAccepted" | "Delivered" | "Opened" | "Destroyed";

export interface SenderReceiptStatus {
  label: string;
  confirmed: boolean;
}

const UNCONFIRMED_STATUS: SenderReceiptStatus = {
  label: "Not confirmed",
  confirmed: false,
};

const CONFIRMED_STATUSES: Readonly<Record<Exclude<SenderReceiptState, "Prepared" | "RelayAccepted">, SenderReceiptStatus>> = {
  Delivered: { label: "Their app reported it delivered", confirmed: true },
  Opened: { label: "Their app reported it opened", confirmed: true },
  Destroyed: { label: "Their app reported it destroyed", confirmed: true },
};

/**
 * Render only facts the sender has received. Unknown and pre-receipt states
 * intentionally share the unconfirmed wording so receipt opt-out is not a
 * distinguishable signal.
 */
export function senderReceiptStatus(receipt: SenderReceiptState | null | undefined): SenderReceiptStatus {
  if (receipt === "Delivered" || receipt === "Opened" || receipt === "Destroyed") {
    return CONFIRMED_STATUSES[receipt];
  }
  return UNCONFIRMED_STATUS;
}
