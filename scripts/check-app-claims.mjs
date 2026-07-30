#!/usr/bin/env node

// WHAT THIS GATE CANNOT DO, BY CONSTRUCTION (recorded 2026-07-27)
//
// This is a STRING gate. It answers exactly one question: does this text
// contain a phrase section D forbids? It cannot answer whether the code behind
// the text is reachable.
//
// The case that proves the limit: `osl_notes` has UI command strings that are
// entirely truthful sentences, and no backend command registered anywhere. This
// gate reads the string, finds nothing forbidden, and passes -- correctly, on
// its own terms. The sentence is not a lie about what the code does; it is a
// true sentence about code that is not wired. No property of the STRING
// distinguishes it from the same sentence about working code.
//
// A narrow reachability check bolted on here would cover almost nothing --
// registry ids are not command names, and most UI strings carry no capability
// marker at all -- while making this gate LOOK more complete. That is the
// false-confidence failure this file exists to prevent.
//
// That class is caught by a DIFFERENT gate with a different input: a
// reachability sweep over generate_handler! and the call graph. It belongs with
// whoever owns app Rust. See "F0" in docs/design/osl-public-claim-allowlist.md.

import { promises as fs } from "node:fs";
import { createHash } from "node:crypto";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const REPO_ROOT = path.resolve(import.meta.dirname, "..");
const ALLOWLIST_PATH = path.join(
  REPO_ROOT,
  "docs/design/osl-public-claim-allowlist.md",
);
const APP_SRC_ROOT = path.join(REPO_ROOT, "apps/osl-hub-ui/src");
const RUST_APP_SRC_ROOT = path.join(REPO_ROOT, "apps/osl-hub/src");
const README_PATH = path.join(REPO_ROOT, "README.md");
const TS_TEST_WORKFLOW_PATH = path.join(REPO_ROOT, ".github/workflows/ts-test.yml");
const SUPPORT_MATRIX_PATH = path.join(REPO_ROOT, "docs/status/support-matrix.json");
const GATE_SOURCE_PATH = fileURLToPath(import.meta.url);
const GATE_CONTRACT_PATTERN =
  /^> Claim-gate source SHA-256: `([0-9a-f]{64})`$/m;

const MIN_BANNED_PHRASES = 8; // Prevents a malformed section-D parse from approving everything.
const MIN_TS_STRING_LITERALS = 300; // Ensures the app copy scan cannot pass after extracting nothing.
const MIN_RUST_STRING_LITERALS = 40; // Ensures the Rust high-precision subset cannot pass after extracting nothing.
const MIN_README_BYTES = 1; // Ensures the public README claim surface was actually scanned.
const MIN_CHAT_APP_EVIDENCE_APPS = 2; // Discord plus Signal are the current dependency-bound app evidence rows.

const REQUIRED_ATTACHMENT_BANS = [
  "discord attachment scanning defeated",
  "defeats discord attachment scanning",
  "discord cannot scan attachments",
  "discord sees only decoys",
  "discord's attachment scanner is defeated by osl",
  "osl bypasses discord's attachment inspection",
  "discord receives harmless cover files instead of the attachment",
  "uploaded files are opaque to discord's scanners",
];
const REQUIRED_BURN_BANS = [
  "cryptographic burn",
  "destroys keys, not messages",
  "burn makes messages unrecoverable",
  "burn removes recipient copies",
  "burn deletes provider messages",
  "burn removes provider messages",
  "burn deletes discord messages",
  "burn removes discord messages",
  "burn unsends messages",
  "permanent ciphertext",
  "permanent gibberish",
  "mathematically opaque",
  "disappears forever",
  "permanently undecryptable",
  "gone for good",
];
const REQUIRED_SUPPORT_BANS = [
  "works on gmail",
  "works on discord",
  "works on signal",
  "works on whatsapp",
  "works on telegram",
  "works on outlook",
  "signal support",
  "signal support is available",
  "whatsapp support",
  "whatsapp support is available",
  "telegram support",
  "outlook support",
  "osl mail support",
  "osl supports signal",
  "osl supports whatsapp",
  "supports gmail",
  "supports discord",
  "supports signal",
  "supports whatsapp",
  "supports telegram",
  "supports outlook",
  "available on gmail",
  "available on discord",
  "available on signal",
  "available on whatsapp",
  "available on telegram",
  "available on outlook",
];
const REQUIRED_CONDITIONAL_APP_EVIDENCE = [
  {
    id: "telegram_desktop_native",
    service: "Telegram",
    claimScope: "protected_native_adapter",
    status: "externally_blocked",
    evidenceReport: "docs/reports/telegram-adapter-verdict.md",
    evidenceAnchor: "TelegramSupportVerdict",
    requiredBoundary: /\bsigned-client UI Automation probe\b.*\bstable, text-exposed message rows\b/i,
    reportVerdict: /Telegram Desktop remains `externally blocked`/i,
    publicClaimPattern: /\btelegram(?:\s+desktop)?\b/i,
    publicSupportClaimPattern:
      /\b(?:supported|available|ready|works?\s+(?:with|on)|protect(?:s|ed|ion)?|protected\s+(?:send|receive|messag(?:e|ing)|reply|delivery|use))\b/i,
    publicClaimName: "Telegram protected support overclaim",
  },
  {
    id: "outlook_osl_mail",
    service: "Outlook",
    claimScope: "osl_mail",
    status: "unsupported",
    evidenceReport: "docs/reports/outlook-osl-mail-verdict.md",
    evidenceAnchor: "OutlookOslMailSupportVerdict",
    requiredBoundary: /\bOutlook is scoped as OSL Mail\b.*\bnot Outlook chat support\b/i,
    reportVerdict: /Outlook inline-reply support as OSL Mail is `unsupported`/i,
    publicClaimPattern: /\b(?:outlook|osl\s+mail)\b/i,
    publicSupportClaimPattern:
      /\b(?:supported|available|ready|works?\s+(?:with|on)|inline[-\s]+repl(?:y|ies)|protected\s+(?:send|receive|messag(?:e|ing)|reply|delivery|use)|e2ee|end[-\s]+to[-\s]+end)\b/i,
    publicClaimName: "Outlook OSL Mail protected support overclaim",
  },
];
const REQUIRED_VERSIONED_PUBLIC_SUPPORT_MATRIX = {
  schemaVersion: 1,
  matrixVersion: "E7",
  entries: [
    {
      id: "signal_desktop_public",
      service: "Signal",
      publicStatus: "coming_soon",
      claimAllowed: false,
      evidenceStatus: "qa_foundations_only",
      evidence: "docs/design/osl-master-decision-2026-07-26.md:805",
      requiredBoundary: /\bQA foundations only\b.*\bcomplete adapter contract\b.*\btwo-peer exact-build proof\b/i,
    },
    {
      id: "whatsapp_windows_public",
      service: "WhatsApp",
      publicStatus: "coming_soon",
      claimAllowed: false,
      evidenceStatus: "separate_qa_required",
      evidence: "docs/design/osl-master-decision-2026-07-26.md:806",
      requiredBoundary: /\bneeds its own runtime proof\b.*\bcomplete adapter contract\b.*\btwo-peer exact-build proof\b/i,
    },
    {
      id: "telegram_desktop_public",
      service: "Telegram",
      publicStatus: "externally_blocked",
      claimAllowed: false,
      evidenceStatus: "externally_blocked",
      evidence: "docs/reports/telegram-adapter-verdict.md#TelegramSupportVerdict",
      requiredBoundary: /\bsigned-client UI Automation probe\b.*\bstable, text-exposed message rows\b/i,
    },
    {
      id: "osl_mail_public",
      service: "OSL Mail",
      publicStatus: "unsupported",
      claimAllowed: false,
      evidenceStatus: "unsupported",
      evidence: "docs/reports/outlook-osl-mail-verdict.md#OutlookOslMailSupportVerdict",
      requiredBoundary: /\bOutlook is scoped as OSL Mail\b.*\bnot Outlook chat support\b/i,
    },
  ],
};
const REQUIRED_VERSIONED_PUBLIC_SUPPORT_ROWS = [
  {
    id: "signal_desktop_public",
    service: "Signal",
    claimScope: "protected_native_adapter",
    status: "designed_only",
    publicLabel: "Coming later",
    publicStatus: "coming_soon",
    evidenceStatus: "qa_foundations_only",
    evidence: "docs/design/osl-master-decision-2026-07-26.md:805",
    evidenceReport: "docs/design/osl-master-decision-2026-07-26.md",
    limitation: /\bcomplete adapter qualification\b.*\btwo-identity exact-build proof\b/i,
    supportBoundary: /\bQA foundations only\b.*\bcomplete adapter contract\b.*\btwo-peer exact-build proof\b/i,
  },
  {
    id: "whatsapp_windows_public",
    service: "WhatsApp",
    claimScope: "protected_native_adapter",
    status: "designed_only",
    publicLabel: "Coming later",
    publicStatus: "coming_soon",
    evidenceStatus: "separate_qa_required",
    evidence: "docs/design/osl-master-decision-2026-07-26.md:806",
    evidenceReport: "docs/design/osl-master-decision-2026-07-26.md",
    limitation: /\bcurrent adapter qualification\b.*\btwo-identity exact-build proof\b/i,
    supportBoundary: /\bneeds its own runtime proof\b.*\bcomplete adapter contract\b.*\btwo-peer exact-build proof\b/i,
  },
  {
    id: "telegram_desktop_public",
    service: "Telegram",
    claimScope: "protected_native_adapter",
    status: "externally_blocked",
    publicLabel: "Externally blocked",
    publicStatus: "externally_blocked",
    evidenceStatus: "externally_blocked",
    evidence: "docs/reports/telegram-adapter-verdict.md#TelegramSupportVerdict",
    evidenceReport: "docs/reports/telegram-adapter-verdict.md",
    limitation: /\bsigned-client row probe\b.*\bstable, text-exposed conversation rows\b/i,
    supportBoundary: /\bsigned-client UI Automation probe\b.*\bstable, text-exposed message rows\b/i,
  },
  {
    id: "osl_mail_public",
    service: "OSL Mail",
    claimScope: "osl_mail",
    status: "unsupported",
    publicLabel: "Coming later",
    publicStatus: "unsupported",
    evidenceStatus: "unsupported",
    evidence: "docs/reports/outlook-osl-mail-verdict.md#OutlookOslMailSupportVerdict",
    evidenceReport: "docs/reports/outlook-osl-mail-verdict.md",
    limitation: /\bmailbox binding\b.*\brecipient authority\b.*\bdraft handling\b.*\bsafe send behavior\b/i,
    supportBoundary: /\bOutlook is scoped as OSL Mail\b.*\bnot Outlook chat support\b/i,
  },
];
const REQUIRED_CHAT_APP_EVIDENCE = [
  {
    id: "signal_desktop_native",
    service: "Signal",
    claimScope: "protected_native_adapter",
    status: "qualified_profile",
    publicStatus: "coming_soon",
    publicClaimAllowed: false,
    evidenceType: "signed_adapter_profile",
    evidenceReport: "crates/adapter-profile/src/defaults.rs",
    evidenceAnchor: "signal_default_profile",
    trustedAnchor: "signal_default_trusted_signing_key_b64",
    requiredBoundary: /\bsigned data-only Signal support profile exists\b.*\bpublic support remains Coming soon\b/i,
  },
  {
    id: "whatsapp_windows_native",
    service: "WhatsApp",
    claimScope: "protected_native_adapter",
    status: "qualified_profile",
    publicStatus: "coming_soon",
    publicClaimAllowed: false,
    evidenceType: "signed_adapter_profile",
    evidenceReport: "crates/adapter-profile/src/defaults.rs",
    evidenceAnchor: "whatsapp_default_profile",
    trustedAnchor: "whatsapp_default_trusted_signing_key_b64",
    requiredBoundary: /\bsigned data-only WhatsApp support profile exists\b.*\bpublic support remains Coming soon\b/i,
  },
];
const REQUIRED_PUBLIC_SUPPORT_EVIDENCE_LINKS = [
  {
    publicId: "signal_desktop_public",
    evidenceCollection: "chat_app_evidence",
    evidenceId: "signal_desktop_native",
    evidenceStatus: "qualified_profile",
    publicStatus: "coming_soon",
    claimAllowed: false,
  },
  {
    publicId: "whatsapp_windows_public",
    evidenceCollection: "chat_app_evidence",
    evidenceId: "whatsapp_windows_native",
    evidenceStatus: "qualified_profile",
    publicStatus: "coming_soon",
    claimAllowed: false,
  },
  {
    publicId: "telegram_desktop_public",
    evidenceCollection: "conditional_app_evidence",
    evidenceId: "telegram_desktop_native",
    evidenceStatus: "externally_blocked",
    publicStatus: "externally_blocked",
    claimAllowed: false,
  },
  {
    publicId: "osl_mail_public",
    evidenceCollection: "conditional_app_evidence",
    evidenceId: "outlook_osl_mail",
    evidenceStatus: "unsupported",
    publicStatus: "unsupported",
    claimAllowed: false,
  },
];
const SUPPORT_CLAIM_TARGETS = [
  { service: "Signal", pattern: /\bsignal\b/i },
  { service: "WhatsApp", pattern: /\bwhats\s*app\b/i },
  { service: "Telegram", pattern: /\btelegram\b/i },
  { service: "OSL Mail", pattern: /\bosl\s+mail\b|\boutlook\b/i },
];
const SUPPORT_MATRIX_PUBLIC_PROOF_NAME = "Gate public claims against exact support evidence";
const PUBLIC_SUPPORT_LIMITATION_RE =
  /\b(?:coming\s+(?:soon|later)|externally\s+blocked|blocked|unavailable|unsupported|not\s+(?:available|supported|ready|proved|proven|qualified)|not\s+yet|cannot|can't|must\s+refuse|refuses?|planned|future|later|until|requires?\s+(?:a\s+)?(?:future|separate|new|verified)|no\s+(?:current|release)\s+support)\b/i;
const PUBLIC_SUPPORT_ALLOWED_STATUSES = new Set(["supported", "verified_live"]);
const IMPLEMENTATION_CONCEPT_PATTERNS = [
  /\bkey\s*server\b/i,
  /\bkeyserver\b/i,
  /\bratchets?\b/i,
  /\bbrowser\s+profiles?\b/i,
  /\bprovider\s+adapters?\b/i,
];
const PUBLIC_HTML_FRAGMENT_RE =
  /<\s*(?:a|button|details|div|fieldset|footer|form|h[1-6]|header|label|li|main|nav|option|p|section|select|small|span|strong|summary|textarea)\b|data-public-claim\s*=/i;

const CHAT_APP_EVIDENCE_SCHEMA = "osl-chat-app-evidence-v1";
const SUPPORT_MATRIX_SCHEMA = "osl-support-matrix-v1";
const REQUIRED_CHAT_APP_UNITS = new Map([
  ["discord", ["d6", "w9"]],
  ["signal", ["s9"]],
]);
const FORBIDDEN_PROMOTION_STATUSES = new Set([
  "available",
  "beta",
  "runtime-proven",
  "verified-live",
  "release-qualified",
  "supported",
]);
const ALLOWED_EVIDENCE_STATUSES = new Set([
  "blocked",
  "source-profile-only",
  "test-proven-only",
  "implemented-unwired",
  "unavailable",
]);
const REQUIRED_AUTHORITY_FLAGS = [
  "user_consent_required",
  "account_binding_required",
  "release_authority_required",
];
const REQUIRED_CHAT_CAPABILITIES = [
  "protected_send",
  "protected_receive",
  "attachments",
  "burn",
];
const FORBIDDEN_EVIDENCE_KEYS = new Set([
  "account_id",
  "account_identifier",
  "api_key",
  "cookie",
  "credential",
  "handle",
  "password",
  "private_key",
  "secret",
  "session",
  "snowflake",
  "token",
  "user_id",
]);

function repoRelative(filePath) {
  return path.relative(REPO_ROOT, filePath).split(path.sep).join("/");
}

async function readUtf8(filePath) {
  return fs.readFile(filePath, "utf8");
}

function supportMatrixFailure(message) {
  return {
    name: "conditional_app_evidence",
    expected: message,
    actual: "invalid support matrix",
  };
}

function versionedPublicSupportMatrixFailure(message) {
  return {
    name: "versioned_public_support_matrix",
    expected: message,
    actual: "invalid support matrix",
  };
}

function chatAppEvidenceFailure(message) {
  return {
    name: "chat_app_evidence",
    expected: message,
    actual: "invalid support matrix",
  };
}

function chatAppQualificationEvidenceFailure(message) {
  return {
    name: "chat_app_qualification_evidence",
    expected: message,
    actual: "invalid support matrix",
  };
}

function supportMatrixClaimFailure(message) {
  return {
    name: "validateSupportMatrixClaims",
    expected: message,
    actual: "invalid support matrix",
  };
}

function validateClaimGateWorkflow(workflowText) {
  const requiredStep = "- name: Claim-gate-in-app-copy-and-README";
  const selfTestCommand = "node scripts/check-app-claims.mjs --self-test";
  const repositoryScanCommand = "node scripts/check-app-claims.mjs";
  const installStepPattern = /^\s*- name: Install\b/m;
  const failures = [];
  const stepIndex = workflowText.indexOf(requiredStep);
  const selfTestIndex = stepIndex === -1
    ? -1
    : workflowText.indexOf(selfTestCommand, stepIndex);
  const repositoryScanIndex = selfTestIndex === -1
    ? -1
    : workflowText.indexOf(repositoryScanCommand, selfTestIndex + selfTestCommand.length);
  const firstInstallStepMatch = installStepPattern.exec(workflowText);
  const firstInstallStepIndex = firstInstallStepMatch?.index ?? -1;

  if (stepIndex === -1) {
    failures.push("missing exact Claim-gate-in-app-copy-and-README workflow step");
  }
  if (selfTestIndex === -1) {
    failures.push("missing app-claim self-test command in workflow step");
  }
  if (repositoryScanIndex === -1) {
    failures.push("missing app-claim repository scan command in workflow step");
  }
  if (
    firstInstallStepIndex !== -1
    && (
      stepIndex === -1
      || selfTestIndex === -1
      || repositoryScanIndex === -1
      || repositoryScanIndex > firstInstallStepIndex
    )
  ) {
    failures.push("app-claim gate must run before dependency installation");
  }

  return failures;
}

function normalizeSupportEvidenceStatus(value) {
  if (typeof value !== "string") {
    return null;
  }
  const normalized = value.replace(/-/g, "_");
  return [
    "supported",
    "verified_live",
    "runtime_proven",
    "test_proven_only",
    "implemented_unwired",
    "designed_only",
    "externally_blocked",
    "qualified_profile",
    "qa_foundations_only",
    "separate_qa_required",
    "unsupported",
  ].includes(normalized) ? normalized : null;
}

function sentenceSpans(text) {
  return [...text.matchAll(/[^.!?;\n]+[.!?;]?/g)]
    .map((match) => ({
      start: match.index ?? 0,
      end: (match.index ?? 0) + match[0].length,
      text: match[0].trim(),
    }))
    .filter(({ text: sentence }) => sentence.length > 0);
}

function supportMatrixClaimViolations(file, fragment, rowsById) {
  const violations = [];
  const normalized = normalizedClaimTextWithSourceMap(fragment.text);

  for (const required of REQUIRED_CONDITIONAL_APP_EVIDENCE) {
    const row = rowsById.get(required.id);
    const status = normalizeSupportEvidenceStatus(row?.status);
    if (status && PUBLIC_SUPPORT_ALLOWED_STATUSES.has(status)) {
      continue;
    }

    for (const sentence of sentenceSpans(normalized.text)) {
      if (
        !required.publicClaimPattern.test(sentence.text) ||
        !required.publicSupportClaimPattern.test(sentence.text) ||
        PUBLIC_SUPPORT_LIMITATION_RE.test(sentence.text)
      ) {
        continue;
      }

      const sourceIndex = normalized.sourceIndexes[sentence.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: required.publicClaimName,
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, sentence.end - sentence.start),
        ),
      });
    }
  }

  return violations;
}

function supportMatrixPublicClaimProofFailures(rowsById) {
  const proofFragments = [
    {
      file: "self-test/support-matrix-public-claims",
      text: "Telegram Desktop protected messaging is supported.",
      line: 1,
      shouldFlag: true,
      expectedPhrase: "Telegram protected support overclaim",
    },
    {
      file: "self-test/support-matrix-public-claims",
      text: "Telegram Desktop protected messaging is externally blocked until stable message rows are proved.",
      line: 1,
      shouldFlag: false,
      expectedPhrase: "Telegram protected support overclaim",
    },
    {
      file: "self-test/support-matrix-public-claims",
      text: "Outlook inline replies are OSL Mail protected delivery.",
      line: 1,
      shouldFlag: true,
      expectedPhrase: "Outlook OSL Mail protected support overclaim",
    },
  ];
  const failures = [];

  for (const proof of proofFragments) {
    const violations = supportMatrixClaimViolations(proof.file, proof, rowsById);
    const flagged = violations.some(({ phrase }) => phrase === proof.expectedPhrase);
    if (flagged !== proof.shouldFlag) {
      failures.push({
        name: SUPPORT_MATRIX_PUBLIC_PROOF_NAME,
        expected: `${proof.expectedPhrase} ${proof.shouldFlag ? "caught" : "not flagged"}`,
        actual: flagged ? "flagged" : "not flagged",
      });
    }
  }

  const supportedRows = new Map(rowsById);
  supportedRows.set("telegram_desktop_native", {
    ...(supportedRows.get("telegram_desktop_native") ?? {}),
    status: "supported",
  });
  const supportedViolations = supportMatrixClaimViolations(
    "self-test/support-matrix-public-claims",
    {
      text: "Telegram Desktop protected messaging is supported.",
      line: 1,
    },
    supportedRows,
  );
  if (supportedViolations.some(({ phrase }) => phrase === "Telegram protected support overclaim")) {
    failures.push({
      name: SUPPORT_MATRIX_PUBLIC_PROOF_NAME,
      expected: "supported evidence permits its matching public claim",
      actual: "flagged supported evidence",
    });
  }

  return failures;
}

