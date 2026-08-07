/**
 * TASK0764 — the two scenes the Whitelisting capture renders.
 *
 * Plain data, no markup: both the fixture page (in Chrome) and
 * capture-linux-whitelisting.mjs (in node) import this file, so the capture
 * checks the screenshot against the same numbers the page was drawn from
 * instead of against numbers typed twice.
 */

/** Two app accounts, so the screen has to say which account a chat lives in. */
// Neither handle may contain the search word, or the account line alone would
// pull every chat in that account into the "search result" and the filtered
// list would silently stop being filtered.
const DISCORD = "Discord · @ada.lovelace";
const SIGNAL = "Signal · +44 7700 900461";

const CONVERSATIONS = [
  { id: "d-study-circle", account: DISCORD, name: "Study Circle", kind: "Group" },
  { id: "d-study-beta", account: DISCORD, name: "Study Group Beta", kind: "Group" },
  { id: "s-weekend-study", account: SIGNAL, name: "Weekend Study", kind: "Group" },
  { id: "s-family", account: SIGNAL, name: "Family", kind: "Group" },
  { id: "d-ada", account: DISCORD, name: "Ada Lovelace", kind: "Direct messages" },
  { id: "d-standup", account: DISCORD, name: "Team Standup", kind: "Channel" },
  { id: "s-katherine", account: SIGNAL, name: "Katherine Johnson", kind: "Direct messages" },
  { id: "d-announcements", account: DISCORD, name: "Announcements", kind: "Channel" },
];

export const whitelistingScenes = {
  /** Nothing has ever been seen: the capture this screen must not look like. */
  empty: {
    conversations: [],
    saved: [],
    draft: [],
    search: "",
    busy: false,
  },
  /**
   * A search that finds three of eight chats, ticked two-on one-off, with one
   * of those three ticks changed and not yet saved. That is what makes Save and
   * Reset live rather than greyed out in the capture.
   */
  mixed: {
    conversations: CONVERSATIONS,
    saved: ["d-study-circle", "d-study-beta", "s-weekend-study", "s-family"],
    draft: ["d-study-circle", "s-weekend-study", "s-family"],
    search: "study",
    busy: false,
  },
};

/** What the mixed scene must show. The capture fails if the PNG disagrees. */
export const whitelistingMixedExpectation = {
  totalCount: 8,
  matchCount: 3,
  allowedMatches: ["d-study-circle", "s-weekend-study"],
  clearedMatches: ["d-study-beta"],
  unsavedChangeCount: 1,
  resultLine: '3 of 8 conversations match "study" · 2 allowed, 1 not allowed',
  controls: ["Select all", "Clear all", "Save", "Reset"],
};
