import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { describe, expect, it } from "vitest";
import {
  assertD2TestClosure,
  readD2ClosureSources,
  type D2ClosureSources,
} from "../scripts/d2-test-closure.js";
import { NATURAL_CRON } from "../src/lib/d2-proof-contract.js";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sources = readD2ClosureSources(projectRoot);

function replaceOnce(source: string, before: string, after: string): string {
  const first = source.indexOf(before);
  expect(first).toBeGreaterThanOrEqual(0);
  expect(source.indexOf(before, first + before.length)).toBe(-1);
  return source.slice(0, first) + after + source.slice(first + before.length);
}

function withSource(
  key: keyof D2ClosureSources,
  value: string,
): D2ClosureSources {
  return { ...sources, [key]: value };
}

function emptyRegisteredTest(source: string, title: string): string {
  const parsed = ts.createSourceFile(
    "scheduled-sweep-proof.test.ts",
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );
  const matches: ts.Block[] = [];
  const walk = (node: ts.Node): void => {
    if (ts.isCallExpression(node)) {
      const name = node.arguments[0];
      const callback = node.arguments[1];
      if (
        name &&
        ts.isStringLiteral(name) &&
        name.text === title &&
        callback &&
        (ts.isArrowFunction(callback) || ts.isFunctionExpression(callback)) &&
        ts.isBlock(callback.body)
      ) {
        matches.push(callback.body);
      }
    }
    node.forEachChild(walk);
  };
  walk(parsed);
  expect(matches).toHaveLength(1);
  const body = matches[0]!;
  return source.slice(0, body.getStart(parsed)) + "{}" + source.slice(body.end);
}

describe("D2 test-closure gate", () => {
  it("binds the proof and two active production-seam tests to the production cron", () => {
    expect(assertD2TestClosure(sources)).toEqual({
      productionCron: NATURAL_CRON,
      registeredScheduledSeamTests: 2,
      sweepCallBeforeMarker: true,
      standaloneCipherStoreTests: true,
    });
  });

  it("rejects semantic cron drift to a daily production trigger", () => {
    const drifted = replaceOnce(
      sources.wranglerToml,
      `crons = ["${NATURAL_CRON}"]`,
      'crons = ["17 3 * * *"]',
    );
    expect(() => assertD2TestClosure(withSource("wranglerToml", drifted))).toThrow(
      /production cron must be exactly/,
    );
  });

  it("rejects skipping the only scheduled production-seam suite", () => {
    const skipped = replaceOnce(
      sources.scheduledTestSource,
      'describe("natural attachment sweep witness"',
      'describe.skip("natural attachment sweep witness"',
    );
    expect(() =>
      assertD2TestClosure(withSource("scheduledTestSource", skipped)),
    ).toThrow(/registered once and may not be skipped/);
  });

  it("rejects severing the production attachment sweep call", () => {
    const severed = replaceOnce(
      sources.workerSource,
      "await sweepExpiredAttachments(env);",
      "await Promise.resolve();",
    );
    expect(() => assertD2TestClosure(withSource("workerSource", severed))).toThrow(
      /one attachment sweep/,
    );
  });

  it("rejects moving the completion marker before the production sweep", () => {
    const withoutMarker = replaceOnce(
      sources.workerSource,
      "      console.log(CYCLE_MARKER);\n",
      "",
    );
    const markerFirst = replaceOnce(
      withoutMarker,
      "      await sweepExpiredAttachments(env);\n",
      "      console.log(CYCLE_MARKER);\n      await sweepExpiredAttachments(env);\n",
    );
    expect(() =>
      assertD2TestClosure(withSource("workerSource", markerFirst)),
    ).toThrow(/marker must follow/);
  });

  it("rejects an empty scheduled positive", () => {
    const emptied = emptyRegisteredTest(
      sources.scheduledTestSource,
      "emits the fixed marker only after R2 abort and D1 removal succeed",
    );
    expect(() =>
      assertD2TestClosure(withSource("scheduledTestSource", emptied)),
    ).toThrow(/create, observe, and reclaim/);
  });
});