async function validateSupportMatrix() {
  const failures = [];
  let matrix;

  try {
    matrix = JSON.parse(await readUtf8(SUPPORT_MATRIX_PATH));
  } catch (error) {
    failures.push(supportMatrixFailure(`readable JSON at ${repoRelative(SUPPORT_MATRIX_PATH)}`));
    return failures;
  }

  if (!matrix || typeof matrix !== "object" || Array.isArray(matrix)) {
    failures.push(supportMatrixFailure("top-level support matrix object"));
    return failures;
  }

  failures.push(...await validateVersionedPublicSupportMatrix(matrix));
  failures.push(...(await validateChatAppEvidence(matrix)));

  if (!Array.isArray(matrix.conditional_app_evidence)) {
    failures.push(supportMatrixFailure("conditional_app_evidence array"));
    return failures;
  }

  const rowsById = new Map();
  for (const row of matrix.conditional_app_evidence) {
    if (!row || typeof row !== "object" || Array.isArray(row)) {
      failures.push(supportMatrixFailure("each conditional_app_evidence row is an object"));
      continue;
    }
    if (typeof row.id !== "string" || row.id.length === 0) {
      failures.push(supportMatrixFailure("each conditional_app_evidence row has a non-empty id"));
      continue;
    }
    if (rowsById.has(row.id)) {
      failures.push(supportMatrixFailure(`unique conditional_app_evidence id ${row.id}`));
      continue;
    }
    rowsById.set(row.id, row);
  }

  for (const required of REQUIRED_CONDITIONAL_APP_EVIDENCE) {
    const row = rowsById.get(required.id);
    if (!row) {
      failures.push(supportMatrixFailure(`row ${required.id}`));
      continue;
    }

    for (const [field, expected] of [
      ["service", required.service],
      ["claim_scope", required.claimScope],
      ["evidence_report", required.evidenceReport],
      ["evidence_anchor", required.evidenceAnchor],
    ]) {
      if (row[field] !== expected) {
        failures.push(
          supportMatrixFailure(`${required.id}.${field}=${JSON.stringify(expected)}`),
        );
      }
    }

    if (normalizeSupportEvidenceStatus(row.status) !== required.status) {
      failures.push(
        supportMatrixFailure(`${required.id}.status=${JSON.stringify(required.status)}`),
      );
    }

    if (
      typeof row.support_boundary !== "string" ||
      !required.requiredBoundary.test(row.support_boundary)
    ) {
      failures.push(supportMatrixFailure(`${required.id}.support_boundary matches verdict scope`));
    }

    if (
      required.id === "outlook_osl_mail" &&
      /\bchat\b/i.test(`${row.surface ?? ""} ${row.support_boundary ?? ""}`) &&
      !/\bnot Outlook chat support\b/i.test(`${row.surface ?? ""} ${row.support_boundary ?? ""}`)
    ) {
      failures.push(supportMatrixFailure("Outlook row refuses chat-support framing"));
    }

    let report;
    try {
      report = await readUtf8(path.join(REPO_ROOT, required.evidenceReport));
    } catch (error) {
      failures.push(supportMatrixFailure(`source report ${required.evidenceReport}`));
      continue;
    }

    if (!new RegExp(`^Anchor: ${required.evidenceAnchor}$`, "m").test(report)) {
      failures.push(supportMatrixFailure(`${required.id} source report anchor`));
    }

    if (!required.reportVerdict.test(report)) {
      failures.push(supportMatrixFailure(`${required.id} source report verdict`));
    }
  }

  failures.push(...supportMatrixPublicClaimProofFailures(rowsById));
  failures.push(...validateSupportMatrixEvidenceLinks(matrix));
  const qualificationEvidence = validateSupportMatrixObject(matrix);
  failures.push(...qualificationEvidence.failures.map(chatAppQualificationEvidenceFailure));
  failures.push(
    ...(await validateSupportMatrixSources(qualificationEvidence.anchors))
      .map(chatAppQualificationEvidenceFailure),
  );

  return failures;
}

function validateSupportMatrixEvidenceLinks(matrix) {
  const failures = [];
  const publicRows = new Map();
  const publicMatrix = matrix?.versioned_public_support_matrix;
  if (publicMatrix && Array.isArray(publicMatrix.rows)) {
    for (const row of publicMatrix.rows) {
      if (row && typeof row === "object" && !Array.isArray(row) && typeof row.id === "string") {
        publicRows.set(row.id, row);
      }
    }
  }

  const evidenceCollections = {
    chat_app_evidence: new Map(),
    conditional_app_evidence: new Map(),
  };
  for (const collectionName of Object.keys(evidenceCollections)) {
    const collection = matrix?.[collectionName];
    if (!Array.isArray(collection)) {
      failures.push(supportMatrixClaimFailure(`${collectionName} array`));
      continue;
    }
    for (const row of collection) {
      if (row && typeof row === "object" && !Array.isArray(row) && typeof row.id === "string") {
        evidenceCollections[collectionName].set(row.id, row);
      }
    }
  }

  for (const link of REQUIRED_PUBLIC_SUPPORT_EVIDENCE_LINKS) {
    const publicRow = publicRows.get(link.publicId);
    if (!publicRow) {
      failures.push(supportMatrixClaimFailure(`public row ${link.publicId}`));
      continue;
    }

    const evidenceRow = evidenceCollections[link.evidenceCollection].get(link.evidenceId);
    if (!evidenceRow) {
      failures.push(supportMatrixClaimFailure(`evidence row ${link.evidenceCollection}.${link.evidenceId}`));
      continue;
    }

    if (evidenceRow.status !== link.evidenceStatus) {
      failures.push(
        supportMatrixClaimFailure(`${link.evidenceId}.status=${JSON.stringify(link.evidenceStatus)}`),
      );
    }

    if (publicRow.public_status !== link.publicStatus) {
      failures.push(
        supportMatrixClaimFailure(`${link.publicId}.public_status=${JSON.stringify(link.publicStatus)}`),
      );
    }

    if (publicRow.claim_allowed !== link.claimAllowed) {
      failures.push(
        supportMatrixClaimFailure(`${link.publicId}.claim_allowed=${JSON.stringify(link.claimAllowed)}`),
      );
    }

    if (
      Object.hasOwn(evidenceRow, "public_status") &&
      evidenceRow.public_status !== publicRow.public_status
    ) {
      failures.push(supportMatrixClaimFailure(`${link.evidenceId}.public_status matches ${link.publicId}`));
    }

    if (
      Object.hasOwn(evidenceRow, "public_claim_allowed") &&
      evidenceRow.public_claim_allowed !== publicRow.claim_allowed
    ) {
      failures.push(supportMatrixClaimFailure(`${link.evidenceId}.public_claim_allowed matches ${link.publicId}`));
    }

    if (publicRow.claim_allowed === true && evidenceRow.public_claim_allowed !== true) {
      failures.push(supportMatrixClaimFailure(`${link.publicId} requires exact public evidence`));
    }
  }

  return failures;
}

async function validateChatAppEvidence(matrix) {
  const failures = [];

  if (!Array.isArray(matrix.chat_app_evidence)) {
    failures.push(chatAppEvidenceFailure("chat_app_evidence array"));
    return failures;
  }

  const rowsById = new Map();
  for (const row of matrix.chat_app_evidence) {
    if (!row || typeof row !== "object" || Array.isArray(row)) {
      failures.push(chatAppEvidenceFailure("each chat_app_evidence row is an object"));
      continue;
    }
    if (typeof row.id !== "string" || row.id.length === 0) {
      failures.push(chatAppEvidenceFailure("each chat_app_evidence row has a non-empty id"));
      continue;
    }
    if (rowsById.has(row.id)) {
      failures.push(chatAppEvidenceFailure(`unique chat_app_evidence id ${row.id}`));
      continue;
    }
    rowsById.set(row.id, row);
  }

  for (const required of REQUIRED_CHAT_APP_EVIDENCE) {
    const row = rowsById.get(required.id);
    if (!row) {
      failures.push(chatAppEvidenceFailure(`row ${required.id}`));
      continue;
    }

    for (const [field, expected] of [
      ["service", required.service],
      ["claim_scope", required.claimScope],
      ["status", required.status],
      ["public_status", required.publicStatus],
      ["public_claim_allowed", required.publicClaimAllowed],
      ["evidence_type", required.evidenceType],
      ["evidence_report", required.evidenceReport],
      ["evidence_anchor", required.evidenceAnchor],
      ["trusted_anchor", required.trustedAnchor],
    ]) {
      if (row[field] !== expected) {
        failures.push(
          chatAppEvidenceFailure(`${required.id}.${field}=${JSON.stringify(expected)}`),
        );
      }
    }

    if (
      typeof row.support_boundary !== "string" ||
      !required.requiredBoundary.test(row.support_boundary)
    ) {
      failures.push(chatAppEvidenceFailure(`${required.id}.support_boundary matches public boundary`));
    }

    let report;
    try {
      report = await readUtf8(path.join(REPO_ROOT, required.evidenceReport));
    } catch (error) {
      failures.push(chatAppEvidenceFailure(`source report ${required.evidenceReport}`));
      continue;
    }

    if (!new RegExp(`^pub fn ${required.evidenceAnchor}\\(\\) -> SignedProfileDoc \\{$`, "m").test(report)) {
      failures.push(chatAppEvidenceFailure(`${required.id} signed profile function`));
    }

    if (!new RegExp(`^pub fn ${required.trustedAnchor}\\(\\) -> &'static str \\{$`, "m").test(report)) {
      failures.push(chatAppEvidenceFailure(`${required.id} trusted signing key function`));
    }
  }

  return failures;
}

async function validateSupportMatrixPublicFragments(publicFragments = []) {
  const failures = [];
  const violations = [];
  let matrix;

  try {
    matrix = JSON.parse(await readUtf8(SUPPORT_MATRIX_PATH));
  } catch (error) {
    failures.push(supportMatrixFailure(`readable JSON at ${repoRelative(SUPPORT_MATRIX_PATH)}`));
    return { failures, violations };
  }

  if (!matrix || typeof matrix !== "object" || Array.isArray(matrix)) {
    failures.push(supportMatrixFailure("top-level support matrix object"));
    return { failures, violations };
  }

  if (!Array.isArray(matrix.conditional_app_evidence)) {
    failures.push(supportMatrixFailure("conditional_app_evidence array"));
    return { failures, violations };
  }

  const rowsById = new Map();
  for (const row of matrix.conditional_app_evidence) {
    if (!row || typeof row !== "object" || Array.isArray(row)) {
      failures.push(supportMatrixFailure("each conditional_app_evidence row is an object"));
      continue;
    }
    if (typeof row.id !== "string" || row.id.length === 0) {
      failures.push(supportMatrixFailure("each conditional_app_evidence row has a non-empty id"));
      continue;
    }
    if (rowsById.has(row.id)) {
      failures.push(supportMatrixFailure(`unique conditional_app_evidence id ${row.id}`));
      continue;
    }
    rowsById.set(row.id, row);
  }

  for (const required of REQUIRED_CONDITIONAL_APP_EVIDENCE) {
    const row = rowsById.get(required.id);
    if (!row) {
      failures.push(supportMatrixFailure(`row ${required.id}`));
      continue;
    }

    for (const [field, expected] of [
      ["service", required.service],
      ["claim_scope", required.claimScope],
      ["evidence_report", required.evidenceReport],
      ["evidence_anchor", required.evidenceAnchor],
    ]) {
      if (row[field] !== expected) {
        failures.push(
          supportMatrixFailure(`${required.id}.${field}=${JSON.stringify(expected)}`),
        );
      }
    }

    if (normalizeSupportEvidenceStatus(row.status) !== required.status) {
      failures.push(
        supportMatrixFailure(`${required.id}.status=${JSON.stringify(required.status)}`),
      );
    }

    if (
      typeof row.support_boundary !== "string" ||
      !required.requiredBoundary.test(row.support_boundary)
    ) {
      failures.push(supportMatrixFailure(`${required.id}.support_boundary matches verdict scope`));
    }

    if (
      required.id === "outlook_osl_mail" &&
      /\bchat\b/i.test(`${row.surface ?? ""} ${row.support_boundary ?? ""}`) &&
      !/\bnot Outlook chat support\b/i.test(`${row.surface ?? ""} ${row.support_boundary ?? ""}`)
    ) {
      failures.push(supportMatrixFailure("Outlook row refuses chat-support framing"));
    }

    let report;
    try {
      report = await readUtf8(path.join(REPO_ROOT, required.evidenceReport));
    } catch (error) {
      failures.push(supportMatrixFailure(`source report ${required.evidenceReport}`));
      continue;
    }

    if (!new RegExp(`^Anchor: ${required.evidenceAnchor}$`, "m").test(report)) {
      failures.push(supportMatrixFailure(`${required.id} source report anchor`));
    }

    if (!required.reportVerdict.test(report)) {
      failures.push(supportMatrixFailure(`${required.id} source report verdict`));
    }
  }

  failures.push(...supportMatrixPublicClaimProofFailures(rowsById));

  for (const fragment of publicFragments) {
    violations.push(...supportMatrixClaimViolations(fragment.file, fragment, rowsById));
  }

  return { failures, violations };
}

async function validateVersionedPublicSupportMatrix(matrix) {
  const failures = [];
  const publicMatrix = matrix?.versioned_public_support_matrix;
  if (!publicMatrix || typeof publicMatrix !== "object" || Array.isArray(publicMatrix)) {
    failures.push(versionedPublicSupportMatrixFailure("top-level versioned_public_support_matrix object"));
    return failures;
  }
  failures.push(...validateVersionedPublicSupportEntries(publicMatrix));
  for (const [field, expected] of [
    ["id", "E7"],
    ["status", "current"],
    ["updated", "2026-07-30"],
  ]) {
    if (publicMatrix[field] !== expected) {
      failures.push(versionedPublicSupportMatrixFailure(`${field}=${JSON.stringify(expected)}`));
    }
  }
  if (typeof publicMatrix.version !== "string" || !/^2026-07-30\.e7$/.test(publicMatrix.version)) {
    failures.push(versionedPublicSupportMatrixFailure("version=2026-07-30.e7"));
  }
  if (!Array.isArray(publicMatrix.rows)) {
    failures.push(versionedPublicSupportMatrixFailure("rows array"));
    return failures;
  }
  const rowsByService = new Map();
  for (const row of publicMatrix.rows) {
    if (!row || typeof row !== "object" || Array.isArray(row) || typeof row.service !== "string") {
      failures.push(versionedPublicSupportMatrixFailure("each row has a service"));
      continue;
    }
    if (rowsByService.has(row.service)) {
      failures.push(versionedPublicSupportMatrixFailure(`unique row for ${row.service}`));
      continue;
    }
    rowsByService.set(row.service, row);
  }
  for (const required of REQUIRED_VERSIONED_PUBLIC_SUPPORT_ROWS) {
    const row = rowsByService.get(required.service);
    if (!row) {
      failures.push(versionedPublicSupportMatrixFailure(`row ${required.service}`));
      continue;
    }
    for (const [field, expected] of [
      ["id", required.id],
      ["claim_scope", required.claimScope],
      ["status", required.status],
      ["public_label", required.publicLabel],
      ["public_claim_allowed", false],
      ["public_status", required.publicStatus],
      ["claim_allowed", false],
      ["evidence_status", required.evidenceStatus],
      ["evidence", required.evidence],
      ["evidence_report", required.evidenceReport],
      ["last_verified", "2026-07-30"],
    ]) {
      if (row[field] !== expected) {
        failures.push(versionedPublicSupportMatrixFailure(`${required.service}.${field}=${JSON.stringify(expected)}`));
      }
    }
    if (typeof row.limitation !== "string" || !required.limitation.test(row.limitation)) {
      failures.push(versionedPublicSupportMatrixFailure(`${required.service}.limitation matches support boundary`));
    }
    if (typeof row.support_boundary !== "string" || !required.supportBoundary.test(row.support_boundary)) {
      failures.push(versionedPublicSupportMatrixFailure(`${required.service}.support_boundary matches public scope`));
    }
    try {
      await readUtf8(path.join(REPO_ROOT, required.evidenceReport));
    } catch {
      failures.push(versionedPublicSupportMatrixFailure(`${required.service}.evidence_report exists`));
    }
  }
  return failures;
}

function validateVersionedPublicSupportEntries(publicMatrix) {
  const failures = [];
  if (publicMatrix.schema_version !== REQUIRED_VERSIONED_PUBLIC_SUPPORT_MATRIX.schemaVersion) {
    failures.push(versionedPublicSupportMatrixFailure(
      `versioned_public_support_matrix.schema_version=${REQUIRED_VERSIONED_PUBLIC_SUPPORT_MATRIX.schemaVersion}`,
    ));
  }
  if (publicMatrix.matrix_version !== REQUIRED_VERSIONED_PUBLIC_SUPPORT_MATRIX.matrixVersion) {
    failures.push(versionedPublicSupportMatrixFailure(
      `versioned_public_support_matrix.matrix_version=${JSON.stringify(REQUIRED_VERSIONED_PUBLIC_SUPPORT_MATRIX.matrixVersion)}`,
    ));
  }
  if (!Array.isArray(publicMatrix.entries)) {
    failures.push(versionedPublicSupportMatrixFailure("entries array"));
    return failures;
  }
  const entriesById = new Map();
  for (const entry of publicMatrix.entries) {
    if (!entry || typeof entry !== "object" || Array.isArray(entry) || typeof entry.id !== "string") {
      failures.push(versionedPublicSupportMatrixFailure("each entry has an id"));
      continue;
    }
    if (entriesById.has(entry.id)) {
      failures.push(versionedPublicSupportMatrixFailure(`unique entry ${entry.id}`));
      continue;
    }
    entriesById.set(entry.id, entry);
  }
  for (const required of REQUIRED_VERSIONED_PUBLIC_SUPPORT_MATRIX.entries) {
    const entry = entriesById.get(required.id);
    if (!entry) {
      failures.push(versionedPublicSupportMatrixFailure(`entry ${required.id}`));
      continue;
    }
    for (const [field, expected] of [
      ["service", required.service],
      ["public_status", required.publicStatus],
      ["claim_allowed", required.claimAllowed],
      ["evidence_status", required.evidenceStatus],
      ["evidence", required.evidence],
    ]) {
      if (entry[field] !== expected) {
        failures.push(versionedPublicSupportMatrixFailure(`${required.id}.${field}=${JSON.stringify(expected)}`));
      }
    }
    if (entry.claim_allowed !== false) {
      failures.push(versionedPublicSupportMatrixFailure(`${required.id}.claim_allowed=false`));
    }
    if (typeof entry.support_boundary !== "string" || !required.requiredBoundary.test(entry.support_boundary)) {
      failures.push(versionedPublicSupportMatrixFailure(`${required.id}.support_boundary`));
    }
  }
  return failures;
}

function normalizeSupportMatrixStatus(value) {
  return typeof value === "string" ? value.replace(/-/g, "_") : "";
}

function supportEvidenceRows(matrix) {
  if (!matrix || typeof matrix !== "object" || Array.isArray(matrix)) {
    return [];
  }
  return [
    ...(Array.isArray(matrix.public_support_matrix) ? matrix.public_support_matrix : []),
    ...(Array.isArray(matrix.versioned_public_support_matrix?.rows)
      ? matrix.versioned_public_support_matrix.rows
      : []),
    ...(Array.isArray(matrix.conditional_app_evidence) ? matrix.conditional_app_evidence : []),
  ].filter((row) => row && typeof row === "object" && !Array.isArray(row));
}

function supportMatrixPublicClaimServices(matrix) {
  const allowed = new Set();
  for (const row of supportEvidenceRows(matrix)) {
    const status = normalizeSupportMatrixStatus(row.status);
    const service =
      row.service === "Outlook" && row.claim_scope === "osl_mail"
        ? "OSL Mail"
        : row.service;
    if (
      typeof service === "string" &&
      (row.public_claim_allowed === true || status === "supported" || status === "verified_live")
    ) {
      allowed.add(service);
    }
  }
  return allowed;
}

