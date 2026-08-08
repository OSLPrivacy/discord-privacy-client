import { describe, expect, it } from "vitest";
import type { HomeAppCatalogEntry, HomeAppId } from "./services";
import {
  carrierTileClick,
  emailRowTiles,
  renderCircularTileRow,
  socialRowTiles,
} from "./home-social-email-rows";
import { toggleHomeTileVisibility } from "./home-tile-arrangement";

/**
 * TASK 5008. Home's SOCIAL row is circular brand tiles, one per connected
 * carrier; EMAIL is the same for mail services. Both rows are data-driven
 * from the shipped-services list -- no tile id is hard-coded anywhere in
 * `home-social-email-rows.ts` -- so cutting a carrier from that list removes
 * its tile by data change alone, with zero code changes.
 */

function carrier(id: HomeAppId, displayName: string): HomeAppCatalogEntry {
  return {
    id,
    displayName,
    serviceId: id as HomeAppCatalogEntry["serviceId"],
    provider: null,
    visibility: "launch",
    section: "social",
    launchState: "available",
    linked: true,
    accountCount: 1,
    setupEligible: false,
  };
}

function mailService(id: HomeAppId, displayName: string): HomeAppCatalogEntry {
  return {
    id,
    displayName,
    serviceId: "email",
    provider: id as HomeAppCatalogEntry["provider"],
    visibility: "launch",
    section: "email",
    launchState: "available",
    linked: true,
    accountCount: 1,
    setupEligible: false,
  };
}

// The shipped-services list this task's finish line names: 4 carriers, 9 mail
// services -- exactly the shape a real freeze at hour 30 would hand Home.
const FOUR_CARRIERS: HomeAppCatalogEntry[] = [
  carrier("discord", "Discord"),
  carrier("telegram", "Telegram"),
  carrier("signal", "Signal"),
  carrier("whatsapp", "WhatsApp"),
];

const NINE_MAIL_SERVICES: HomeAppCatalogEntry[] = [
  mailService("gmail", "Gmail"),
  mailService("outlook", "Outlook"),
  mailService("proton", "Proton Mail"),
  mailService("yahoo", "Yahoo Mail"),
  mailService("aol", "AOL Mail"),
  mailService("gmx", "GMX"),
  mailService("maildotcom", "Mail.com"),
  mailService("icloud", "iCloud Mail"),
  mailService("tuta", "Tuta"),
];

const EMPTY_ARRANGEMENT = { order: [], hidden: [] };

function tileIds(markup: string): string[] {
  return [...markup.matchAll(/<article[^>]*\bdata-tile-id="([^"]+)"/gu)].map((match) => match[1]);
}

describe("TASK 5008 Home SOCIAL/EMAIL circular tile rows", () => {
  it("shows exactly 4 circular tiles in SOCIAL and 9 in EMAIL for the fixture list", () => {
    const catalog = [...FOUR_CARRIERS, ...NINE_MAIL_SERVICES];
    const socialTiles = socialRowTiles(catalog, EMPTY_ARRANGEMENT);
    const emailTiles = emailRowTiles(catalog, EMPTY_ARRANGEMENT);
    expect(socialTiles).toHaveLength(4);
    expect(emailTiles).toHaveLength(9);

    const socialMarkup = renderCircularTileRow(socialTiles, false);
    const emailMarkup = renderCircularTileRow(emailTiles, false);
    expect(tileIds(socialMarkup)).toHaveLength(4);
    expect(tileIds(emailMarkup)).toHaveLength(9);
    expect(socialMarkup).toContain('class="circular-tile');
    expect(emailMarkup).toContain('class="circular-tile');
  });

  it("removing one carrier from the list removes exactly 1 tile, with 0 code changes", () => {
    const fullCatalog = [...FOUR_CARRIERS, ...NINE_MAIL_SERVICES];
    const before = socialRowTiles(fullCatalog, EMPTY_ARRANGEMENT);
    expect(before).toHaveLength(4);

    // The whole edit is a data change: drop one carrier from the fixture list.
    // No function in home-social-email-rows.ts is touched.
    const cutCatalog = fullCatalog.filter((entry) => entry.id !== "whatsapp");
    const after = socialRowTiles(cutCatalog, EMPTY_ARRANGEMENT);
    expect(after).toHaveLength(3);
    expect(before.length - after.length).toBe(1);
    expect(after.some((tile) => tile.id === "whatsapp")).toBe(false);

    const afterMarkup = renderCircularTileRow(after, false);
    expect(tileIds(afterMarkup)).toHaveLength(3);
    expect(afterMarkup).not.toContain('data-tile-id="whatsapp"');

    // The email row is untouched by the carrier cut.
    expect(emailRowTiles(cutCatalog, EMPTY_ARRANGEMENT)).toHaveLength(9);
  });

  it("clicking a carrier tile opens the Strip over that named carrier", () => {
    const catalog = [...FOUR_CARRIERS, ...NINE_MAIL_SERVICES];
    const tiles = socialRowTiles(catalog, EMPTY_ARRANGEMENT);
    const markup = renderCircularTileRow(tiles, false);

    // The tile carries the wiring point a click handler binds to.
    expect(markup).toContain('data-open-strip="discord"');

    const action = carrierTileClick("discord");
    expect(action).toEqual({ kind: "openStrip", carrierId: "discord" });

    // Every tile opens the Strip over ITS OWN carrier, not a fixed one.
    for (const tile of tiles) {
      expect(carrierTileClick(tile.id)).toEqual({ kind: "openStrip", carrierId: tile.id });
    }
  });

  it("stays compatible with the tune icon's edit mode, which can hide any tile", () => {
    const catalog = [...FOUR_CARRIERS, ...NINE_MAIL_SERVICES];
    const defaults = FOUR_CARRIERS.map((entry) => entry.id);
    // The tune icon writes through the SAME HomeTileArrangement contract the
    // rest of Home already uses (home-tile-arrangement.ts) -- this row does
    // not need to know anything about how edit mode works.
    const arrangement = toggleHomeTileVisibility(defaults, EMPTY_ARRANGEMENT, "signal");
    expect(arrangement.hidden).toEqual(["signal"]);

    const outsideEditMode = socialRowTiles(catalog, arrangement);
    expect(outsideEditMode.find((tile) => tile.id === "signal")?.hidden).toBe(true);
    const outsideEditMarkup = renderCircularTileRow(outsideEditMode, false);
    // Hidden tiles are left out of the row outside edit mode -- same as the
    // rest of Home's tiles.
    expect(tileIds(outsideEditMarkup)).toHaveLength(3);
    expect(outsideEditMarkup).not.toContain('data-tile-id="signal"');

    const insideEditMarkup = renderCircularTileRow(outsideEditMode, true);
    // Inside edit mode every tile -- including hidden ones -- stays present
    // so the tune icon can un-hide it again.
    expect(tileIds(insideEditMarkup)).toHaveLength(4);
    expect(insideEditMarkup).toContain('data-tile-id="signal"');
    expect(insideEditMarkup).toContain('class="circular-tile tile-hidden"');
    expect(insideEditMarkup).toContain('data-tile-toggle="signal"');
  });
});
