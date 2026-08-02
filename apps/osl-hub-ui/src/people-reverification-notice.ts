import { breakingChangeNoticeMarkup } from "./breaking-change-notice";

/**
 * The People DTO intentionally does not expose its encrypted file version.
 * T5 clears legacy claims before it returns the roster, so an unverified person
 * with no pending key change is the UI's durable indication of this migration.
 */
export function peopleReverificationNoticeMarkup(reverificationRequired: boolean): string {
  return breakingChangeNoticeMarkup(reverificationRequired ? 2 : 3);
}
