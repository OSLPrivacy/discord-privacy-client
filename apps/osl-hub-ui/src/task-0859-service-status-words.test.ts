/**
 * TASK 0859 — run the banned-word, plain-English and forbidden-future-label
 * check on the Service status page.
 *
 * The page under test is the shipped one: `tileStatusPageMarkup` from TASK 0856,
 * rendered from the real catalog (`loadNativeApps`), not from a fixture typed
 * for this test. The finish line names one page — "the status page" — and the
 * page TASK 0856 built and photographed is the placing-only one, so that is the
 * page the finish line is read against. Every other service in the catalog is
 * rendered and reported too, so a service that quietly goes wrong cannot hide
 * behind the one under test.
 */

import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { loadNativeApps, type NativeApp } from "./services";
import { FUTURE_PROMISE_PHRASES } from "./tile-status-route";
import {
  TILE_STATUS_PICTURE_CARRIAGE_PROVEN,
  TILE_STATUS_RULED_ON,
  TILE_STATUS_RULING_FILE,
  tileStatusCapabilityLabel,
  tileStatusCapabilityFacts,
  tileStatusPageFor,
  tileStatusPageMarkup,
} from "./tile-status-page";
import {
  SERVICE_STATUS_LEAST_WORDS,
  SERVICE_STATUS_REQUIRED_WORDS,
  SERVICE_STATUS_TITLE,
  checkServiceStatusWords,
  findFutureLabels,
} from "./service-status-words";

const REPO = path.join(import.meta.dirname, "..", "..", "..");
const DESIGN_FEEL_DOC = path.join(REPO, "docs", "design", "osl-subjective-design-feel.md");
const RECEIPTS = path.join(REPO, "apps", "osl-hub", "carry-receipts");

/**
 * The banned concepts the shipped product contract declares, read off the doc
 * the Rust design-feel test reads. Copying the list in here would let the two
 * drift apart silently, and this check would then pass on a screen the contract
 * bans.
 */
function contractBannedConcepts(): string[] {
  const doc = readFileSync(DESIGN_FEEL_DOC, "utf8");
  const fence = /```json\s*([\s\S]*?)```/u.exec(doc);
  if (fence === null) throw new Error("no JSON fixture block in the design-feel doc");
  const banned = (JSON.parse(fence[1]) as { banned_user_facing_concepts?: string[] }).banned_user_facing_concepts ?? [];
  if (banned.length === 0) throw new Error("design-feel doc declares no banned concepts");
  return banned;
}

/** The one paragraph on the page the backend generated, not the UI. */
function generatedText(app: NativeApp): string[] {
  return [app.statusPage.explanation];
}

function check(app: NativeApp, markup = tileStatusPageMarkup(app)) {
  return checkServiceStatusWords(markup, {
    generatedText: generatedText(app),
    bannedWords: contractBannedConcepts(),
  });
}

/** The placing-only service, found by its label rather than named by hand. */
function placingOnly(catalog: readonly NativeApp[]): NativeApp {
  const placing = catalog.filter((app) => tileStatusCapabilityLabel(tileStatusCapabilityFacts(app)) === "Placing only");
  expect(placing.length, "exactly one service should be placing-only").toBe(1);
  return placing[0];
}

