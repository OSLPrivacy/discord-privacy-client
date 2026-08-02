export type UiProGate = Readonly<{
  id: string;
  enforcement: "native" | "cosmetic";
  reason: string;
}>;

export type AiCarrier = "word-bank" | "local-ai" | "cloud";
export type AiCarrierRequest = Exclude<AiCarrier, "word-bank">;
export type CloudGenerationConsent = "granted" | "declined" | "unavailable";

export interface AiCarrierEntitlement {
  readonly access: "free" | "pro" | "offlineGrace";
  readonly requestedCarrier: AiCarrierRequest;
  readonly localModelAvailable: boolean;
  readonly cloudConsent: CloudGenerationConsent;
}

/**
 * The UI may advertise a Pro capability only when this inventory says whether
 * native code enforces it. Cosmetic entries are deliberately unavailable, not
 * a client-side paywall.
 */
export const uiProGates = [
  {
    id: "encrypted-attachments",
    enforcement: "native",
    reason: "Native attachment transport checks the active entitlement before send or open.",
  },
  {
    id: "compatibility-carrier-typing",
    enforcement: "native",
    reason: "Native Discord placement selects typing compatibility only for an active entitlement.",
  },
  {
    id: "autoscrub-run-control",
    enforcement: "native",
    reason: "Native AutoScrub run commands check the active entitlement.",
  },
  {
    id: "mass-cleanup",
    enforcement: "native",
    reason: "Native Mass Cleanup commands check the active entitlement.",
  },
  {
    id: "burn-for-friends",
    enforcement: "cosmetic",
    reason: "The consent-and-acknowledgment workflow is unavailable in this build.",
  },
  {
    id: "aiCarrier",
    enforcement: "cosmetic",
    reason: "AI generation is not built; aiCarrierForEntitlement preserves the word-bank floor for its future caller.",
  },
] as const satisfies readonly UiProGate[];

export function uiProGateProblems(gates: readonly UiProGate[] = uiProGates): string[] {
  return gates.flatMap((gate) => {
    if (!gate.id.trim()) return ["UI Pro gate must have an id."];
    if (!gate.reason.trim()) return [`UI Pro gate ${gate.id} must explain its enforcement.`];
    if (gate.enforcement !== "native" && gate.enforcement !== "cosmetic") {
      return [`UI Pro gate ${gate.id} must be native-enforced or cosmetic.`];
    }
    return [];
  });
}

// D69: hiding local message previews is a free privacy control.
export function chatPreviewHidingVisible(previewsVisible: boolean): boolean {
  return previewsVisible;
}

/**
 * The sole entitlement seam for the AI carrier.  Sending must always retain a
 * usable word-bank path: Free, lapsed, declined, and unavailable AI all land
 * there rather than preventing encryption or delivery.
 */
export function aiCarrierForEntitlement(input: AiCarrierEntitlement): AiCarrier {
  const hasProEntitlement = input.access === "pro" || input.access === "offlineGrace";
  if (!hasProEntitlement) return "word-bank";

  if (input.requestedCarrier === "local-ai") {
    return input.localModelAvailable ? "local-ai" : "word-bank";
  }

  return input.cloudConsent === "granted" ? "cloud" : "word-bank";
}
