/**
 * A small, deterministic record of the controls that a screen reader can
 * reach during a guided journey.  The record deliberately fails closed: a
 * control without an accessible name, a disabled requested control, a missing
 * target, or a saved change without a live confirmation makes the journey
 * incomplete.
 *
 * Windows Narrator exposes the same accessible names and disabled state that
 * the browser accessibility tree does.  Keeping this dependency-free lets the
 * journey be replayed in the UI test target as well as on a Windows machine.
 */

export interface NarratorStep {
  readonly surfaceHtml: string;
  /** A control name or text which Narrator must be able to find. */
  readonly target: string;
  readonly kind: "control" | "text";
  /** A persisted change must have exactly one status announcement. */
  readonly savedChange?: boolean;
  readonly spokenConfirmation?: string;
}

export interface NarratorJourneySpec {
  readonly name: string;
  readonly steps: readonly NarratorStep[];
}

export interface NarratorJourneyResult {
  readonly name: string;
  readonly completed: boolean;
  readonly blockedControls: number;
  readonly unnamedControls: number;
  readonly savedChanges: number;
  readonly spokenConfirmations: readonly string[];
}

export interface NarratorJourneyReport {
  readonly journeys: readonly NarratorJourneyResult[];
  readonly completedJourneys: number;
  readonly blockedControls: number;
  readonly unnamedControls: number;
  readonly savedChanges: number;
  readonly spokenConfirmations: readonly string[];
}

interface Control {
  readonly markup: string;
  readonly name: string;
  readonly disabled: boolean;
}

const controlPattern = /<(button|input|select|textarea)\b[^>]*>(?:[\s\S]*?<\/\1>)?|<input\b[^>]*>/giu;

function stripTags(value: string): string {
  return value.replace(/<[^>]+>/gu, " ").replace(/\s+/gu, " ").trim();
}

function attribute(markup: string, name: string): string {
  return new RegExp(`\\b${name}="([^"]*)"`, "iu").exec(markup)?.[1] ?? "";
}

function labelForInput(surfaceHtml: string, markup: string): string {
  const id = attribute(markup, "id");
  if (!id) return "";
  const escaped = id.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  return stripTags(new RegExp(`<label\\b[^>]*\\bfor="${escaped}"[^>]*>([\\s\\S]*?)<\\/label>`, "iu").exec(surfaceHtml)?.[1] ?? "");
}

function controls(surfaceHtml: string): readonly Control[] {
  return [...surfaceHtml.matchAll(controlPattern)].map((match) => {
    const markup = match[0];
    const name = attribute(markup, "aria-label") || stripTags(markup) || labelForInput(surfaceHtml, markup);
    return {
      markup,
      name,
      disabled: /\bdisabled\b/iu.test(markup) || /\baria-disabled="true"/iu.test(markup),
    };
  });
}

function includesText(surfaceHtml: string, target: string): boolean {
  return stripTags(surfaceHtml).toLowerCase().includes(target.toLowerCase());
}

function runJourney(spec: NarratorJourneySpec): NarratorJourneyResult {
  let blockedControls = 0;
  let unnamedControls = 0;
  let savedChanges = 0;
  const spokenConfirmations: string[] = [];

  for (const step of spec.steps) {
    const availableControls = controls(step.surfaceHtml);
    unnamedControls += availableControls.filter((control) => !control.name).length;
    if (step.kind === "control") {
      const control = availableControls.find((candidate) => candidate.name.toLowerCase() === step.target.toLowerCase());
      if (!control) throw new Error(`Narrator control missing: ${spec.name}: ${step.target}`);
      if (control.disabled) blockedControls += 1;
    } else if (!includesText(step.surfaceHtml, step.target)) {
      throw new Error(`Narrator text missing: ${spec.name}: ${step.target}`);
    }

    if (step.savedChange) {
      savedChanges += 1;
      const confirmation = step.spokenConfirmation?.trim() ?? "";
      if (!confirmation || !includesText(step.surfaceHtml, confirmation)) {
        throw new Error(`Narrator saved change has no spoken confirmation: ${spec.name}: ${step.target}`);
      }
      spokenConfirmations.push(confirmation);
    }
  }

  const completed = blockedControls === 0 && unnamedControls === 0 && spokenConfirmations.length === savedChanges;
  return { name: spec.name, completed, blockedControls, unnamedControls, savedChanges, spokenConfirmations };
}

export function runNarratorJourneys(specs: readonly NarratorJourneySpec[]): NarratorJourneyReport {
  const journeys = specs.map(runJourney);
  return {
    journeys,
    completedJourneys: journeys.filter((journey) => journey.completed).length,
    blockedControls: journeys.reduce((total, journey) => total + journey.blockedControls, 0),
    unnamedControls: journeys.reduce((total, journey) => total + journey.unnamedControls, 0),
    savedChanges: journeys.reduce((total, journey) => total + journey.savedChanges, 0),
    spokenConfirmations: journeys.flatMap((journey) => journey.spokenConfirmations),
  };
}

export function formatNarratorJourneyReport(report: NarratorJourneyReport): string {
  const lines = ["WINDOWS_NARRATOR_JOURNEY_REPORT"];
  for (const journey of report.journeys) {
    lines.push([
      `journey="${journey.name}"`,
      `completed=${journey.completed}`,
      `blocked_controls=${journey.blockedControls}`,
      `unnamed_controls=${journey.unnamedControls}`,
      `saved_changes=${journey.savedChanges}`,
      `spoken_confirmations=${journey.spokenConfirmations.length}`,
    ].join(" "));
  }
  lines.push([
    "summary",
    `saved_journeys=${report.journeys.length}`,
    `completed_journeys=${report.completedJourneys}`,
    `blocked_controls=${report.blockedControls}`,
    `unnamed_controls=${report.unnamedControls}`,
    `saved_changes=${report.savedChanges}`,
    `spoken_confirmations=${report.spokenConfirmations.length}`,
  ].join(" "));
  return lines.join("\n");
}
