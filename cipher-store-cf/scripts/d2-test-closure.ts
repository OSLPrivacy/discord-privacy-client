import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import ts from "typescript";
import { CYCLE_MARKER, NATURAL_CRON } from "../src/lib/d2-proof-contract.js";

export interface D2ClosureSources {
  wranglerToml: string;
  workerSource: string;
  scheduledTestSource: string;
  promotionProofSource: string;
  proofContractSource: string;
  proofContractTypesSource: string;
  linkGrantTestSource: string;
  issuerFixtureSource: string;
  /** D-262: source of every file named in `D2_PROPERTY_TEST_SUITES`. */
  propertyTestSources: Readonly<Record<string, string>>;
  /** D-262: `*.test.ts` names per suite directory, for the deletion census. */
  testFileCensus: Readonly<Record<string, readonly string[]>>;
  /** D-262: the out-of-suite entry point that runs these contracts. */
  contractGateSource: string;
  /** D-262: pinned manifest whose `test` script must invoke that entry point. */
  packageJson: string;
}

export interface D2ClosureFacts {
  productionCron: typeof NATURAL_CRON;
  registeredScheduledSeamTests: 2;
  sweepCallBeforeMarker: true;
  promotionCallsClosure: true;
  cycleMarkerSemanticsPinned: true;
  canonicalIssuerPayloadPinned: true;
  standaloneCipherStoreTests: true;
  /** Number of named security-property suites verified present and active. */
  propertyTestSuites: number;
  /** Observed `*.test.ts` count per directory; each is at or above its floor. */
  testFiles: Record<string, number>;
  /** `npm test` still runs both contracts from outside the suite. */
  contractGateWired: true;
}

/** The exact prefix `package.json`'s `test` script must keep. */
const REQUIRED_TEST_SCRIPT_PREFIX = "node scripts/d2-contract-gate.ts && ";

/**
 * D-262 — what the release digest deliberately does NOT do.
 *
 * `D2_RELEASE_SOURCE_SHA256` pins release source byte-exactly and covers no
 * test file. That was filed as a defect because the two suites holding the
 * D-255 existence-oracle and D-256 atomicity properties could be deleted
 * without moving the digest by a bit. The fix is NOT to widen the digest over
 * `test/**`:
 *
 *   * A byte-exact pin over tests re-anchors on every legitimate test edit —
 *     a new case, a renamed helper, a clearer message. Re-anchoring is the one
 *     operation in this repo that must stay rare enough to be read; making it
 *     routine trains the reviewer to move the digest without reading it, which
 *     is the exact defeat D-262 warns about. It would also weaken the source
 *     pin, because "the digest moved" would stop meaning "shipped code
 *     changed".
 *   * Byte-exactness is the wrong instrument anyway. What must not happen to a
 *     test is deletion, skipping, or gutting — properties of its STRUCTURE. A
 *     hash cannot tell those apart from a typo fix; an AST can.
 *
 * So test integrity gets its own gate with its own failure mode, here, in the
 * closure that already refuses a skipped suite and an emptied test body. Two
 * layers, chosen for what each is bad at:
 *
 *   1. NAMED suites below: registered exactly once, `describe`/`it` and never
 *      `.skip`/`.only`/`.todo`, with load-bearing assertion text still inside
 *      the body. Adding a case costs nothing; deleting, renaming, skipping or
 *      emptying one is red.
 *   2. A per-directory `*.test.ts` FLOOR. The named list is a hand-written list
 *      and therefore omission-shaped, exactly like the old pinned file list.
 *      The floor catches what the list cannot know it is missing: deleting ANY
 *      test file drops the count. It only ratchets, so adding tests never
 *      re-pins anything.
 *
 * What this still costs, stated rather than implied: the floor is a number a
 * human maintains, so a genuinely retired test needs it lowered by hand — a
 * deliberate edit, which is the point. And no in-suite gate survives deletion
 * of the gate itself; that is why `npm test` invokes this closure as its own
 * step from `package.json`, which IS inside the release digest, so removing
 * the step turns the release-source contract red.
 */
