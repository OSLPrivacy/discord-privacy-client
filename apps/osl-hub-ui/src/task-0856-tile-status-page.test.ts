import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { loadNativeApps, nativeAppGeneratedLabel, type NativeApp } from "./services";
import { futurePromisesIn, tileStatusRouteFor } from "./tile-status-route";
import {
  TILE_STATUS_BLOCKER_TEXT,
  TILE_STATUS_CAPABILITY_IDS,
  TILE_STATUS_CAPABILITY_LABELS,
  TILE_STATUS_CAPABILITY_TEXT,
  tileStatusCapabilityFacts,
  tileStatusCapabilityLabel,
  tileStatusPageFor,
  tileStatusPageMarkup,
  tileStatusPagePromises,
  tileStatusPages,
  type TileStatusCapabilityFacts,
} from "./tile-status-page";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styleSource = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

/** TASK 0804's own table, retyped here so this is a check and not a mirror. */
const TASK_0804_LADDER: readonly [string, TileStatusCapabilityFacts, string][] = [
  ["ready", { placing: true, reading: true, opening: true, protectedMessaging: true }, "Ready"],
  ["placing_only", { placing: true, reading: false, opening: true, protectedMessaging: false }, "Placing only"],
  ["reading_only", { placing: false, reading: true, opening: true, protectedMessaging: false }, "Reading only"],
  ["opens_the_app", { placing: false, reading: false, opening: true, protectedMessaging: false }, "Opens the app"],
  ["not_started", { placing: false, reading: false, opening: false, protectedMessaging: false }, "Not started"],
];

function telegram(catalog: readonly NativeApp[]): NativeApp {
  const app = catalog.find((candidate) => candidate.id === "telegram");
  expect(app, "the catalog should carry Telegram").toBeDefined();
  return app as NativeApp;
}

