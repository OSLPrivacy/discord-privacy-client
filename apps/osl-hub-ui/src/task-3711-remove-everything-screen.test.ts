import { describe, expect, it } from "vitest";
import {
  REMOVE_EVERYTHING_CONTROLS,
  REMOVE_EVERYTHING_TITLE,
  checkRemoveEverythingScreen,
  removeEverythingScreenMarkup,
  removeEverythingScreenTree,
} from "./remove-everything-screen";

describe("TASK 3711 - connect Remove everything screen", () => {
  it("draws the title and exactly the four required controls", () => {
    const tree = removeEverythingScreenTree();
    const markup = removeEverythingScreenMarkup();
    const checked = checkRemoveEverythingScreen(markup);

    expect(tree.title).toBe(REMOVE_EVERYTHING_TITLE);
    expect(tree.controls).toEqual([
      "local data",
      "service data",
      "Remove everything",
      "Cancel",
    ]);
    expect(REMOVE_EVERYTHING_CONTROLS).toHaveLength(4);
    expect(checked).toEqual({
      title: REMOVE_EVERYTHING_TITLE,
      controls: REMOVE_EVERYTHING_CONTROLS,
      pass: true,
    });
    expect(markup).toContain(`<h2 id="remove-everything-title">${REMOVE_EVERYTHING_TITLE}</h2>`);
    for (const control of tree.controls) expect(markup).toContain(`>${control}<`);

    console.info(`TASK3711_TITLE=${tree.title}`);
    console.info(`TASK3711_CONTROLS=${tree.controls.join("|")}`);
    console.info(`TASK3711_CONTROL_COUNT=${tree.controls.length}`);
  });

  it("refuses a screen tree with any required control omitted", () => {
    const markup = removeEverythingScreenMarkup();
    for (const control of REMOVE_EVERYTHING_CONTROLS) {
      const broken = control === "local data" || control === "service data"
        ? markup.replace(`<summary>${control}</summary>`, "<summary></summary>")
        : markup.replace(`>${control}</button>`, "></button>");
      const checked = checkRemoveEverythingScreen(broken);
      expect(checked.pass).toBe(false);
      expect(checked.controls).not.toContain(control);
      console.info(`TASK3711_OMIT_${control.replaceAll(" ", "_")}=refused`);
    }
  });
});