export interface D2PropertyTestSuite {
  /** Path relative to the project root. */
  file: string;
  /** The defect or leak class whose property this suite holds. */
  defect: string;
  /** `describe` title; must be registered exactly once and never skipped. */
  suite: string;
  /**
   * Titles that must exist as ACTIVE `it`s. A superset is allowed on purpose:
   * adding a case must not be a re-pin, while removing or renaming one is red.
   */
  tests: readonly string[];
  /** Assertion text that must still appear inside the suite body. */
  assertions: readonly string[];
}

export const D2_PROPERTY_TEST_SUITES: readonly D2PropertyTestSuite[] = [
  {
    file: "test/blob-upload-existence-oracle.test.ts",
    defect: "D-255",
    suite: "D-255 blob upload is not an existence oracle",
    tests: [
      "answers a taken blob id exactly as it answers an unused one",
      "leaves the row it collided with exactly as it found it",
    ],
    assertions: [
      "expect(taken.status).toBe(unused.status)",
      "expect(taken.headers).toEqual(unused.headers)",
      "expect(taken.maskedBody).toBe(unused.maskedBody)",
    ],
  },
  {
    file: "test/blob-capacity-atomic.test.ts",
    defect: "D-256",
    suite: "D-256 the blob capacity gate admits atomically (HIGH-2)",
    tests: [
      "refuses the second of two uploads that race for the last byte",
      "still admits the one upload the headroom genuinely allows",
    ],
    assertions: [
      "expect(await storedBytes()).toBeLessThanOrEqual(MAX_LIVE_BLOB_BYTES)",
      'expect(statuses.filter((status) => status === 503)).toHaveLength(1)',
      'expect(statuses.filter((status) => status === 201)).toHaveLength(1)',
    ],
  },
  {
    file: "test/blob-payload-no-overwrite.test.ts",
    defect: "D-264",
    suite: "D-264 a caller-named payload key is written only when it is free",
    tests: [
      "refuses to replace an existing payload under a digest the caller supplied",
      "still stores bytes under a digest no object occupies",
    ],
    assertions: [
      "expect(new Uint8Array(await stored!.arrayBuffer())).toEqual(VICTIM_BYTES)",
      "expect(attack.status).toBe(201)",
    ],
  },
  {
    file: "test/blob-capability.test.ts",
    defect: "D-257",
    suite: "R2 capability blob route",
    tests: [
      "stores payload bytes only in R2 and burns only with manage_cap",
      "makes every GET failure the same 404",
    ],
    assertions: ['"x-osl-fetch-cap": fetchCap'],
  },
  {
    file: "test/d81-fetch-carries-no-identity.test.ts",
    defect: "D81",
    suite: "D81 — a cipher-store fetch carries no identity",
    tests: [
      "serves a blob to a caller who presents the capability and nothing else",
      "writes no blob access receipt or fetcher identity",
      "refuses a capability presented in the URL instead of the header",
    ],
    assertions: [
      'expect(rateCounters).not.toContain("198.51.100.")',
      "expect(rateCounters).not.toContain(id)",
    ],
  },
  {
    file: "test/d81-fetch-carries-no-identity.test.ts",
    defect: "D81/t1-15",
    suite: "D81 — a retired legacy row is treated as absent",
    tests: [
      "does not serve a retired row to a caller holding only its id",
      "answers a retired row exactly as it answers an id that was never stored",
      "does not let an id-only caller destroy a retired row",
      "rejects a tokenless upload, so no new capability-less index row can be created",
    ],
    assertions: [
      "expect(legacy.status).toBe(absent.status)",
      "expect(await legacy.text()).toBe(await absent.text())",
    ],
  },
  {
    file: "test/harness-strictness.test.ts",
    defect: "instrument",
    suite: "the R2 double refuses what production refuses",
    tests: [
      "rejects a put body with no known length",
      "rejects a multipart part with no known length",
      "still accepts a known-length body, so the guard is not simply refusing everything",
      "honours onlyIf etagDoesNotMatch instead of silently overwriting",
    ],
    assertions: [
      'onlyIf: { etagDoesNotMatch: "*" }',
      "expect(second).toBeNull()",
    ],
  },
];

