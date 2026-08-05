import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { describe, expect, it } from "vitest";
import {
  assertD2TestClosure,
  D2_PROPERTY_TEST_SUITES,
  D2_TEST_FILE_FLOORS,
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

function withPropertySource(file: string, value: string): D2ClosureSources {
  return {
    ...sources,
    propertyTestSources: { ...sources.propertyTestSources, [file]: value },
  };
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
      promotionCallsClosure: true,
      cycleMarkerSemanticsPinned: true,
      canonicalIssuerPayloadPinned: true,
      standaloneCipherStoreTests: true,
      contractGateWired: true,
      propertyTestSuites: D2_PROPERTY_TEST_SUITES.length,
      testFiles: Object.fromEntries(
        Object.keys(D2_TEST_FILE_FLOORS).map((directory) => [
          directory,
          sources.testFileCensus[directory]!
            .filter((name) => name.endsWith(".test.ts")).length,
        ]),
      ),
    });
    expect(D2_PROPERTY_TEST_SUITES.length).toBeGreaterThanOrEqual(7);
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
      "const attachmentSweep = await sweepExpiredAttachments(env);",
      "const attachmentSweep = { failed: 0 };",
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
      "      const attachmentSweep = await sweepExpiredAttachments(env);\n",
      "      console.log(CYCLE_MARKER);\n"
        + "      const attachmentSweep = await sweepExpiredAttachments(env);\n",
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

  it("rejects bypassing the closure in the promotion execution path", () => {
    const bypassed = replaceOnce(
      sources.promotionProofSource,
      "assertD2TestClosure(readD2ClosureSources(projectRoot));",
      "void readD2ClosureSources(projectRoot);",
    );
    expect(() =>
      assertD2TestClosure(withSource("promotionProofSource", bypassed)),
    ).toThrow(/directly invoke the D2 closure/);
  });

  it("rejects drift of the independently pinned cycle marker", () => {
    const drifted = replaceOnce(
      sources.proofContractSource,
      "[attachment-sweep-cycle] complete",
      "[attachment-sweep-cycle] drifted",
    );
    expect(() =>
      assertD2TestClosure(withSource("proofContractSource", drifted)),
    ).toThrow(/cycle marker semantics drifted/);
  });

  /// D-262 — the tests holding the release's properties are guarded here,
  /// structurally, instead of being folded into the byte-exact source pin.
  it("rejects deleting a security-property suite", () => {
    const oracle = "test/blob-upload-existence-oracle.test.ts";
    const { [oracle]: _deleted, ...survivors } = sources.propertyTestSources;
    expect(() =>
      assertD2TestClosure({ ...sources, propertyTestSources: survivors }),
    ).toThrow(/property suite .* is missing or empty/);
  });

  it("rejects skipping or emptying a security-property suite", () => {
    const atomicity = "test/blob-capacity-atomic.test.ts";
    const skipped = replaceOnce(
      sources.propertyTestSources[atomicity]!,
      'describe("D-256 the blob capacity gate admits atomically (HIGH-2)"',
      'describe.skip("D-256 the blob capacity gate admits atomically (HIGH-2)"',
    );
    expect(() =>
      assertD2TestClosure(withPropertySource(atomicity, skipped)),
    ).toThrow(/registered exactly once as an active describe/);

    const focused = replaceOnce(
      sources.propertyTestSources[atomicity]!,
      'it("refuses the second of two uploads that race for the last byte"',
      'it.skip("refuses the second of two uploads that race for the last byte"',
    );
    expect(() =>
      assertD2TestClosure(withPropertySource(atomicity, focused)),
    ).toThrow(/skipped or focused tests/);

    const gutted = replaceOnce(
      sources.propertyTestSources[atomicity]!,
      "expect(await storedBytes()).toBeLessThanOrEqual(MAX_LIVE_BLOB_BYTES);",
      "",
    );
    expect(() =>
      assertD2TestClosure(withPropertySource(atomicity, gutted)),
    ).toThrow(/no longer asserts/);
  });

  it("accepts an ADDED case in a guarded suite, so improving a test is not a re-pin", () => {
    const oracle = "test/blob-upload-existence-oracle.test.ts";
    const widened = replaceOnce(
      sources.propertyTestSources[oracle]!,
      '  it("leaves the row it collided with exactly as it found it"',
      '  it("a newly added case", async () => { expect(1).toBe(1); });\n'
        + '  it("leaves the row it collided with exactly as it found it"',
    );
    expect(() => assertD2TestClosure(withPropertySource(oracle, widened)))
      .not.toThrow();
  });

  it("rejects deleting a test file the property list never named", () => {
    const shortened = Object.fromEntries(
      Object.entries(sources.testFileCensus).map(([directory, files]) => [
        directory,
        files.filter((name) => name !== "ack.test.ts"),
      ]),
    );
    expect(() =>
      assertD2TestClosure({ ...sources, testFileCensus: shortened }),
    ).toThrow(/below the recorded floor of/);
  });

  it("rejects unwiring the out-of-suite contract gate from npm test", () => {
    const unwired = replaceOnce(
      sources.packageJson,
      '"test": "node scripts/d2-contract-gate.ts && ',
      '"test": "',
    );
    expect(() => assertD2TestClosure(withSource("packageJson", unwired)))
      .toThrow(/must run the contract gate first/);

    const hollowed = replaceOnce(
      sources.contractGateSource,
      "observed !== D2_RELEASE_SOURCE_SHA256",
      "false",
    );
    expect(() =>
      assertD2TestClosure(withSource("contractGateSource", hollowed)),
    ).toThrow(/no longer checks the release digest/);
  });

  it("rejects reordering the canonical issuer payload", () => {
    const reordered = replaceOnce(
      sources.issuerFixtureSource,
      '`{"aud":"${ISSUER_GRANT_AUDIENCE}","exp":${expiresAt},"jti":"${jti}"}`',
      '`{"jti":"${jti}","aud":"${ISSUER_GRANT_AUDIENCE}","exp":${expiresAt}}`',
    );
    expect(() =>
      assertD2TestClosure(withSource("issuerFixtureSource", reordered)),
    ).toThrow(/exact canonical byte order/);
  });
});
