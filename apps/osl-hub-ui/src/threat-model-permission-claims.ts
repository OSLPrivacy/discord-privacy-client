/**
 * TASK 1545 - threat-model words must not promise service permission.
 *
 * The Scrub threat-model page tells the reader what OSL cannot do. A sentence
 * on that page that says a service allows, permits, supports or guarantees
 * scanning or deleting promises something no service has agreed to: OSL has no
 * arrangement with Discord, a mail host or anyone else, and an account can be
 * rate-limited, suspended or banned for either action. This module pulls the
 * page apart into sentences and reports the ones that make that promise.
 *
 * A denial is not a promise. "No service has agreed to let OSL scan or delete
 * on your behalf" has to stay sayable, so a sentence is cleared only when every
 * permission verb in it sits behind a negation.
 */

/** Words that name the other side of the connection, not OSL. */
const SERVICE = String.raw`(?:service|services|provider|providers|platform|platforms|host|hosts|server|servers|site|sites|app|apps|discord|gmail|outlook|slack|imap|mailbox)`;
/** Verbs that hand over permission. */
const PERMISSION = String.raw`(?:allow|allows|allowed|allowing|permit|permits|permitted|permitting|let|lets|letting|approve|approves|approved|authorise|authorises|authorised|authorize|authorizes|authorized|grant|grants|granted|support|supports|supported|guarantee|guarantees|guaranteed|ensure|ensures|ensured|agree|agrees|agreed|sanction|sanctions|sanctioned|bless|blesses|welcome|welcomes)`;
/** The two acts this build must never say a service has signed off on. */
const ACTION = String.raw`(?:scan|scans|scanned|scanning|scrub|scrubs|scrubbed|scrubbing|delete|deletes|deleted|deleting|deletion|deletions|remove|removes|removed|removal|erase|erases|erased|erasure|wipe|wipes|wiped)`;
const NEGATION = String.raw`(?:not|never|no|none|nothing|nobody|cannot|n't|without|neither|nor|refuse|refuses|refused|refusing|deny|denies|denied|blocks|blocked|forbid|forbids|forbidden)`;

/** "The service allows deleting", with room for the words in between. */
const FORWARD_PROMISE = new RegExp(
  String.raw`\b${SERVICE}\b[^.!?;]{0,50}?\b${PERMISSION}\b[^.!?;]{0,50}?\b${ACTION}\b`,
  'iu',
);
/** The same promise inverted: "deletion is supported by your provider". */
const REVERSE_PROMISE = new RegExp(
  String.raw`\b${ACTION}\b[^.!?;]{0,50}?\b(?:is|are|was|were|will\s+be|gets|get)\s+(?:${PERMISSION})\b[^.!?;]{0,40}?\bby\b[^.!?;]{0,30}?\b${SERVICE}\b`,
  'iu',
);

/** Splits page text into the units a reader takes in as one statement. */
export function sentencesOf(text: string): string[] {
  return text
    .split(/(?<=[.!?;])\s+|\n+/u)
    .map((piece) => piece.replace(/\s+/gu, ' ').trim())
    .filter(Boolean);
}

/** True when every permission verb in the sentence sits behind a negation. */
export function isDenial(sentence: string): boolean {
  const negations = [...sentence.matchAll(new RegExp(String.raw`\b${NEGATION}\b`, 'giu'))]
    .map((match) => match.index ?? 0);
  if (negations.length === 0) return false;
  const verbs = [...sentence.matchAll(new RegExp(String.raw`\b${PERMISSION}\b`, 'giu'))]
    .map((match) => match.index ?? 0);
  // A denial glued to a promise -- "no service is asked, and Discord allows
  // deletion" -- is still a promise, so one uncovered verb is enough to fail.
  return verbs.every((at) => negations.some((no) => no < at));
}

/** Sentences that talk about scanning or deleting at all. */
export function actionSentences(text: string): string[] {
  const mentions = new RegExp(String.raw`\b${ACTION}\b`, 'iu');
  return sentencesOf(text).filter((sentence) => mentions.test(sentence));
}

/** Every sentence that says a service allows scanning or deleting. */
export function servicePermissionPromises(text: string): string[] {
  return actionSentences(text).filter(
    (sentence) => !isDenial(sentence)
      && (FORWARD_PROMISE.test(sentence) || REVERSE_PROMISE.test(sentence)),
  );
}

/** Turns rendered markup into the words a reader is shown. */
export function pageText(markup: string): string {
  return markup
    .replace(/<(script|style)\b[\s\S]*?<\/\1>/giu, ' ')
    .replace(/<\/(?:p|li|ul|div|details|h\d)>/giu, '$& ')
    .replace(/<[^>]*>/gu, ' ')
    .replace(/&nbsp;|&#160;/giu, ' ')
    .replace(/&amp;/giu, '&')
    .replace(/&quot;/giu, '"')
    .replace(/&#39;|&apos;/giu, "'")
    .replace(/\s+/gu, ' ')
    .trim();
}

/** The promise this task exists to keep off the page, and its neighbours. */
export const SELF_TEST_PROMISES = [
  'The service allows scanning and deletion.',
  'Discord permits Scrub to delete your messages.',
  'Your provider lets OSL scan the account.',
  'Deletion is supported by your mail host.',
  'Every platform has agreed to let OSL remove old posts.',
  'The app approves automated deletion for connected accounts.',
];
/** Honest sentences the checker must leave alone. */
export const SELF_TEST_SAFE = [
  'No service has agreed to let OSL scan or delete on your behalf.',
  'Scrub cannot guarantee service permission.',
  'A service is free to refuse, rate-limit, suspend, or ban an account over scanning or deleting.',
  'Only a service recheck can verify removal within its stated coverage.',
  'You are responsible. Check the original app and delete each message yourself.',
  'This build only gives manual directions. It does not delete app messages.',
];

export function selfTest(): { missed: string[]; falsePositives: string[] } {
  return {
    missed: SELF_TEST_PROMISES.filter((sentence) => servicePermissionPromises(sentence).length === 0),
    falsePositives: SELF_TEST_SAFE.filter((sentence) => servicePermissionPromises(sentence).length > 0),
  };
}