/**
 * `*.test.ts` floors per suite directory. Ratchet-only: `>=`, so every added
 * test file leaves them true and only a deletion is red.
 */
export const D2_TEST_FILE_FLOORS: Readonly<Record<string, number>> = {
  test: 41,
  "test-node": 14,
};

const REQUIRED_NATURAL_CRON = "*/5 * * * *";
const REQUIRED_CYCLE_MARKER = "[attachment-sweep-cycle] complete";
const REQUIRED_ISSUER_PAYLOAD =
  '`{"aud":"${ISSUER_GRANT_AUDIENCE}","exp":${expiresAt},"jti":"${jti}"}`';
const REQUIRED_INTEROP_PAYLOAD =
  '`{"aud":"osl-link-create","exp":${issuedAt + 300},"jti":"${claims.jti}"}`';
const SUITE_NAME = "natural attachment sweep witness";
const POSITIVE_NAME = "emits the fixed marker only after R2 abort and D1 removal succeed";
const NEGATIVE_NAME =
  "emits no success marker and retains retryable metadata when R2 abort fails";

function fail(message: string): never {
  throw new Error(`D2 test closure: ${message}`);
}

function sourceFile(source: string, name: string): ts.SourceFile {
  return ts.createSourceFile(name, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
}

function visit(node: ts.Node, callback: (node: ts.Node) => void): void {
  callback(node);
  node.forEachChild((child) => visit(child, callback));
}

function expressionName(expression: ts.Expression): string | null {
  if (ts.isIdentifier(expression)) return expression.text;
  if (ts.isPropertyAccessExpression(expression)) {
    const base = expressionName(expression.expression);
    return base ? `${base}.${expression.name.text}` : null;
  }
  return null;
}

function stringArgument(call: ts.CallExpression): string | null {
  const value = call.arguments[0];
  return value && ts.isStringLiteral(value) ? value.text : null;
}

function callsWithin(node: ts.Node): ts.CallExpression[] {
  const calls: ts.CallExpression[] = [];
  visit(node, (candidate) => {
    if (ts.isCallExpression(candidate)) calls.push(candidate);
  });
  return calls;
}

function callbackBody(call: ts.CallExpression, label: string): ts.Block {
  const callback = call.arguments[1];
  if (
    !callback ||
    (!ts.isArrowFunction(callback) && !ts.isFunctionExpression(callback)) ||
    !ts.isBlock(callback.body)
  ) {
    return fail(`${label} must have a block callback`);
  }
  return callback.body;
}

function requireNamedImport(
  source: ts.SourceFile,
  moduleName: string,
  names: readonly string[],
): void {
  const imported = new Set<string>();
  for (const statement of source.statements) {
    if (
      !ts.isImportDeclaration(statement) ||
      !ts.isStringLiteral(statement.moduleSpecifier) ||
      statement.moduleSpecifier.text !== moduleName
    ) {
      continue;
    }
    const bindings = statement.importClause?.namedBindings;
    if (bindings && ts.isNamedImports(bindings)) {
      for (const element of bindings.elements) imported.add(element.name.text);
    }
  }
  for (const name of names) {
    if (!imported.has(name)) {
      fail(`${source.fileName} must import ${name} from ${moduleName}`);
    }
  }
}

function productionCron(wranglerToml: string): typeof NATURAL_CRON {
  const lines = wranglerToml.split(/\r?\n/);
  const triggerStarts = lines
    .map((line, index) => line.trim() === "[triggers]" ? index : -1)
    .filter((index) => index >= 0);
  if (triggerStarts.length !== 1) {
    fail("wrangler.toml must have exactly one production [triggers] section");
  }
  const start = triggerStarts[0]! + 1;
  const relativeEnd = lines
    .slice(start)
    .findIndex((line) => /^\s*\[[^\]]+\]\s*$/.test(line));
  const end = relativeEnd < 0 ? lines.length : start + relativeEnd;
  const cronLines = lines
    .slice(start, end)
    .map((line) => /^\s*crons\s*=\s*(.+?)\s*$/.exec(line))
    .filter((match): match is RegExpExecArray => match !== null);
  if (cronLines.length !== 1) {
    fail("wrangler.toml must have exactly one production crons assignment");
  }
  let values: unknown;
  try {
    values = JSON.parse(cronLines[0]![1]!);
  } catch {
    fail("wrangler.toml production crons assignment is not a string array");
  }
  if (
    !Array.isArray(values) ||
    values.length !== 1 ||
    values[0] !== NATURAL_CRON
  ) {
    fail(`production cron must be exactly ${NATURAL_CRON}`);
  }
  return NATURAL_CRON;
}

