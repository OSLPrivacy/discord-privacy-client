/**
 * The v3 People migration invalidates a ceremony that compared an OSL-generated
 * value with itself. This is deliberately presentational: migration ownership
 * remains with T5, while this surface makes its consequence explicit.
 */
export const PEOPLE_SCHEMA_VERSION = 3;

/**
 * This notice has no dismiss control. Until the updated two-party code is
 * compared out of band, an affected person remains unverified; if they are
 * offline, that comparison has to wait.
 */
export function breakingChangeNoticeMarkup(peopleVersion: number): string {
  if (peopleVersion >= PEOPLE_SCHEMA_VERSION) return "";
  return `<aside class="people-reverification-notice" data-people-reverification-notice role="note"><strong>Verification codes changed</strong><p>Every friend must be verified again — the previous check compared a number OSL generated against itself. Earlier OSL verification records did not prove you compared both people&#39;s keys.</p><p>Verify each person again before approving protected chats. If a friend is offline, wait to compare the new verification code another way. OSL will not treat them as verified in the meantime.</p></aside>`;
}