function decodeClaimEntity(entity) {
  const body = entity.slice(1, -1).toLowerCase();
  if (/^#\d+$/.test(body)) {
    const codePoint = Number.parseInt(body.slice(1), 10);
    return codePoint <= 0x10ffff ? String.fromCodePoint(codePoint) : entity;
  }
  if (/^#x[0-9a-f]+$/.test(body)) {
    const codePoint = Number.parseInt(body.slice(2), 16);
    return codePoint <= 0x10ffff ? String.fromCodePoint(codePoint) : entity;
  }
  return new Map([
    ["amp", "&"],
    ["apos", "'"],
    ["gt", ">"],
    ["lt", "<"],
    ["nbsp", " "],
    ["quot", '"'],
  ]).get(body) ?? entity;
}

function normalizedClaimTextWithSourceMap(source) {
  const characters = [];
  const sourceIndexes = [];
  const blockTags =
    /^(?:address|article|aside|blockquote|br|dd|div|dl|dt|footer|form|h[1-6]|header|hr|li|main|nav|ol|p|section|table|td|th|tr|ul)$/i;

  function append(character, sourceIndex) {
    const normalized = character === "’" || character === "‘" ? "'" : character;
    if (normalized === "\n" || normalized === "\r") {
      if (characters.length > 0 && characters.at(-1) !== "\n") {
        characters.push("\n");
        sourceIndexes.push(sourceIndex);
      }
      return;
    }
    if (normalized === "·") {
      if (characters.length > 0 && characters.at(-1) !== "\n") {
        characters.push("\n");
        sourceIndexes.push(sourceIndex);
      }
      return;
    }
    if (/\s/.test(normalized)) {
      if (characters.length === 0 || characters.at(-1) === " " || characters.at(-1) === "\n") {
        return;
      }
      characters.push(" ");
      sourceIndexes.push(sourceIndex);
      return;
    }
    characters.push(normalized.toLowerCase());
    sourceIndexes.push(sourceIndex);
  }

  for (let index = 0; index < source.length;) {
    if (source[index] === "<") {
      if (source.startsWith("<!--", index)) {
        const close = source.indexOf("-->", index + 4);
        if (close !== -1) {
          for (let inner = index + 4; inner < close; inner += 1) {
            append(source[inner], inner);
          }
          index = close + 3;
          continue;
        }
      }
      const close = source.indexOf(">", index + 1);
      if (close !== -1) {
        const rawTag = source.slice(index, close + 1);
        for (const attribute of rawTag.matchAll(
          /\bdata-[a-z0-9_:-]+\s*=\s*(["'])([\s\S]*?)\1/gi,
        )) {
          const valueStart = index + (attribute.index ?? 0)
            + attribute[0].indexOf(attribute[2]);
          append("\n", valueStart);
          for (let offset = 0; offset < attribute[2].length;) {
            if (attribute[2][offset] === "&") {
              const entity = attribute[2].slice(offset)
                .match(/^&(?:#[0-9]+|#x[0-9a-f]+|[a-z][a-z0-9]+);/i);
              if (entity) {
                for (const character of decodeClaimEntity(entity[0])) {
                  append(character, valueStart + offset);
                }
                offset += entity[0].length;
                continue;
              }
            }
            append(attribute[2][offset], valueStart + offset);
            offset += 1;
          }
          append("\n", valueStart + attribute[2].length);
        }
        const tag = source.slice(index, close + 1).match(/^<\s*\/?\s*([a-z][a-z0-9]*)\b/i);
        if (tag && blockTags.test(tag[1])) {
          append("\n", index);
        }
        index = close + 1;
        continue;
      }
    }
    if (source[index] === "&") {
      const entity = source.slice(index).match(/^&(?:#[0-9]+|#x[0-9a-f]+|[a-z][a-z0-9]+);/i);
      if (entity) {
        for (const character of decodeClaimEntity(entity[0])) {
          append(character, index);
        }
        index += entity[0].length;
        continue;
      }
    }
    append(source[index], index);
    index += 1;
  }

  return {
    text: characters.join(""),
    sourceIndexes,
  };
}

function splitMarkdownRow(line) {
  const trimmed = line.trim();
  if (!trimmed.startsWith("|") || !trimmed.endsWith("|")) {
    return [];
  }

  const cells = [];
  let cell = "";
  let escaped = false;

  for (let i = 1; i < trimmed.length - 1; i += 1) {
    const char = trimmed[i];

    if (escaped) {
      cell += char;
      escaped = false;
      continue;
    }

    if (char === "\\") {
      escaped = true;
      cell += char;
      continue;
    }

    if (char === "|") {
      cells.push(cell.trim());
      cell = "";
      continue;
    }

    cell += char;
  }

  cells.push(cell.trim());
  return cells;
}

function splitQuotedAlternatives(phrase) {
  const parts = phrase
    .split("/")
    .map((part) => part.trim())
    .filter(Boolean);

  if (parts.length <= 1) {
    return [phrase.trim()];
  }

  const first = parts[0];
  const prefixEnd = first.lastIndexOf(" ");
  const sharedPrefix = prefixEnd === -1 ? "" : first.slice(0, prefixEnd + 1);

  return parts.map((part, index) => {
    if (index === 0 || part.includes(" ") || sharedPrefix === "") {
      return part;
    }

    return `${sharedPrefix}${part}`;
  });
}

function parseBannedPhrases(markdown) {
  const startMatch = markdown.match(/^## D · NOT ELIGIBLE\b.*$/m);
  if (!startMatch || startMatch.index === undefined) {
    return [];
  }

  const sectionStart = startMatch.index + startMatch[0].length;
  const rest = markdown.slice(sectionStart);
  const endMatch = rest.match(/^##\s+/m);
  const section = endMatch ? rest.slice(0, endMatch.index) : rest;
  const phrases = new Map();

  for (const line of section.split(/\r?\n/)) {
    const cells = splitMarkdownRow(line);
    if (cells.length < 2) {
      continue;
    }

    const firstCell = cells[0].trim();
    if (
      /^forbidden phrase$/i.test(firstCell) ||
      /^:?-{3,}:?$/.test(firstCell)
    ) {
      continue;
    }

    const matches = firstCell.matchAll(/"([^"]+)"/g);
    for (const match of matches) {
      for (const alternative of splitQuotedAlternatives(match[1])) {
        const normalized = normalizedClaimTextWithSourceMap(alternative.trim()).text;
        if (normalized) {
          phrases.set(normalized, alternative.trim());
        }
      }
    }
  }

  return [...phrases.entries()].map(([normalized, display]) => ({
    normalized,
    display,
  }));
}

async function listTypeScriptFiles(root) {
  const files = [];

  async function walk(dir) {
    const entries = await fs.readdir(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(fullPath);
        continue;
      }

      if (
        entry.isFile() &&
        entry.name.endsWith(".ts") &&
        !entry.name.endsWith(".test.ts") &&
        !entry.name.endsWith(".d.ts")
      ) {
        files.push(fullPath);
      }
    }
  }

  await walk(root);
  files.sort();
  return files;
}

async function listRustFiles(root) {
  const files = [];

  async function walk(dir) {
    const entries = await fs.readdir(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        await walk(fullPath);
        continue;
      }

      if (entry.isFile() && entry.name.endsWith(".rs")) {
        files.push(fullPath);
      }
    }
  }

  await walk(root);
  files.sort();
  return files;
}

function lineStartsFor(source) {
  const starts = [0];
  for (let i = 0; i < source.length; i += 1) {
    if (source[i] === "\n") {
      starts.push(i + 1);
    }
  }
  return starts;
}

function lineNumberAt(lineStarts, index) {
  let low = 0;
  let high = lineStarts.length - 1;

  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (lineStarts[mid] <= index) {
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }

  return high + 1;
}

function skipQuotedString(source, start, quote) {
  let i = start + 1;
  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      i += 2;
      continue;
    }

    if (char === quote) {
      return i + 1;
    }

    i += 1;
  }

  return source.length;
}

function skipTemplate(source, start) {
  let i = start + 1;
  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      i += 2;
      continue;
    }

    if (char === "`") {
      return i + 1;
    }

    if (char === "$" && source[i + 1] === "{") {
      i = skipTemplateExpression(source, i + 2);
      continue;
    }

    i += 1;
  }

  return source.length;
}

function skipTemplateExpression(source, start) {
  let depth = 1;
  let i = start;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      const newline = source.indexOf("\n", i + 2);
      i = newline === -1 ? source.length : newline + 1;
      continue;
    }

    if (char === "/" && next === "*") {
      const close = source.indexOf("*/", i + 2);
      i = close === -1 ? source.length : close + 2;
      continue;
    }

    if (char === "'" || char === '"') {
      i = skipQuotedString(source, i, char);
      continue;
    }

    if (char === "`") {
      i = skipTemplate(source, i);
      continue;
    }

    if (char === "{") {
      depth += 1;
      i += 1;
      continue;
    }

    if (char === "}") {
      depth -= 1;
      i += 1;
      if (depth === 0) {
        return i;
      }
      continue;
    }

    i += 1;
  }

  return source.length;
}

function parseQuotedLiteral(source, start, quote) {
  let text = "";
  let i = start + 1;

  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      if (i + 1 < source.length) {
        text += source[i + 1];
      }
      i += 2;
      continue;
    }

    if (char === quote) {
      return { text, end: i + 1 };
    }

    text += char;
    i += 1;
  }

  return { text, end: source.length };
}

function parseTemplateLiteral(source, start) {
  let text = "";
  let i = start + 1;

  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      if (i + 1 < source.length) {
        text += source[i + 1];
      }
      i += 2;
      continue;
    }

    if (char === "`") {
      return { text, end: i + 1 };
    }

    if (char === "$" && source[i + 1] === "{") {
      text += " ";
      i = skipTemplateExpression(source, i + 2);
      continue;
    }

    text += char;
    i += 1;
  }

  return { text, end: source.length };
}

function isImportExportSpecifier(source, literalStart) {
  const before = source.slice(Math.max(0, literalStart - 1000), literalStart);
  const lastSemicolon = before.lastIndexOf(";");
  const statement = before.slice(lastSemicolon + 1);

  if (/^\s*import(?:\s|$)[\s\S]*(?:\bfrom\s*)?$/.test(statement)) {
    return true;
  }

  if (/\bimport\s*\($/.test(statement)) {
    return true;
  }

  if (/^\s*export\b[\s\S]*\bfrom\s*$/.test(statement)) {
    return true;
  }

  return false;
}

function extractTypeScriptStrings(source) {
  const strings = [];
  const lineStarts = lineStartsFor(source);
  let i = 0;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      const newline = source.indexOf("\n", i + 2);
      i = newline === -1 ? source.length : newline + 1;
      continue;
    }

    if (char === "/" && next === "*") {
      const close = source.indexOf("*/", i + 2);
      i = close === -1 ? source.length : close + 2;
      continue;
    }

    if (char === "'" || char === '"') {
      const literal = parseQuotedLiteral(source, i, char);
      if (!isImportExportSpecifier(source, i)) {
        strings.push({
          text: literal.text,
          line: lineNumberAt(lineStarts, i),
        });
      }
      i = literal.end;
      continue;
    }

    if (char === "`") {
      const literal = parseTemplateLiteral(source, i);
      if (!isImportExportSpecifier(source, i)) {
        strings.push({
          text: literal.text,
          line: lineNumberAt(lineStarts, i),
        });
      }
      i = literal.end;
      continue;
    }

    i += 1;
  }

  return strings;
}

function parseRustQuotedLiteral(source, start) {
  let text = "";
  let i = start + 1;

  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      if (i + 1 < source.length) {
        text += source[i + 1];
      }
      i += 2;
      continue;
    }

    if (char === '"') {
      return { text, end: i + 1 };
    }

    text += char;
    i += 1;
  }

  return { text, end: source.length };
}

function parseRustRawLiteral(source, start) {
  if (source[start] !== "r") {
    return null;
  }

  let i = start + 1;
  while (source[i] === "#") {
    i += 1;
  }

  if (source[i] !== '"') {
    return null;
  }

  const hashes = i - start - 1;
  const close = `"${"#".repeat(hashes)}`;
  const textStart = i + 1;
  const textEnd = source.indexOf(close, textStart);

  if (textEnd === -1) {
    return { text: source.slice(textStart), end: source.length };
  }

  return {
    text: source.slice(textStart, textEnd),
    end: textEnd + close.length,
  };
}

function skipRustCharLiteral(source, start) {
  let i = start + 1;
  while (i < source.length) {
    const char = source[i];
    if (char === "\\") {
      i += 2;
      continue;
    }

    if (char === "'") {
      return i + 1;
    }

    if (char === "\n") {
      return i;
    }

    i += 1;
  }

  return source.length;
}

function skipRustStringLike(source, start) {
  const raw = parseRustRawLiteral(source, start);
  if (raw) {
    return raw.end;
  }

  if (source[start] === '"') {
    return parseRustQuotedLiteral(source, start).end;
  }

  if (source[start] === "'") {
    return skipRustCharLiteral(source, start);
  }

  return start + 1;
}

function skipRustLineComment(source, start) {
  const newline = source.indexOf("\n", start + 2);
  return newline === -1 ? source.length : newline + 1;
}

function skipRustBlockComment(source, start) {
  let depth = 1;
  let i = start + 2;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "*") {
      depth += 1;
      i += 2;
      continue;
    }

    if (char === "*" && next === "/") {
      depth -= 1;
      i += 2;
      if (depth === 0) {
        return i;
      }
      continue;
    }

    i += 1;
  }

  return source.length;
}

function findMatchingRustBrace(source, openIndex) {
  let depth = 1;
  let i = openIndex + 1;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      i = skipRustLineComment(source, i);
      continue;
    }

    if (char === "/" && next === "*") {
      i = skipRustBlockComment(source, i);
      continue;
    }

    if (char === "r" || char === '"' || char === "'") {
      i = skipRustStringLike(source, i);
      continue;
    }

    if (char === "{") {
      depth += 1;
      i += 1;
      continue;
    }

    if (char === "}") {
      depth -= 1;
      i += 1;
      if (depth === 0) {
        return i;
      }
      continue;
    }

    i += 1;
  }

  return source.length;
}

function findRustItemEnd(source, start) {
  let i = start;

  while (i < source.length) {
    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      i = skipRustLineComment(source, i);
      continue;
    }

    if (char === "/" && next === "*") {
      i = skipRustBlockComment(source, i);
      continue;
    }

    if (char === "r" || char === '"' || char === "'") {
      i = skipRustStringLike(source, i);
      continue;
    }

    if (char === "{") {
      return findMatchingRustBrace(source, i);
    }

    if (char === ";") {
      return i + 1;
    }

    i += 1;
  }

  return source.length;
}

