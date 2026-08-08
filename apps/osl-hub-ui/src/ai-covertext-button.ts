import { MODEL_PACK_NEEDED } from "./cover-writing-controls";

export type AiCovertextPressOutcome =
  | { readonly kind: "written"; readonly coverMessage: string }
  | { readonly kind: "refused"; readonly reason: typeof MODEL_PACK_NEEDED }
  | { readonly kind: "write-failed" };

export interface AiCovertextButtonDependencies {
  /** Rechecked for every press so removing a pack fails closed immediately. */
  readonly modelPackPresent: () => Promise<boolean>;
  /** The protected-message writer alone owns private message handling. */
  readonly writeCoverMessage: () => Promise<string | null>;
}

/** Run one AI Covertext press without ever handing private words to the model gate. */
export async function pressAiCovertextButton(
  dependencies: AiCovertextButtonDependencies,
): Promise<AiCovertextPressOutcome> {
  if (!await dependencies.modelPackPresent()) {
    return { kind: "refused", reason: MODEL_PACK_NEEDED };
  }
  const coverMessage = await dependencies.writeCoverMessage();
  if (!coverMessage) {
    return { kind: "write-failed" };
  }
  return { kind: "written", coverMessage };
}
