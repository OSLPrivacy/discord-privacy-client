import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  generatedCapabilityLabel,
  type GeneratedCapabilityLabel,
  type ServiceCapabilityFacts,
} from "./services";

const EXPECTED_TILES = ["osl-chats", "discord", "scrub", "osl-mail"] as const;
const GENERATED_LABELS = ["Ready", "Placing only", "Reading only", "Opens the app", "Not started"] as const;
const fixtureDirectory = path.dirname(fileURLToPath(import.meta.url));
const defaultFixture = path.join(fixtureDirectory, "fixtures/task-0806-generated-label-records.json");
const fixturePath = process.env.TASK0806_LABEL_FIXTURE
  ? path.resolve(process.env.TASK0806_LABEL_FIXTURE)
  : defaultFixture;

type GeneratedLabelRecord = {
  tileId: (typeof EXPECTED_TILES)[number];
  name: string;
  capabilityFacts: ServiceCapabilityFacts;
  generatedLabel: GeneratedCapabilityLabel;
};

function isCapabilityFacts(value: unknown): value is ServiceCapabilityFacts {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const facts = value as Record<string, unknown>;
  const keys = ["placing", "reading", "opening", "realTwoPersonProtectedMessaging"];
  return Object.keys(facts).length === keys.length
    && keys.every((key) => typeof facts[key] === "boolean");
}

function generatedLabelRecords(pathname: string): GeneratedLabelRecord[] {
  const value: unknown = JSON.parse(readFileSync(pathname, "utf8"));
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`TASK0806 invalid fixture: ${pathname}`);
  }
  const records = (value as { generatedLabelRecords?: unknown }).generatedLabelRecords;
  if (!Array.isArray(records)) {
    throw new Error(`TASK0806 fixture has no generatedLabelRecords: ${pathname}`);
  }
  if (records.length !== EXPECTED_TILES.length) {
    throw new Error(`TASK0806 fixture must contain ${EXPECTED_TILES.length} generated label records: ${pathname}`);
  }

  const seen = new Set<string>();
  return records.map((record) => {
    if (typeof record !== "object" || record === null || Array.isArray(record)) {
      throw new Error(`TASK0806 invalid generated label record: ${pathname}`);
    }
    const { tileId, name, capabilityFacts, generatedLabel } = record as Record<string, unknown>;
    if (!EXPECTED_TILES.includes(tileId as (typeof EXPECTED_TILES)[number])
      || seen.has(String(tileId))
      || typeof name !== "string"
      || !isCapabilityFacts(capabilityFacts)
      || !GENERATED_LABELS.includes(generatedLabel as GeneratedCapabilityLabel)) {
      throw new Error(`TASK0806 invalid or duplicate generated label record: ${pathname}`);
    }
    seen.add(tileId as string);
    return { tileId, name, capabilityFacts, generatedLabel } as GeneratedLabelRecord;
  });
}

describe("TASK 0806 generated Home tile label proof", () => {
  it("generates every required label from the fixture capability facts", () => {
    const records = generatedLabelRecords(fixturePath);
    console.info(`TASK0806_FIXTURE_PATH=${path.relative(process.cwd(), fixturePath)}`);

    for (const record of records) {
      const generatedLabel = generatedCapabilityLabel(record.capabilityFacts);
      console.info(`TASK0806_GENERATED_LABEL ${record.name} => ${generatedLabel}`);
      expect(generatedLabel, `${record.name} generated the wrong tile label`).toBe(record.generatedLabel);
    }
  });
});
