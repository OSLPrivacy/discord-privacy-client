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

function moveAbortGuard(source: string, destinationNeedle: string): string {
  const start = source.indexOf("        if (abortFailure) {");
  const end = source.indexOf("        if (completedObject) {", start);
  if (start < 0 || end < 0) throw new Error("missing abort guard mutation");
  const block = source.slice(start, end);
  const without = source.slice(0, start) + source.slice(end);
  const destination = without.indexOf(destinationNeedle, start);
  if (destination < 0) throw new Error("missing abort guard destination");
  return without.slice(0, destination) + block + without.slice(destination);
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

  it("rejects widening the consumed-upload classifier", () => {
    const value = sources();
    value.sweep = value.sweep.replace(
      'return candidate.code === "NoSuchUpload"\n    || candidate.name === "NoSuchUpload";',
      "return true;",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /classifier is widened/,
    );
  });

  it("rejects treating every abort error as retryable instead of terminal", () => {
    const value = sources();
    value.sweep = value.sweep.replace(
      "abortFailure = isConsumedMultipartUpload(error) ? null : error;",
      "abortFailure = error;",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /post-abort HEAD|terminal consumed-upload aborts/,
    );
  });

  it("rejects classifying AccessDenied as a consumed upload", () => {
    const value = sources();
    value.sweep = value.sweep.replace(
      '    || candidate.name === "NoSuchUpload";',
      `    || candidate.name === "NoSuchUpload"
    || candidate.code === "AccessDenied";`,
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /classifier is widened/,
    );
  });

  it("rejects a function-local unconditional classifier shadow", () => {
    const value = sources();
    value.sweep = mutateFunction(
      value.sweep,
      "sweepExpiredAttachments",
      "  const now = Math.floor(Date.now() / 1000);",
      `  const isConsumedMultipartUpload = (_error: unknown): boolean => true;
  const now = Math.floor(Date.now() / 1000);`,
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /locally shadows.*classifier/,
    );
  });

  it("rejects moving abort failure handling after object deletion", () => {
    const value = sources();
    value.sweep = moveAbortGuard(
      value.sweep,
      `        if (claim.storage_fence_state !== "object_absent_confirmed") {`,
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /ordering is unsafe/,
    );
  });

  it("rejects moving abort failure handling after absence confirmation", () => {
    const value = sources();
    value.sweep = moveAbortGuard(
      value.sweep,
      "        const completion = await completeAttachmentSweepClaim",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /ordering is unsafe/,
    );
  });

  it("rejects absence CAS without lease version in the UPDATE", () => {
    const value = sources();
    value.claims = value.claims.replace(
      "        AND lease_version = ?\n        AND lease_expires_at > ?\n        AND EXISTS (",
      "        AND lease_expires_at > ?\n        AND EXISTS (",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /absence marker UPDATE.*lease-version CAS/,
    );
  });

  it("rejects unsafe absence SQL hidden behind an unused exact SQL decoy", () => {
    const value = sources();
    value.claims = mutateFunction(
      value.claims,
      "confirmAttachmentObjectAbsent",
      `  validateClaim(claim);
`,
      `  validateClaim(claim);
  void \`UPDATE attachment_sweep_claims SET storage_fence_state = 'object_absent_confirmed' WHERE attachment_id = ? AND worker_id = ? AND claim_token = ? AND lease_version = ? AND lease_expires_at > ?\`;
`,
    );
    value.claims = mutateFunction(
      value.claims,
      "confirmAttachmentObjectAbsent",
      `        AND lease_version = ?
        AND lease_expires_at > ?
        AND EXISTS (`,
      `        AND lease_expires_at > ?
        AND EXISTS (`,
    );
    value.claims = mutateFunction(
      value.claims,
      "confirmAttachmentObjectAbsent",
      `    claim.claim_token,
    claim.lease_version,
    now,
`,
      `    claim.claim_token,
    now,
`,
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /absence marker UPDATE.*lease-version CAS/,
    );
  });

  it("rejects reordered absence CAS bindings", () => {
    const value = sources();
    value.claims = mutateFunction(
      value.claims,
      "confirmAttachmentObjectAbsent",
      `    claim.claim_token,
    claim.lease_version,
`,
      `    claim.lease_version,
    claim.claim_token,
`,
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /reordered CAS bindings/,
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

  it("rejects metadata deletion without the durable absence fence", () => {
    const value = sources();
    value.claims = mutateFunction(
      value.claims,
      "completeAttachmentSweepClaim",
      `             AND (
               attachment_objects.state <> 'completing'
               OR owned.storage_fence_state = 'object_absent_confirmed'
             )
`,
      "",
    );
    expect(() => validateAttachmentPredecessorAdoption(value)).toThrow(
      /metadata deletion.*confirmed R2 absence/,
    );
  });
});
