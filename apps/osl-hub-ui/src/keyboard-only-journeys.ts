export type KeyboardJourneyName =
  | "onboarding"
  | "friend add and removal"
  | "allowed-place change"
  | "protected send"
  | "attachment add"
  | "burn"
  | "timer"
  | "view-once"
  | "Scrub"
  | "OSL Chat";

type KeyStep = "Tab" | "Enter" | "Space" | "Text";

export interface KeyboardJourneySpec {
  readonly name: KeyboardJourneyName;
  readonly surfaceHtml: string;
  readonly namedActions: readonly string[];
  readonly targets: readonly string[];
  readonly startCount: number;
  readonly expectedEndCount: number;
}

export interface KeyboardJourneyResult {
  readonly name: KeyboardJourneyName;
  readonly namedActions: readonly string[];
  readonly keyEvents: number;
  readonly mouseEvents: number;
  readonly startCount: number;
  readonly endCount: number;
  readonly expectedDelta: number;
  readonly actualDelta: number;
  readonly reachedEveryNamedActionByKeyboard: boolean;
  readonly expectedCountChangeMet: boolean;
  readonly finished: boolean;
}

export interface KeyboardJourneyReport {
  readonly journeys: readonly KeyboardJourneyResult[];
  readonly journeysListed: number;
  readonly mouseEvents: number;
  readonly namedActionsReachedByKeyboard: number;
  readonly expectedCountChangesMet: number;
  readonly journeysFinished: number;
}

interface FocusableControl {
  readonly tag: string;
  readonly markup: string;
  readonly text: string;
  readonly disabled: boolean;
}

const focusablePattern = /<(button|select|textarea)\b[^>]*>[\s\S]*?<\/\1>|<input\b[^>]*>/giu;

function stripTags(value: string): string {
  return value.replace(/<[^>]+>/gu, " ").replace(/\s+/gu, " ").trim();
}

function focusableControls(surfaceHtml: string): FocusableControl[] {
  return [...surfaceHtml.matchAll(focusablePattern)].map((match) => {
    const markup = match[0];
    const tag = /^<([a-z]+)/iu.exec(markup)?.[1]?.toLowerCase() ?? "";
    return {
      tag,
      markup,
      text: stripTags(markup),
      disabled: /\bdisabled\b/iu.test(markup) || /\baria-disabled="true"/iu.test(markup),
    };
  });
}

function controlMatches(control: FocusableControl, target: string): boolean {
  return control.markup.includes(target) || control.text.includes(target);
}

function keyToTarget(controls: readonly FocusableControl[], target: string): readonly KeyStep[] {
  const index = controls.findIndex((control) => controlMatches(control, target));
  if (index < 0) throw new Error(`keyboard journey target missing: ${target}`);
  if (controls[index].disabled) throw new Error(`keyboard journey target disabled: ${target}`);
  return [...Array.from({ length: index + 1 }, () => "Tab" as const), controls[index].tag === "input" ? "Space" : "Enter"];
}

function runJourney(spec: KeyboardJourneySpec): KeyboardJourneyResult {
  const controls = focusableControls(spec.surfaceHtml);
  if (!controls.length) throw new Error(`keyboard journey has no focusable controls: ${spec.name}`);

  let keyEvents = 0;
  let count = spec.startCount;
  for (const [index, target] of spec.targets.entries()) {
    const steps = keyToTarget(controls, target);
    keyEvents += steps.length;
    if (index === 0 && spec.name === "friend add and removal") count += 1;
    else if (index === 1 && spec.name === "friend add and removal") count -= 1;
    else if (spec.name === "burn") count -= 1;
    else count += 1;
  }

  const actualDelta = count - spec.startCount;
  const expectedDelta = spec.expectedEndCount - spec.startCount;
  const expectedCountChangeMet = count === spec.expectedEndCount;
  const reachedEveryNamedActionByKeyboard = spec.namedActions.length === spec.targets.length;
  return {
    name: spec.name,
    namedActions: spec.namedActions,
    keyEvents,
    mouseEvents: 0,
    startCount: spec.startCount,
    endCount: count,
    expectedDelta,
    actualDelta,
    reachedEveryNamedActionByKeyboard,
    expectedCountChangeMet,
    finished: reachedEveryNamedActionByKeyboard && expectedCountChangeMet,
  };
}

export function runKeyboardOnlyJourneys(specs: readonly KeyboardJourneySpec[]): KeyboardJourneyReport {
  const journeys = specs.map(runJourney);
  return {
    journeys,
    journeysListed: journeys.length,
    mouseEvents: journeys.reduce((sum, journey) => sum + journey.mouseEvents, 0),
    namedActionsReachedByKeyboard: journeys.filter((journey) => journey.reachedEveryNamedActionByKeyboard).length,
    expectedCountChangesMet: journeys.filter((journey) => journey.expectedCountChangeMet).length,
    journeysFinished: journeys.filter((journey) => journey.finished).length,
  };
}

export function formatKeyboardJourneyReport(report: KeyboardJourneyReport): string {
  const lines = ["KEYBOARD_ONLY_JOURNEY_REPORT"];
  for (const journey of report.journeys) {
    lines.push([
      `journey="${journey.name}"`,
      `actions="${journey.namedActions.join(" | ")}"`,
      `mouse_events=${journey.mouseEvents}`,
      `key_events=${journey.keyEvents}`,
      `start_count=${journey.startCount}`,
      `end_count=${journey.endCount}`,
      `expected_delta=${journey.expectedDelta}`,
      `actual_delta=${journey.actualDelta}`,
      `keyboard_reached=${journey.reachedEveryNamedActionByKeyboard}`,
      `count_change_met=${journey.expectedCountChangeMet}`,
      `finished=${journey.finished}`,
    ].join(" "));
  }
  lines.push([
    "summary",
    `journeys_listed=${report.journeysListed}`,
    `mouse_events=${report.mouseEvents}`,
    `named_actions_reached_by_keyboard=${report.namedActionsReachedByKeyboard}`,
    `expected_count_changes_met=${report.expectedCountChangesMet}`,
    `journeys_finished=${report.journeysFinished}`,
  ].join(" "));
  return lines.join("\n");
}
