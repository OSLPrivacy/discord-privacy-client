import type { HomeAppCatalogEntry, HomeAppId } from "./services";
import {
  normalizeHomeTileArrangement,
  type HomeTileArrangement,
} from "./home-tile-arrangement";

/**
 * TASK 5008. Home's SOCIAL and EMAIL rows of circular brand tiles, built
 * from whatever `HomeAppCatalogEntry[]` the caller hands in -- the shipped-
 * services list that freezes at hour 30. No id is hard-coded here: a carrier
 * cut from that list by its failed proof disappears from the row by data
 * change alone, with nothing in this file touched.
 *
 * `HomeTileArrangement` (order/hidden) is the same shape the tune icon's
 * edit mode already reads and writes for the rest of Home
 * (`home-tile-arrangement.ts`), so hiding a tile from edit mode works here
 * without this module knowing anything about that feature.
 */

export type CarrierTileClickAction = {
  kind: "openStrip";
  carrierId: HomeAppId;
};

export interface CircularTile {
  id: HomeAppId;
  displayName: string;
  hidden: boolean;
}

function rowTiles(
  catalog: readonly HomeAppCatalogEntry[],
  section: "social" | "email",
  arrangement: HomeTileArrangement,
): CircularTile[] {
  const entries = catalog.filter((entry) => entry.section === section);
  const byId = new Map(entries.map((entry) => [entry.id, entry]));
  const defaults = entries.map((entry) => entry.id);
  const normalized = normalizeHomeTileArrangement(defaults, arrangement);
  const hidden = new Set(normalized.hidden);
  return normalized.order
    .filter((id) => byId.has(id as HomeAppId))
    .map((id) => ({
      id: id as HomeAppId,
      displayName: byId.get(id as HomeAppId)!.displayName,
      hidden: hidden.has(id),
    }));
}

/** SOCIAL row: one circular tile per connected carrier in the shipped-services list. */
export function socialRowTiles(
  catalog: readonly HomeAppCatalogEntry[],
  arrangement: HomeTileArrangement,
): CircularTile[] {
  return rowTiles(catalog, "social", arrangement);
}

/** EMAIL row: one circular tile per mail service in the shipped-services list. */
export function emailRowTiles(
  catalog: readonly HomeAppCatalogEntry[],
  arrangement: HomeTileArrangement,
): CircularTile[] {
  return rowTiles(catalog, "email", arrangement);
}

function circularTileMarkup(tile: CircularTile, editMode: boolean): string {
  const initial = tile.displayName.slice(0, 1).toLocaleUpperCase();
  const removeControl = editMode
    ? `<button type="button" class="tile-remove" data-tile-toggle="${tile.id}" aria-label="${tile.hidden ? "Show" : "Remove"} ${tile.displayName}">${tile.hidden ? "+" : "−"}</button>`
    : "";
  return `<article class="circular-tile ${tile.hidden ? "tile-hidden" : ""}" data-tile-id="${tile.id}"><button type="button" class="circular-tile-button" data-open-strip="${tile.id}" aria-label="Open ${tile.displayName}"><span class="circular-tile-plate">${initial}</span><span class="circular-tile-label">${tile.displayName}</span></button>${removeControl}</article>`;
}

/**
 * Renders a row of circular tiles. Outside edit mode a hidden tile is left
 * out of the markup entirely, matching how the rest of Home's tune-icon
 * edit mode treats `tile-hidden` ids (`main.ts`'s `renderHomeTile`).
 */
export function renderCircularTileRow(tiles: readonly CircularTile[], editMode: boolean): string {
  return tiles.filter((tile) => !tile.hidden || editMode).map((tile) => circularTileMarkup(tile, editMode)).join("");
}

/** What clicking a carrier tile does: open the Strip over that named carrier. */
export function carrierTileClick(tileId: HomeAppId): CarrierTileClickAction {
  return { kind: "openStrip", carrierId: tileId };
}