function scheduledSeamTests(source: string): 2 {
  const parsed = sourceFile(source, "test/scheduled-sweep-proof.test.ts");
  requireNamedImport(parsed, "../src/lib/d2-proof-contract.js", [
    "CYCLE_MARKER",
    "NATURAL_CRON",
  ]);

  const namedSuites = callsWithin(parsed).filter(
    (call) => stringArgument(call) === SUITE_NAME,
  );
  if (namedSuites.length !== 1 || expressionName(namedSuites[0]!.expression) !== "describe") {
    fail("scheduled production-seam suite must be registered once and may not be skipped");
  }
  const suite = callbackBody(namedSuites[0]!, "scheduled production-seam suite");
  const tests = callsWithin(suite).filter((call) => {
    const name = expressionName(call.expression);
    return name === "it" || name?.startsWith("it.") === true;
  });
  const activeNames = tests
    .filter((call) => expressionName(call.expression) === "it")
    .map(stringArgument);
  if (
    tests.length !== 2 ||
    activeNames.length !== 2 ||
    activeNames[0] !== POSITIVE_NAME ||
    activeNames[1] !== NEGATIVE_NAME
  ) {
    fail("exactly two active scheduled production-seam tests must be registered");
  }

  let cronUsesContract = false;
  visit(parsed, (node) => {
    if (
      ts.isPropertyAssignment(node) &&
      node.name.getText(parsed) === "cron" &&
      ts.isIdentifier(node.initializer) &&
      node.initializer.text === "NATURAL_CRON"
    ) {
      cronUsesContract = true;
    }
  });
  if (!cronUsesContract) fail("scheduled event must use the shared production cron");

  const positive = tests[0]!;
  const positiveBody = callbackBody(positive, "scheduled positive");
  const positiveCalls = callsWithin(positiveBody);
  const callNames = positiveCalls.map((call) => expressionName(call.expression));
  if (
    !callNames.includes("insertStaleLegacy") ||
    !callNames.includes("worker.scheduled") ||
    callNames.filter((name) => name === "d1Count").length < 2
  ) {
    fail("scheduled positive must create, observe, and reclaim a stale reservation");
  }
  const positiveText = positiveBody.getText(parsed);
  for (const required of [
    "expect(resume).toHaveBeenCalledWith",
    "expect(marker).toHaveBeenCalledTimes(1)",
    "expect(marker).toHaveBeenCalledWith(CYCLE_MARKER)",
  ]) {
    if (!positiveText.includes(required)) {
      fail(`scheduled positive is missing ${required}`);
    }
  }
  return 2;
}

