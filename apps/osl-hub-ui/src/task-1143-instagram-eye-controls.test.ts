import { describe, expect, it } from "vitest";
import {
  instagramEyeControlsMarkup,
  pressInstagramEyeControl,
  type InstagramEyeFixtureRow,
} from "./instagram-eye-controls";

const markedMarker = "TASK1143-MARKED-INSTAGRAM-ROW";
const protectedText = "TASK1143 protected receiver text";
const fixture: InstagramEyeFixtureRow[] = [
  { marker: "TASK1143-ORDINARY-ONE", ordinaryText: "A friend shared a photo", shownText: "A friend shared a photo", marked: false, eye: "closed" },
  { marker: markedMarker, ordinaryText: "Weekend plans are coming together", shownText: "Weekend plans are coming together", marked: true, eye: "closed" },
  { marker: "TASK1143-ORDINARY-TWO", ordinaryText: "See you after work", shownText: "See you after work", marked: false, eye: "closed" },
];

function receiverBoundEyeCommand(request: string): string {
  const command = JSON.parse(request) as { marker: string; state: "normal" | "protected" };
  expect(Object.keys(command).sort()).toEqual(["marker", "state"]);
  expect(request).not.toContain(protectedText);
  expect(command.marker).toBe(markedMarker);
  return JSON.stringify({
    ok: true,
    result: {
      marker: markedMarker,
      after: command.state,
      shownText: command.state === "protected" ? protectedText : fixture[1].ordinaryText,
    },
  });
}

const protectedRows = (rows: readonly InstagramEyeFixtureRow[]) => rows.filter((row) => row.shownText === protectedText);

describe("TASK1143 Instagram eye controls", () => {
  it("keeps the marked row ordinary with the closed eye, opens only it, then restores it", () => {
    const markup = instagramEyeControlsMarkup(fixture);
    expect((markup.match(/data-instagram-eye="closed-eye"/gu) ?? []).length).toBe(3);
    expect((markup.match(/data-instagram-eye="eye"/gu) ?? []).length).toBe(3);

    const closed = pressInstagramEyeControl(fixture, markedMarker, "closed-eye", receiverBoundEyeCommand);
    expect(closed.filter((row) => row.marked)).toHaveLength(1);
    expect(protectedRows(closed)).toHaveLength(0);
    expect(closed[1]).toMatchObject({ shownText: fixture[1].ordinaryText, eye: "closed" });

    const opened = pressInstagramEyeControl(closed, markedMarker, "eye", receiverBoundEyeCommand);
    expect(protectedRows(opened)).toHaveLength(1);
    expect(opened[1]).toMatchObject({ shownText: protectedText, eye: "open" });
    expect(opened.filter((row) => row.marker !== markedMarker)).toEqual(fixture.filter((row) => row.marker !== markedMarker));

    const restored = pressInstagramEyeControl(opened, markedMarker, "closed-eye", receiverBoundEyeCommand);
    expect(protectedRows(restored)).toHaveLength(0);
    expect(restored).toEqual(fixture);
    console.info("TASK1143 marked_fixture_rows=1 closed_protected_rows=0 open_protected_rows=1 restored_protected_rows=0 non_target_changes=0");
  });
});
