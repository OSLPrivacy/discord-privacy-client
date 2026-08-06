import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, relative, resolve } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
export const DEFAULT_LADDER_PATH = resolve(
  HERE,
  "../ROW-OWNERSHIP-PROOF-LADDER.json",
);
const FORMAT = "osl.row-ownership-proof-ladder.v1";

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

function requireString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be a nonempty string`);
  }
  return value;
}

function requireStringArray(value, label) {
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    value.some((item) => typeof item !== "string" || item.length === 0)
  ) {
    throw new Error(`${label} must be a nonempty string array`);
  }
  return value;
}

function kindById(ladder, id) {
  return ladder.evidence.find((kind) => kind.id === id);
}

export function readRowOwnershipLadder(path = DEFAULT_LADDER_PATH) {
  const ladder = JSON.parse(readFileSync(path, "utf8"));
  return validateRowOwnershipLadder(ladder);
}

export function validateRowOwnershipLadder(value) {
  const ladder = requireObject(value, "ladder");
  if (ladder.format !== FORMAT) {
    throw new Error("row ownership ladder format is unsupported");
  }
  const forbiddenBases = requireStringArray(
    ladder.forbidden_bases,
    "ladder.forbidden_bases",
  );
  const evidence = ladder.evidence;
  if (!Array.isArray(evidence) || evidence.length < 4) {
    throw new Error("row ownership ladder needs at least 4 evidence kinds");
  }

  const seen = new Set();
  evidence.forEach((raw, index) => {
    const kind = requireObject(raw, `ladder.evidence[${index}]`);
    if (kind.rank !== index + 1) {
      throw new Error("row ownership ladder ranks must be contiguous and ordered");
    }
    const id = requireString(kind.id, `ladder.evidence[${index}].id`);
    if (seen.has(id)) throw new Error(`duplicate evidence kind: ${id}`);
    seen.add(id);
    if (!["strong", "weak"].includes(kind.strength)) {
      throw new Error(`${id} strength must be strong or weak`);
    }
    if (typeof kind.allowed_to_mark !== "boolean") {
      throw new Error(`${id} allowed_to_mark must be boolean`);
    }
    requireStringArray(kind.basis, `${id}.basis`);
    requireString(kind.why, `${id}.why`);
  });

  const top = evidence[0];
  if (top.id !== "discord_row_account_number_matches_account_panel_number") {
    throw new Error("Discord account-number comparison must be the top example");
  }
  for (const required of [
    "row_account_number",
    "signed_in_account_panel_account_number",
  ]) {
    if (!top.basis.includes(required)) {
      throw new Error(`Discord top example is missing ${required}`);
    }
  }

  const floor = requireString(
    ladder.lowest_allowed_evidence_kind,
    "ladder.lowest_allowed_evidence_kind",
  );
  const floorKind = kindById({ evidence }, floor);
  if (!floorKind || !floorKind.allowed_to_mark) {
    throw new Error("lowest allowed evidence kind must exist and allow marking");
  }
  for (const kind of evidence) {
    if ((kind.rank <= floorKind.rank) !== kind.allowed_to_mark) {
      throw new Error(`${kind.id} is on the wrong side of the marking floor`);
    }
  }

  const forbiddenEvidence = evidenceKindsUsingForbiddenBasis({
    ...ladder,
    evidence,
    forbidden_bases: forbiddenBases,
  });
  if (forbiddenEvidence.length !== 0) {
    throw new Error(
      `position or bubble colour cannot be evidence: ${forbiddenEvidence.join(",")}`,
    );
  }
  return Object.freeze({
    format: ladder.format,
    lowest_allowed_evidence_kind: floor,
    forbidden_bases: Object.freeze([...forbiddenBases]),
    evidence: Object.freeze(evidence.map((kind) => Object.freeze({ ...kind }))),
  });
}

export function evidenceKindsUsingForbiddenBasis(ladder) {
  const forbidden = new Set(ladder.forbidden_bases);
  return ladder.evidence
    .filter((kind) => kind.basis.some((basis) => forbidden.has(basis)))
    .map((kind) => kind.id);
}

export function assessRowOwnershipEvidence(ladder, app, evidenceId) {
  if (!/^[a-z][a-z0-9_-]{0,63}$/.test(app)) {
    throw new Error("invalid app name");
  }
  const kind = kindById(ladder, evidenceId);
  if (!kind) throw new Error(`unknown evidence kind: ${evidenceId}`);
  if (!kind.allowed_to_mark) {
    return {
      ok: false,
      line: `refused app=${app} evidence=${kind.id} below_minimum=${ladder.lowest_allowed_evidence_kind} reason=${kind.why}`,
    };
  }
  return {
    ok: true,
    line: `allowed app=${app} evidence=${kind.id} minimum=${ladder.lowest_allowed_evidence_kind}`,
  };
}

export function describeRowOwnershipLadder(ladder) {
  const top = ladder.evidence[0];
  return [
    `format=${ladder.format}`,
    `evidence_kind_count=${ladder.evidence.length}`,
    `lowest_allowed_evidence_kind=${ladder.lowest_allowed_evidence_kind}`,
    `top_discord_compares=${top.basis.join(",")}`,
    `position_or_bubble_colour_evidence_kinds=${evidenceKindsUsingForbiddenBasis(ladder).length}`,
    ...ladder.evidence.map(
      (kind) =>
        `rank=${kind.rank} id=${kind.id} strength=${kind.strength} allowed_to_mark=${kind.allowed_to_mark} why=${kind.why}`,
    ),
  ].join("\n");
}

export function parseArgs(argv) {
  const args = { ladder: DEFAULT_LADDER_PATH, list: false };
  for (let index = 2; index < argv.length; index += 1) {
    const key = argv[index];
    if (key === "--list") {
      args.list = true;
      continue;
    }
    const value = argv[index + 1];
    if (!key?.startsWith("--") || value === undefined) {
      throw new Error(
        "usage: row-ownership-ladder.mjs [--ladder <json>] --list | --app <app> --evidence <kind>",
      );
    }
    args[key.slice(2)] = value;
    index += 1;
  }
  return args;
}

function displayPath(path) {
  return relative(process.cwd(), path) || path;
}

function main(argv) {
  let args;
  try {
    args = parseArgs(argv);
    const ladder = readRowOwnershipLadder(resolve(args.ladder));
    console.log(`ladder=${displayPath(resolve(args.ladder))}`);
    if (args.list) {
      console.log(describeRowOwnershipLadder(ladder));
      return 0;
    }
    if (typeof args.app !== "string" || typeof args.evidence !== "string") {
      throw new Error("missing --app or --evidence");
    }
    const assessment = assessRowOwnershipEvidence(
      ladder,
      args.app,
      args.evidence,
    );
    console.log(assessment.line);
    return assessment.ok ? 0 : 1;
  } catch (error) {
    console.error(error.message);
    return 2;
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  process.exit(main(process.argv));
}