function scheduledWorkerBoundary(source: string): true {
  const parsed = sourceFile(source, "src/index.ts");
  requireNamedImport(parsed, "./lib/d2-proof-contract.js", ["CYCLE_MARKER"]);
  let scheduledBody: ts.Block | null = null;
  visit(parsed, (node) => {
    if (
      ts.isMethodDeclaration(node) &&
      node.name.getText(parsed) === "scheduled" &&
      node.body
    ) {
      scheduledBody = node.body;
    }
  });
  if (!scheduledBody) fail("Worker scheduled handler is absent");

  const calls = callsWithin(scheduledBody);
  const sweepCalls = calls.filter(
    (call) => expressionName(call.expression) === "sweepExpiredAttachments",
  );
  const markerCalls = calls.filter(
    (call) =>
      expressionName(call.expression) === "console.log" &&
      call.arguments.length === 1 &&
      ts.isIdentifier(call.arguments[0]!) &&
      call.arguments[0]!.text === "CYCLE_MARKER",
  );
  if (sweepCalls.length !== 1 || markerCalls.length !== 1) {
    fail("scheduled handler must contain one attachment sweep and one fixed marker");
  }
  if (!ts.isAwaitExpression(sweepCalls[0]!.parent)) {
    fail("scheduled attachment sweep must be awaited");
  }

  const enclosingTry = (node: ts.Node): ts.TryStatement | null => {
    let current: ts.Node | undefined = node;
    while (current && current !== scheduledBody) {
      if (ts.isTryStatement(current)) return current;
      current = current.parent;
    }
    return null;
  };
  const sweepTry = enclosingTry(sweepCalls[0]!);
  const markerTry = enclosingTry(markerCalls[0]!);
  if (!sweepTry || sweepTry !== markerTry) {
    fail("attachment sweep and marker must share one failure boundary");
  }
  if (sweepCalls[0]!.getStart(parsed) >= markerCalls[0]!.getStart(parsed)) {
    fail("completion marker must follow the awaited attachment sweep");
  }
  return true;
}

function promotionProofBinding(source: string): void {
  const parsed = sourceFile(source, "scripts/d2-promotion-proof.ts");
  requireNamedImport(parsed, "../src/lib/d2-proof-contract.js", [
    "CYCLE_MARKER",
    "NATURAL_CRON",
  ]);
  requireNamedImport(parsed, "./d2-test-closure.ts", [
    "assertD2TestClosure",
    "readD2ClosureSources",
  ]);
  if (
    /\b(?:export\s+)?const\s+(?:NATURAL_CRON|CYCLE_MARKER)\s*=/.test(source)
  ) {
    fail("promotion proof may not redeclare the cron or cycle marker");
  }

  let executionBody: ts.Block | null = null;
  visit(parsed, (node) => {
    if (
      ts.isFunctionDeclaration(node) &&
      node.name?.text === "executePromotionAndProof" &&
      node.body
    ) {
      executionBody = node.body;
    }
  });
  const body = executionBody as ts.Block | null;
  if (!body) fail("promotion execution function is absent");
  const gateStatements = body.statements.filter((statement) => {
    if (!ts.isExpressionStatement(statement)) return false;
    const expression = statement.expression;
    return (
      ts.isCallExpression(expression) &&
      expressionName(expression.expression) === "assertD2TestClosure"
    );
  });
  if (gateStatements.length !== 1) {
    fail("promotion execution must directly invoke the D2 closure exactly once");
  }
  const gateCall = (gateStatements[0] as ts.ExpressionStatement)
    .expression as ts.CallExpression;
  const gateInput = gateCall.arguments[0];
  if (
    !gateInput ||
    !ts.isCallExpression(gateInput) ||
    expressionName(gateInput.expression) !== "readD2ClosureSources" ||
    gateInput.arguments.length !== 1 ||
    !ts.isIdentifier(gateInput.arguments[0]!) ||
    gateInput.arguments[0]!.text !== "projectRoot"
  ) {
    fail("promotion execution must gate the current project closure");
  }
  const sourceRead = callsWithin(body).find(
    (call) => expressionName(call.expression) === "sourceFacts",
  );
  if (!sourceRead || gateCall.getStart(parsed) >= sourceRead.getStart(parsed)) {
    fail("promotion closure must run before source and promotion work");
  }
}