function rustTestRanges(source) {
  const ranges = [];
  const cfgTestRe = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/g;
  const modTestsRe = /\bmod\s+tests\s*\{/g;
  let match;

  while ((match = cfgTestRe.exec(source)) !== null) {
    ranges.push({
      start: match.index,
      end: findRustItemEnd(source, match.index + match[0].length),
    });
  }

  while ((match = modTestsRe.exec(source)) !== null) {
    const openIndex = source.indexOf("{", match.index);
    ranges.push({
      start: match.index,
      end: findMatchingRustBrace(source, openIndex),
    });
  }

  ranges.sort((a, b) => a.start - b.start || a.end - b.end);
  return ranges;
}

function isInRange(index, range) {
  return range && index >= range.start && index < range.end;
}

function isIdentifierShapedRustLiteral(text) {
  const trimmed = text.trim();
  if (/\s/.test(trimmed)) {
    return false;
  }

  return (
    trimmed.includes("::") ||
    trimmed.includes("/") ||
    trimmed.includes("_") ||
    /^[a-z]+$/.test(trimmed)
  );
}

const RUST_USER_VISIBLE_FIELDS = new Set([
  "label",
  "title",
  "detail",
  "message",
  "warning",
  "warnings",
  "display_name",
  "summary",
  "description",
  "body",
  "heading",
  "subtitle",
]);
const RUST_USER_VISIBLE_FIELD_RE = new RegExp(
  `(?:^|[,{]\\s*)(${[...RUST_USER_VISIBLE_FIELDS].join("|")})\\s*:\\s*(?:(?:[A-Za-z_][A-Za-z0-9_:]*!?\\s*)?[\\(\\[\\{]\\s*)*$`,
);
const RUST_USER_VISIBLE_METHOD_RE =
  /\.(?:title|set_title|add_filter|set_message|set_detail)\s*\(\s*$/;

function rustLiteralIsStructFieldValue(source, start) {
  const before = source.slice(Math.max(0, start - 300), start);
  const match = before.match(RUST_USER_VISIBLE_FIELD_RE);
  return Boolean(match && RUST_USER_VISIBLE_FIELDS.has(match[1]));
}

function rustLiteralIsMethodArgument(source, start) {
  const before = source.slice(Math.max(0, start - 160), start);
  return RUST_USER_VISIBLE_METHOD_RE.test(before);
}

function rustLiteralIsErrValue(source, start, end) {
  const before = source.slice(Math.max(0, start - 160), start);
  const after = source.slice(end, Math.min(source.length, end + 160));

  if (/(?:^|[^\w])Err\s*\(\s*$/.test(before)) {
    return /^\s*\.\s*(?:into|to_string)\s*\(\s*\)\s*\)/.test(after);
  }

  if (/(?:^|[^\w])Err\s*\(\s*format!\s*\(\s*$/.test(before)) {
    return /^\s*(?:,|\)\s*\))/.test(after);
  }

  return false;
}

function rustLiteralIsBareReturnedValue(source, start, end) {
  const before = source.slice(Math.max(0, start - 160), start);
  const after = source.slice(end, Math.min(source.length, end + 80));

  return (
    /(?:\breturn\s+|=>\s*)$/.test(before) &&
    /^\s*\.\s*(?:into|to_string)\s*\(\s*\)/.test(after)
  );
}

function shouldSelectRustLiteral(source, start, end, text) {
  if (isIdentifierShapedRustLiteral(text)) {
    return false;
  }

  return (
    rustLiteralIsStructFieldValue(source, start) ||
    rustLiteralIsMethodArgument(source, start) ||
    rustLiteralIsErrValue(source, start, end) ||
    rustLiteralIsBareReturnedValue(source, start, end)
  );
}

function extractRustStrings(source) {
  const strings = [];
  const lineStarts = lineStartsFor(source);
  const testRanges = rustTestRanges(source);
  let testRangeIndex = 0;
  let i = 0;

  while (i < source.length) {
    while (testRangeIndex < testRanges.length && i >= testRanges[testRangeIndex].end) {
      testRangeIndex += 1;
    }

    if (isInRange(i, testRanges[testRangeIndex])) {
      i = testRanges[testRangeIndex].end;
      continue;
    }

    const char = source[i];
    const next = source[i + 1];

    if (char === "/" && next === "/") {
      i = skipRustLineComment(source, i);
      continue;
    }

    if (char === "/" && next === "*") {
      i = skipRustBlockComment(source, i);
      continue;
    }

    const raw = parseRustRawLiteral(source, i);
    if (raw) {
      if (shouldSelectRustLiteral(source, i, raw.end, raw.text)) {
        strings.push({
          text: raw.text,
          line: lineNumberAt(lineStarts, i),
        });
      }
      i = raw.end;
      continue;
    }

    if (char === '"') {
      const literal = parseRustQuotedLiteral(source, i);
      if (shouldSelectRustLiteral(source, i, literal.end, literal.text)) {
        strings.push({
          text: literal.text,
          line: lineNumberAt(lineStarts, i),
        });
      }
      i = literal.end;
      continue;
    }

    if (char === "'") {
      i = skipRustCharLiteral(source, i);
      continue;
    }

    i += 1;
  }

  return strings;
}

function excerptAround(text, index, length) {
  const start = Math.max(0, index - 50);
  const end = Math.min(text.length, index + length + 50);
  return text
    .slice(start, end)
    .replace(/\s+/g, " ")
    .trim();
}

function sentenceBounds(text, start, end) {
  const before = text.slice(0, start);
  const leftBoundary = Math.max(
    before.lastIndexOf("."),
    before.lastIndexOf("!"),
    before.lastIndexOf("?"),
    before.lastIndexOf(";"),
    before.lastIndexOf("\n"),
  );
  const boundaryIndexes = [".", "!", "?", ";", "\n"]
    .map((boundary) => text.indexOf(boundary, end))
    .filter((index) => index !== -1);
  const rightBoundary =
    boundaryIndexes.length === 0 ? text.length : Math.min(...boundaryIndexes);
  return {
    start: leftBoundary + 1,
    end: rightBoundary,
  };
}

function genericNegationGovernsClaim(text, start, end) {
  const bounds = sentenceBounds(text, start, end);
  const before = text.slice(bounds.start, start);
  return (
    /\b(?:is|are|was|were|does|do|did|has|have|had|can|could|will|would)\s+not\s+(?:yet\s+)?(?:an?\s+)?(?:osl\s+)?$/i.test(before)
    || /\bnever\s+(?:an?\s+)?(?:osl\s+)?$/i.test(before)
    || /\bnot\s+(?:an?\s+)?(?:claim|promise|assertion)\s+(?:of|that)\s*$/i.test(before)
  );
}

function attachmentLimitationGovernsClaim(text, start, end) {
  const bounds = sentenceBounds(text, start, end);
  const before = text.slice(bounds.start, start);
  const after = text.slice(end, bounds.end);
  const limitation =
    "(?:planned|unavailable|unproved|unproven|unknown|not\\s+yet\\s+(?:available|implemented)|not\\s+established)";
  const beforePattern = new RegExp(
    `(?:\\b${limitation}\\b|\\bno\\b[^.!?;]{0,80}\\b(?:proves?|establishes?|shows?|demonstrates?|verifies?)\\s+)`
      + "\\s*"
      + "(?:(?:claim|property|behaviou?r|assertion)\\s+)?"
      + "(?:(?:that|whether|if)\\s+)?"
      + "(?:(?:the|this|it|osl|release|build|feature|transport)\\s+){0,6}$",
    "i",
  );
  const afterPattern = new RegExp(
    "^\\s*(?:,?\\s*(?:(?:a|the|this)\\s+)?"
      + "(?:(?:claim|property|behaviou?r|assertion)\\s+)?"
      + "(?:(?:that|which)\\s+)?)?"
      + `(?:is|are|remains?|stays?|has\\s+not\\s+been|have\\s+not\\s+been)\\s+${limitation}\\b`,
    "i",
  );
  return beforePattern.test(before) || afterPattern.test(after);
}

function semanticAttachmentClaimSpans(text) {
  const spans = [];
  const sentencePattern = /[^.!?;\n]+[.!?;]?/g;

  function token(sentence, pattern) {
    const match = sentence.match(pattern);
    return match && match.index !== undefined
      ? { start: match.index, end: match.index + match[0].length }
      : null;
  }

  function tokenAfter(sentence, pattern, after) {
    if (!after) {
      return null;
    }
    const match = sentence.slice(after.end).match(pattern);
    return match && match.index !== undefined
      ? {
          start: after.end + match.index,
          end: after.end + match.index + match[0].length,
        }
      : null;
  }

  function record(sentenceStart, tokens) {
    const present = tokens.filter(Boolean);
    if (present.length !== tokens.length) {
      return;
    }
    const start = sentenceStart + Math.min(...present.map((item) => item.start));
    const end = sentenceStart + Math.max(...present.map((item) => item.end));
    const key = `${start}:${end}`;
    if (!spans.some((span) => span.key === key)) {
      spans.push({ key, start, end });
    }
  }

  function recordPattern(pattern) {
    for (const match of text.matchAll(pattern)) {
      const start = match.index ?? 0;
      const end = start + match[0].length;
      const key = `${start}:${end}`;
      if (!spans.some((span) => span.key === key)) {
        spans.push({ key, start, end });
      }
    }
  }

  for (const sentenceMatch of text.matchAll(sentencePattern)) {
    const sentence = sentenceMatch[0];
    const sentenceStart = sentenceMatch.index ?? 0;
    const discord = token(sentence, /\bdiscord\b/i);
    const attachment = token(sentence, /\b(?:attachments?|uploaded\s+files?|files?)\b/i);
    const inspection = token(sentence, /\b(?:scann?(?:er|ers|ing|ed|s)?|inspection)\b/i);
    const defeated = token(
      sentence,
      /\b(?:defeat(?:ed|s|ing)?|solv(?:e|ed|es|ing)|bypass(?:ed|es|ing)?|neutraliz(?:e|ed|es|ing)|block(?:ed|s|ing)?|evad(?:e|ed|es|ing)|opaque)\b/i,
    );
    record(sentenceStart, [discord, attachment, inspection, defeated]);

    const receives = token(sentence, /\b(?:receiv(?:e|es|ed|ing)|gets?|sees?)\b/i);
    const cover = token(sentence, /\b(?:harmless\s+)?cover\s+files?\b/i);
    const instead = token(sentence, /\b(?:instead\s+of|rather\s+than)\b/i);
    const replacedAttachment = tokenAfter(
      sentence,
      /\b(?:attachments?|uploaded\s+files?|files?)\b/i,
      instead,
    );
    record(sentenceStart, [discord, receives, cover, instead, replacedAttachment]);

    const decoy = token(sentence, /\bdecoys?\b/i);
    const exclusive = token(sentence, /\b(?:only|instead\s+of|rather\s+than)\b/i);
    record(sentenceStart, [discord, decoy, exclusive]);
  }

  // Relational paraphrases found by the independent successor audit. These
  // patterns bind the actor, downstream surface, and claimed outcome; they do
  // not ban isolated words such as "placeholder", "blocks", or "unreadable".
  for (const pattern of [
    /\bosl\s+(?:thwart(?:s|ed|ing)?|circumvent(?:s|ed|ing)?)\s+discord(?:'s)?[^.!?;\n]{0,80}\b(?:attachment|upload(?:ed)?\s+files?)[^.!?;\n]{0,45}\b(?:scann?(?:er|ers|ing)?|inspection|checks?)\b/gi,
    /\bosl\s+prevent(?:s|ed|ing)?\s+discord\s+from\s+inspect(?:s|ed|ing)?\s+(?:attachments?|uploaded\s+files?|files?|uploads?)\b/gi,
    /\bdiscord(?:'s)?\s+checks?\s+on\s+attachments?\s+(?:are|is|were|was|have\s+been|has\s+been)\s+rendered\s+ineffective\s+by\s+osl\b/gi,
    /\battachment\s+inspection\s+by\s+discord\s+no\s+longer\s+works(?:\s+when\s+osl\s+is\s+used)?\b/gi,
    /\bdiscord\s+(?:gets?|receives?|sees?)\s+(?:an?\s+|the\s+)?(?:(?:harmless|benign|safe)\s+)?(?:placeholder|stand-in|dummy|surrogate|replacement|decoy)(?:\s+(?:file|image|blob|media))?\s+(?:instead\s+of|rather\s+than)\s+(?:the\s+)?(?:(?:real|original|actual|user's)\s+)?(?:attachments?|uploads?|uploaded\s+files?|files?)\b/gi,
    /\bonly\s+(?:an?\s+)?(?:(?:harmless|benign|safe)\s+)?(?:placeholder|stand-in|dummy|surrogate|replacement|decoy)(?:\s+(?:file|image|blob|media))?\s+reaches\s+discord(?:\s*(?:;|,)\s*(?:the\s+)?(?:(?:real|original|actual|user's)\s+)?(?:attachments?|uploads?|files?)\s+(?:does\s+not|stays?\s+off)|\s+(?:instead\s+of|rather\s+than)\s+(?:the\s+)?(?:(?:real|original|actual|user's)\s+)?(?:attachments?|uploads?|files?))\b/gi,
    /\bosl\s+substitut(?:e|es|ed|ing)\s+(?:an?\s+|the\s+)?(?:(?:harmless|benign|safe)\s+)?(?:placeholder|stand-in|dummy|surrogate|replacement|decoy)(?:\s+(?:file|image|blob|media))?\s+for\s+(?:every\s+|the\s+|an?\s+)?(?:attachments?|uploads?|files?)\s+(?:sent|uploaded)\s+to\s+discord\b/gi,
    /\b(?:actual|original|real)\s+(?:attachments?|uploads?|files?)\s+(?:stays?|remains?)\s+off\s+discord\s*;\s*(?:an?\s+)?(?:(?:harmless|benign|safe)\s+)?(?:placeholder|stand-in|dummy|surrogate|replacement|decoy)(?:\s+(?:file|image|blob|media))?\s+is\s+(?:uploaded|sent)\s+in\s+its\s+place\b/gi,
    /\bdiscord\s+sees\s+nothing\s+except\s+(?:decoy|fake|dummy|placeholder|surrogate)(?:\s+(?:files?|images?|media|blobs?))?\b/gi,
    /\bevery\s+(?:file|attachment|upload)\s+visible\s+to\s+discord\s+is\s+(?:an?\s+)?(?:decoy|fake|dummy|placeholder|surrogate)(?:\s+(?:file|image|blob))?\b/gi,
    /\bdiscord\s+can\s+inspect\s+only\s+(?:an?\s+)?(?:decoy|fake|dummy|placeholder|surrogate)(?:\s+(?:file|image|blob|upload))?\s*,?\s*not\s+(?:the\s+)?(?:user's|real|original|actual)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\b(?:the\s+)?(?:user's|real|original|actual)\s+(?:attachments?|uploads?|files?)\s+(?:is|are|being|remains?|stays?)\s+(?:opaque|unreadable)\s+to\s+discord\b/gi,
    /\bdiscord(?:'s)?\s+(?:attachment\s+)?scann?(?:er|ers)\s+learns?\s+nothing\s+about\s+(?:the\s+)?(?:user's|real|original|actual)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\bosl\s+(?:sidestep(?:s|ped|ping)?|dodg(?:e|es|ed|ing)|outflank(?:s|ed|ing)?|nullif(?:y|ies|ied|ying)|slip(?:s|ped|ping)?\s+(?:attachments?|uploads?)\s+past|route(?:s|d|ing)?\s+(?:attachments?|uploads?)\s+around|tunnel(?:s|ed|ing)?\s+(?:attachments?|uploads?)\s+(?:past|beyond))\s+discord(?:'s)?[^.!?;\n]{0,90}\b(?:inspection|review|scrutiny|screening|file[- ]analysis(?:\s+pass)?)\b/gi,
    /\bosl\s+(?:sidestep(?:s|ped|ping)?|dodg(?:e|es|ed|ing)|outflank(?:s|ed|ing)?|nullif(?:y|ies|ied|ying))\s+discord(?:'s)?\s+(?:inspection|review|scrutiny|screening|file[- ]analysis(?:\s+pass)?)\s+of\s+(?:uploaded\s+)?(?:attachments?|uploads?|files?)\b/gi,
    /\bosl\s+sidesteps\s+discord(?:'s)?\s+inspection\s+of\s+uploaded\s+attachments\b/gi,
    /\bosl\b[^.!?]{0,90}[.!?]\s*it\s+(?:sidestep(?:s|ped|ping)?|dodg(?:e|es|ed|ing)|outflank(?:s|ed|ing)?|nullif(?:y|ies|ied|ying))\s+discord(?:'s)?\s+(?:inspection|review|scrutiny|screening|file[- ]analysis(?:\s+pass)?)\s+of\s+(?:uploaded\s+)?(?:attachments?|uploads?|files?)\b/gi,
    /\bdiscord(?:'s)?\s+(?:attachment|upload|file)?\s*(?:inspection|review|scrutiny|screening|file[- ]analysis(?:\s+pass)?)\s+(?:is|was|has\s+been)\s+(?:outflanked|nullified|sidestepped|dodged|made\s+(?:useless|ineffective))\s+by\s+osl\b/gi,
    /\bdiscord\s+(?:examines?|reviews?|screens?|inspects?)\s+(?:attachments?|uploads?|files?)[.!?]\s*(?:with\s+osl[^.!?]{0,35},?\s*)?(?:that|the)\s+(?:inspection|review|screening)\s+cannot\s+reach\s+(?:the\s+)?(?:actual|real|source|user's)?\s*(?:attachments?|uploads?|files?)\b/gi,
    /\bdiscord(?:'s)?\s+(?:attachment|upload)?\s*(?:inspection|review|screening)\s+(?:is|was)\s+made\s+(?:useless|ineffective)\s+(?:whenever|when)\s+osl\s+(?:sends?|is\s+used)\b/gi,
    /\bdiscord\s+(?:is\s+handed|gets?|receives?|is\s+shown)\s+(?:an?\s+|the\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)(?:\s+(?:file|upload|blob|media))?\b[^.!?;\n]{0,100}\b(?:genuine|actual|real|original|source|user's)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\b(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:is|are)\s+(?:exchanged|swapped|substituted)\s+for\s+(?:an?\s+|the\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\b[^.!?;\n]{0,80}\bdiscord\b/gi,
    /\bin\s+(?:the\s+)?(?:attachments?|uploads?|files?)(?:'s)?\s+place\s*,?\s*discord\s+(?:gets?|receives?|is\s+handed)\s+(?:an?\s+|the\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\b/gi,
    /\b(?:the\s+)?(?:actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:stays?|remains?)\s+local[.!?]\s*discord\s+(?:gets?|receives?|is\s+handed)\s+(?:an?\s+)?(?:separate\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\b/gi,
    /\b(?:(?:an?\s+)?(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\s+(?:is\s+)?(?:sent|handed|given)\s+to\s+discord\s+(?:in\s+place\s+of|instead\s+of|rather\s+than)\s+(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\bosl\s+(?:swaps?|exchanges?|substitutes?)\s+(?:an?\s+|the\s+)?(?:(?:clean|benign|sanitized|scrubbed|innocuous|harmless)\s+)?(?:proxy|facade|shell|substitute|stand-in|surrogate|cover)\s+for\s+(?:each|every|the|an?)\s+(?:user's|genuine|actual|real|original|source)?\s*(?:attachments?|uploads?|files?)[^.!?;\n]{0,55}\bdiscord\b/gi,
    /\bdiscord\s+(?:is\s+blind\s+to|gains?\s+(?:no|zero)\s+information\s+from|cannot\s+(?:discern|decipher|understand|read))\s+(?:the\s+)?(?:contents?\s+of\s+)?(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\b(?:nothing|zero\s+information)\s+(?:about\s+)?(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:is\s+)?(?:intelligible|visible|available|revealed)\s+to\s+discord\b/gi,
    /\b(?:the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:is|are|becomes?|remain(?:s)?)\s+(?:indecipherable|invisible|unintelligible)\s+to\s+discord\b/gi,
    /\b(?:the\s+)?(?:referenced\s+)?(?:attachments?|uploads?|files?)\s+reveals?\s+(?:no|zero)\s+(?:content|information|meaning)\s+to\s+discord\b/gi,
    /\bdiscord\s+receives?\s+(?:an?\s+|the\s+)?(?:upload|attachment|file)\s+reference[.!?]\s*(?:the\s+)?referenced\s+(?:attachments?|uploads?|files?)\s+reveals?\s+(?:no|zero)\s+(?:content|information|meaning)\s+to\s+it\b/gi,
    /\bto\s+discord\s*,?\s*(?:every\s+|the\s+)?(?:user's|genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\s+(?:is|are|becomes?|remain(?:s)?)\s+(?:indecipherable|invisible|unintelligible)\b/gi,
    /\bdiscord\s+can\s+extract\s+(?:no|zero)\s+(?:content|information|meaning)\s+from\s+(?:the\s+)?(?:payload|attachment|upload|file)\b/gi,
    /\bosl\s+(?:skirts?|skirted|skirting)\s+discord(?:'s)?\s+(?:attachment\s+|file\s+|upload\s+)?audit\b/gi,
    /\b(?:osl\s+)?renders?\s+discord(?:'s)?\s+(?:attachment\s+|file\s+|upload\s+)?audit\s+toothless\b/gi,
    /\bdiscord\s+(?:gets?|receives?|sees?)\s+(?:an?\s+|the\s+)?(?:harmless|benign|safe|sanitized)?\s*(?:double|lookalike)\b/gi,
    /\b(?:an?\s+|the\s+)?(?:harmless|benign|safe|sanitized)\s+(?:double|lookalike)\s+(?:in\s+lieu\s+of|instead\s+of|rather\s+than)\s+(?:the\s+)?(?:genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\bdiscord\s+cannot\s+make\s+sense\s+of\s+(?:the\s+)?(?:genuine|actual|real|original|source)\s+(?:attachments?|uploads?|files?)\b/gi,
    /\bdiscord\s+sees\s+only\s+gibberish\b/gi,
    /\b(?:discord|the\s+(?:service|platform)|the\s+downstream\s+(?:service|platform))(?:'s)?\s+(?:content[-\s]+analysis\s+(?:machinery|system|pipeline)|analysis\s+(?:machinery|system|pipeline))\s+(?:gets?|receives?|has)\s+no\s+(?:useful|meaningful)\s+(?:view|visibility|information)\b/gi,
    /\b(?:discord|the\s+(?:service|platform))\s+(?:gets?|receives?|sees?)\s+(?:an?\s+)?(?:benign|harmless|sanitized)\s+(?:twin|double|lookalike)\b[^\n]{0,120}\b(?:source|original|real|actual)\s+(?:attachments?|uploads?|files?)\s+never\s+(?:leaves?|reaches?|arrives?)\b/gi,
    /\b(?:the\s+)?(?:attachments?|uploads?|files?)(?:'s)?\s+(?:substance|contents?|meaning)\s+(?:is|are|remains?|becomes?)\s+(?:unintelligible|indecipherable|invisible)\s+to\s+(?:discord|the\s+(?:service|platform))\b/gi,
    /\b(?:osl\s+)?skirts?\s+discord(?:'s)?\s+(?:attachment\s+|file\s+|upload\s+)?audit\b/gi,
  ]) {
    recordPattern(pattern);
  }

  return spans.map(({ start, end }) => ({ start, end }));
}

function semanticAtRestClaimSpans(text) {
  const spans = [];
  const universalScope =
    /(?<!\bat\s)\ball\b|\b(?:each|every|everything|entire|entirety|whole|complete|totality|no|none|nothing|never|zero)\b|100\s*%/i;
  const stateObject =
    /\b(?:state|data|information|records?|storage|metadata|preferences?|settings?|profile|history|files?|content|cache|database|items?|things?|secrets?|artifacts?|residue|material)\b/i;
  const broadCategory =
    /\b(?:private|local|sensitive|confidential)\s+(?:conversation\s+)?(?:state|data|information|records?|storage|metadata|preferences?|settings?|profile|history|artifacts?|residue|material)\b/i;
  const protection =
    /\b(?:encrypt(?:s|ed|ing)?|decrypt(?:s|ed|ing)?|encipher(?:s|ed|ing)?|unencrypted|ciphertext|cleartext|plain[-\s]*text|sealed?|protect(?:s|ed|ing)?|secur(?:e|es|ed|ing)?|gated|guards?|locked|unlocks?|inaccessible|unreadable|opaque|passphrase|password|in\s+the\s+clear)\b/i;
  const localContext =
    /\bat[-\s]+rest\b|\bon[-\s]+disk\b|\bfilesystem\b|\blocal(?:ly)?\b|\bon[-\s]+device\b|\bon\s+(?:this|your|the)\s+(?:device|computer|machine)\b|\bwhole[-\s]+profile\b|\b(?:persist(?:s|ed|ing)?|retain(?:s|ed|ing)?|saved?|stored?)\b/i;
  const destructive =
    /\b(?:delete|deletes|deleted|deleting|remove|removes|removed|removing|uninstall|clear|clears|cleared|clearing)\b/i;
  const residueAbsence =
    /\b(?:(?:osl\s+)?leaves?\s+(?:behind\s+)?(?:no|zero)\s+(?:readable\s+)?(?:private|confidential|sensitive)?\s*(?:residue|artifacts?|data|material)|nothing\s+(?:private|confidential|sensitive)\s+survives?\s+(?:on[-\s]+disk|at[-\s]+rest|locally|on\s+(?:the|your|this)\s+(?:device|computer|machine)))\b/i;
  const attachedLimitation =
    /\b(?:planned|unavailable|unknown|unproved|unproven|not\s+yet\s+(?:available|implemented|proved)|not\s+established|does\s+not\s+(?:claim|cover|protect|encrypt|secure|mean|imply)(?:\s+that)?\s+(?:all|each|every|nothing)|not\s+(?:all|each|every|everything|the\s+(?:entire|whole|complete))|may\s+remain\s+plaintext|plaintext\s+(?:fallback|writes?)|without\s+(?:an?\s+)?(?:installed\s+)?storage\s+key|remov(?:e|es|ed|ing)\b.{0,80}\b(?:restores?|causes?)\s+plaintext\s+writes?)\b/i;

  for (const block of text.matchAll(/[^\n]+/g)) {
    const blockText = block[0];
    const sentences = [...blockText.matchAll(/[^.!?;\n]+[.!?;]?/g)]
      .map((match) => ({
        start: match.index ?? 0,
        text: match[0].trim(),
      }))
      .filter(({ text: sentence }) => sentence);
    const scope = sentences.find(({ text: sentence }) => {
      const broadProtectedState = protection.test(sentence)
        && localContext.test(sentence)
        && (broadCategory.test(sentence)
          || (universalScope.test(sentence) && stateObject.test(sentence)));
      return !attachedLimitation.test(sentence)
        && !destructive.test(sentence)
        && (residueAbsence.test(sentence) || broadProtectedState);
    });
    if (!scope) {
      continue;
    }
    spans.push({
      start: (block.index ?? 0) + scope.start,
      end: (block.index ?? 0) + scope.start + scope.text.length,
    });
  }
  return spans;
}

function semanticScrubClaimSpans(text) {
  const spans = [];
  const scrubContext = /\b(?:auto\s*scrub|scrub)\b/i;
  const attachedLimitation =
    /\b(?:planned|coming\s+soon|unavailable|not\s+(?:available|implemented|wired|supported|proved|proven|qualified)|not\s+yet\s+(?:available|implemented|wired|supported|proved|proven|qualified)|implemented[-\s]+unwired|test[-\s]+proven(?:[-\s]+only)?|unwired|unproved|unproven|unknown|view[-\s]+only|manual(?:ly|\s+only)?|requires?\s+(?:your\s+)?(?:review|confirmation)|does\s+not|cannot|never|may\s+(?:omit|exclude|miss)|can\s+be\s+incomplete|future|intended|design)\b/i;
  const completeHistory =
    /(?:\b(?:complete|full|entire|whole|all|fully\s+reconciled)\b.{0,45}\b(?:history|content|messages?|posts?|records?|account\s+data|exports?|downloads?)\b|\b(?:history|content|messages?|posts?|records?|account\s+data|exports?|downloads?)\b.{0,45}\b(?:complete|full|entire|whole|all|fully\s+reconciled|nothing\s+(?:is\s+)?omitted)\b|\bnothing\s+(?:is\s+)?omitted\b.{0,45}\b(?:scrub|exports?|downloads?|history|content)\b)/i;
  const awayOperation =
    /(?:\b(?:works?|runs?|scans?|cleans?|delet(?:e|es|ed|ing)|remov(?:e|es|ed|ing))\b.{0,55}\b(?:while\s+you.{0,8}\baway|while\s+the\s+user\s+is\s+away|while\s+away|unattended|in\s+the\s+background|without\s+(?:you|the\s+user))\b|\b(?:while\s+you.{0,8}\baway|while\s+the\s+user\s+is\s+away|while\s+away|unattended|in\s+the\s+background|without\s+(?:you|the\s+user))\b.{0,55}\b(?:works?|runs?|scans?|cleans?|delet(?:e|es|ed|ing)|remov(?:e|es|ed|ing))\b)/i;
  const automaticDeletion =
    /(?:\b(?:automatically|autonomously|on\s+its\s+own|without\s+(?:your\s+)?(?:review|confirmation|approval))\b.{0,45}\b(?:deletes?|removes?|cleans?|erases?)\b|\b(?:deletes?|removes?|cleans?|erases?)\b.{0,45}\b(?:automatically|autonomously|on\s+its\s+own|without\s+(?:your\s+)?(?:review|confirmation|approval))\b)/i;
  const providerSupport =
    /\b(?:supports?|works?\s+with|handles?|imports?\s+from|covers?|available\s+(?:for|across|on)|compatible\s+with|(?:natively\s+)?understands?)\b/i;
  const fiveProviderWording =
    /\b(?:five|5)[-\s]+(?:providers?|services?|platforms?|apps?|connectors?)\b/i;
  const providerPatterns = [
    /\bdiscord\b/i,
    /\b(?:meta|facebook|instagram)\b/i,
    /\bwhats\s*app\b/i,
    /\b(?:google|gmail)\b/i,
    /\b(?:twitter|x\/twitter|x)\b/i,
    /\b(?:microsoft|outlook)\b/i,
  ];

  for (const sentenceMatch of text.matchAll(/[^.!?;\n]+[.!?;]?/g)) {
    const sentence = sentenceMatch[0].trim();
    if (!sentence || !scrubContext.test(sentence) || attachedLimitation.test(sentence)) {
      continue;
    }
    const providerCount = providerPatterns.filter((pattern) => pattern.test(sentence)).length;
    if (
      completeHistory.test(sentence)
      || awayOperation.test(sentence)
      || automaticDeletion.test(sentence)
      || fiveProviderWording.test(sentence)
      || (providerSupport.test(sentence) && providerCount >= 5)
    ) {
      const start = sentenceMatch.index ?? 0;
      spans.push({ start, end: start + sentenceMatch[0].length });
    }
  }
  return spans;
}

function supportClaimLimitationGoverns(text, start, end) {
  const bounds = sentenceBounds(text, start, end);
  const sentence = text.slice(bounds.start, bounds.end);
  return /\b(?:planned|coming\s+(?:soon|later)|unavailable|unsupported|not\s+(?:available|supported|ready|proved|proven)|not\s+yet\s+(?:available|supported|ready|proved|proven)|externally\s+blocked|blocked|experimental|beta|qa\s+builds?|testing\s+only|future|roadmap|does\s+not\s+(?:support|protect|cover|claim)|cannot\s+(?:support|protect|cover|claim)|no\s+(?:support|claim))\b/i.test(sentence);
}

function semanticSupportMatrixClaimSpans(text, publicClaimServices) {
  const spans = [];
  const claimPattern =
    /\b(?:supports?|supported|support\s+for|available\s+(?:for|on|with)|works?\s+(?:with|on|in|for)|protects?|protected\s+(?:use|mode|send|receive|replies?|delivery)|compatible\s+with|ready\s+(?:for|on|with))\b/i;

  for (const sentenceMatch of text.matchAll(/[^.!?;\n]+[.!?;]?/g)) {
    const sentence = sentenceMatch[0];
    if (!claimPattern.test(sentence) || /\bsupported\s+connected-account\s+views\b/i.test(sentence)) {
      continue;
    }
    const sentenceStart = sentenceMatch.index ?? 0;
    const sentenceEnd = sentenceStart + sentence.length;
    const blockedServices = SUPPORT_CLAIM_TARGETS
      .filter(({ service, pattern }) => pattern.test(sentence) && !publicClaimServices.has(service));
    if (blockedServices.length === 0) {
      continue;
    }
    if (supportClaimLimitationGoverns(text, sentenceStart, sentenceEnd)) {
      continue;
    }
    spans.push({
      start: sentenceStart,
      end: sentenceEnd,
      service: blockedServices.map(({ service }) => service).join(", "),
    });
  }

  return spans;
}

function validateSupportMatrixClaims(file, fragments, publicClaimServices) {
  const violations = [];
  for (const fragment of fragments) {
    const normalized = normalizedClaimTextWithSourceMap(fragment.text);
    for (const span of semanticSupportMatrixClaimSpans(normalized.text, publicClaimServices)) {
      const sourceIndex = normalized.sourceIndexes[span.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: `validateSupportMatrixClaims: ${span.service} support claim without exact matrix evidence`,
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, span.end - span.start),
        ),
      });
    }
  }
  return violations;
}

function semanticImplementationConceptSpans(source, normalized) {
  if (!PUBLIC_HTML_FRAGMENT_RE.test(source)) {
    return [];
  }

  const spans = [];
  for (const pattern of IMPLEMENTATION_CONCEPT_PATTERNS) {
    for (const match of normalized.matchAll(new RegExp(pattern.source, "gi"))) {
      spans.push({
        start: match.index ?? 0,
        end: (match.index ?? 0) + match[0].length,
      });
    }
  }
  return spans;
}

function countNewlinesBefore(text, index) {
  let count = 0;
  for (let i = 0; i < index; i += 1) {
    if (text[i] === "\n") {
      count += 1;
    }
  }
  return count;
}

// Section D bans '"Audited" / "reviewed" / "independently verified"' as SECURITY
// claims, except the exact b90 source-review claim documented in the public
// allowlist. "Reviewed" is also ordinary English: the first run of this gate
// flagged "Selected apps reviewed" and "Every batch is reviewed and confirmed",
// which are about the *user* reviewing and have nothing to do with an audit.
//
// A gate that cries wolf on honest UI copy gets switched off, so these terms
// only fire in a security context. Multi-word section D phrases stay absolute —
// "cryptographic burn" is never innocent.
const CONTEXT_GATED_TERMS = new Set(["reviewed", "independently verified"]);
const SECURITY_CONTEXT_RE =
  /\b(security|securely|crypto|cryptograph\w*|encryption|encrypted|protocol|third[- ]party|outside firm|externally|independent\w*|auditor\w*|penetration|pentest)\b/i;
const SECURITY_CONTEXT_WINDOW = 90;
const ALLOWED_PUBLIC_REVIEW_CLAIM =
  "a narrow session_reset ratchet remediation was independently reviewed and signed off";
const ALLOWED_PUBLIC_REVIEW_LIMIT =
  "this was source review of one remediation, not a third-party cryptographic audit of osl";
const PUBLIC_REVIEW_CLAIM_DISALLOWED_CONTEXT_RE =
  /\b(?:audit|audited|auditor|cryptograph\w*|encryption|encrypted|outside firm|provider|discord|approval|approved|penetration|pentest)\b/i;

function inSecurityContext(text, index, length) {
  const start = Math.max(0, index - SECURITY_CONTEXT_WINDOW);
  const end = Math.min(text.length, index + length + SECURITY_CONTEXT_WINDOW);
  const before = text.slice(start, index);
  const after = text.slice(index + length, end);
  return SECURITY_CONTEXT_RE.test(before) || SECURITY_CONTEXT_RE.test(after);
}

function sentenceAround(text, index) {
  const beforeBreaks = [".", "!", "?", "\n", ";"].map((mark) => text.lastIndexOf(mark, index));
  const afterBreaks = [".", "!", "?", "\n", ";"]
    .map((mark) => text.indexOf(mark, index))
    .filter((position) => position !== -1);
  const start = Math.max(-1, ...beforeBreaks) + 1;
  const end = afterBreaks.length === 0 ? text.length : Math.min(...afterBreaks);
  return text.slice(start, end).trim();
}

function publicReviewClaimAllowed(text, index, phrase) {
  if (phrase !== "reviewed") {
    return false;
  }

  const claimIndex = text.indexOf(ALLOWED_PUBLIC_REVIEW_CLAIM);
  if (
    claimIndex === -1 ||
    index < claimIndex ||
    index >= claimIndex + ALLOWED_PUBLIC_REVIEW_CLAIM.length ||
    !text.includes(ALLOWED_PUBLIC_REVIEW_LIMIT)
  ) {
    return false;
  }

  return !PUBLIC_REVIEW_CLAIM_DISALLOWED_CONTEXT_RE.test(sentenceAround(text, index));
}

function analyseFragments(file, fragments, bannedPhrases, publicClaimServices = new Set()) {
  const violations = [];

  for (const fragment of fragments) {
    const normalized = normalizedClaimTextWithSourceMap(fragment.text);
    const lower = normalized.text;
    const exactAttachmentRanges = [];

    for (const phrase of bannedPhrases) {
      const contextGated = CONTEXT_GATED_TERMS.has(phrase.normalized);
      let index = lower.indexOf(phrase.normalized);
      while (index !== -1) {
        const end = index + phrase.normalized.length;
        const attachmentClaim = REQUIRED_ATTACHMENT_BANS.includes(phrase.normalized);
        const gatedOut =
          contextGated && !inSecurityContext(lower, index, phrase.normalized.length);
        const limited =
          genericNegationGovernsClaim(lower, index, end)
          || (attachmentClaim && attachmentLimitationGovernsClaim(lower, index, end));
        if (attachmentClaim) {
          exactAttachmentRanges.push({ start: index, end });
        }
        if (
          !gatedOut &&
          !limited &&
          !publicReviewClaimAllowed(lower, index, phrase.normalized)
        ) {
          const sourceIndex = normalized.sourceIndexes[index] ?? 0;
          violations.push({
            file,
            line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
            phrase: phrase.display,
            excerpt: excerptAround(fragment.text, sourceIndex, phrase.normalized.length),
          });
        }

        index = lower.indexOf(phrase.normalized, index + phrase.normalized.length);
      }
    }

    for (const span of semanticAttachmentClaimSpans(lower)) {
      const overlapsExact = exactAttachmentRanges.some(
        (exact) => span.start < exact.end && span.end > exact.start,
      );
      if (overlapsExact || attachmentLimitationGovernsClaim(lower, span.start, span.end)) {
        continue;
      }
      const sourceIndex = normalized.sourceIndexes[span.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: "attachment-scanning overclaim",
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, span.end - span.start),
        ),
      });
    }

    for (const span of semanticAtRestClaimSpans(lower)) {
      const sourceIndex = normalized.sourceIndexes[span.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: "at-rest/local-protection overclaim",
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, span.end - span.start),
        ),
      });
    }

    for (const span of semanticScrubClaimSpans(lower)) {
      const sourceIndex = normalized.sourceIndexes[span.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: "Scrub capability overclaim",
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, span.end - span.start),
        ),
      });
    }

    for (const span of semanticImplementationConceptSpans(fragment.text, lower)) {
      const sourceIndex = normalized.sourceIndexes[span.start] ?? 0;
      violations.push({
        file,
        line: fragment.line + countNewlinesBefore(fragment.text, sourceIndex),
        phrase: "implementation concept in public copy",
        excerpt: excerptAround(
          fragment.text,
          sourceIndex,
          Math.max(1, span.end - span.start),
        ),
      });
    }

    violations.push(...validateSupportMatrixClaims(file, [fragment], publicClaimServices));
  }

  return violations;
}

function padCell(value, width) {
  return String(value).padEnd(width, " ");
}

function printSummary(rows, totals) {
  const headers = ["File", "Units", "Violations"];
  const widths = [
    Math.max(headers[0].length, ...rows.map((row) => row.file.length)),
    Math.max(headers[1].length, ...rows.map((row) => String(row.units).length)),
    Math.max(headers[2].length, ...rows.map((row) => String(row.violations).length)),
  ];

  console.log(
    `${padCell(headers[0], widths[0])}  ${padCell(headers[1], widths[1])}  ${padCell(headers[2], widths[2])}`,
  );
  console.log(`${"-".repeat(widths[0])}  ${"-".repeat(widths[1])}  ${"-".repeat(widths[2])}`);

  for (const row of rows) {
    console.log(
      `${padCell(row.file, widths[0])}  ${padCell(row.units, widths[1])}  ${padCell(row.violations, widths[2])}`,
    );
  }

  console.log(`${"-".repeat(widths[0])}  ${"-".repeat(widths[1])}  ${"-".repeat(widths[2])}`);
  console.log(
    `${padCell("TOTAL", widths[0])}  ${padCell(totals.units, widths[1])}  ${padCell(totals.violations, widths[2])}`,
  );
}

function printViolations(violations) {
  if (violations.length === 0) {
    return;
  }

  console.error("\nViolations:");
  for (const violation of violations) {
    console.error(
      `${violation.file}:${violation.line}: "${violation.phrase}" in "${violation.excerpt}"`,
    );
  }
}

function printFloorFailures(failures) {
  if (failures.length === 0) {
    return;
  }

  console.error("\nFloor failures:");
  for (const failure of failures) {
    console.error(
      `${failure.name}: expected at least ${failure.expected}, actual ${failure.actual}`,
    );
  }
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function jsonPointer(pathParts) {
  if (pathParts.length === 0) {
    return "$";
  }
  return `$${pathParts.map((part) => `[${JSON.stringify(String(part))}]`).join("")}`;
}

function pushMatrixFailure(failures, pathParts, message) {
  failures.push(`${jsonPointer(pathParts)}: ${message}`);
}

function normalizedStatus(value) {
  return typeof value === "string" ? value.trim().toLowerCase() : "";
}

function scanSensitiveEvidenceMaterial(value, failures, pathParts = []) {
  if (Array.isArray(value)) {
    value.forEach((item, index) => {
      scanSensitiveEvidenceMaterial(item, failures, [...pathParts, index]);
    });
    return;
  }

  if (!isPlainObject(value)) {
    return;
  }

  for (const [key, child] of Object.entries(value)) {
    const lowerKey = key.toLowerCase();
    if (FORBIDDEN_EVIDENCE_KEYS.has(lowerKey)) {
      pushMatrixFailure(
        failures,
        [...pathParts, key],
        "chat-app evidence must not carry secrets, credentials, handles, account identifiers, sessions, tokens, or snowflakes",
      );
    }
    scanSensitiveEvidenceMaterial(child, failures, [...pathParts, key]);
  }
}

function collectMatrixSourceAnchors(value, anchors = []) {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectMatrixSourceAnchors(item, anchors);
    }
    return anchors;
  }

  if (!isPlainObject(value)) {
    return anchors;
  }

  if (
    typeof value.path === "string"
    && typeof value.contains === "string"
    && Object.keys(value).some((key) => key === "contains")
  ) {
    anchors.push(value);
  }

  for (const child of Object.values(value)) {
    collectMatrixSourceAnchors(child, anchors);
  }
  return anchors;
}

function validateSupportMatrixObject(matrix) {
  const failures = [];
  if (!isPlainObject(matrix)) {
    return {
      failures: ["$: support matrix must be a JSON object"],
      appCount: 0,
      anchors: [],
    };
  }

  if (Object.hasOwn(matrix, "schema") && matrix.schema !== SUPPORT_MATRIX_SCHEMA) {
    pushMatrixFailure(failures, ["schema"], `must be ${SUPPORT_MATRIX_SCHEMA}`);
  }

  const evidencePath = isPlainObject(matrix.chat_app_qualification_evidence)
    ? ["chat_app_qualification_evidence"]
    : ["chat_app_evidence"];
  const evidence = evidencePath[0] === "chat_app_qualification_evidence"
    ? matrix.chat_app_qualification_evidence
    : matrix.chat_app_evidence;
  if (!isPlainObject(evidence)) {
    pushMatrixFailure(failures, evidencePath, "must be an object");
    return { failures, appCount: 0, anchors: [] };
  }

  if (evidence.schema !== CHAT_APP_EVIDENCE_SCHEMA) {
    pushMatrixFailure(
      failures,
      [...evidencePath, "schema"],
      `must be ${CHAT_APP_EVIDENCE_SCHEMA}`,
    );
  }

  const policy = evidence.evidence_policy;
  if (!isPlainObject(policy)) {
    pushMatrixFailure(failures, [...evidencePath, "evidence_policy"], "must be an object");
  } else {
    for (const key of [
      "no_runtime_inference",
      "no_secret_or_account_identifier_material",
      "absence_of_authority_means_refusal",
    ]) {
      if (policy[key] !== true) {
        pushMatrixFailure(
          failures,
          [...evidencePath, "evidence_policy", key],
          "must be true",
        );
      }
    }
  }

  if (!Array.isArray(evidence.apps)) {
    pushMatrixFailure(failures, [...evidencePath, "apps"], "must be an array");
    return { failures, appCount: 0, anchors: collectMatrixSourceAnchors(evidence) };
  }

  if (evidence.apps.length < MIN_CHAT_APP_EVIDENCE_APPS) {
    pushMatrixFailure(
      failures,
      [...evidencePath, "apps"],
      `must contain at least ${MIN_CHAT_APP_EVIDENCE_APPS} app evidence rows`,
    );
  }

  const byAppId = new Map();
  evidence.apps.forEach((app, index) => {
    const pathParts = [...evidencePath, "apps", index];
    if (!isPlainObject(app)) {
      pushMatrixFailure(failures, pathParts, "must be an object");
      return;
    }

    const appId = typeof app.app_id === "string" ? app.app_id : "";
    if (!appId) {
      pushMatrixFailure(failures, [...pathParts, "app_id"], "must be a non-empty string");
    } else if (byAppId.has(appId)) {
      pushMatrixFailure(failures, [...pathParts, "app_id"], "must be unique");
    } else {
      byAppId.set(appId, { app, index });
    }

    for (const key of ["display_name", "service_family", "public_status", "evidence_status", "qualification"]) {
      if (typeof app[key] !== "string" || app[key].trim() === "") {
        pushMatrixFailure(failures, [...pathParts, key], "must be a non-empty string");
      }
    }

    const evidenceStatus = normalizedStatus(app.evidence_status);
    const publicStatus = normalizedStatus(app.public_status);
    if (!ALLOWED_EVIDENCE_STATUSES.has(evidenceStatus)) {
      pushMatrixFailure(
        failures,
        [...pathParts, "evidence_status"],
        `must be one of ${[...ALLOWED_EVIDENCE_STATUSES].join(", ")}`,
      );
    }
    if (FORBIDDEN_PROMOTION_STATUSES.has(evidenceStatus)) {
      pushMatrixFailure(
        failures,
        [...pathParts, "evidence_status"],
        "must not promote chat-app evidence to runtime, verified-live, release-qualified, or available status",
      );
    }
    if (FORBIDDEN_PROMOTION_STATUSES.has(publicStatus)) {
      pushMatrixFailure(
        failures,
        [...pathParts, "public_status"],
        "must not publish unsupported chat-app availability",
      );
    }

    if (!isPlainObject(app.authority_requirements)) {
      pushMatrixFailure(failures, [...pathParts, "authority_requirements"], "must be an object");
    } else {
      for (const flag of REQUIRED_AUTHORITY_FLAGS) {
        if (app.authority_requirements[flag] !== true) {
          pushMatrixFailure(
            failures,
            [...pathParts, "authority_requirements", flag],
            "must be true; absence of authority means refusal",
          );
        }
      }
    }

    if (!isPlainObject(app.capabilities)) {
      pushMatrixFailure(failures, [...pathParts, "capabilities"], "must be an object");
    } else {
      for (const capability of REQUIRED_CHAT_CAPABILITIES) {
        const status = normalizedStatus(app.capabilities[capability]);
        if (!status) {
          pushMatrixFailure(failures, [...pathParts, "capabilities", capability], "must be present");
        } else if (FORBIDDEN_PROMOTION_STATUSES.has(status)) {
          pushMatrixFailure(
            failures,
            [...pathParts, "capabilities", capability],
            "must remain unavailable until exact chat-app runtime evidence exists",
          );
        }
      }
    }

    if (!Array.isArray(app.dependency_units) || app.dependency_units.length === 0) {
      pushMatrixFailure(failures, [...pathParts, "dependency_units"], "must be a non-empty array");
      return;
    }

    const dependencyUnitIds = new Set();
    app.dependency_units.forEach((dependency, dependencyIndex) => {
      const dependencyPath = [...pathParts, "dependency_units", dependencyIndex];
      if (!isPlainObject(dependency)) {
        pushMatrixFailure(failures, dependencyPath, "must be an object");
        return;
      }
      const unitId = typeof dependency.unit_id === "string" ? dependency.unit_id.toLowerCase() : "";
      if (!unitId) {
        pushMatrixFailure(failures, [...dependencyPath, "unit_id"], "must be a non-empty string");
      } else if (dependencyUnitIds.has(unitId)) {
        pushMatrixFailure(failures, [...dependencyPath, "unit_id"], "must be unique per app");
      } else {
        dependencyUnitIds.add(unitId);
      }

      if (FORBIDDEN_PROMOTION_STATUSES.has(normalizedStatus(dependency.status))) {
        pushMatrixFailure(
          failures,
          [...dependencyPath, "status"],
          "dependency evidence must not be promoted beyond its source proof",
        );
      }
      if (FORBIDDEN_PROMOTION_STATUSES.has(normalizedStatus(dependency.evidence_tier))) {
        pushMatrixFailure(
          failures,
          [...dependencyPath, "evidence_tier"],
          "dependency tier must not claim runtime, verified-live, release-qualified, or supported evidence",
        );
      }
      if (!Array.isArray(dependency.source_anchors) || dependency.source_anchors.length === 0) {
        pushMatrixFailure(failures, [...dependencyPath, "source_anchors"], "must be a non-empty array");
      } else {
        dependency.source_anchors.forEach((anchor, anchorIndex) => {
          const anchorPath = [...dependencyPath, "source_anchors", anchorIndex];
          if (!isPlainObject(anchor)) {
            pushMatrixFailure(failures, anchorPath, "must be an object");
            return;
          }
          if (typeof anchor.path !== "string" || !anchor.path || path.isAbsolute(anchor.path)) {
            pushMatrixFailure(failures, [...anchorPath, "path"], "must be a relative file path");
          }
          if (typeof anchor.contains !== "string" || anchor.contains.length < 8) {
            pushMatrixFailure(failures, [...anchorPath, "contains"], "must be a non-empty source snippet");
          }
        });
      }
    });

    const requiredUnits = REQUIRED_CHAT_APP_UNITS.get(appId);
    if (requiredUnits) {
      for (const unitId of requiredUnits) {
        if (!dependencyUnitIds.has(unitId)) {
          pushMatrixFailure(
            failures,
            [...pathParts, "dependency_units"],
            `must include prerequisite unit ${unitId}`,
          );
        }
      }
    }
  });

  for (const appId of REQUIRED_CHAT_APP_UNITS.keys()) {
    if (!byAppId.has(appId)) {
      pushMatrixFailure(
        failures,
        [...evidencePath, "apps"],
        `must include ${appId} chat-app evidence`,
      );
    }
  }

  scanSensitiveEvidenceMaterial(evidence, failures, evidencePath);

  return {
    failures,
    appCount: evidence.apps.length,
    anchors: collectMatrixSourceAnchors(evidence),
  };
}

async function validateSupportMatrixSources(anchors) {
  const failures = [];
  const sourceCache = new Map();

  for (const [index, anchor] of anchors.entries()) {
    if (
      !isPlainObject(anchor)
      || typeof anchor.path !== "string"
      || typeof anchor.contains !== "string"
      || path.isAbsolute(anchor.path)
    ) {
      continue;
    }

    const sourcePath = path.join(REPO_ROOT, anchor.path);
    let source = sourceCache.get(sourcePath);
    if (source === undefined) {
      try {
        source = await readUtf8(sourcePath);
      } catch (error) {
        if (error && error.code === "ENOENT") {
          failures.push(`source_anchors[${index}]: ${anchor.path} does not exist`);
          sourceCache.set(sourcePath, null);
          continue;
        }
        throw error;
      }
      sourceCache.set(sourcePath, source);
    }

    if (source === null) {
      continue;
    }

    if (!source.includes(anchor.contains)) {
      failures.push(
        `source_anchors[${index}]: ${anchor.path} does not contain ${JSON.stringify(anchor.contains)}`,
      );
    }
  }

  return failures;
}

async function validateSupportMatrixFile() {
  let matrixText = "";
  try {
    matrixText = await readUtf8(SUPPORT_MATRIX_PATH);
  } catch (error) {
    if (error && error.code === "ENOENT") {
      return {
        failures: [`${repoRelative(SUPPORT_MATRIX_PATH)}: file is missing`],
        appCount: 0,
        anchorCount: 0,
      };
    }
    throw error;
  }

  let matrix;
  try {
    matrix = JSON.parse(matrixText);
  } catch (error) {
    return {
      failures: [`${repoRelative(SUPPORT_MATRIX_PATH)}: invalid JSON: ${error.message}`],
      appCount: 0,
      anchorCount: 0,
    };
  }

  const validation = validateSupportMatrixObject(matrix);
  const sourceFailures = await validateSupportMatrixSources(validation.anchors);
  return {
    failures: [...validation.failures, ...sourceFailures],
    appCount: validation.appCount,
    anchorCount: validation.anchors.length,
  };
}

function printSupportMatrixFailures(failures) {
  if (failures.length === 0) {
    return;
  }

  console.error("\nSupport matrix failures:");
  for (const failure of failures) {
    console.error(failure);
  }
}

async function loadBannedPhrases() {
  const allowlist = await readUtf8(ALLOWLIST_PATH);
  return parseBannedPhrases(allowlist);
}

function bannedPhraseInputFailures(bannedPhrases) {
  const failures = [];
  if (bannedPhrases.length < MIN_BANNED_PHRASES) {
    failures.push({
      name: "banned phrases parsed from section D",
      expected: MIN_BANNED_PHRASES,
      actual: bannedPhrases.length,
    });
  }
  const present = new Set(bannedPhrases.map((phrase) => phrase.normalized));
  const attachmentBanCount = REQUIRED_ATTACHMENT_BANS.filter((phrase) => present.has(phrase)).length;
  if (attachmentBanCount < REQUIRED_ATTACHMENT_BANS.length) {
    failures.push({
      name: "attachment bans parsed from section D",
      expected: REQUIRED_ATTACHMENT_BANS.length,
      actual: attachmentBanCount,
    });
  }
  const burnBanCount = REQUIRED_BURN_BANS.filter((phrase) => present.has(phrase)).length;
  if (burnBanCount < REQUIRED_BURN_BANS.length) {
    failures.push({
      name: "burn bans parsed from section D",
      expected: REQUIRED_BURN_BANS.length,
      actual: burnBanCount,
    });
  }
  const supportBanCount = REQUIRED_SUPPORT_BANS.filter((phrase) => present.has(phrase)).length;
  if (supportBanCount < REQUIRED_SUPPORT_BANS.length) {
    failures.push({
      name: "forbidden_support_phrases",
      expected: REQUIRED_SUPPORT_BANS.length,
      actual: supportBanCount,
    });
  }
  return failures;
}

async function scanRepository() {
  const bannedPhrases = await loadBannedPhrases();
  let supportMatrix = null;
  try {
    supportMatrix = JSON.parse(await readUtf8(SUPPORT_MATRIX_PATH));
  } catch {
    supportMatrix = null;
  }
  const publicClaimServices = supportMatrixPublicClaimServices(supportMatrix);
  const supportMatrixFailures = await validateSupportMatrix();
  const rows = [];
  const allViolations = [];
  const publicFragments = [];
  const floorFailures = [
    ...bannedPhraseInputFailures(bannedPhrases),
    ...supportMatrixFailures,
  ];
  let tsStringCount = 0;
  let rustStringCount = 0;
  let readmeBytes = 0;

  const tsFiles = await listTypeScriptFiles(APP_SRC_ROOT);
  for (const filePath of tsFiles) {
    const source = await readUtf8(filePath);
    const fragments = extractTypeScriptStrings(source);
    const file = repoRelative(filePath);
    const violations = analyseFragments(file, fragments, bannedPhrases, publicClaimServices);

    tsStringCount += fragments.length;
    publicFragments.push(...fragments.map((fragment) => ({ ...fragment, file })));
    allViolations.push(...violations);
    rows.push({
      file,
      units: fragments.length,
      violations: violations.length,
    });
  }

  const rustFiles = await listRustFiles(RUST_APP_SRC_ROOT);
  for (const filePath of rustFiles) {
    const source = await readUtf8(filePath);
    const fragments = extractRustStrings(source);
    const file = repoRelative(filePath);
    const violations = analyseFragments(file, fragments, bannedPhrases, publicClaimServices);

    rustStringCount += fragments.length;
    publicFragments.push(...fragments.map((fragment) => ({ ...fragment, file })));
    allViolations.push(...violations);
    rows.push({
      file,
      units: fragments.length,
      violations: violations.length,
    });
  }

  let readmeText = "";
  try {
    readmeText = await readUtf8(README_PATH);
    readmeBytes = Buffer.byteLength(readmeText);
  } catch (error) {
    if (error && error.code !== "ENOENT") {
      throw error;
    }
  }

  const readmeFragments = readmeText ? [{ text: readmeText, line: 1 }] : [];
  const readmeViolations = analyseFragments("README.md", readmeFragments, bannedPhrases, publicClaimServices);
  publicFragments.push(...readmeFragments.map((fragment) => ({ ...fragment, file: "README.md" })));
  allViolations.push(...readmeViolations);
  rows.push({
    file: "README.md",
    units: readmeFragments.length,
    violations: readmeViolations.length,
  });

  if (tsStringCount < MIN_TS_STRING_LITERALS) {
    floorFailures.push({
      name: "TypeScript string literals extracted",
      expected: MIN_TS_STRING_LITERALS,
      actual: tsStringCount,
    });
  }

  if (rustStringCount < MIN_RUST_STRING_LITERALS) {
    floorFailures.push({
      name: "Rust user-visible string literals extracted",
      expected: MIN_RUST_STRING_LITERALS,
      actual: rustStringCount,
    });
  }

  if (readmeBytes < MIN_README_BYTES) {
    floorFailures.push({
      name: "README.md bytes",
      expected: MIN_README_BYTES,
      actual: readmeBytes,
    });
  }

  const supportMatrixResult = await validateSupportMatrixPublicFragments(publicFragments);
  floorFailures.push(...supportMatrixResult.failures);
  allViolations.push(...supportMatrixResult.violations);
  for (const violation of supportMatrixResult.violations) {
    const row = rows.find((candidate) => candidate.file === violation.file);
    if (row) {
      row.violations += 1;
    }
  }

  rows.sort((a, b) => a.file.localeCompare(b.file));
  printSummary(rows, {
    units: tsStringCount + rustStringCount + readmeFragments.length,
    violations: allViolations.length,
  });
  console.log(
    `\nRust scan is a KNOWN-INCOMPLETE high-precision subset: ${rustStringCount} literals from user-visible positions. It does not prove the absence of banned phrases elsewhere in Rust.`,
  );
  console.log(
    `Counts: phrases parsed=${bannedPhrases.length}, TypeScript strings extracted=${tsStringCount}, Rust strings extracted=${rustStringCount}, README bytes=${readmeBytes}, violations found=${allViolations.length}`,
  );
  const qualificationEvidenceSummary = supportMatrix
    ? validateSupportMatrixObject(supportMatrix)
    : { appCount: 0, anchors: [] };
  console.log(
    `Support matrix: qualification apps=${qualificationEvidenceSummary.appCount}, source anchors=${qualificationEvidenceSummary.anchors.length}, floor failures=${supportMatrixFailures.length}`,
  );

  printViolations(allViolations);
  printFloorFailures(floorFailures);

  return allViolations.length === 0
    && floorFailures.length === 0
    ? 0
    : 1;
}

async function runSelfTest() {
  const allowlist = await readUtf8(ALLOWLIST_PATH);
  const tsTestWorkflow = await readUtf8(TS_TEST_WORKFLOW_PATH);
  const bannedPhrases = parseBannedPhrases(allowlist);
  const fixtures = [
    {
      name: "catches destroys-keys inversion",
      text: "The product destroys keys, not messages.",
      shouldFlag: true,
    },
    {
      name: "catches cryptographic burn",
      text: "The app offers cryptographic burn for sensitive notes.",
      shouldFlag: true,
    },
    {
      name: "catches burn recipient-copy overclaim",
      text: "Burn removes recipient copies.",
      shouldFlag: true,
    },
    {
      name: "catches forbidden support phrase",
      text: "Telegram support is available today.",
      shouldFlag: true,
    },
    {
      name: "catches military-grade",
      text: "Protect every message with military-grade privacy controls.",
      shouldFlag: true,
    },
    {
      name: "catches permanent ciphertext",
      text: "All sent messages become permanent ciphertext.",
      shouldFlag: true,
    },
    {
      name: "catches banned provider Burn deletion wording",
      text: "Burn unsends messages from the connected service.",
      shouldFlag: true,
    },
    {
      name: "catches banned Signal support wording",
      text: "OSL supports Signal for protected messages.",
      shouldFlag: true,
    },
    {
      name: "passes denied cryptographic-erasure wording",
      text: "Burn is not cryptographic erasure.",
      shouldFlag: false,
    },
    {
      name: "passes denied permanent-ciphertext wording",
      text: "This is never a permanent ciphertext claim.",
      shouldFlag: false,
    },
    {
      name: "passes ordinary UI use of reviewed (user reviews a batch)",
      text: "Every batch is reviewed and confirmed before anything changes.",
      shouldFlag: false,
    },
    {
      name: "passes ordinary UI use of reviewed (empty state)",
      text: "<strong>Selected apps reviewed</strong><p>Finish setup.</p>",
      shouldFlag: false,
    },
    {
      name: "catches reviewed as a security claim about OSL",
      text: "OSL has been reviewed by an outside firm.",
      shouldFlag: true,
    },
    {
      name: "passes narrow independently reviewed remediation claim",
      text:
        "A narrow SESSION_RESET ratchet remediation was independently reviewed and signed off. "
        + "This was source review of one remediation, not a third-party cryptographic audit of OSL.",
      shouldFlag: false,
    },
    {
      name: "catches narrow independently reviewed claim without limitation",
      text: "A narrow SESSION_RESET ratchet remediation was independently reviewed and signed off.",
      shouldFlag: true,
    },
    {
      name: "catches broad independently reviewed encryption claim",
      text: "OSL encryption was independently reviewed.",
      shouldFlag: true,
    },
    {
      name: "catches independently reviewed claim that implies outside firm",
      text:
        "A narrow SESSION_RESET ratchet remediation was independently reviewed and signed off by an outside firm. "
        + "This was source review of one remediation, not a third-party cryptographic audit of OSL.",
      shouldFlag: true,
    },
    {
      name: "catches audited as a security claim",
      text: "The encryption protocol was audited last year.",
      shouldFlag: true,
    },
    {
      name: "passes clean copy",
      text: "Local removal clears the cached message body.",
      shouldFlag: false,
    },
    {
      name: "passes explicit non-OSL end-to-end limitation",
      text: "Ordinary external email is not OSL end-to-end encrypted.",
      shouldFlag: false,
    },
    {
      name: "catches works-on unsupported app phrasing",
      text: "OSL works on Signal.",
      shouldFlag: true,
    },
    {
      name: "catches supports unsupported app phrasing",
      text: "OSL supports WhatsApp.",
      shouldFlag: true,
    },
    {
      name: "catches available-on unsupported app phrasing",
      text: "Protected messaging is available on Outlook.",
      shouldFlag: true,
    },
    {
      name: "passes status-framed service copy",
      text: "Telegram is Externally blocked until a signed probe proves stable message rows.",
      shouldFlag: false,
    },
    {
      name: "passes denied cryptographic-burn wording",
      text: "This is not cryptographic burn.",
      shouldFlag: false,
    },
    {
      name: "unrelated prior negation cannot launder decoy claim",
      text: "This feature is not beta. Discord sees only decoys.",
      shouldFlag: true,
    },
    {
      name: "catches scanner defeated formulation",
      text: "Discord's attachment scanner is defeated by OSL.",
      shouldFlag: true,
    },
    {
      name: "catches inspection bypass formulation",
      text: "OSL bypasses Discord's attachment inspection.",
      shouldFlag: true,
    },
    {
      name: "catches cover-file substitution formulation",
      text: "Discord receives harmless cover files instead of the attachment.",
      shouldFlag: true,
    },
    {
      name: "catches opaque-upload formulation",
      text: "Uploaded files are opaque to Discord's scanners.",
      shouldFlag: true,
    },
    {
      name: "catches inline-markup scanner defeat",
      text: "Discord&apos;s attachment scanner is de<strong>feated</strong> by OSL.",
      shouldFlag: true,
    },
    {
      name: "catches inline-markup entity bypass",
      text: "OSL bypasses Discord&#39;s <em>attachment inspection</em>.",
      shouldFlag: true,
    },
    {
      name: "catches inline-markup cover substitution",
      text: "Discord receives harmless&nbsp;<strong>cover files</strong> instead of the attachment.",
      shouldFlag: true,
    },
    {
      name: "catches reversed scanner defeat",
      text: "Defeated by OSL is Discord&#x27;s attachment scanner.",
      shouldFlag: true,
    },
    {
      name: "catches reversed inspection bypass",
      text: "Discord's attachment inspection is bypassed by OSL.",
      shouldFlag: true,
    },
    {
      name: "catches neutralized-scanner synonym",
      text: "OSL neutralizes Discord's attachment scanner.",
      shouldFlag: true,
    },
    {
      name: "catches blocked-inspection synonym",
      text: "OSL blocks Discord's attachment inspection.",
      shouldFlag: true,
    },
    {
      name: "catches evaded-scanner synonym",
      text: "OSL evades Discord's attachment scanner.",
      shouldFlag: true,
    },
    {
      name: "unrelated planned sentence cannot launder decoy claim",
      text: "Image transport is Planned. Discord sees only decoys.",
      shouldFlag: true,
    },
    {
      name: "unrelated unproved clause cannot launder bypass claim",
      text: "Another feature is unproved; OSL bypasses Discord's attachment inspection.",
      shouldFlag: true,
    },
    {
      name: "unrelated unknown sentence cannot launder opaque claim",
      text: "Beta status is unknown. Uploaded files are opaque to Discord's scanners.",
      shouldFlag: true,
    },
    {
      name: "unrelated unimplemented sentence cannot launder cover claim",
      text:
        "AutoScrub is not yet implemented. Discord receives harmless cover files instead of the attachment.",
      shouldFlag: true,
    },
    {
      name: "unrelated not-established sentence cannot launder defeated claim",
      text:
        "The release date is not established. Discord's attachment scanner is defeated by OSL.",
      shouldFlag: true,
    },
    {
      name: "passes attached Planned limitation",
      text: "The claim that Discord sees only decoys is Planned.",
      shouldFlag: false,
    },
    {
      name: "passes attached unproved limitation",
      text: "Whether Discord's attachment scanner is defeated by OSL is unproved.",
      shouldFlag: false,
    },
    {
      name: "passes attached unknown limitation",
      text: "Whether OSL bypasses Discord's attachment inspection is unknown.",
      shouldFlag: false,
    },
    {
      name: "passes attached not-yet-implemented limitation",
      text:
        "Discord receiving harmless cover files instead of the attachment is not yet implemented.",
      shouldFlag: false,
    },
    {
      name: "passes attached not-established limitation",
      text:
        "The assertion that uploaded files are opaque to Discord's scanners is not established.",
      shouldFlag: false,
    },
    {
      name: "passes leading attached unproved limitation",
      text: "It is unproved that Discord sees only decoys.",
      shouldFlag: false,
    },
    {
      name: "catches thwarts-scanner paraphrase",
      text: "OSL thwarts Discord's attachment scanner.",
      shouldFlag: true,
    },
    {
      name: "catches circumvents-inspection markup paraphrase",
      text:
        "OSL circum<strong>vents</strong> Discord&apos;s attachment inspection.",
      shouldFlag: true,
    },
    {
      name: "catches prevents-inspection paraphrase",
      text: "OSL prevents Discord from inspecting uploaded files.",
      shouldFlag: true,
    },
    {
      name: "catches ineffective-checks paraphrase",
      text:
        "Discord&apos;s checks on <strong>attachments</strong> are rendered ineffective by OSL.",
      shouldFlag: true,
    },
    {
      name: "catches inspection-no-longer-works paraphrase",
      text: "Attachment inspection by Discord no longer works when OSL is used.",
      shouldFlag: true,
    },
    {
      name: "catches placeholder-instead paraphrase",
      text: "Discord gets a harmless placeholder file instead of the real attachment.",
      shouldFlag: true,
    },
    {
      name: "catches stand-in-only adjacent-clause paraphrase",
      text: "Only a benign stand-in reaches Discord; the original upload does not.",
      shouldFlag: true,
    },
    {
      name: "catches dummy-substitution paraphrase",
      text: "OSL substitutes a dummy image for every attachment sent to Discord.",
      shouldFlag: true,
    },
    {
      name: "catches surrogate-rather-than paraphrase",
      text: "Discord receives a surrogate blob rather than the uploaded file.",
      shouldFlag: true,
    },
    {
      name: "catches placeholder-in-its-place paraphrase",
      text:
        "The actual attachment stays off Discord; a safe placeholder is uploaded in its place.",
      shouldFlag: true,
    },
    {
      name: "catches nothing-except-decoys paraphrase",
      text: "Discord sees nothing except decoy media.",
      shouldFlag: true,
    },
    {
      name: "catches every-visible-file-decoy paraphrase",
      text: "Every file visible to Discord is a decoy.",
      shouldFlag: true,
    },
    {
      name: "catches only-fake-image paraphrase",
      text: "Discord can inspect only a fake image, not the user's file.",
      shouldFlag: true,
    },
    {
      name: "catches original-upload-unreadable paraphrase",
      text: "The original upload remains unreadable to Discord.",
      shouldFlag: true,
    },
    {
      name: "catches scanner-learns-nothing paraphrase",
      text: "Discord's scanner learns nothing about the real attachment.",
      shouldFlag: true,
    },
    {
      name: "passes attached thwarts limitation",
      text: "Whether OSL thwarts Discord's attachment scanner is unproved.",
      shouldFlag: false,
    },
    {
      name: "passes attached prevention limitation",
      text:
        "OSL preventing Discord from inspecting uploaded files is Planned and not yet implemented.",
      shouldFlag: false,
    },
    {
      name: "passes attached placeholder limitation",
      text:
        "The claim that Discord gets a harmless placeholder instead of the real attachment is unknown.",
      shouldFlag: false,
    },
    {
      name: "passes attached stand-in limitation",
      text:
        "Whether only a benign stand-in reaches Discord instead of the upload is not established.",
      shouldFlag: false,
    },
    {
      name: "passes attached visible-decoy limitation",
      text: "Whether every file visible to Discord is a decoy is not established.",
      shouldFlag: false,
    },
    {
      name: "passes attached unreadable-upload limitation",
      text:
        "The original upload being unreadable to Discord is not yet implemented.",
      shouldFlag: false,
    },
    {
      name: "passes ordinary placeholder status copy",
      text: "A placeholder explains that Discord inspection is pending.",
      shouldFlag: false,
    },
    {
      name: "passes local corrupt-file explanation",
      text: "The original upload is unreadable because the local file is corrupt.",
      shouldFlag: false,
    },
    {
      name: "passes ordinary pre-upload prevention copy",
      text: "OSL prevents accidental uploads before Discord opens.",
      shouldFlag: false,
    },
    {
      name: "release I1 unrelated denial cannot launder sidestep",
      text: "OSL does not alter message text. It sidesteps Discord's inspection of uploaded attachments.",
      shouldFlag: true,
    },
    {
      name: "release I2 distant Planned limitation cannot launder review bypass",
      text: "Attachment shielding is Planned for a later release. Account settings are available today. OSL routes uploads around Discord's file review.",
      shouldFlag: true,
    },
    {
      name: "release I3 split-inline scrutiny outflank",
      text: "Discord's upload scru<em>tiny</em> is outflanked by OSL.",
      shouldFlag: true,
    },
    {
      name: "release I4 file-analysis nullification",
      text: "OSL nullifies Discord's file-analysis pass for attachments.",
      shouldFlag: true,
    },
    {
      name: "release I5 split-clause review cannot reach attachment",
      text: "Discord examines uploads.<span>With OSL enabled, that review cannot reach the attachment.</span>",
      shouldFlag: true,
    },
    {
      name: "release I6 public data attribute inspection dodge",
      text: 'data-public-claim="OSL dodges Discord&apos;s attachment review."',
      shouldFlag: true,
    },
    {
      name: "release I7 public comment inspection slip",
      text: "<!-- OSL slips attachments past Discord inspection. -->",
      shouldFlag: true,
    },
    {
      name: "release I8 entity-split useless screening",
      text: "Discord's attachment screening is made use&#108;ess whenever OSL sends.",
      shouldFlag: true,
    },
    {
      name: "release C1 unrelated denial cannot launder proxy substitution",
      text: "OSL does not rename the upload. Discord is handed a benign proxy while the genuine attachment is retained locally.",
      shouldFlag: true,
    },
    {
      name: "release C2 distant Planned limitation cannot launder facade substitution",
      text: "Future attachment privacy remains Planned. Account export is available. A clean facade is sent to Discord in place of the source upload.",
      shouldFlag: true,
    },
    {
      name: "release C3 split-inline proxy exchange",
      text: "The user's attachment is exchanged for a pr<em>ox</em>y before Discord receives it.",
      shouldFlag: true,
    },
    {
      name: "release C4 sanitized shell in attachment place",
      text: "In the attachment's place, Discord receives a sanitized shell.",
      shouldFlag: true,
    },
    {
      name: "release C5 benign proxy swap",
      text: "OSL swaps a benign proxy for each real upload bound for Discord.",
      shouldFlag: true,
    },
    {
      name: "release C6 split-clause separate shell",
      text: "The actual file remains local.<span>Discord receives a separate innocuous shell.</span>",
      shouldFlag: true,
    },
    {
      name: "release C7 public data attribute scrubbed proxy",
      text: 'data-public-claim="Discord gets a scrubbed proxy rather than the source attachment."',
      shouldFlag: true,
    },
    {
      name: "release C8 entity-split surrogate",
      text: "Discord receives a surr&#111;gate instead of the uploaded file.",
      shouldFlag: true,
    },
    {
      name: "release D1 unrelated denial cannot launder blind-content claim",
      text: "OSL does not hide message timestamps. Discord is blind to the contents of real attachments.",
      shouldFlag: true,
    },
    {
      name: "release D2 distant Planned limitation cannot launder zero-information claim",
      text: "Some protections are Planned. Account export is available. Discord gains no information from the actual upload.",
      shouldFlag: true,
    },
    {
      name: "release D3 split-inline unintelligible claim",
      text: "Nothing about the source file is intelligible to Dis<strong>cord</strong>.",
      shouldFlag: true,
    },
    {
      name: "release D4 indecipherable upload",
      text: "Real uploads are indecipherable to Discord.",
      shouldFlag: true,
    },
    {
      name: "release D5 split-clause zero-content reference",
      text: "Discord receives an upload reference.<span>The referenced attachment reveals zero content to it.</span>",
      shouldFlag: true,
    },
    {
      name: "release D6 public data attribute cannot-discern claim",
      text: 'data-public-claim="Discord cannot discern the user&apos;s attachment."',
      shouldFlag: true,
    },
    {
      name: "release D7 public comment invisible claim",
      text: "<!-- To Discord, every genuine attachment is invisible. -->",
      shouldFlag: true,
    },
    {
      name: "release D8 entity-split invisible claim",
      text: "To Discord, the genuine attachment is invisi&#98;le.",
      shouldFlag: true,
    },
    {
      name: "new inspection paraphrase tunnels beyond review",
      text: "OSL tunnels uploads beyond Discord's attachment review.",
      shouldFlag: true,
    },
    {
      name: "new cover paraphrase sanitized stand-in",
      text: "Discord is shown a sanitized stand-in while the source file remains local.",
      shouldFlag: true,
    },
    {
      name: "new visibility paraphrase extracts no meaning",
      text: "Discord can extract no meaning from the attachment payload.",
      shouldFlag: true,
    },
    {
      name: "passes honest unavailable inspection statement",
      text: "OSL sidestepping Discord's attachment inspection is unavailable.",
      shouldFlag: false,
    },
    {
      name: "passes honest explicit denial of blind claim",
      text: "Discord is not blind to the contents of real attachments.",
      shouldFlag: false,
    },
    {
      name: "passes honest unknown proxy limitation",
      text: "Whether Discord receives a sanitized proxy instead of the source upload is unknown.",
      shouldFlag: false,
    },
    {
      name: "catches all private state encrypted at rest",
      text: "All private state is encrypted at rest.",
      shouldFlag: true,
    },
    {
      name: "catches all local state password protected",
      text: "All local state is password-protected.",
      shouldFlag: true,
    },
    {
      name: "catches password encrypts all local data",
      text: "Your password encrypts all local data.",
      shouldFlag: true,
    },
    {
      name: "catches every private device record",
      text: "Every private record on this device is protected with your password.",
      shouldFlag: true,
    },
    {
      name: "catches recovery material for broad local state",
      text: "OSL provides recovery material for password-protected local state.",
      shouldFlag: true,
    },
    {
      name: "catches inline and entity split local protection",
      text: "All <em>local</em> state is password&#45;protected.",
      shouldFlag: true,
    },
    {
      name: "catches reversed broad at-rest phrasing",
      text: "Encrypted at rest with your password: all private state on this device.",
      shouldFlag: true,
    },
    {
      name: "catches broad public comment",
      text: "<!-- All private state is encrypted at rest. -->",
      shouldFlag: true,
    },
    {
      name: "catches broad public data attribute",
      text: '<div data-public-claim="Your password encrypts all local data."></div>',
      shouldFlag: true,
    },
    {
      name: "unrelated planned statement cannot launder broad at-rest claim",
      text: "Account export is Planned. All local state is password-protected.",
      shouldFlag: true,
    },
    {
      name: "passes honest key-scoped identity statement",
      text: "Private identity keys are encrypted at rest by the operating-system credential sealer.",
      shouldFlag: false,
    },
    {
      name: "passes honest key-scoped message-store statement",
      text: "Decrypted message bodies in the message store are encrypted at rest.",
      shouldFlag: false,
    },
    {
      name: "passes explicit plaintext limitation",
      text: "Some conversation metadata and preferences may remain plaintext.",
      shouldFlag: false,
    },
    {
      name: "passes explicit all-record denial",
      text: "Password protection does not cover every local record.",
      shouldFlag: false,
    },
    {
      name: "passes attached unknown broad claim",
      text: "Whether all local state is password-protected is unknown.",
      shouldFlag: false,
    },
    {
      name: "catches skirts attachment audit paraphrase",
      text: "OSL skirts Discord’s audit.",
      shouldFlag: true,
    },
    {
      name: "catches toothless attachment audit paraphrase",
      text: "OSL renders Discord’s attachment audit toothless.",
      shouldFlag: true,
    },
    {
      name: "catches harmless double paraphrase",
      text: "Discord receives a harmless double.",
      shouldFlag: true,
    },
    {
      name: "catches sanitized lookalike paraphrase",
      text: "Discord receives a sanitized lookalike in lieu of the real file.",
      shouldFlag: true,
    },
    {
      name: "catches cannot make sense paraphrase",
      text: "Discord cannot make sense of the original attachment.",
      shouldFlag: true,
    },
    {
      name: "catches sees only gibberish paraphrase",
      text: "Discord sees only gibberish.",
      shouldFlag: true,
    },
    {
      name: "passes attached unknown gibberish limitation",
      text: "Whether Discord sees only gibberish is unknown.",
      shouldFlag: false,
    },
    {
      name: "catches complete Scrub history claim",
      text: "Scrub imports your complete account history.",
      shouldFlag: true,
    },
    {
      name: "catches reversed full-history claim",
      text: "Your full history is covered by Scrub.",
      shouldFlag: true,
    },
    {
      name: "catches all-content markup and entity claim",
      text: "Scrub scans <strong>all&nbsp;content</strong> in the export.",
      shouldFlag: true,
    },
    {
      name: "catches away-operation public comment",
      text: "<!-- Scrub runs unattended. -->",
      shouldFlag: true,
    },
    {
      name: "catches works-while-away phrasing",
      text: "Scrub works while you're away.",
      shouldFlag: true,
    },
    {
      name: "catches automatic deletion data attribute",
      text: '<button data-public-claim="AutoScrub automatically deletes old posts.">Run</button>',
      shouldFlag: true,
    },
    {
      name: "catches generic five-provider wording",
      text: "Scrub works with five providers.",
      shouldFlag: true,
    },
    {
      name: "catches enumerated five-provider support",
      text: "Scrub supports Discord, Meta, WhatsApp, Google, and X.",
      shouldFlag: true,
    },
    {
      name: "unrelated planned sentence cannot launder complete-history claim",
      text: "AutoScrub is Planned. Scrub imports your complete history.",
      shouldFlag: true,
    },
    {
      name: "passes explicit incomplete-export limitation",
      text: "A Scrub provider export may omit remote-only messages and can be incomplete.",
      shouldFlag: false,
    },
    {
      name: "passes view-only away-operation denial",
      text: "Free Scrub is view-only and never works while you are away.",
      shouldFlag: false,
    },
    {
      name: "passes planned automatic-deletion statement",
      text: "AutoScrub automatic deletion is Planned and unavailable in this build.",
      shouldFlag: false,
    },
    {
      name: "passes implemented-unwired parser statement",
      text: "Scrub provider-export parsing is implemented-unwired and test-proven-only.",
      shouldFlag: false,
    },
    {
      name: "passes planned five-provider targets",
      text: "Discord, Meta, WhatsApp, Google, and X are Planned targets for Scrub.",
      shouldFlag: false,
    },
    {
      name: "passes explicit provider-support unknown",
      text: "Whether Scrub supports five providers is unknown.",
      shouldFlag: false,
    },
    {
      name: "passes non-Scrub provider list",
      text: "The roadmap names Discord, Meta, WhatsApp, Google, and X.",
      shouldFlag: false,
    },
    {
      name: "catches every-sensitive-artifact sealing claim",
      text: "Every sensitive artifact retained by OSL is sealed cryptographically.",
      shouldFlag: true,
    },
    {
      name: "catches no-readable-private-residue claim",
      text: "OSL leaves no readable private residue.",
      shouldFlag: true,
    },
    {
      name: "catches nothing-confidential-survives claim",
      text: "Nothing confidential survives on disk.",
      shouldFlag: true,
    },
    {
      name: "catches content-analysis-no-view claim",
      text: "Discord's content-analysis machinery gets no useful view of the attachment.",
      shouldFlag: true,
    },
    {
      name: "catches benign-twin source-never-leaves claim",
      text: "The service receives a benign twin; the source attachment never leaves your device.",
      shouldFlag: true,
    },
    {
      name: "catches unintelligible-substance claim",
      text: "The attachment's substance is unintelligible to Discord.",
      shouldFlag: true,
    },
    {
      name: "catches generalized skirts-audit claim",
      text: "Skirts Discord's attachment audit.",
      shouldFlag: true,
    },
    {
      name: "catches keeps-deleting-while-away claim",
      text: "AutoScrub keeps deleting old posts while you're away.",
      shouldFlag: true,
    },
    {
      name: "catches fully-reconciled-download claim",
      text: "Scrub downloads are fully reconciled and nothing is omitted.",
      shouldFlag: true,
    },
    {
      name: "catches five-provider native-understanding claim",
      text: "Scrub natively understands exports from Discord, Meta, WhatsApp, Google, and Microsoft.",
      shouldFlag: true,
    },
    {
      name: "passes limited residue statement",
      text: "OSL does not claim that nothing confidential survives on disk; some local records may remain plaintext.",
      shouldFlag: false,
    },
    {
      name: "passes planned benign-twin wording",
      text: "A benign twin replacing the source attachment remains Planned and unproved.",
      shouldFlag: false,
    },
    {
      name: "passes limited Scrub downloads wording",
      text: "Scrub downloads may omit records and are not yet qualified as complete.",
      shouldFlag: false,
    },
    {
      name: "passes planned Microsoft provider wording",
      text: "Discord, Meta, WhatsApp, Google, and Microsoft are Planned targets for Scrub.",
      shouldFlag: false,
    },
  ];
  const rustFixtures = [
    {
      name: "rust catches banned struct label",
      source: 'fn live() { let view = UiCopy { label: "cryptographic burn".into() }; }',
      shouldFlag: true,
      expectedSelected: 1,
    },
    {
      name: "rust catches banned Err message",
      source: 'fn live() -> Result<(), String> { Err("cryptographic burn".into()) }',
      shouldFlag: true,
      expectedSelected: 1,
    },
    {
      name: "rust skips cfg-test block",
      source:
        '#[cfg(test)]\nmod tests { fn claim() { let view = UiCopy { label: "cryptographic burn".into() }; } }',
      shouldFlag: false,
      expectedSelected: 0,
    },
    {
      name: "rust skips identifier-shaped literal",
      source: 'fn live() { let view = UiCopy { label: "unbreakable".into() }; }',
      shouldFlag: false,
      expectedSelected: 0,
    },
  ];
  const supportMatrixFixture = {
    schema: SUPPORT_MATRIX_SCHEMA,
    updated_utc: "2026-07-30T00:00:00Z",
    chat_app_evidence: {
      schema: CHAT_APP_EVIDENCE_SCHEMA,
      unit: "p2",
      evidence_policy: {
        no_runtime_inference: true,
        no_secret_or_account_identifier_material: true,
        absence_of_authority_means_refusal: true,
      },
      apps: [
        {
          app_id: "discord",
          display_name: "Discord",
          service_family: "messaging",
          public_status: "unavailable",
          evidence_status: "blocked",
          qualification: "not-qualified",
          authority_requirements: {
            user_consent_required: true,
            account_binding_required: true,
            release_authority_required: true,
          },
          capabilities: {
            protected_send: "unavailable",
            protected_receive: "unavailable",
            attachments: "unavailable",
            burn: "unavailable",
          },
          dependency_units: [
            {
              unit_id: "d6",
              status: "open-security-finding",
              evidence_tier: "source/test-proven-only",
              source_anchors: [
                {
                  path: "docs/design/osl-internal-build-checklist.md",
                  contains: "D6 · Bilateral Burn",
                },
              ],
            },
            {
              unit_id: "w9",
              status: "blocks-core-feature",
              evidence_tier: "source-audit",
              source_anchors: [
                {
                  path: "docs/OSL-DISCORD-STATE-MAP.md",
                  contains: "| W9 | Nothing raises Discord above the composer at engage time |",
                },
              ],
            },
          ],
        },
        {
          app_id: "signal",
          display_name: "Signal",
          service_family: "messaging",
          public_status: "unavailable",
          evidence_status: "source-profile-only",
          qualification: "profile-published-unwired",
          authority_requirements: {
            user_consent_required: true,
            account_binding_required: true,
            release_authority_required: true,
          },
          capabilities: {
            protected_send: "unavailable",
            protected_receive: "unavailable",
            attachments: "unavailable",
            burn: "unavailable",
          },
          dependency_units: [
            {
              unit_id: "s9",
              status: "source-profile-published",
              evidence_tier: "source/test-proven-only",
              source_anchors: [
                {
                  path: "crates/adapter-profile/src/defaults.rs",
                  contains: "signal_default_profile",
                },
              ],
            },
          ],
        },
      ],
    },
  };
  const cloneSupportMatrixFixture = () => JSON.parse(JSON.stringify(supportMatrixFixture));
  const supportMatrixFixtures = [
    {
      name: "support matrix accepts dependency-bound chat-app evidence",
      mutate: () => {},
      shouldPass: true,
    },
    {
      name: "support matrix catches missing W9 dependency",
      mutate: (matrix) => {
        matrix.chat_app_evidence.apps[0].dependency_units =
          matrix.chat_app_evidence.apps[0].dependency_units.filter(
            (dependency) => dependency.unit_id !== "w9",
          );
      },
      shouldPass: false,
    },
    {
      name: "support matrix catches Signal runtime promotion",
      mutate: (matrix) => {
        matrix.chat_app_evidence.apps[1].evidence_status = "runtime-proven";
      },
      shouldPass: false,
    },
    {
      name: "support matrix catches capability availability promotion",
      mutate: (matrix) => {
        matrix.chat_app_evidence.apps[0].capabilities.protected_send = "available";
      },
      shouldPass: false,
    },
    {
      name: "support matrix catches missing account-binding authority",
      mutate: (matrix) => {
        matrix.chat_app_evidence.apps[1].authority_requirements.account_binding_required = false;
      },
      shouldPass: false,
    },
    {
      name: "support matrix catches missing source anchors",
      mutate: (matrix) => {
        matrix.chat_app_evidence.apps[0].dependency_units[0].source_anchors = [];
      },
      shouldPass: false,
    },
    {
      name: "support matrix catches sensitive handle material",
      mutate: (matrix) => {
        matrix.chat_app_evidence.apps[0].handle = "not-allowed";
      },
      shouldPass: false,
    },
  ];

  let failures = 0;
  const renamedSection = allowlist.replace(
    "## D · NOT ELIGIBLE",
    "## D (renamed) · NOT ELIGIBLE",
  );
  const sectionStart = allowlist.indexOf("## D · NOT ELIGIBLE");
  const sectionEnd = allowlist.indexOf("\n## E ·", sectionStart);
  const starvedSection =
    sectionStart === -1 || sectionEnd === -1
      ? allowlist
      : `${allowlist.slice(0, sectionStart)}`
        + "## D · NOT ELIGIBLE — these phrases may not appear anywhere\n\n"
        + "| Forbidden phrase | Why it is forbidden |\n|---|---|\n"
        + `${allowlist.slice(sectionEnd + 1)}`;
  const productionSupportMatrix = JSON.parse(await readUtf8(SUPPORT_MATRIX_PATH));
  const productionConditionalRows = new Map(
    Array.isArray(productionSupportMatrix.conditional_app_evidence)
      ? productionSupportMatrix.conditional_app_evidence
        .filter((row) => row && typeof row === "object" && typeof row.id === "string")
        .map((row) => [row.id, row])
      : [],
  );
  const supportMatrixWithoutPublic = JSON.parse(JSON.stringify(productionSupportMatrix));
  delete supportMatrixWithoutPublic.versioned_public_support_matrix;
  const supportMatrixWithPublicSchemaDrift = JSON.parse(JSON.stringify(productionSupportMatrix));
  supportMatrixWithPublicSchemaDrift.versioned_public_support_matrix.schema_version = 2;
  const supportMatrixWithoutChatEvidence = JSON.parse(JSON.stringify(productionSupportMatrix));
  delete supportMatrixWithoutChatEvidence.chat_app_evidence;
  const supportMatrixWithoutSignalEvidence = JSON.parse(JSON.stringify(productionSupportMatrix));
  supportMatrixWithoutSignalEvidence.chat_app_evidence =
    supportMatrixWithoutSignalEvidence.chat_app_evidence.filter(
      (row) => row.id !== "signal_desktop_native",
    );
  const supportMatrixWithPromotedChatEvidence = JSON.parse(JSON.stringify(productionSupportMatrix));
  supportMatrixWithPromotedChatEvidence.chat_app_evidence
    .find((row) => row.id === "signal_desktop_native").status = "supported";
  const supportMatrixWithoutTelegramVerdict = JSON.parse(JSON.stringify(productionSupportMatrix));
  supportMatrixWithoutTelegramVerdict.conditional_app_evidence =
    supportMatrixWithoutTelegramVerdict.conditional_app_evidence.filter(
      (row) => row.id !== "telegram_desktop_native",
    );
  const supportMatrixWithPromotedTelegramVerdict = JSON.parse(JSON.stringify(productionSupportMatrix));
  supportMatrixWithPromotedTelegramVerdict.conditional_app_evidence
    .find((row) => row.id === "telegram_desktop_native").status = "supported";
  const supportMatrixWithPromotedMailClaim = JSON.parse(JSON.stringify(productionSupportMatrix));
  supportMatrixWithPromotedMailClaim.versioned_public_support_matrix.rows
    .find((row) => row.id === "osl_mail_public").claim_allowed = true;
  const supportMatrixWithForgedPublicClaim = JSON.parse(JSON.stringify(productionSupportMatrix));
  supportMatrixWithForgedPublicClaim.versioned_public_support_matrix.rows
    .find((row) => row.id === "signal_desktop_public").claim_allowed = true;
  const telegramMailPublicProof = await validateSupportMatrixPublicFragments([
    {
      file: "self-test/telegram-mail-verdicts",
      text: "Telegram Desktop protected messaging is supported.",
      line: 1,
    },
    {
      file: "self-test/telegram-mail-verdicts",
      text: "OSL Mail works with Outlook protected replies.",
      line: 2,
    },
    {
      file: "self-test/telegram-mail-verdicts",
      text: "Telegram Desktop protected messaging is externally blocked until stable message rows are proved.",
      line: 3,
    },
  ]);
  const claimGateWorkflowWithoutSelfTest = tsTestWorkflow.replace(
    /\n\s+node scripts\/check-app-claims\.mjs --self-test\n/,
    "\n",
  );
  const claimGateWorkflowWithoutScan = tsTestWorkflow.replace(
    /\n\s+node scripts\/check-app-claims\.mjs\n/,
    "\n",
  );
  const minimalForbiddenPhraseAllowlist = [
    "# Fixture",
    "## D · NOT ELIGIBLE — these phrases may not appear anywhere",
    "",
    "| Forbidden phrase | Why it is forbidden |",
    "|---|---|",
    '| **"Cryptographic burn" / "permanent ciphertext"** | unearned burn wording |',
    "",
    "## E · Status mapping",
  ].join("\n");
  const minimalForbiddenPhrases = parseBannedPhrases(minimalForbiddenPhraseAllowlist);
  const inputCases = [
    {
      name: "Gate public claims through the allowlist self-test before release.",
      passed:
        validateClaimGateWorkflow(tsTestWorkflow).length === 0
        && validateClaimGateWorkflow(claimGateWorkflowWithoutSelfTest).some(
          (failure) => failure.includes("self-test"),
        )
        && validateClaimGateWorkflow(claimGateWorkflowWithoutScan).some(
          (failure) => failure.includes("repository scan"),
        ),
    },
    {
      name: "Bind banned public phrases into the app-claim parser.",
      passed:
        minimalForbiddenPhrases.some((phrase) => phrase.normalized === "cryptographic burn")
        && minimalForbiddenPhrases.some((phrase) => phrase.normalized === "permanent ciphertext")
        && analyseFragments(
          "self-test/minimal-forbidden-phrases",
          [{ text: "This build offers cryptographic burn.", line: 1 }],
          minimalForbiddenPhrases,
        ).some((violation) => violation.phrase === "Cryptographic burn")
        && analyseFragments(
          "self-test/minimal-forbidden-phrases",
          [{ text: "This build offers local deletion.", line: 1 }],
          minimalForbiddenPhrases,
        ).length === 0,
    },
    {
      name: "Scan repository copy without exposing implementation concepts to users.",
      passed:
        analyseFragments(
          "self-test/public-copy",
          [{ text: '<button data-public-claim="Open keyserver settings">Keyserver settings</button>', line: 1 }],
          bannedPhrases,
        ).some((violation) => violation.phrase === "implementation concept in public copy")
        && analyseFragments(
          "self-test/public-copy",
          [{ text: "<button>Open protected messages</button>", line: 1 }],
          bannedPhrases,
        ).length === 0
        && extractTypeScriptStrings('import secret from "keyserver";\n// "ratchet"\nconst label = "<button>Open protected messages</button>";')
          .every((fragment) => !/\b(?:keyserver|ratchet)\b/i.test(fragment.text)),
    },
    {
      name: SUPPORT_MATRIX_PUBLIC_PROOF_NAME,
      passed:
        supportMatrixPublicClaimProofFailures(productionConditionalRows).length === 0
        && validateSupportMatrixClaims(
          "self-test/exact-support-evidence",
          [{ text: "Signal is supported for protected messaging.", line: 1 }],
          new Set(),
        ).some((violation) => violation.phrase.includes("Signal support claim"))
        && validateSupportMatrixClaims(
          "self-test/exact-support-evidence",
          [{ text: "Signal is supported for protected messaging.", line: 1 }],
          new Set(["Signal"]),
        ).length === 0,
    },
    {
      name: "Claim-gate-in-app-copy-and-README",
      passed:
        validateClaimGateWorkflow(tsTestWorkflow).length === 0
        && validateClaimGateWorkflow(claimGateWorkflowWithoutScan).some(
          (failure) => failure.includes("repository scan"),
        ),
    },
    {
      name: "real docs allowlist supplies every required attachment ban",
      passed: bannedPhraseInputFailures(bannedPhrases).length === 0,
    },
    {
      name: "real docs allowlist binds section-D quoted alternatives",
      passed: [
        "better than signal",
        "post-quantum authentication",
        "cryptographic burn",
        "destroys keys, not messages",
        "permanent ciphertext",
        "permanent gibberish",
        "mathematically opaque",
        "disappears forever",
        "permanently undecryptable",
        "gone for good",
        "burn unsends messages",
        "works on gmail",
        "works on discord",
        "osl supports signal",
        "works on whatsapp",
        "provider-tested",
        "verified by discord",
        "works with discord's approval",
        "audited",
        "reviewed",
        "independently verified",
        "military-grade",
        "unbreakable",
        "nsa-proof",
        "screenshot-proof",
        "prevents screenshots",
        "end-to-end encrypted",
        "your month starts when you enter the code",
        "anti-spyware",
        "malware detection",
        "protection score",
        "works on signal",
        "works on whatsapp",
        "works on telegram",
        "works on outlook",
        "supports gmail",
        "supports discord",
        "supports signal",
        "supports whatsapp",
        "supports telegram",
        "supports outlook",
        "available on gmail",
        "available on discord",
        "available on signal",
        "available on whatsapp",
        "available on telegram",
        "available on outlook",
      ].every((phrase) => bannedPhrases.some((parsed) => parsed.normalized === phrase)),
    },
    {
      name: "forbidden_support_phrases",
      passed: REQUIRED_SUPPORT_BANS.every((phrase) => bannedPhrases.some((parsed) => parsed.normalized === phrase))
        && REQUIRED_BURN_BANS.every((phrase) => bannedPhrases.some((parsed) => parsed.normalized === phrase)),
    },
    {
      name: "renamed section D fails the production phrase floor",
      passed:
        parseBannedPhrases(renamedSection).length === 0
        && bannedPhraseInputFailures(parseBannedPhrases(renamedSection)).length > 0,
    },
    {
      name: "starved section D fails the production phrase floor",
      passed:
        parseBannedPhrases(starvedSection).length === 0
        && bannedPhraseInputFailures(parseBannedPhrases(starvedSection)).length > 0,
    },
    {
      name: "Define the versioned public support matrix schema",
      passed:
        (await validateVersionedPublicSupportMatrix(productionSupportMatrix)).length === 0
        && (await validateVersionedPublicSupportMatrix(supportMatrixWithoutPublic)).some(
          (failure) => failure.name === "versioned_public_support_matrix",
        )
        && (await validateVersionedPublicSupportMatrix(supportMatrixWithPublicSchemaDrift)).some(
          (failure) => failure.expected.includes("schema_version=1"),
        )
        && (await validateVersionedPublicSupportMatrix(supportMatrixWithForgedPublicClaim)).some(
          (failure) => failure.expected.includes("Signal.claim_allowed=false"),
        ),
    },
    {
      name: "Ingest qualified chat-app evidence into the matrix",
      passed:
        (await validateChatAppEvidence(productionSupportMatrix)).length === 0
        && validateSupportMatrixEvidenceLinks(productionSupportMatrix).length === 0
        && (await validateChatAppEvidence(supportMatrixWithoutChatEvidence)).some(
          (failure) => failure.name === "chat_app_evidence",
        )
        && (await validateChatAppEvidence(supportMatrixWithoutSignalEvidence)).some(
          (failure) => failure.expected === "row signal_desktop_native",
        )
        && (await validateChatAppEvidence(supportMatrixWithPromotedChatEvidence)).some(
          (failure) => failure.expected.includes("signal_desktop_native.status=\"qualified_profile\""),
        ),
    },
    {
      name: "Ingest Telegram and Mail verdicts into the matrix",
      passed:
        telegramMailPublicProof.failures.length === 0
        && telegramMailPublicProof.violations.some(
          (violation) => violation.phrase === "Telegram protected support overclaim",
        )
        && telegramMailPublicProof.violations.some(
          (violation) => violation.phrase === "Outlook OSL Mail protected support overclaim",
        )
        && !telegramMailPublicProof.violations.some(
          (violation) => violation.line === 3,
        )
        && validateSupportMatrixEvidenceLinks(supportMatrixWithoutTelegramVerdict).some(
          (failure) => failure.expected === "evidence row conditional_app_evidence.telegram_desktop_native",
        )
        && validateSupportMatrixEvidenceLinks(supportMatrixWithPromotedTelegramVerdict).some(
          (failure) => failure.expected.includes("telegram_desktop_native.status=\"externally_blocked\""),
        )
        && validateSupportMatrixEvidenceLinks(supportMatrixWithPromotedMailClaim).some(
          (failure) => failure.expected.includes("osl_mail_public.claim_allowed=false"),
        ),
    },
    {
      name: "versioned_public_support_matrix",
      passed:
        (await validateVersionedPublicSupportMatrix(productionSupportMatrix)).length === 0
        && (await validateVersionedPublicSupportMatrix(supportMatrixWithoutPublic)).some(
          (failure) => failure.name === "versioned_public_support_matrix",
        ),
    },
    {
      name: "chat_app_evidence",
      passed:
        (await validateChatAppEvidence(productionSupportMatrix)).length === 0
        && (await validateChatAppEvidence(supportMatrixWithoutChatEvidence)).some(
          (failure) => failure.name === "chat_app_evidence",
        ),
    },
    {
      name: "validateSupportMatrixClaims",
      passed:
        validateSupportMatrixEvidenceLinks(productionSupportMatrix).length === 0
        && validateSupportMatrixEvidenceLinks(supportMatrixWithForgedPublicClaim).some(
          (failure) => failure.name === "validateSupportMatrixClaims",
        ),
    },
  ];
  for (const inputCase of inputCases) {
    if (!inputCase.passed) {
      failures += 1;
    }
    console.log(
      `${inputCase.passed ? "PASS" : "FAIL"} ${inputCase.name}`,
    );
  }

  for (const fixture of supportMatrixFixtures) {
    const matrix = cloneSupportMatrixFixture();
    fixture.mutate(matrix);
    const validation = validateSupportMatrixObject(matrix);
    const passed = validation.failures.length === 0;
    const ok = passed === fixture.shouldPass;
    if (!ok) {
      failures += 1;
    }
    console.log(
      `${ok ? "PASS" : "FAIL"} ${fixture.name}: expected ${fixture.shouldPass ? "pass" : "fail"}, actual ${passed ? "pass" : "fail"}`
        + (ok ? "" : ` (${validation.failures.join("; ") || "no failure"})`),
    );
  }

  const mutatedVersionedPublicMatrix = structuredClone(productionSupportMatrix);
  mutatedVersionedPublicMatrix.versioned_public_support_matrix.rows[0].public_claim_allowed = true;
  const versionedPublicSupportMatrixPassed =
    (await validateVersionedPublicSupportMatrix(productionSupportMatrix)).length === 0
    && (await validateVersionedPublicSupportMatrix(mutatedVersionedPublicMatrix)).some(
      (failure) => failure.name === "versioned_public_support_matrix",
    );
  if (!versionedPublicSupportMatrixPassed) {
    failures += 1;
  }
  console.log(
    `${versionedPublicSupportMatrixPassed ? "PASS" : "FAIL"} versioned_public_support_matrix rejects public_claim_allowed`,
  );

  const supportClaimFixtures = [
    {
      name: "validateSupportMatrixClaims",
      text: "Telegram is supported for protected native messaging.",
      shouldFlag: true,
    },
    {
      name: "validateSupportMatrixClaims passes exact supported evidence",
      text: "Signal is supported for protected messaging.",
      shouldFlag: false,
      publicClaimServices: new Set(["Signal"]),
    },
    {
      name: "validateSupportMatrixClaims passes honest blocked wording",
      text: "Telegram is externally blocked for protected messaging.",
      shouldFlag: false,
    },
    {
      name: "validateSupportMatrixClaims catches Outlook OSL Mail overclaim",
      text: "OSL Mail works with Outlook protected replies.",
      shouldFlag: true,
    },
  ];
  for (const fixture of supportClaimFixtures) {
    const violations = validateSupportMatrixClaims(
      `self-test/${fixture.name}`,
      [{ text: fixture.text, line: 1 }],
      fixture.publicClaimServices ?? new Set(),
    );
    const flagged = violations.length > 0;
    const ok = flagged === fixture.shouldFlag;
    if (!ok) {
      failures += 1;
    }
    console.log(
      `${ok ? "PASS" : "FAIL"} ${fixture.name}: expected ${fixture.shouldFlag ? "flag" : "pass"}, actual ${flagged ? "flag" : "pass"}`,
    );
  }

  for (const fixture of fixtures) {
    const violations = analyseFragments(
      `self-test/${fixture.name}`,
      [{ text: fixture.text, line: 1 }],
      bannedPhrases,
    );
    const flagged = violations.length > 0;
    const ok = flagged === fixture.shouldFlag;
    if (!ok) {
      failures += 1;
    }

    console.log(
      `${ok ? "PASS" : "FAIL"} ${fixture.name}: expected ${fixture.shouldFlag ? "flag" : "pass"}, actual ${flagged ? "flag" : "pass"}`
        + (ok ? "" : ` (${violations.map((violation) => violation.phrase).join(", ") || "no violation"})`),
    );
  }

  for (const fixture of rustFixtures) {
    const fragments = extractRustStrings(fixture.source);
    const violations = analyseFragments(`self-test/${fixture.name}`, fragments, bannedPhrases);
    const flagged = violations.length > 0;
    const ok = flagged === fixture.shouldFlag && fragments.length === fixture.expectedSelected;
    if (!ok) {
      failures += 1;
    }

    console.log(
      `${ok ? "PASS" : "FAIL"} ${fixture.name}: expected ${fixture.shouldFlag ? "flag" : "pass"} with ${fixture.expectedSelected} selected, actual ${flagged ? "flag" : "pass"} with ${fragments.length} selected`,
    );
  }

  const productionReadme = await readUtf8(README_PATH);
  const readmeMarker = "## What it protects and what it does not";
  const readmeOccurrences = productionReadme.split(readmeMarker).length - 1;
  const mutatedReadme = productionReadme.replace(
    readmeMarker,
    `${readmeMarker}\n\nAll private state is encrypted at rest.`,
  );
  const readmeMutationViolations = analyseFragments(
    "README.md",
    [{ text: mutatedReadme, line: 1 }],
    bannedPhrases,
  );
  const readmeMutationCaught = readmeOccurrences === 1
    && mutatedReadme !== productionReadme
    && readmeMutationViolations.some(
      ({ phrase }) => phrase === "at-rest/local-protection overclaim",
    );
  if (!readmeMutationCaught) {
    failures += 1;
  }
  console.log(
    `${readmeMutationCaught ? "PASS" : "FAIL"} actual README broad at-rest mutation is nonvacuous and caught`,
  );

  const supportMarker = "Windows 10 or newer. macOS and Linux are not supported";
  const supportOccurrences = productionReadme.split(supportMarker).length - 1;
  const supportMutatedReadme = productionReadme.replace(
    supportMarker,
    "OSL supports WhatsApp. Windows 10 or newer. macOS and Linux are not supported",
  );
  const supportMutationViolations = analyseFragments(
    "README.md",
    [{ text: supportMutatedReadme, line: 1 }],
    bannedPhrases,
  );
  const supportMutationCaught = supportOccurrences === 1
    && supportMutatedReadme !== productionReadme
    && supportMutationViolations.some(
      ({ phrase }) => phrase === "Supports WhatsApp",
    );
  if (!supportMutationCaught) {
    failures += 1;
  }
  console.log(
    `${supportMutationCaught ? "PASS" : "FAIL"} actual README unsupported-service mutation is nonvacuous and caught`,
  );

  const productionMain = await readUtf8(path.join(APP_SRC_ROOT, "main.ts"));
  const scrubMarker = "<span class=\"privacy-local-mark\">FREE · THIS DEVICE ONLY</span><h2>Recommended action</h2><h3>Review an export</h3>";
  const scrubMarkerOccurrences = productionMain.split(scrubMarker).length - 1;
  const mutatedMain = productionMain.replace(
    scrubMarker,
    `${scrubMarker}<p>Scrub imports your complete account history.</p>`,
  );
  const scrubMutationViolations = analyseFragments(
    "apps/osl-hub-ui/src/main.ts",
    extractTypeScriptStrings(mutatedMain),
    bannedPhrases,
  );
  const scrubMutationCaught = scrubMarkerOccurrences === 1
    && mutatedMain !== productionMain
    && scrubMutationViolations.some(
      ({ phrase }) => phrase === "Scrub capability overclaim",
    );
  if (!scrubMutationCaught) {
    failures += 1;
  }
  console.log(
    `${scrubMutationCaught ? "PASS" : "FAIL"} actual Scrub UI completeness mutation is nonvacuous and caught`,
  );

  const productionSupportMatrixShape = validateSupportMatrixObject(productionSupportMatrix);
  const mutatedSourceMatrix = JSON.parse(JSON.stringify(productionSupportMatrix));
  const firstAnchor = collectMatrixSourceAnchors(mutatedSourceMatrix)[0];
  if (firstAnchor) {
    firstAnchor.contains = "p2 self-test intentionally missing support-matrix source anchor";
  }
  const supportMatrixSourceFailures = await validateSupportMatrixSources(
    collectMatrixSourceAnchors(mutatedSourceMatrix),
  );
  const supportMatrixMutationCaught = productionSupportMatrixShape.failures.length === 0
    && Boolean(firstAnchor)
    && supportMatrixSourceFailures.length > 0;
  if (!supportMatrixMutationCaught) {
    failures += 1;
  }
  console.log(
    `${supportMatrixMutationCaught ? "PASS" : "FAIL"} actual support matrix source-anchor mutation is nonvacuous and caught`,
  );

  console.log(
    `Self-test: phrases parsed=${bannedPhrases.length}, fixtures=${fixtures.length + rustFixtures.length + inputCases.length + supportClaimFixtures.length + supportMatrixFixtures.length + 4}, failures=${failures}`,
  );

  return failures === 0 ? 0 : 1;
}

async function main() {
  const [gateSource, allowlist] = await Promise.all([
    readUtf8(GATE_SOURCE_PATH),
    readUtf8(ALLOWLIST_PATH),
  ]);
  const expectedDigest = allowlist.match(GATE_CONTRACT_PATTERN)?.[1];
  const actualDigest = createHash("sha256").update(gateSource).digest("hex");
  if (!expectedDigest || actualDigest !== expectedDigest) {
    console.error(
      `Claim-gate source contract mismatch: expected ${expectedDigest ?? "missing"}, actual ${actualDigest}.`,
    );
    return 1;
  }

  const args = process.argv.slice(2);
  if (args.length > 1 || (args.length === 1 && args[0] !== "--self-test")) {
    console.error("Usage: node scripts/check-app-claims.mjs [--self-test]");
    return 1;
  }

  if (args[0] === "--self-test") {
    return runSelfTest();
  }

  return scanRepository();
}

process.exitCode = await main();
