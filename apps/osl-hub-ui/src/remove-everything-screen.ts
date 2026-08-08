/**
 * The deliberately small final review before opening Account Burn.  Keeping
 * this surface separate from the burn dialog makes the scope readable before
 * a destructive confirmation asks the user to type anything.
 */
export const REMOVE_EVERYTHING_TITLE = "Remove everything";

export const REMOVE_EVERYTHING_CONTROLS = [
  "local data",
  "service data",
  "Remove everything",
  "Cancel",
] as const;

export type RemoveEverythingControl = (typeof REMOVE_EVERYTHING_CONTROLS)[number];

export interface RemoveEverythingScreenTree {
  readonly title: typeof REMOVE_EVERYTHING_TITLE;
  readonly controls: readonly RemoveEverythingControl[];
}

/** The small, testable accessibility tree promised by this page. */
export function removeEverythingScreenTree(): RemoveEverythingScreenTree {
  return { title: REMOVE_EVERYTHING_TITLE, controls: REMOVE_EVERYTHING_CONTROLS };
}

/**
 * Verify the rendered page rather than trusting its source constants.  The
 * check intentionally reads only this page's two disclosure summaries and
 * two buttons, so a matching sentence in explanatory copy cannot satisfy it.
 */
export function checkRemoveEverythingScreen(markup: string): {
  readonly title: string | null;
  readonly controls: readonly string[];
  readonly pass: boolean;
} {
  const title = markup.match(/<h2 id="remove-everything-title">([^<]*)<\/h2>/u)?.[1] ?? null;
  const controls = [
    ...Array.from(markup.matchAll(/<summary>([^<]*)<\/summary>/gu), (match) => match[1]),
    ...Array.from(markup.matchAll(/<button[^>]*>([^<]*)<\/button>/gu), (match) => match[1]),
  ];
  return {
    title,
    controls,
    pass: title === REMOVE_EVERYTHING_TITLE
      && controls.length === REMOVE_EVERYTHING_CONTROLS.length
      && controls.every((control, index) => control === REMOVE_EVERYTHING_CONTROLS[index]),
  };
}

/**
 * The Account screen owns navigation; this function owns only the review
 * screen.  In particular, do not add navigation, help, or a second submit
 * control here: screen-reader users should encounter exactly this review's
 * two summaries and two choices.
 */
export function removeEverythingScreenMarkup(): string {
  return [
    '<section class="remove-everything-screen" aria-labelledby="remove-everything-title">',
    `<h2 id="remove-everything-title">${REMOVE_EVERYTHING_TITLE}</h2>`,
    '<p>Review what OSL can remove from this device before continuing.</p>',
    '<details class="remove-everything-summary"><summary>local data</summary><p>OSL keys, encrypted messages, settings, files, and local sessions for this account.</p></details>',
    '<details class="remove-everything-summary"><summary>service data</summary><p>Indexed OSL service data for this account. Other service copies can remain outside OSL.</p></details>',
    '<div class="remove-everything-actions">',
    '<button class="button danger" id="remove-everything-confirm" type="button">Remove everything</button>',
    '<button class="button" id="remove-everything-cancel" type="button">Cancel</button>',
    '</div>',
    '</section>',
  ].join("");
}