describe("TASK 0859 Service status page words", () => {
  it("reads the status page: right title, enough words, every named word, zero banned words, zero future labels", async () => {
    const catalog = await loadNativeApps();
    const app = placingOnly(catalog);
    const report = check(app);

    console.log(
      `TASK 0859 report: service=${app.id} title="${report.title}" words=${report.wordCount}`
      + ` present=${report.present.length}/${SERVICE_STATUS_REQUIRED_WORDS.length}`
      + ` missing=${JSON.stringify(report.missing)}`
      + ` banned=${report.banned.length} ${JSON.stringify(report.banned)}`
      + ` future=${report.future.length} ${JSON.stringify(report.future.map((hit) => hit.phrase))}`
      + ` bannedTermsChecked=${report.bannedTermsChecked} futurePhrasesChecked=${report.futurePhrasesChecked}`,
    );

    expect(report.title).toBe(SERVICE_STATUS_TITLE);
    expect(report.titleMatches).toBe(true);
    expect(report.wordCount).toBeGreaterThanOrEqual(SERVICE_STATUS_LEAST_WORDS);
    expect(report.missing).toEqual([]);
    expect(report.present).toEqual([...SERVICE_STATUS_REQUIRED_WORDS]);
    expect(report.banned).toEqual([]);
    expect(report.future).toEqual([]);
    expect(report.ok).toBe(true);
  });

  it("reports every other service in the catalog, page words and authored words apart", async () => {
    const catalog = await loadNativeApps();
    let authoredBanned = 0;
    let authoredFuture = 0;
    let pagesWithGeneratedBanned = 0;
    for (const app of catalog) {
      const report = check(app);
      const page = tileStatusPageFor(app);
      console.log(
        `task_0859_page ${app.id} label="${page.capabilityLabel}" title="${report.title}"`
        + ` words=${report.wordCount} missing=${report.missing.length}`
        + ` page_banned=${report.banned.length}${report.banned.length ? `=${JSON.stringify(report.banned.map((hit) => hit.found))}` : ""}`
        + ` page_future=${report.future.length}${report.future.length ? `=${JSON.stringify(report.future.map((hit) => hit.phrase))}` : ""}`
        + ` authored_words=${report.authored.wordCount}`
        + ` authored_banned=${report.authored.banned.length}`
        + ` authored_future=${report.authored.future.length}`,
      );
      // Every page carries the title, the named words and enough of them. Those
      // are this build's own words on every single service, with no excuse.
      expect(report.title, app.id).toBe(SERVICE_STATUS_TITLE);
      expect(report.wordCount, app.id).toBeGreaterThanOrEqual(SERVICE_STATUS_LEAST_WORDS);
      expect(report.missing, app.id).toEqual([]);
      // Nothing the UI typed may be jargon or a promise, on any service.
      expect(report.authored.banned, app.id).toEqual([]);
      expect(report.authored.future, app.id).toEqual([]);
      authoredBanned += report.authored.banned.length;
      authoredFuture += report.authored.future.length;
      if (report.banned.length > 0) pagesWithGeneratedBanned += 1;
    }
    console.log(
      `task_0859_catalog pages=${catalog.length} authored_banned_total=${authoredBanned}`
      + ` authored_future_total=${authoredFuture} pages_with_generated_banned=${pagesWithGeneratedBanned}`,
    );
    expect(authoredBanned).toBe(0);
    expect(authoredFuture).toBe(0);
  });

  it("fails on a throwaway copy of the screen that is missing one named word", async () => {
    const catalog = await loadNativeApps();
    const app = placingOnly(catalog);
    const markup = tileStatusPageMarkup(app);
    for (const word of SERVICE_STATUS_REQUIRED_WORDS) {
      // A throwaway copy of the screen, one named word cut out of it. The page
      // itself is untouched.
      const mutant = markup.split(word).join("");
      const report = check(app, mutant);
      console.log(`TASK 0859 mutant: dropped "${word}" -> ok=${report.ok} missing=${JSON.stringify(report.missing)} title="${report.title}"`);
      expect(report.ok, word).toBe(false);
      expect(report.missing, word).toContain(word);
    }
  });

  it("fails on a throwaway copy that says a banned word, promises a future, or is too short to read", async () => {
    const catalog = await loadNativeApps();
    const app = placingOnly(catalog);
    const markup = tileStatusPageMarkup(app);

    for (const jargon of ["payload", "provider adapters", "receipts"]) {
      const mutant = markup.replace("</h1>", `</h1><p>${jargon}</p>`);
      const report = check(app, mutant);
      console.log(`TASK 0859 mutant: said "${jargon}" -> ok=${report.ok} banned=${JSON.stringify(report.banned)} authored_banned=${report.authored.banned.length}`);
      expect(report.ok, jargon).toBe(false);
      expect(report.banned.length, jargon).toBeGreaterThan(0);
      expect(report.authored.banned.length, jargon).toBeGreaterThan(0);
    }

    for (const promise of ["Coming soon", "Full support is coming later", "Pictures will be supported"]) {
      const mutant = markup.replace("</h1>", `</h1><p>${promise}</p>`);
      const report = check(app, mutant);
      console.log(`TASK 0859 mutant: promised "${promise}" -> ok=${report.ok} future=${JSON.stringify(report.future.map((hit) => hit.phrase))}`);
      expect(report.ok, promise).toBe(false);
      expect(report.future.length, promise).toBeGreaterThan(0);
      expect(report.authored.future.length, promise).toBeGreaterThan(0);
    }

    const short = `<main><h1>${SERVICE_STATUS_TITLE}</h1><p>Messages Friends Pictures Current status Last updated</p></main>`;
    const shortReport = check(app, short);
    console.log(`TASK 0859 mutant: short screen -> words=${shortReport.wordCount} ok=${shortReport.ok} enoughWords=${shortReport.enoughWords}`);
    expect(shortReport.enoughWords).toBe(false);
    expect(shortReport.ok).toBe(false);
    expect(shortReport.missing).toEqual([]);
  });

  it("takes Last updated from the ruling file, and Pictures from what the carry records actually hold", async () => {
    const catalog = await loadNativeApps();

    const ruling = JSON.parse(readFileSync(path.join(REPO, TILE_STATUS_RULING_FILE), "utf8")) as { ruled_on?: string };
    console.log(`task_0859_last_updated file=${TILE_STATUS_RULING_FILE} ruled_on=${ruling.ruled_on} page_constant=${TILE_STATUS_RULED_ON}`);
    expect(ruling.ruled_on).toBe(TILE_STATUS_RULED_ON);
    const markup = tileStatusPageMarkup(placingOnly(catalog));
    expect(markup).toContain("Last updated 5 August 2026");

    const records = readdirSync(RECEIPTS).filter((name) => name.endsWith(".json") && !name.includes("baseline"));
    const withPictures = records.filter((name) => {
      const body = JSON.parse(readFileSync(path.join(RECEIPTS, name), "utf8")) as Record<string, unknown>;
      return Object.keys(body).some((key) => /picture|image|attachment|photo|file/iu.test(key));
    });
    console.log(
      `task_0859_pictures carry_records=${records.length} ${JSON.stringify(records)}`
      + ` records_with_a_picture=${withPictures.length} picture_carriage_proven=${TILE_STATUS_PICTURE_CARRIAGE_PROVEN}`,
    );
    expect(records.length).toBeGreaterThan(0);
    expect(withPictures).toEqual([]);
    expect(TILE_STATUS_PICTURE_CARRIAGE_PROVEN).toBe(withPictures.length > 0);

    for (const app of catalog) {
      const pictures = tileStatusPageFor(app).carried.find((thing) => thing.id === "pictures");
      expect(pictures?.carried, app.id).toBe(false);
      expect(pictures?.state, app.id).toBe("Not carried");
    }
  });

  it("checks the words it says it checks", () => {
    const banned = contractBannedConcepts();
    console.log(
      `task_0859_vocabulary contract_banned=${banned.length} ${JSON.stringify(banned)}`
      + ` future_phrases=${FUTURE_PROMISE_PHRASES.length}`,
    );
    expect(banned).toContain("receipts");
    expect(banned).toContain("provider adapters");
    expect(FUTURE_PROMISE_PHRASES).toContain("coming soon");
    expect(FUTURE_PROMISE_PHRASES).toContain("coming later");
    // The future scan reads visible words only, exactly like the banned scan.
    expect(findFutureLabels(`<p data-x="coming soon">Current status</p>`)).toEqual([]);
    expect(findFutureLabels(`<p>Coming soon</p>`).map((hit) => hit.phrase)).toContain("coming soon");
  });
});
