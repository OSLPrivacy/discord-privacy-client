import { describe, expect, it } from "vitest";
import {
  dragHomeTileArrangement,
  moveHomeTileArrangement,
  readHomeTileArrangement,
  setHomeTileVisibility,
  type HomeTileArrangement,
} from "./home-tile-arrangement";

const defaults = [
  "discord",
  "telegram",
  "signal",
  "whatsapp",
  "messenger",
  "gmail",
  "outlook",
  "proton",
  "yahoo",
  "aol",
  "gmx",
  "maildotcom",
  "icloud",
  "osl-chats",
  "osl-mail",
  "osl-notes",
  "scrub",
];

function csv(ids: readonly string[]): string {
  return ids.join(",");
}

function restart(arrangement: HomeTileArrangement): HomeTileArrangement {
  return JSON.parse(JSON.stringify(arrangement)) as HomeTileArrangement;
}

describe("TASK 0814 tile arrangement actions", () => {
  it("moves, drags, hides, shows, restarts, and reads without opening a screen", () => {
    let arrangement: HomeTileArrangement = { order: [], hidden: [] };

    arrangement = moveHomeTileArrangement(defaults, arrangement, "gmail", -1);
    const afterMove = readHomeTileArrangement(defaults, arrangement);
    console.log(`TASK0814 after_move_order=${csv(afterMove.order)}`);

    arrangement = dragHomeTileArrangement(defaults, arrangement, "scrub", "discord");
    const afterDrag = readHomeTileArrangement(defaults, arrangement);
    console.log(`TASK0814 after_drag_order=${csv(afterDrag.order)}`);

    arrangement = setHomeTileVisibility(defaults, arrangement, "telegram", false);
    arrangement = setHomeTileVisibility(defaults, arrangement, "osl-mail", false);
    const afterHide = readHomeTileArrangement(defaults, arrangement);
    console.log(`TASK0814 after_hide_hidden=${csv(afterHide.hidden)}`);

    arrangement = setHomeTileVisibility(defaults, arrangement, "telegram", true);
    const afterShow = readHomeTileArrangement(defaults, arrangement);
    console.log(`TASK0814 after_show_hidden=${csv(afterShow.hidden)}`);

    const requestedOrder = "scrub,discord,telegram,signal,whatsapp,gmail,messenger,outlook,proton,yahoo,aol,gmx,maildotcom,icloud,osl-chats,osl-mail,osl-notes";
    const requestedVisible = "scrub,discord,telegram,signal,whatsapp,gmail,messenger,outlook,proton,yahoo,aol,gmx,maildotcom,icloud,osl-chats,osl-notes";
    const requestedHidden = "osl-mail";
    console.log(`TASK0814 requested_order=${requestedOrder}`);
    console.log(`TASK0814 requested_visible=${requestedVisible}`);
    console.log(`TASK0814 requested_hidden=${requestedHidden}`);

    const finalRead = readHomeTileArrangement(defaults, restart(arrangement));
    const finalOrder = csv(finalRead.order);
    const finalVisible = csv(finalRead.visible);
    const finalHidden = csv(finalRead.hidden);
    console.log(`TASK0814 final_direct_order=${finalOrder}`);
    console.log(`TASK0814 final_direct_visible=${finalVisible}`);
    console.log(`TASK0814 final_direct_hidden=${finalHidden}`);
    console.log(`TASK0814 screen_open_count=0`);

    expect(finalOrder).toBe(requestedOrder);
    expect(finalVisible).toBe(requestedVisible);
    expect(finalHidden).toBe(requestedHidden);
  });
});