function proofContractSemantics(source: string, typesSource: string): true {
  const parsed = sourceFile(source, "src/lib/d2-proof-contract.js");
  const value = (name: string): string => {
    const matches: string[] = [];
    visit(parsed, (node) => {
      if (
        ts.isVariableDeclaration(node) &&
        ts.isIdentifier(node.name) &&
        node.name.text === name &&
        node.initializer &&
        ts.isStringLiteral(node.initializer)
      ) {
        matches.push(node.initializer.text);
      }
    });
    if (matches.length !== 1) fail(`proof contract must declare ${name} once`);
    return matches[0]!;
  };
  if (
    value("NATURAL_CRON") !== REQUIRED_NATURAL_CRON ||
    NATURAL_CRON !== REQUIRED_NATURAL_CRON
  ) {
    fail("shared natural cron semantics drifted");
  }
  if (
    value("CYCLE_MARKER") !== REQUIRED_CYCLE_MARKER ||
    CYCLE_MARKER !== REQUIRED_CYCLE_MARKER
  ) {
    fail("shared cycle marker semantics drifted");
  }
  for (const declaration of [
    `export const NATURAL_CRON: "${REQUIRED_NATURAL_CRON}";`,
    `export const CYCLE_MARKER: "${REQUIRED_CYCLE_MARKER}";`,
  ]) {
    if (!typesSource.includes(declaration)) {
      fail("proof contract runtime and literal types must stay byte-identical");
    }
  }
  return true;
}

function standaloneLinkGrantClosure(
  source: string,
  fixtureSource: string,
): true {
  if (/keyserver-cf\/src\/lib\/link-grant-issuer/.test(source)) {
    fail("cipher-store tests may not import keyserver production source");
  }
  const parsed = sourceFile(source, "test/link-grant.test.ts");
  requireNamedImport(parsed, "./helpers/link-grant-issuer-fixture.js", [
    "mintKeyserverGrantFixture",
  ]);
  if (
    !callsWithin(parsed).some(
      (call) => expressionName(call.expression) === "mintKeyserverGrantFixture",
    )
  ) {
    fail("standalone keyserver wire fixture must be exercised");
  }
  const fixture = sourceFile(
    fixtureSource,
    "test/helpers/link-grant-issuer-fixture.ts",
  );
  const payloadInitializers: string[] = [];
  visit(fixture, (node) => {
    if (
      ts.isVariableDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      node.name.text === "payload" &&
      node.initializer
    ) {
      payloadInitializers.push(node.initializer.getText(fixture));
    }
  });
  if (
    payloadInitializers.length !== 1 ||
    payloadInitializers[0] !== REQUIRED_ISSUER_PAYLOAD
  ) {
    fail("standalone issuer payload is not the exact canonical byte order");
  }
  if (
    !source.includes("expect(grant.payload).toBe(") ||
    !source.includes(REQUIRED_INTEROP_PAYLOAD)
  ) {
    fail("interoperability test must independently pin canonical payload bytes");
  }
  return true;
}

/**
 * One named security-property suite: registered exactly once, never skipped,
 * every named test active, and the load-bearing assertions still in the body.
 *
 * Unlike `scheduledSeamTests`, the test COUNT is not pinned — there the "two
 * tests" is itself the property, here a superset is a legitimate improvement
 * and must not cost a re-pin.
 */
