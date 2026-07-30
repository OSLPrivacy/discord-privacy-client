import ts from "typescript";

export interface AttachmentPredecessorAdoptionSources {
  migration: string;
  recoveryMigration: string;
  claims: string;
  sweep: string;
}

export interface AttachmentPredecessorAdoptionFacts {
  boundedMarker: true;
  continuousRecoveryMarker: true;
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
  const { node, parsed } = functionNode(source, name);
  return node.getText(parsed);
}

function functionNode(
  source: string,
  name: string,
): { node: ts.FunctionDeclaration; parsed: ts.SourceFile } {
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
  return { node: found as ts.FunctionDeclaration, parsed };
}

function bindingContains(
  name: ts.BindingName,
  expected: string,
): boolean {
  if (ts.isIdentifier(name)) return name.text === expected;
  return name.elements.some((element) =>
    !ts.isOmittedExpression(element)
    && bindingContains(element.name, expected)
  );
}

function functionHasLocalBinding(
  source: string,
  functionName: string,
  bindingName: string,
): boolean {
  const { node } = functionNode(source, functionName);
  let found = false;
  const visit = (child: ts.Node): void => {
    if (
      (ts.isVariableDeclaration(child) || ts.isParameter(child))
      && bindingContains(child.name, bindingName)
    ) {
      found = true;
      return;
    }
    if (
      (
        ts.isFunctionDeclaration(child)
        || ts.isClassDeclaration(child)
        || ts.isEnumDeclaration(child)
      )
      && child.name?.text === bindingName
    ) {
      found = true;
      return;
    }
    child.forEachChild(visit);
  };
  node.body?.forEachChild(visit);
  return found;
}

interface PreparedStatement {
  sql: string;
  bindArguments: string[];
}

function preparedStatements(
  source: string,
  functionName: string,
  marker: string,
): PreparedStatement[] {
  const { node, parsed } = functionNode(source, functionName);
  const statements: PreparedStatement[] = [];
  const visit = (child: ts.Node): void => {
    if (
      ts.isCallExpression(child)
      && ts.isPropertyAccessExpression(child.expression)
      && child.expression.name.text === "prepare"
      && compact(child.expression.expression.getText(parsed)) === "env.DB"
      && child.arguments.length === 1
    ) {
      const argument = child.arguments[0]!;
      const sql = (
        ts.isStringLiteral(argument)
        || ts.isNoSubstitutionTemplateLiteral(argument)
      )
        ? argument.text
        : "";
      if (sql.includes(marker)) {
        const bindAccess = child.parent;
        const bindCall = ts.isPropertyAccessExpression(bindAccess)
          && bindAccess.expression === child
          && bindAccess.name.text === "bind"
          && ts.isCallExpression(bindAccess.parent)
          && bindAccess.parent.expression === bindAccess
          ? bindAccess.parent
          : null;
        statements.push({
          sql,
          bindArguments: bindCall
            ? bindCall.arguments.map((value) => compact(value.getText(parsed)))
            : [],
        });
      }
    }
    child.forEachChild(visit);
  };
  node.body?.forEachChild(visit);
  return statements;
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
  const recoveryStatements = sources.recoveryMigration
    .replace(/--[^\n]*/g, "")
    .split(";")
    .map((statement) => compact(statement).trim())
    .filter(Boolean);
  if (recoveryStatements.length !== 2) {
    fail("0010 recovery migration statement count is not exact");
  }
  requirePattern(
    recoveryStatements[0]!,
    /^CREATE TABLE attachment_predecessor_recovery /,
    "continuous predecessor recovery marker table is absent",
  );
  requirePattern(
    recoveryStatements[1]!,
    /VALUES \(1, 'osl\.cipher-store\.continuous-predecessor-recovery\.v1', 100\)$/,
    "continuous predecessor recovery marker is not exact",
  );

  const claim = compact(
    functionText(sources.claims, "claimNextExpiredAttachment"),
  );
  requirePattern(
    claim,
    /LEFT JOIN attachment_predecessor_recovery AS recovery ON recovery\.singleton = 1/,
    "claim does not consume the migration-owned recovery marker",
  );
  if (/candidate\.created_at <= adoption\.eligible_created_through/.test(claim)) {
    fail("legacy creation-time cutoff can strand unlineaged completing rows");
  }
  requirePattern(
    claim,
    /candidate\.state <> 'completing' OR existing\.attachment_id IS NOT NULL OR \( recovery\.format = 'osl\.cipher-store\.continuous-predecessor-recovery\.v1' AND recovery\.max_claims_per_cycle = 100 \)/,
    "unlineaged completing rows are not continuously recoverable",
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
    /completedObject\?\.size !== claim\.size_bytes[\s\S]*?completedObject !== null[\s\S]*?claim\.storage_fence_state !== "object_absent_confirmed"[\s\S]*?resumeMultipartUpload\(claim\.object_key, claim\.upload_id\)/,
    "wrong-size objects do not unconditionally enter the multipart abort fence",
  );
  requirePattern(
    compact(sources.sweep),
    /function isConsumedMultipartUpload\(error: unknown\): boolean \{ if \(typeof error !== "object" \|\| error === null\) return false; const candidate = error as \{ code\?: unknown; name\?: unknown \}; return candidate\.code === "NoSuchUpload" \|\| candidate\.name === "NoSuchUpload"; \}/,
    "consumed-upload classifier is widened beyond exact terminal signals",
  );
  requirePattern(
    compact(sources.sweep),
    /function isConsumedMultipartUpload\(error: unknown\): boolean[\s\S]*?NoSuchUpload[\s\S]*?abortFailure = isConsumedMultipartUpload\(error\) \? null : error/,
    "retry does not distinguish terminal consumed-upload aborts from unknown failures",
  );
  if (
    functionHasLocalBinding(
      sources.sweep,
      "sweepExpiredAttachments",
      "isConsumedMultipartUpload",
    )
  ) {
    fail("sweep locally shadows the exact consumed-upload classifier");
  }
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
  const absenceStatements = preparedStatements(
    sources.claims,
    "confirmAttachmentObjectAbsent",
    "SET storage_fence_state = 'object_absent_confirmed'",
  );
  if (absenceStatements.length !== 1) {
    fail("absence marker is not bound to one executable prepare call");
  }
  const absenceStatement = absenceStatements[0]!;
  const absence = compact(absenceStatement.sql);
  requirePattern(
    absence,
    /UPDATE attachment_sweep_claims SET storage_fence_state = 'object_absent_confirmed' WHERE attachment_id = \? AND worker_id = \? AND claim_token = \? AND lease_version = \? AND lease_expires_at > \?/,
    "absence marker UPDATE is missing exact lease-version CAS",
  );
  requirePattern(
    absence,
    /worker_id = \? AND claim_token = \? AND lease_version = \? AND lease_expires_at > \?/,
    "absence marker is missing exact token/version lease CAS",
  );
  if (
    absenceStatement.bindArguments.join(",") !== [
      "claim.attachment_id",
      "claim.worker_id",
      "claim.claim_token",
      "claim.lease_version",
      "now",
    ].join(",")
  ) {
    fail("absence marker prepare call has stale or reordered CAS bindings");
  }
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
    continuousRecoveryMarker: true,
    predecessorOnlyAdoption: true,
    activeLineageLeasePreserved: true,
    postAbortHeadRequired: true,
    wrongSizeAbortRequired: true,
    exactReadyVersionCas: true,
    exactAbsenceVersionCas: true,
  };
}
