import ts from "typescript";

export interface AttachmentPredecessorAdoptionSources {
  migration: string;
  claims: string;
  sweep: string;
}

export interface AttachmentPredecessorAdoptionFacts {
  boundedMarker: true;
  predecessorOnlyAdoption: true;
  activeLineageLeasePreserved: true;
  postAbortHeadRequired: true;
  wrongSizeAbortRequired: true;
  exactReadyVersionCas: true;
  exactAbsenceVersionCas: true;
}

function fail(message: string): never {
  throw new Error(`attachment predecessor adoption contract: ${message}`);
}

function functionText(
  source: string,
  name: string,
): string {
  const parsed = ts.createSourceFile(
    `${name}.ts`,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );
  let found: ts.FunctionDeclaration | null = null;
  const visit = (node: ts.Node): void => {
    if (
      ts.isFunctionDeclaration(node)
      && node.name?.text === name
    ) {
      found = node;
      return;
    }
    node.forEachChild(visit);
  };
  visit(parsed);
  if (!found) fail(`missing function ${name}`);
  return (found as ts.FunctionDeclaration).getText(parsed);
}

function compact(value: string): string {
  return value
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/\/\/[^\n]*/g, " ")
    .replace(/\s+/g, " ");
}

function requirePattern(
  value: string,
  pattern: RegExp,
  label: string,
): void {
  if (!pattern.test(value)) fail(label);
}

export function validateAttachmentPredecessorAdoption(
  sources: AttachmentPredecessorAdoptionSources,
): AttachmentPredecessorAdoptionFacts {
  const migration = sources.migration.replace(/--[^\n]*/g, "");
  const statements = migration
    .split(";")
    .map((statement) => compact(statement).trim())
    .filter(Boolean);
  if (statements.length !== 4) {
    fail("0009 migration statement count is not exact");
  }
  requirePattern(
    statements[0]!,
    /^ALTER TABLE attachment_sweep_claims ADD COLUMN claim_origin /,
    "claim origin migration is absent",
  );
  requirePattern(
    statements[1]!,
    /^ALTER TABLE attachment_sweep_claims ADD COLUMN storage_fence_state /,
    "storage fence migration is absent",
  );
  requirePattern(
    statements[2]!,
    /^CREATE TABLE attachment_predecessor_adoption /,
    "bounded predecessor marker table is absent",
  );
  requirePattern(
    statements[3]!,
    /unixepoch\(\), unixepoch\(\) \+ 3600, 100\)/,
    "predecessor marker window or cycle bound is not exact",
  );

  const claim = compact(
    functionText(sources.claims, "claimNextExpiredAttachment"),
  );
  requirePattern(
    claim,
    /LEFT JOIN attachment_predecessor_adoption AS adoption ON adoption\.singleton = 1/,
    "claim does not consume the migration-owned adoption marker",
  );
  requirePattern(
    claim,
    /candidate\.state <> 'completing' OR existing\.attachment_id IS NOT NULL OR \( adoption\.format = 'osl\.cipher-store\.predecessor-adoption\.v1' AND adoption\.max_claims_per_cycle = 100 AND candidate\.created_at <= adoption\.eligible_created_through \)/,
    "old lineage-only predicate still excludes bounded predecessor rows",
  );
  requirePattern(
    claim,
    /existing\.attachment_id IS NULL OR \( existing\.lease_expires_at <= \? AND existing\.retry_not_before <= \? \)/,
    "active lineaged leases are no longer excluded",
  );
  requirePattern(
    claim,
    /WHEN candidate\.state = 'completing' AND existing\.attachment_id IS NULL THEN 'predecessor_adoption' ELSE 'sweep'/,
    "adopted predecessor claims are not explicitly marked",
  );

  const sweep = compact(
    functionText(sources.sweep, "sweepExpiredAttachments"),
  );
  requirePattern(
    sweep,
    /completedObject = await env\.ATTACHMENTS\.head\(claim\.object_key\); let abortFailure:[^;]+ = null;[\s\S]*?\.abort\(\); \} catch \(error\) \{ abortFailure = isConsumedMultipartUpload\(error\) \? null : error; \} completedObject = await env\.ATTACHMENTS\.head\(claim\.object_key\);/,
    "abort is not bracketed by HEAD and mandatory post-abort HEAD",
  );
  requirePattern(
    sweep,
    /completedObject\?\.size !== claim\.size_bytes[\s\S]*?claim\.storage_fence_state !== "object_absent_confirmed"[\s\S]*?resumeMultipartUpload\(claim\.object_key, claim\.upload_id\)/,
    "wrong-size objects do not unconditionally enter the multipart abort fence",
  );
  requirePattern(
    sources.sweep,
    /function isConsumedMultipartUpload\(error: unknown\): boolean[\s\S]*?NoSuchUpload[\s\S]*?abortFailure = isConsumedMultipartUpload\(error\) \? null : error/,
    "retry does not distinguish terminal consumed-upload aborts from unknown failures",
  );
  requirePattern(
    sweep,
    /if \(abortFailure\) \{[\s\S]*?throw abortFailure; \}[\s\S]*?if \(completedObject\) \{[\s\S]*?delete\(claim\.object_key\)[\s\S]*?confirmAttachmentObjectAbsent\(env, claim, now\)[\s\S]*?completeAttachmentSweepClaim\(env, claim, now\)/,
    "ambiguous abort or absence-marker ordering is unsafe",
  );

  const ready = compact(
    functionText(sources.claims, "finalizeAttachmentReadyClaim"),
  );
  requirePattern(
    ready,
    /owned\.worker_id = \? AND owned\.claim_token = \? AND owned\.lease_version = \? AND owned\.lease_expires_at > \?/,
    "ready publication is missing exact token/version lease CAS",
  );
  const absence = compact(
    functionText(sources.claims, "confirmAttachmentObjectAbsent"),
  );
  requirePattern(
    absence,
    /worker_id = \? AND claim_token = \? AND lease_version = \? AND lease_expires_at > \?/,
    "absence marker is missing exact token/version lease CAS",
  );
  const completion = compact(
    functionText(sources.claims, "completeAttachmentSweepClaim"),
  );
  requirePattern(
    completion,
    /owned\.worker_id = \? AND owned\.claim_token = \? AND owned\.lease_version = \? AND owned\.lease_expires_at > \? AND \( attachment_objects\.state <> 'completing' OR owned\.storage_fence_state = 'object_absent_confirmed' \)/,
    "metadata deletion is not version-fenced by confirmed R2 absence",
  );

  return {
    boundedMarker: true,
    predecessorOnlyAdoption: true,
    activeLineageLeasePreserved: true,
    postAbortHeadRequired: true,
    wrongSizeAbortRequired: true,
    exactReadyVersionCas: true,
    exactAbsenceVersionCas: true,
  };
}
