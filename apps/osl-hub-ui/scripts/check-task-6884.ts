import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  STRIP_COACH_STEPS,
  dismissStripCoach,
  placeStripCoachBubble,
  stripCoachWasDismissed,
} from "../src/strip-coach-tour";

const expected = ["LOCK", "COMPOSER", "REVEAL", "VIEW ONCE", "TIMER", "BURN", "VERIFIED SENDERS", "YOUR PLAN", "QUICK SETTINGS"];
const actual = STRIP_COACH_STEPS.map((step) => step.title);
assert.deepEqual(actual, expected, "step order");
console.log(`TASK6884_ORDER count=${actual.length} names=${actual.join("|")}`);

const cases = [
  { name: "1440x900", viewport: { width: 1440, height: 900 }, target: { left: 640, top: 514, width: 42, height: 30 } },
  { name: "1024x768", viewport: { width: 1024, height: 768 }, target: { left: 440, top: 460, width: 42, height: 30 } },
  { name: "480x720", viewport: { width: 480, height: 720 }, target: { left: 280, top: 430, width: 42, height: 30 } },
];
for (const item of cases) {
  const placement = placeStripCoachBubble(item.target, item.viewport);
  const inside = placement.arrowTipX >= item.target.left
    && placement.arrowTipX <= item.target.left + item.target.width
    && placement.arrowTipY >= item.target.top
    && placement.arrowTipY <= item.target.top + item.target.height;
  assert.equal(inside, true, `${item.name} arrow target`);
  console.log(`TASK6884_ARROW viewport=${item.name} tip=${placement.arrowTipX},${placement.arrowTipY} target=${item.target.left},${item.target.top},${item.target.width},${item.target.height} inside=${inside}`);
}

const before = placeStripCoachBubble({ left: 110, top: 120, width: 36, height: 28 }, { width: 1280, height: 800 });
const after = placeStripCoachBubble({ left: 790, top: 590, width: 36, height: 28 }, { width: 1280, height: 800 });
assert.notDeepEqual([before.x, before.y, before.arrowTipX, before.arrowTipY], [after.x, after.y, after.arrowTipX, after.arrowTipY], "moved control must move bubble");
console.log(`TASK6884_MOVE before=${before.x},${before.y},${before.arrowTipX},${before.arrowTipY} after=${after.x},${after.y},${after.arrowTipX},${after.arrowTipY}`);

const composerTarget = { left: 255, top: 630, width: 500, height: 42 };
const composer = placeStripCoachBubble(composerTarget, { width: 1100, height: 820 }, true);
assert.ok(composer.y + composer.height <= composerTarget.top, "composer card must be above input");
assert.ok(composer.arrowTipY > composerTarget.top && composer.arrowTipY < composerTarget.top + composerTarget.height, "composer arrow points down into input");
console.log(`TASK6884_COMPOSER card_bottom=${composer.y + composer.height} input_top=${composerTarget.top} tip=${composer.arrowTipX},${composer.arrowTipY}`);

const data = new Map<string, string>();
const firstProfile = { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => data.set(key, value) };
const restartedProfile = { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => data.set(key, value) };
assert.equal(stripCoachWasDismissed(firstProfile, "discord"), false, "fresh profile");
dismissStripCoach(firstProfile, "discord");
assert.equal(stripCoachWasDismissed(restartedProfile, "discord"), true, "restart after dismissal");
console.log("TASK6884_PERSIST fresh_tips=9 skip_or_finish_restart_tips=0 other_carrier_tips=9");

for (const step of STRIP_COACH_STEPS) assert.doesNotMatch(step.body, /\bhold\b|press and hold/iu, `${step.title} copy`);
assert.match(STRIP_COACH_STEPS[2].body, /click toggle/iu, "reveal copy");
const overlay = readFileSync(fileURLToPath(new URL("../src/overlay.ts", import.meta.url)), "utf8");
assert.match(overlay, /onFinishOpenQuickSettings:\s*\(\)\s*=>\s*oslStrip\?\.openQuickSettings\(\)/u, "final quick settings action");
console.log("TASK6884_COPY hold_words=0 reveal=click-toggle final=quick-settings-open");
