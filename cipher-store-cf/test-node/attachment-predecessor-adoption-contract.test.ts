import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  validateAttachmentPredecessorAdoption,
  type AttachmentPredecessorAdoptionSources,
} from "../scripts/attachment-predecessor-adoption-contract.js";

function read(relative: string): string {
  return readFileSync(
    fileURLToPath(new URL(`../${relative}`, import.meta.url)),
    "utf8",
  );
}

function sources(): AttachmentPredecessorAdoptionSources {
  return {
    migration: read("migrations/0009_predecessor_completing_adoption.sql"),
    claims: read("src/lib/attachment-sweep-claims.ts"),
    sweep: read("src/lib/sweep.ts"),
  };
}

function mutateFunction(
  source: string,
  name: string,
  before: string,
  after: string,
): string {
  const start = source.indexOf(`export async function ${name}`);
  if (start < 0) throw new Error(`missing mutation function ${name}`);
  const prefix = source.slice(0, start);
  const tail = source.slice(start);
  const mutated = tail.replace(before, after);
  if (mutated === tail) throw new Error(`mutation did not apply in ${name}`);
  return prefix + mutated;
}

describe("attachment predecessor adoption source closure", () => {
  it("accepts the nonempty shipping migration/claim/R2 closure", () => {
    expect(validateAttachmentPredecessorAdoption(sources())).toEqual({
      boundedMarker: true,
      predecessorOnlyAdoption: true,
      activeLineageLeasePreserved: true,
      postAbortHeadRequired: true,
      wrongSizeAbortRequired: true,
      exactReadyVersionCas: true,
      exactAbsenceVersionCas: true,
    });
  });

  it("rejects the old lineage-required completing predicate", () => {
    const value = sources();
    value.claims = value.claims.replace(
      `          OR (
            adoption.format = 'osl.cipher-store.predecessor-adoption.v1'
            AND adoption.max_claims_per_cycle = 100
            AND candidate.created_at <= adoption.eligible_created_through
          )
`,
      "",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /old lineage-only predicate/,
    );
  });

  it("rejects removal of the post-abort HEAD", () => {
    const value = sources();
    value.sweep = value.sweep.replace(
      "completedObject = await env.ATTACHMENTS.head(claim.object_key);\n        }\n        if (completedObject?.size",
      "completedObject = null;\n        }\n        if (completedObject?.size",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /post-abort HEAD/,
    );
  });

  it("rejects reverting wrong-size abort to empty-only", () => {
    const value = sources();
    value.sweep = value.sweep.replace(
      "completedObject?.size !== claim.size_bytes",
      "!completedObject",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /wrong-size objects.*abort fence/,
    );
  });

  it("rejects disabling the wrong-size abort branch", () => {
    const value = sources();
    value.sweep = value.sweep.replace(
      "completedObject?.size !== claim.size_bytes",
      "false",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /wrong-size objects.*abort fence/,
    );
  });

  it("rejects removal of the ready lease-version CAS", () => {
    const value = sources();
    value.claims = mutateFunction(
      value.claims,
      "finalizeAttachmentReadyClaim",
      "             AND owned.lease_version = ?\n",
      "",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /ready publication.*version/,
    );
  });
});
