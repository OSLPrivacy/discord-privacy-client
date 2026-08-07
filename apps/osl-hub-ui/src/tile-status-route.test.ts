import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { loadNativeApps, nativeAppGeneratedLabel, type NativeApp } from "./services";
import {
  auditTileStatusRoute,
  auditTileStatusRoutes,
  futurePromisesIn,
  tileStatusRouteFor,
  tileStatusRoutes,
  FUTURE_PROMISE_PHRASES,
  TILE_STATUS_AUTHORED_TEXT,
} from "./tile-status-route";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

/** The routes `[data-route]` can navigate to, read off main.ts's own union. */
function shippedRoutes(): string[] {
  const union = mainSource.match(/export type Route =([^;]+);/u);
  expect(union, "main.ts should declare its Route union").not.toBeNull();
  return [...(union?.[1] ?? "").matchAll(/"([^"]+)"/gu)].map((match) => match[1]);
}

describe("TASK 0853 honest tile status routes", () => {
  it("gives every tile route data with no hand-written future promise", async () => {
    const catalog = await loadNativeApps();
    const routes = tileStatusRoutes(catalog);
    expect(routes.length, "every tile in the catalog needs a status route").toBe(catalog.length);
    console.log(`task_0853_tile_status_route_count=${routes.length}`);

    const audits = auditTileStatusRoutes(catalog);
    let handWrittenFieldCount = 0;
    const offenders: string[] = [];
    for (const audit of audits) {
      handWrittenFieldCount += audit.handWrittenFields.length;
      const route = routes.find((candidate) => candidate.tileId === audit.tileId);
      console.log(
        `task_0853_route ${audit.tileId} route="${route?.route}" label="${route?.generatedLabel}" `
        + `label_is_generated=${audit.labelIsGenerated} generated_fields=${audit.generatedFields.length} `
        + `hand_written_fields=${audit.handWrittenFields.length} `
        + `hand_written_promises=${audit.handWrittenPromises.length} `
        + `generated_promises=${audit.generatedPromises.map((entry) => entry.phrase).join("|") || "none"} `
        + `next_action="${route?.nextAction.label}" next_target=${route?.nextAction.target}`,
      );
      for (const promise of audit.handWrittenPromises) {
        offenders.push(`${audit.tileId} ${promise.field} promises "${promise.phrase}" in "${promise.text}"`);
      }
    }
    console.log(`task_0853_hand_written_fields_scanned=${handWrittenFieldCount}`);
    console.log(`task_0853_hand_written_future_promises=${offenders.length}`);

    // The bar: nothing this module wrote promises a future. A route whose
    // label stopped agreeing with the status that generates it is counted as
    // hand-written first, so "Coming later" cannot hide behind a field name.
    expect(offenders, offenders.join("; ")).toEqual([]);
    // A scan of nothing finds nothing. Every tile has to be putting real
    // hand-written strings in front of this check.
    expect(handWrittenFieldCount).toBe(catalog.length * 3);
    expect(audits.every((audit) => audit.labelIsGenerated)).toBe(true);
  });

  it("copies the explanation and label out of the generated status page, byte for byte", async () => {
    const catalog = await loadNativeApps();
    for (const app of catalog) {
      const route = tileStatusRouteFor(app);
      expect(route.generatedLabel, `${app.id} label`).toBe(nativeAppGeneratedLabel(app.supportStatus));
      expect(route.generatedLabel, `${app.id} label`).toBe(app.statusPage.generatedLabel);
      expect(route.capability, `${app.id} capability`).toBe(app.statusPage.capability);
      expect(route.explanation, `${app.id} explanation`).toBe(app.statusPage.explanation);
      expect(route.explanation, `${app.id} explanation`).toBe(app.claimNote);
    }
  });

  it("points every tile at an action that works today", async () => {
    const catalog = await loadNativeApps();
    const routes = tileStatusRoutes(catalog);
    const known = new Set(shippedRoutes());
    for (const route of routes) {
      for (const action of [route.nextAction, route.evidenceAction, route.back]) {
        expect(known.has(action.target), `${route.tileId} action target ${action.target} is not a shipped route`).toBe(true);
      }
      // The handler is not invented here either: main.ts already binds both.
      expect(mainSource).toContain(`querySelectorAll<HTMLButtonElement>("[${route.nextAction.handler}]")`);
      if (route.nextAction.handler === "data-home-app") {
        expect(route.nextAction.handlerValue).toBe(route.tileId);
      }
    }
    const opens = routes.filter((route) => route.nextAction.handler === "data-home-app").map((route) => route.tileId);
    const chat = routes.filter((route) => route.nextAction.target === "osl-chat").map((route) => route.tileId);
    console.log(`task_0853_next_action_open_profile=${opens.join(",")}`);
    console.log(`task_0853_next_action_osl_chat=${chat.join(",")}`);
    expect(opens.length + chat.length).toBe(routes.length);
  });

  /**
   * TASK 0856 superseded the two-paragraph render this test used to name.
   * `tileStatusRouteMarkup` in main.ts is gone; the service route now returns
   * `tileStatusPageMarkup(claimedApp)` (`tile-status-page.ts`), which is built
   * from this same route data. The contract this test exists for is unchanged
   * — the route shows the route data and never a typed sentence — so it is
   * checked against the module that renders it now.
   */
  it("renders the route data on the service route rather than a typed sentence", () => {
    expect(mainSource).toContain(`import { tileStatusPageMarkup } from "./tile-status-page"`);
    expect(mainSource).toContain("const claimedApp = activeNativeApp();");
    expect(mainSource).toContain("return tileStatusPageMarkup(claimedApp, {");
    expect(mainSource).not.toContain("function tileStatusRouteMarkup");

    const body = readFileSync(new URL("./tile-status-page.ts", import.meta.url), "utf8");
    const markup = body.slice(body.indexOf("export function tileStatusPageMarkup"));
    for (const phrase of FUTURE_PROMISE_PHRASES) {
      expect(futurePromisesIn(markup).includes(phrase), `service-route markup promises "${phrase}"`).toBe(false);
    }
    console.log(`task_0853_service_route_markup_future_promises=${futurePromisesIn(markup).length}`);
  });

  it("goes red when a status route is given a hand-written promise", async () => {
    const [app] = await loadNativeApps();
    const honest = auditTileStatusRoute(app);
    expect(honest.handWrittenPromises).toEqual([]);

    const promised = auditTileStatusRoute(app, {
      ...tileStatusRouteFor(app),
      nextAction: { ...tileStatusRouteFor(app).nextAction, label: "Full protection is coming soon" },
    });
    expect(promised.handWrittenPromises.map((entry) => entry.phrase)).toContain("coming soon");

    // A label typed by hand is not a generated one, so its promise is caught.
    const drifted: NativeApp = { ...app, statusPage: { ...app.statusPage, generatedLabel: "Coming later" } };
    const driftedAudit = auditTileStatusRoute(drifted);
    expect(driftedAudit.labelIsGenerated).toBe(false);
    expect(driftedAudit.handWrittenPromises.map((entry) => entry.phrase)).toContain("coming later");
    console.log(
      `task_0853_negative_control hand_written_promise="${promised.handWrittenPromises[0]?.phrase}" `
      + `drifted_label_promise="${driftedAudit.handWrittenPromises[0]?.phrase}"`,
    );
  });

  it("writes only the four strings it is allowed to write", () => {
    const authored = Object.values(TILE_STATUS_AUTHORED_TEXT);
    expect(authored.length).toBe(4);
    for (const text of authored) {
      expect(futurePromisesIn(text), `authored string "${text}"`).toEqual([]);
    }
    console.log(`task_0853_authored_strings=${authored.length}`);
  });
});
