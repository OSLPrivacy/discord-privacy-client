import { readFileSync } from "node:fs";
import { join } from "node:path";
import ts from "typescript";
import { CYCLE_MARKER, NATURAL_CRON } from "../src/lib/d2-proof-contract.js";

export interface D2ClosureSources {
  wranglerToml: string;
  workerSource: string;
  scheduledTestSource: string;
  promotionProofSource: string;
  linkGrantTestSource: string;
}

export interface D2ClosureFacts {
  productionCron: typeof NATURAL_CRON;
  registeredScheduledSeamTests: 2;
  sweepCallBeforeMarker: true;
  standaloneCipherStoreTests: true;
}

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
  if (
    /\b(?:export\s+)?const\s+(?:NATURAL_CRON|CYCLE_MARKER)\s*=/.test(source)
  ) {
    fail("promotion proof may not redeclare the cron or cycle marker");
  }
}

function standaloneLinkGrantClosure(source: string): true {
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
  return true;
}

export function readD2ClosureSources(projectRoot: string): D2ClosureSources {
  const read = (path: string) => readFileSync(join(projectRoot, path), "utf8");
  return {
    wranglerToml: read("wrangler.toml"),
    workerSource: read("src/index.ts"),
    scheduledTestSource: read("test/scheduled-sweep-proof.test.ts"),
    promotionProofSource: read("scripts/d2-promotion-proof.ts"),
    linkGrantTestSource: read("test/link-grant.test.ts"),
  };
}

export function assertD2TestClosure(sources: D2ClosureSources): D2ClosureFacts {
  const cron = productionCron(sources.wranglerToml);
  promotionProofBinding(sources.promotionProofSource);
  const registered = scheduledSeamTests(sources.scheduledTestSource);
  const ordered = scheduledWorkerBoundary(sources.workerSource);
  const standalone = standaloneLinkGrantClosure(sources.linkGrantTestSource);
  return {
    productionCron: cron,
    registeredScheduledSeamTests: registered,
    sweepCallBeforeMarker: ordered,
    standaloneCipherStoreTests: standalone,
  };
}
