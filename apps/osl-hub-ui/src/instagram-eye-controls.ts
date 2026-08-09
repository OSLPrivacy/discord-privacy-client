/**
 * The small controller behind the eye / closed-eye buttons on Instagram rows.
 *
 * The renderer deliberately owns only ordinary text and the currently shown
 * text.  Protected words are returned by the receiver-bound eye-state command;
 * a click sends only the row marker and the requested state.
 */
export type InstagramEyeControl = "eye" | "closed-eye";
export type InstagramEyeState = "normal" | "protected";

export interface InstagramEyeFixtureRow {
  marker: string;
  ordinaryText: string;
  shownText: string;
  marked: boolean;
  eye: "open" | "closed";
}

interface EyeCommandResult {
  ok: true;
  result: {
    marker: string;
    after: InstagramEyeState;
    shownText: string;
  };
}

export type InstagramEyeStateCommand = (request: string) => string;

function commandResult(raw: string): EyeCommandResult {
  const parsed: unknown = JSON.parse(raw);
  if (!parsed || typeof parsed !== "object") throw new Error("Instagram eye command returned no object");
  const value = parsed as { ok?: unknown; result?: { marker?: unknown; after?: unknown; shownText?: unknown } };
  if (value.ok !== true
    || typeof value.result?.marker !== "string"
    || (value.result.after !== "normal" && value.result.after !== "protected")
    || typeof value.result.shownText !== "string") {
    throw new Error("Instagram eye command returned an invalid result");
  }
  return value as EyeCommandResult;
}

/** Render both named controls for every Instagram row. */
export function instagramEyeControlsMarkup(rows: readonly InstagramEyeFixtureRow[]): string {
  return rows.map((row) => `<article data-instagram-row-marker="${row.marker}" data-instagram-marked="${row.marked}">`
    + `<p data-instagram-row-text>${row.shownText}</p>`
    + `<button type="button" data-instagram-eye="closed-eye" aria-label="Show ordinary Instagram content">Closed eye</button>`
    + `<button type="button" data-instagram-eye="eye" aria-label="Show protected text">Eye</button>`
    + `</article>`).join("");
}

/**
 * Apply one button press.  The backend is authoritative: this controller never
 * accepts protected text from the page or includes it in its command request.
 */
export function pressInstagramEyeControl(
  rows: readonly InstagramEyeFixtureRow[],
  marker: string,
  control: InstagramEyeControl,
  writeEyeState: InstagramEyeStateCommand,
): InstagramEyeFixtureRow[] {
  const requestedState: InstagramEyeState = control === "eye" ? "protected" : "normal";
  const request = JSON.stringify({ marker, state: requestedState });
  const response = commandResult(writeEyeState(request));
  if (response.result.marker !== marker || response.result.after !== requestedState) {
    throw new Error("Instagram eye command changed a different row or state");
  }
  return rows.map((row) => row.marker === marker
    ? {
      ...row,
      shownText: response.result.shownText,
      eye: requestedState === "protected" ? "open" : "closed",
    }
    : row);
}
