export type UiProGate = Readonly<{
  id: string;
  enforcement: "native" | "cosmetic";
  reason: string;
}>;

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
