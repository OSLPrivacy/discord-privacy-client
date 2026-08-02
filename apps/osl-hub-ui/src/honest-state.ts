/**
 * The only presentation states allowed for claims whose evidence may be
 * incomplete. Copy belongs to the surface that makes the claim; this module
 * solely constrains the tone that copy may use.
 */
export type HonestState =
  | "confirmed"
  | "not-confirmed"
  | "refused"
  | "unknown";

/** CSS-facing tone names shared by honest-state surfaces. */
export type HonestStateTone = "affirmative" | "neutral" | "refusal";

/**
 * Maps an evidence state to its permitted visual tone. In particular, lack of
 * evidence is never evidence of success.
 */
export function honestStateTone(state: HonestState): HonestStateTone {
  switch (state) {
    case "confirmed":
      return "affirmative";
    case "refused":
      return "refusal";
    case "not-confirmed":
    case "unknown":
      return "neutral";
  }
}