function propertyTestSuite(
  entry: D2PropertyTestSuite,
  source: string | undefined,
): void {
  const label = `${entry.defect} property suite ${entry.file}`;
  if (source === undefined || source.length === 0) {
    fail(`${label} is missing or empty`);
  }
  const parsed = sourceFile(source, entry.file);
  const calls = callsWithin(parsed);
  const named = calls.filter((call) => stringArgument(call) === entry.suite);
  const active = named.filter(
    (call) => expressionName(call.expression) === "describe",
  );
  if (named.length !== 1 || active.length !== 1) {
    fail(
      `${label}: "${entry.suite}" must be registered exactly once as an active `
      + "describe and may not be skipped",
    );
  }
  const body = callbackBody(active[0]!, label);
  const registered = callsWithin(body).filter((call) => {
    const name = expressionName(call.expression);
    return name === "it" || name === "test"
      || name?.startsWith("it.") === true || name?.startsWith("test.") === true;
  });
  const inactive = registered
    .filter((call) => {
      const name = expressionName(call.expression);
      return name !== "it" && name !== "test";
    })
    .map(stringArgument);
  if (inactive.length > 0) {
    fail(`${label} has skipped or focused tests: ${inactive.join(", ")}`);
  }
  const activeNames = new Set(registered.map(stringArgument));
  for (const title of entry.tests) {
    if (!activeNames.has(title)) {
      fail(`${label} no longer registers an active test named "${title}"`);
    }
  }
  const text = body.getText(parsed);
  for (const assertion of entry.assertions) {
    if (!text.includes(assertion)) {
      fail(`${label} no longer asserts \`${assertion}\``);
    }
  }
}

function propertyTestClosure(
  sources: Readonly<Record<string, string>>,
): number {
  if (D2_PROPERTY_TEST_SUITES.length === 0) {
    fail("the property-suite list is empty, so this gate cannot fail");
  }
  for (const entry of D2_PROPERTY_TEST_SUITES) {
    propertyTestSuite(entry, sources[entry.file]);
  }
  return D2_PROPERTY_TEST_SUITES.length;
}

/**
 * The census the named list above cannot replace: a floor on `*.test.ts` per
 * directory, so deleting a test file nobody thought to name is still red.
 */
function testFileCensus(
  census: Readonly<Record<string, readonly string[]>>,
): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const [directory, floor] of Object.entries(D2_TEST_FILE_FLOORS)) {
    const files = census[directory];
    if (files === undefined) fail(`no test census for ${directory}/`);
    const observed = files.filter((name) => name.endsWith(".test.ts")).length;
    if (observed < floor) {
      fail(
        `${directory}/ holds ${observed} *.test.ts files, below the recorded `
        + `floor of ${floor}: a test file was removed`,
      );
    }
    counts[directory] = observed;
  }
  return counts;
}

/**
 * The gate that survives the suite. `package.json` is a pinned release file, so
 * dropping the step below moves `D2_RELEASE_SOURCE_SHA256`; the AST checks stop
 * the step from being kept as decoration over an emptied entry point.
 */
function contractGateBinding(packageJson: string, gateSource: string): true {
  let scripts: Record<string, unknown>;
  try {
    scripts = (JSON.parse(packageJson) as { scripts?: Record<string, unknown> })
      .scripts ?? {};
  } catch {
    return fail("package.json is not valid JSON");
  }
  const test = scripts.test;
  if (typeof test !== "string" || !test.startsWith(REQUIRED_TEST_SCRIPT_PREFIX)) {
    fail(
      "package.json `test` must run the contract gate first: "
      + `\`${REQUIRED_TEST_SCRIPT_PREFIX}...\``,
    );
  }
  if (!test.includes("vitest run")) {
    fail("package.json `test` no longer runs the suites it gates");
  }

  const parsed = sourceFile(gateSource, "scripts/d2-contract-gate.ts");
  requireNamedImport(parsed, "./d2-test-closure.ts", [
    "assertD2TestClosure",
    "readD2ClosureSources",
  ]);
  requireNamedImport(parsed, "./d2-release-source-manifest.ts", [
    "deriveReleaseSourceFiles",
    "localModuleClosure",
    "releaseSourceManifestSha256",
  ]);
  requireNamedImport(parsed, "./d2-0010-release-contract.ts", [
    "D2_RELEASE_SOURCE_SHA256",
  ]);
  const invocations = callsWithin(parsed).filter(
    (call) => expressionName(call.expression) === "assertD2TestClosure",
  );
  if (invocations.length !== 1) {
    fail("the contract gate must invoke the test closure exactly once");
  }
  const argument = invocations[0]!.arguments[0];
  if (
    !argument
    || !ts.isCallExpression(argument)
    || expressionName(argument.expression) !== "readD2ClosureSources"
  ) {
    fail("the contract gate must run the closure over the current project");
  }
  for (const required of [
    "releaseSourceManifestSha256(projectRoot, files)",
    "observed !== D2_RELEASE_SOURCE_SHA256",
  ]) {
    if (!gateSource.includes(required)) {
      fail(`the contract gate no longer checks the release digest: ${required}`);
    }
  }
  return true;
}

