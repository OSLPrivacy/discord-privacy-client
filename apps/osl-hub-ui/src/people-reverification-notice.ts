/**
 * Explain the v3 verification reset wherever a previously stored person now
 * appears as an ordinary unverified person. A key-change review is already
 * explained in its own row, so this deliberately covers the no-change case.
 */
export function peopleReverificationNoticeMarkup(reverificationRequired: boolean): string {
  if (!reverificationRequired) return "";
  return `<aside class="people-reverification-notice" data-people-reverification-notice role="note"><strong>Verify people again</strong><p>Earlier OSL verification records did not prove you compared both people&#39;s keys, so OSL no longer trusts them. Verify each person again before approving protected chats.</p></aside>`;
}
