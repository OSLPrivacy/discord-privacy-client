#!/usr/bin/env node
/**
 * TASK 6818 — drive the shipped place menu, and render the leaver's surface.
 *
 * `menu`  reads the engine's model report, renders the real sidebar markup for
 *         each point, activates the leave item for each planned step *out of
 *         that markup*, and writes the activation bundle the engine will act
 *         on. A step whose item is missing or disabled produces no activation.
 *
 * `after` reads the departure report and renders the leaver's post-departure
 *         surface from it, so the check grades real markup rather than a claim
 *         that the place is gone.
 */

import { readFileSync, writeFileSync } from "node:fs";

import {
  activateLeave,
  leaverViewMarkup,
  placeListFromMarkup,
  placeSidebarMarkup,
  type LeaverViewModel,
  type SidebarModel,
} from "../src/place-departure-6818";

interface LeaveStepPlan {
  step: string;
  place: string;
  delete_local_history: boolean;
  model_point: string;
}

interface EngineReport {
  task: number;
  leaver: string;
  models: SidebarModel[];
  leave_plan: LeaveStepPlan[];
  leaver_view: LeaverViewModel;
}

const [mode, input, output] = process.argv.slice(2);
if (!mode || !input || !output) {
  throw new Error("TASK6818 render: usage: render-place-departure-6818 <menu|after> <in> <out>");
}

const report = JSON.parse(readFileSync(input, "utf8")) as EngineReport;

/** Mutations the proof applies to show the check can go red. */
const mutation = process.env.OSL6818_RENDER_MUTATION ?? "";

if (mode === "menu") {
  const markupByPoint = new Map<string, string>();
  for (const model of report.models) {
    let rendered = placeSidebarMarkup(model);
    if (mutation === "starve-menu") {
      // Drop every leave item from the rendered rail.
      rendered = rendered.replace(
        /<button class="place-menu-item place-menu-item--leave"[\s\S]*?<\/button>/g,
        "",
      );
    }
    if (mutation === "starve-delete-copy") {
      rendered = rendered.replace(
        /<button class="place-menu-item place-menu-item--delete-copy"[\s\S]*?<\/button>/g,
        "",
      );
    }
    if (mutation === "enable-blocked-item") {
      rendered = rendered.replace(/data-leave-enabled="false"/g, 'data-leave-enabled="true"');
    }
    markupByPoint.set(model.point, rendered);
  }

  const activations = [];
  const skipped: Array<{ step: string; reason: string }> = [];
  for (const plan of report.leave_plan) {
    const markup = markupByPoint.get(plan.model_point);
    if (!markup) {
      skipped.push({ step: plan.step, reason: `no rendered menu for point ${plan.model_point}` });
      continue;
    }
    const result = activateLeave(markup, plan.place, {
      deleteLocalCopy: plan.delete_local_history,
    });
    if (!result.ok) {
      skipped.push({ step: plan.step, reason: result.reason });
      continue;
    }
    activations.push({
      step: plan.step,
      place: result.activation.place,
      menu_action: result.activation.menu_action,
      actor: report.leaver,
      delete_local_history: result.activation.delete_local_history,
      rendered_enabled: result.activation.rendered_enabled,
      rendered_label: result.activation.rendered_label,
    });
  }

  writeFileSync(
    output,
    JSON.stringify(
      {
        task: 6818,
        source: "apps/osl-hub-ui/src/place-departure-6818.ts",
        activations,
        skipped,
        markup: Object.fromEntries(markupByPoint),
      },
      null,
      2,
    ),
    "utf8",
  );
  console.log(
    `TASK6818 rendered ${markupByPoint.size} menu points, ${activations.length} activations, ${skipped.length} skipped -> ${output}`,
  );
} else if (mode === "after") {
  let markup = leaverViewMarkup(report.leaver_view);
  if (mutation === "keep-departed-place-in-list") {
    const first = report.leaver_view.departed[0];
    markup = markup.replace(
      "<ul class=\"place-rail-list\" data-place-list>",
      `<ul class="place-rail-list" data-place-list><li class="place-row" data-place-row="${first.handle}">${first.handle}</li>`,
    );
  }
  if (mutation === "claim-history-deleted") {
    markup = markup.replace(
      /It did not delete the messages you already had[^<]*/g,
      "Your copy of the messages was deleted from this device.",
    );
  }
  writeFileSync(
    output,
    JSON.stringify(
      {
        task: 6818,
        leaver: report.leaver_view.leaver,
        markup,
        place_list_in_markup: placeListFromMarkup(markup),
      },
      null,
      2,
    ),
    "utf8",
  );
  console.log(`TASK6818 rendered the leaver surface -> ${output}`);
} else {
  throw new Error(`TASK6818 render: unknown mode ${mode}`);
}