export function readD2ClosureSources(projectRoot: string): D2ClosureSources {
  const read = (path: string) => readFileSync(join(projectRoot, path), "utf8");
  const propertyTestSources: Record<string, string> = {};
  for (const entry of D2_PROPERTY_TEST_SUITES) {
    // Reading is the deletion signal for a named suite; the floor below covers
    // the files nobody named. A raw ENOENT would be true but would not say
    // WHICH property just stopped being held, so name it.
    if (propertyTestSources[entry.file] === undefined) {
      try {
        propertyTestSources[entry.file] = read(entry.file);
      } catch {
        fail(
          `${entry.defect} property suite ${entry.file} is gone: nothing now `
          + `holds "${entry.suite}"`,
        );
      }
    }
  }
  const testFileCensusSources: Record<string, string[]> = {};
  for (const directory of Object.keys(D2_TEST_FILE_FLOORS)) {
    testFileCensusSources[directory] = readdirSync(join(projectRoot, directory))
      .sort();
  }
  return {
    propertyTestSources,
    testFileCensus: testFileCensusSources,
    contractGateSource: read("scripts/d2-contract-gate.ts"),
    packageJson: read("package.json"),
    wranglerToml: read("wrangler.toml"),
    workerSource: read("src/index.ts"),
    scheduledTestSource: read("test/scheduled-sweep-proof.test.ts"),
    promotionProofSource: read("scripts/d2-promotion-proof.ts"),
    proofContractSource: read("src/lib/d2-proof-contract.js"),
    proofContractTypesSource: read("src/lib/d2-proof-contract.d.ts"),
    linkGrantTestSource: read("test/link-grant.test.ts"),
    issuerFixtureSource: read("test/helpers/link-grant-issuer-fixture.ts"),
  };
}

export function assertD2TestClosure(sources: D2ClosureSources): D2ClosureFacts {
  const markerPinned = proofContractSemantics(
    sources.proofContractSource,
    sources.proofContractTypesSource,
  );
  const cron = productionCron(sources.wranglerToml);
  promotionProofBinding(sources.promotionProofSource);
  const registered = scheduledSeamTests(sources.scheduledTestSource);
  const ordered = scheduledWorkerBoundary(sources.workerSource);
  const standalone = standaloneLinkGrantClosure(
    sources.linkGrantTestSource,
    sources.issuerFixtureSource,
  );
  const propertySuites = propertyTestClosure(sources.propertyTestSources);
  const testFiles = testFileCensus(sources.testFileCensus);
  const gateWired = contractGateBinding(
    sources.packageJson,
    sources.contractGateSource,
  );
  return {
    contractGateWired: gateWired,
    productionCron: cron,
    registeredScheduledSeamTests: registered,
    sweepCallBeforeMarker: ordered,
    promotionCallsClosure: true,
    cycleMarkerSemanticsPinned: markerPinned,
    canonicalIssuerPayloadPinned: true,
    standaloneCipherStoreTests: standalone,
    propertyTestSuites: propertySuites,
    testFiles,
  };
}