describe("TASK 0856 honest tile status page", () => {
  it("generates the capability label on the TASK 0804 ladder", () => {
    for (const [name, facts, expected] of TASK_0804_LADDER) {
      const actual = tileStatusCapabilityLabel(facts);
      console.log(
        `task_0856_ladder case=${name} placing=${facts.placing} reading=${facts.reading} `
        + `opening=${facts.opening} protected_messaging=${facts.protectedMessaging} label=${actual}`,
      );
      expect(actual, `${name} generated the wrong capability label`).toBe(expected);
    }
    expect(new Set(TASK_0804_LADDER.map(([, , label]) => label))).toEqual(new Set(TILE_STATUS_CAPABILITY_LABELS));
  });

  it("finds exactly one placing-only connected app, and it is the one with the carry receipt", async () => {
    const catalog = await loadNativeApps();
    const pages = tileStatusPages(catalog);
    expect(pages.length).toBe(catalog.length);
    const byLabel = new Map<string, string[]>();
    for (const page of pages) {
      const app = catalog.find((candidate) => candidate.id === page.tileId) as NativeApp;
      console.log(
        `task_0856_page ${page.tileId} capability_label="${page.capabilityLabel}" claim_label="${page.claimLabel}" `
        + `placing=${page.facts.placing} reading=${page.facts.reading} opening=${page.facts.opening} `
        + `protected_messaging=${page.facts.protectedMessaging} can=${page.can.length} limits=${page.limits.length} `
        + `carrier=${app.carrierEvidence} delivery=${app.deliveryEvidence} next_action="${page.nextAction.label}" back="${page.back.label}"`,
      );
      byLabel.set(page.capabilityLabel, [...(byLabel.get(page.capabilityLabel) ?? []), page.tileId]);
    }
    const placingOnly = byLabel.get("Placing only") ?? [];
    console.log(`task_0856_capability_label_counts=${[...byLabel].map(([label, ids]) => `${label}:${ids.length}`).join("|")}`);
    console.log(`task_0856_placing_only_apps=${placingOnly.join(",") || "none"}`);

    expect(placingOnly).toEqual(["telegram"]);
    // Placing-only because of the receipt, not because it was typed that way.
    const app = telegram(catalog);
    expect(app.carrierEvidence).toBe("provenLiveWithReceipt");
    expect(app.deliveryEvidence).toBe("neverProvenLive");
    expect(catalog.filter((candidate) => candidate.carrierEvidence === "provenLiveWithReceipt").map((c) => c.id))
      .toEqual(["telegram"]);
    // Nothing here reaches "Ready", which is what claim_state.rs and TASK 0808 both say.
    expect(byLabel.get("Ready")).toBeUndefined();
  });

  it("states the real capability, the plain limits, one useful action and Back to Home", async () => {
    const catalog = await loadNativeApps();
    const app = telegram(catalog);
    const page = tileStatusPageFor(app);
    const route = tileStatusRouteFor(app);

    // 1. the real capability -- generated, byte for byte off the catalog.
    expect(page.capabilityLabel).toBe("Placing only");
    expect(page.capability).toBe(app.statusPage.capability);
    expect(page.explanation).toBe(app.statusPage.explanation);
    expect(page.explanation).toBe(app.claimNote);
    expect(page.claimLabel).toBe(nativeAppGeneratedLabel(app.supportStatus));

    // 2. plain limits -- one per capability OSL does not hold here.
    const heldIds = page.can.map((row) => row.id);
    const limitIds = page.limits.map((row) => row.id);
    expect(heldIds).toEqual(["placing", "opening"]);
    expect(limitIds).toEqual(["reading", "protectedMessaging"]);
    expect(page.can.length + page.limits.length).toBe(TILE_STATUS_CAPABILITY_IDS.length + app.claimBlockers.length);
    for (const limit of page.limits) {
      expect(limit.text.length, `${limit.id} limit is empty`).toBeGreaterThan(20);
    }

    // 3. one useful action, reached by a handler main.ts already binds.
    expect(page.nextAction).toEqual(route.nextAction);
    expect(page.nextAction.label).toBe("Open Telegram in a separate OSL profile");
    expect(page.nextAction.handler).toBe("data-home-app");
    expect(page.nextAction.handlerValue).toBe("telegram");
    expect(mainSource).toContain(`querySelectorAll<HTMLButtonElement>("[data-home-app]")`);
    expect(mainSource).toContain(`querySelectorAll<HTMLButtonElement>("[data-route]")`);

    // 4. Back to Home -- the action TASK 0853 carried and never drew.
    expect(page.back.label).toBe("Back to Home");
    expect(page.back.target).toBe("home");
    expect(page.back.handler).toBe("data-route");
    expect(page.back.handlerValue).toBe("home");

    console.log(`task_0856_capability="${page.capabilityLabel}" claim="${page.claimLabel}"`);
    console.log(`task_0856_capability_sentence="${page.capability}"`);
    for (const row of page.can) console.log(`task_0856_can ${row.id}: ${row.text}`);
    for (const row of page.limits) console.log(`task_0856_limit ${row.id}: ${row.text}`);
    console.log(`task_0856_next_action="${page.nextAction.label}" handler=${page.nextAction.handler}=${page.nextAction.handlerValue}`);
    console.log(`task_0856_back="${page.back.label}" handler=${page.back.handler}=${page.back.handlerValue}`);
  });

  it("puts no future promise on any page, nor in the strings it is allowed to write", async () => {
    const catalog = await loadNativeApps();
    const offenders: string[] = [];
    for (const page of tileStatusPages(catalog)) {
      for (const promise of tileStatusPagePromises(page)) {
        offenders.push(`${page.tileId} ${promise.field} promises "${promise.phrase}" in "${promise.text}"`);
      }
    }
    const authored = [
      ...Object.values(TILE_STATUS_CAPABILITY_TEXT).flatMap((entry) => [entry.name, entry.held, entry.missing]),
      ...Object.values(TILE_STATUS_BLOCKER_TEXT),
    ];
    const authoredOffenders = authored.flatMap((text) => futurePromisesIn(text).map((phrase) => `"${phrase}" in "${text}"`));
    console.log(`task_0856_authored_strings=${authored.length}`);
    console.log(`task_0856_pages_scanned=${catalog.length}`);
    console.log(`task_0856_page_future_promises=${offenders.length}`);
    console.log(`task_0856_authored_future_promises=${authoredOffenders.length}`);
    expect(offenders, offenders.join("; ")).toEqual([]);
    expect(authoredOffenders, authoredOffenders.join("; ")).toEqual([]);
    // A scan of nothing finds nothing.
    expect(authored.length).toBe(TILE_STATUS_CAPABILITY_IDS.length * 3 + Object.keys(TILE_STATUS_BLOCKER_TEXT).length);
  });

  it("renders every part of the page into markup, and paints every class it writes", async () => {
    const catalog = await loadNativeApps();
    const app = telegram(catalog);
    const page = tileStatusPageFor(app);
    const markup = tileStatusPageMarkup(app);

    for (const needle of [
      `data-tile-status-page="telegram"`,
      `data-tile-status-capability-label="Placing only"`,
      `>Placing only<`,
      `>${page.claimLabel}<`,
      page.capability,
      "What OSL can do here",
      "What OSL cannot do here",
      `data-tile-status-limit-count="2"`,
      `id="embedded-service-setup"`,
      `>Open Telegram in a separate OSL profile<`,
      `id="native-app-back"`,
      `>Back to Home<`,
      `data-route="connections"`,
    ]) {
      expect(markup, `markup is missing ${needle}`).toContain(needle);
    }

    // Every control on the page is one main.ts already binds. The next action's
    // declared handler is data-home-app; on this route the control that
    // performs it is #embedded-service-setup, so both are recorded.
    expect(markup).toContain(`data-tile-status-handler="data-home-app" id="embedded-service-setup"`);
    for (const binding of [
      `document.querySelector<HTMLButtonElement>("#embedded-service-setup")?.addEventListener`,
      `document.querySelector("#native-app-back")?.addEventListener`,
    ]) {
      expect(mainSource, `main.ts does not bind ${binding}`).toContain(binding);
    }
    // #native-app-back has always ended on Home; this page is the first thing
    // to say so on its face.
    const backHandler = mainSource.slice(mainSource.indexOf(`document.querySelector("#native-app-back")`));
    expect(backHandler.slice(0, 400)).toContain(`route = "home"`);

    const classes = new Set([...markup.matchAll(/class="([^"]+)"/gu)].flatMap((match) => match[1].split(/\s+/u)).filter(Boolean));
    const unpainted = [...classes].filter((name) => !styleSource.includes(`.${name}`));
    console.log(`task_0856_markup_bytes=${markup.length}`);
    console.log(`task_0856_markup_classes=${classes.size}`);
    console.log(`task_0856_unpainted_classes=${unpainted.length}${unpainted.length ? ` (${unpainted.join(",")})` : ""}`);
    expect(unpainted, unpainted.join(",")).toEqual([]);

    // The page is what the service route renders. A page nobody can reach is
    // decoration, so the wiring is checked against main.ts's own source.
    expect(mainSource).toContain(`import { tileStatusPageMarkup } from "./tile-status-page"`);
    expect(mainSource).toContain("return tileStatusPageMarkup(claimedApp, {");
    console.log(`task_0856_service_route_wired=1`);
  });

  it("goes red when the page stops being honest", async () => {
    const catalog = await loadNativeApps();
    const app = telegram(catalog);

    // Negative control 1: a surface with a receipt AND proven delivery would be
    // "Ready", so "Placing only" is being derived and not printed.
    const delivered: NativeApp = { ...app, deliveryEvidence: "provenLiveBothWays" };
    expect(tileStatusCapabilityLabel(tileStatusCapabilityFacts(delivered))).toBe("Ready");
    // ... and one without the receipt falls off the placing rung entirely.
    const noReceipt: NativeApp = { ...app, carrierEvidence: "builtNeverProvenLive" };
    expect(tileStatusCapabilityLabel(tileStatusCapabilityFacts(noReceipt))).toBe("Opens the app");

    // Negative control 2: a hand-written promise anywhere on the page is found.
    const promised: NativeApp = {
      ...app,
      statusPage: { ...app.statusPage, capability: "full protection for Telegram is coming soon" },
    };
    const found = tileStatusPagePromises(tileStatusPageFor(promised));
    console.log(`task_0856_negative_control ready_when_delivered=Ready no_receipt=Opens the app promises_found=${found.length}`);
    console.log(`task_0856_negative_control_phrases=${found.map((entry) => entry.phrase).join("|")}`);
    expect(found.length).toBeGreaterThan(0);
    expect(found.map((entry) => entry.phrase)).toContain("coming soon");
  });
});
