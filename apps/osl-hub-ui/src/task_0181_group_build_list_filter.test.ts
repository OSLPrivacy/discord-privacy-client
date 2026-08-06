import fs from "node:fs";
import vm from "node:vm";
import { describe, expect, it } from "vitest";

const PERSON = "INDIGO-0181";
const HELPER = "oslBuildTwoWayFriendIdsForBulkWhitelist";
const bootScript = fs.readFileSync(
  new URL("../../../src-tauri/src/injection/boot.js", import.meta.url),
  "utf8",
);

function extractFunction(source: string, name: string): string {
  const start = source.indexOf(`function ${name}(`);
  expect(start).toBeGreaterThan(-1);
  const firstBrace = source.indexOf("{", start);
  expect(firstBrace).toBeGreaterThan(start);

  let depth = 0;
  for (let i = firstBrace; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    if (source[i] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, i + 1);
  }
  throw new Error(`${name} body did not close`);
}

const helperSource = extractFunction(bootScript, HELPER);

function buildList(relationships: unknown[]): string[] {
  const context = { __relationships: relationships, __result: undefined as unknown };
  vm.runInNewContext(
    `${helperSource}\nglobalThis.__result = ${HELPER}(globalThis.__relationships);`,
    context,
  );
  return context.__result as string[];
}

function resultFromFixture(fixture: { relationships: unknown[] }) {
  const names = buildList(fixture.relationships);
  return { count: names.length, names };
}

function readableSourceIds(fixture: { relationships: Array<{ user?: { id?: unknown } }> }) {
  return fixture.relationships
    .map((relationship) => relationship.user?.id)
    .filter((id): id is string => typeof id === "string");
}

function changedPaths(left: unknown, right: unknown, prefix = ""): string[] {
  if (Object.is(left, right)) return [];
  if (
    typeof left !== "object" ||
    left === null ||
    typeof right !== "object" ||
    right === null
  ) {
    return [prefix];
  }
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  return [...keys].flatMap((key) =>
    changedPaths(
      (left as Record<string, unknown>)[key],
      (right as Record<string, unknown>)[key],
      prefix ? `${prefix}.${key}` : key,
    ),
  );
}

describe("TASK 0181 group build list filter", () => {
  it("admits only unmodified two-way relationships into the group build list", () => {
    const goodFixture = {
      relationships: [
        {
          type: 1,
          user: {
            id: PERSON,
          },
        },
      ],
    };
    const goodFixtureBytes = JSON.stringify(goodFixture);
    const sourceIds = readableSourceIds(goodFixture);
    const sourceCountBefore = sourceIds.length;

    const goodResult = resultFromFixture(goodFixture);
    const goodResultBytes = JSON.stringify(goodResult);

    const badFixture = JSON.parse(goodFixtureBytes) as typeof goodFixture;
    badFixture.relationships[0].type = 3;
    const changed = changedPaths(goodFixture, badFixture);
    const badResult = resultFromFixture(badFixture);
    const refusedAsNotEligible = !badResult.names.includes(PERSON);

    const goodFixtureUnchanged = JSON.stringify(goodFixture) === goodFixtureBytes;
    const goodResultUnchanged = JSON.stringify(goodResult) === goodResultBytes;

    console.log("TASK0181_PERSON_READABLE=" + sourceIds.includes(PERSON));
    console.log("TASK0181_PERSON=" + PERSON);
    console.log("TASK0181_SOURCE_COUNT_BEFORE=" + sourceCountBefore);
    console.log("TASK0181_GOOD_BUILD_COUNT=" + goodResult.count);
    console.log("TASK0181_GOOD_BUILD_NAMES=" + goodResult.names.join(","));
    console.log("TASK0181_CHANGED_RELATIONSHIP=one-way");
    console.log("TASK0181_CHANGED_RELATIONSHIP_TYPE=" + badFixture.relationships[0].type);
    console.log("TASK0181_ONLY_CHANGED_FIELD=" + changed.join(","));
    console.log("TASK0181_BAD_REFUSED_AS_NOT_ELIGIBLE=" + refusedAsNotEligible);
    console.log("TASK0181_BAD_BUILD_COUNT=" + badResult.count);
    console.log("TASK0181_GOOD_FIXTURE_BYTES_UNCHANGED=" + goodFixtureUnchanged);
    console.log("TASK0181_GOOD_RESULT_BYTES_UNCHANGED=" + goodResultUnchanged);

    expect(sourceIds).toEqual([PERSON]);
    expect(sourceCountBefore).toBe(1);
    expect(goodResult).toEqual({ count: 1, names: [PERSON] });
    expect(changed).toEqual(["relationships.0.type"]);
    expect(refusedAsNotEligible).toBe(true);
    expect(badResult).toEqual({ count: 0, names: [] });
    expect(goodFixtureUnchanged).toBe(true);
    expect(goodResultUnchanged).toBe(true);
  });
});
