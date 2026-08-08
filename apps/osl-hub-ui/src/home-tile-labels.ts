/**
 * TASK 0805 — the generated capability label every Home tile carries.
 *
 * The label is DERIVED, never typed. `generatedTileLabel` is the renderer's
 * copy of the backend rule in `apps/osl-hub/src/services.rs`
 * (`generated_tile_label`), and the facts it reads are the same four booleans
 * the backend keeps in `ServiceCapabilityFacts`. Nothing here may hold a
 * sentence about what a surface will do later: a tile that has no built
 * capability says "Not started", which is a statement about today.
 */

export interface HomeTileCapabilityFacts {
  placing: boolean;
  reading: boolean;
  opening: boolean;
  realTwoPersonProtectedMessaging: boolean;
}

/** The complete set of labels tile generation can produce. */
export const GENERATED_TILE_LABELS = [
  "Ready",
  "Placing only",
  "Reading only",
  "Opens the app",
  "Not started",
] as const;

export type GeneratedTileLabel = (typeof GENERATED_TILE_LABELS)[number];

/**
 * Mirror of `osl_privacy_hub::services::generated_tile_label` (TASK 0804).
 * Keep the branch order identical: "Ready" is the strongest claim and is only
 * reachable from proven two-person protected messaging or from a surface that
 * both places and reads.
 */
export function generatedTileLabel(facts: HomeTileCapabilityFacts): GeneratedTileLabel {
  if (facts.realTwoPersonProtectedMessaging || (facts.placing && facts.reading)) return "Ready";
  if (facts.placing) return "Placing only";
  if (facts.reading) return "Reading only";
  if (facts.opening) return "Opens the app";
  return "Not started";
}

function facts(
  placing: boolean,
  reading: boolean,
  opening: boolean,
  realTwoPersonProtectedMessaging: boolean,
): HomeTileCapabilityFacts {
  return { placing, reading, opening, realTwoPersonProtectedMessaging };
}

// Connected-service rows are the renderer's copy of SERVICE_CAPABILITY_FACTS in
// `apps/osl-hub/src/services.rs`. Every email provider tile is the one `Email`
// service kind there, so they all carry the same facts.
const EMAIL_WEB_FACTS = facts(false, false, true, false);

// First-party module rows come from `apps/osl-hub/src/claim_state.rs`:
//   - OSL Chat  : DeliveryEvidence::ProvenLiveBothWays — carries messages both
//                 ways through the deployed OSL service, so all four hold.
//   - OSL Mail  : DeliveryEvidence::NotDeliverable — nothing sent through it can
//                 be read, and the Home tile is disabled, so nothing is built.
//   - OSL Notes : no claim row and a disabled Home tile — nothing is built.
//   - Scrub     : a first-party screen the Home tile opens. It is not a carrier:
//                 it neither places nor reads messages in another app.
const HOME_TILE_CAPABILITY_FACTS: Readonly<Record<string, HomeTileCapabilityFacts>> = {
  discord: facts(true, true, true, false),
  telegram: facts(true, true, true, false),
  whatsapp: facts(false, true, true, false),
  signal: facts(false, true, true, false),
  gmail: EMAIL_WEB_FACTS,
  outlook: EMAIL_WEB_FACTS,
  proton: EMAIL_WEB_FACTS,
  yahoo: EMAIL_WEB_FACTS,
  aol: EMAIL_WEB_FACTS,
  gmx: EMAIL_WEB_FACTS,
  maildotcom: EMAIL_WEB_FACTS,
  icloud: EMAIL_WEB_FACTS,
  tuta: EMAIL_WEB_FACTS,
  "osl-chats": facts(true, true, true, true),
  "osl-mail": facts(false, false, false, false),
  "osl-notes": facts(false, false, false, false),
  scrub: facts(false, false, true, false),
};

/**
 * Facts for one Home tile. An id with no record has nothing built behind it,
 * which is exactly the all-false row — so a new tile cannot slip onto Home
 * carrying no label at all.
 */
export function homeTileCapabilityFacts(tileId: string): HomeTileCapabilityFacts {
  return HOME_TILE_CAPABILITY_FACTS[tileId] ?? facts(false, false, false, false);
}

/** The label a Home tile shows. Never empty, never hand-written. */
export function homeTileGeneratedLabel(tileId: string): GeneratedTileLabel {
  return generatedTileLabel(homeTileCapabilityFacts(tileId));
}
