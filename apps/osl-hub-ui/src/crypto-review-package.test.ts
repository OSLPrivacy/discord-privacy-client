import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

type SpillAuditContract = {
  schemaVersion: number;
  audit: string;
  sourceRoots: string[];
  allowedSinks: string[];
  forbiddenSinks: string[];
  durableStateAuthorities: string[];
  failClosedAuthorities: string[];
  rnSessionState: {
    requiredStore: string;
    requiredSealerSelection: string;
    plaintextPersistenceAllowed: boolean;
  };
  nonImageAttachmentOpen: {
    refusalBefore: string[];
    durablePlaintextCopyAllowed: boolean;
  };
  debugRedaction: { forbiddenFields: string[] };
  negativeControls: string[];
  status: string;
};

type CryptoReviewBundleContract = {
  schemaVersion: number;
  bundle: string;
  requiredEvidence: string[];
  reviewedRootMinimum: string[];
  authorityAbsenceRule: "refuse" | "permit";
  runtimeClaimRule: string;
  invalidConditions: string[];
  status: string;
};

type SeededImapReceiptContract = {
  schemaVersion: number;
  test: string;
  requiredEvidence: string[];
  positiveRequirement: {
    seededMessageAbsentAfterReread: boolean;
    deletionAuthority: string;
  };
  negativeRequirement: {
    minimumRetainedNegativeControls: number;
    negativeControlsRemainPresent: boolean;
  };
  rejectedIf: string[];
  status: string;
};

type UsernameOnlyScaffoldContract = {
  schemaVersion: number;
  test: string;
  websiteRepo: {
    repo: string;
    pathEnv: string;
    role: string;
    located: boolean;
  };
  workerScaffold: {
    repo: string;
    route: string;
    entrypoint: string;
    router: string;
    responseVersion: number;
  };
  requestContract: {
    exactBodyKeys: string[];
    acceptedUsernameExamples: string[];
    refusedInputs: string[];
  };
  responseContract: {
    resultStatus: string;
    signalsEmpty: boolean;
    forbiddenClaims: string[];
  };
  status: string;
};

function readRepo(relativePath: string): string {
  return readFileSync(new URL(`../../../${relativePath}`, import.meta.url), "utf8");
}

function headingSection(source: string, heading: string, level = 3): string {
  const marker = `${"#".repeat(level)} ${heading}`;
  const lines = source.split("\n");
  const start = lines.findIndex((line) => line.trim() === marker);
  expect(start, `missing heading ${heading}`).toBeGreaterThanOrEqual(0);
  const nextHeading = new RegExp(`^#{1,${level}}\\s+`, "u");
  const end = lines.findIndex((line, index) => index > start && nextHeading.test(line));
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n");
}

function jsonBlockAfterLabel<T>(section: string, label: string): T {
  const lines = section.split("\n");
  const labelIndex = lines.findIndex((line) => line.trim() === label);
  expect(labelIndex, `missing label ${label}`).toBeGreaterThanOrEqual(0);
  const fenceStart = lines.findIndex(
    (line, index) => index > labelIndex && line.trim() === "```json",
  );
  expect(fenceStart, `missing json fence after ${label}`).toBeGreaterThan(labelIndex);
  const fenceEnd = lines.findIndex(
    (line, index) => index > fenceStart && line.trim() === "```",
  );
  expect(fenceEnd, `unterminated json fence after ${label}`).toBeGreaterThan(fenceStart);
  return JSON.parse(lines.slice(fenceStart + 1, fenceEnd).join("\n")) as T;
}

function extractSpillAuditContract(): SpillAuditContract {
  return jsonBlockAfterLabel<SpillAuditContract>(
    headingSection(
      readRepo("docs/reports/crypto-lane-2026-07-26.md"),
      "decrypted_plaintext_spill_audit_apps_hub_and_ipc",
    ),
    "Structured audit contract:",
  );
}

function extractBundleContract(): CryptoReviewBundleContract {
  return jsonBlockAfterLabel<CryptoReviewBundleContract>(
    headingSection(
      readRepo("docs/reports/crypto-lane-2026-07-26.md"),
      "crypto_review_package_bundle_definition",
    ),
    "Structured bundle contract:",
  );
}

function extractSeededImapContract(): SeededImapReceiptContract {
  return jsonBlockAfterLabel<SeededImapReceiptContract>(
    headingSection(readRepo("docs/plans/osl-acceleration-plan-2026-07-27.md"), "Scrub"),
    "Structured receipt-package contract:",
  );
}

function extractUsernameOnlyScaffoldContract(): UsernameOnlyScaffoldContract {
  return jsonBlockAfterLabel<UsernameOnlyScaffoldContract>(
    headingSection(readRepo("docs/design/osl-internal-build-checklist.md"), "F · Scrub and AutoScrub — 45 points (17 earned)", 2),
    "Current scaffold confirmation:",
  );
}

function classifyPlaintextEvent(
  contract: SpillAuditContract,
  event: {
    sink: string;
    durable: boolean;
    authority?: string;
    debugFields?: string[];
    nonImageAttachmentOpen?: boolean;
    completedBeforeRefusal?: string[];
    rnSessionPersistence?: boolean;
    rnStore?: string;
    rnSealer?: string;
  },
): "pass" | "fail" {
  if (contract.forbiddenSinks.includes(event.sink)) return "fail";
  if (!contract.allowedSinks.includes(event.sink)) return "fail";
  if (event.durable && !contract.durableStateAuthorities.includes(event.authority ?? "")) {
    return "fail";
  }
  if (
    event.debugFields?.some((field) =>
      contract.debugRedaction.forbiddenFields.includes(field),
    )
  ) {
    return "fail";
  }
  if (event.nonImageAttachmentOpen) {
    const forbiddenBeforeRefusal = new Set(contract.nonImageAttachmentOpen.refusalBefore);
    if (event.completedBeforeRefusal?.some((step) => forbiddenBeforeRefusal.has(step))) {
      return "fail";
    }
  }
  if (event.rnSessionPersistence) {
    return event.rnStore === contract.rnSessionState.requiredStore &&
      event.rnSealer === contract.rnSessionState.requiredSealerSelection &&
      contract.rnSessionState.plaintextPersistenceAllowed === false
      ? "pass"
      : "fail";
  }
  return "pass";
}

function validateReviewPackage(
  contract: CryptoReviewBundleContract,
  candidate: {
    evidence: string[];
    roots: string[];
    invalidConditions: string[];
    claimsRuntime: boolean;
    hasRuntimeEvidence: boolean;
    absenceOfAuthority: "refuse" | "permit";
  },
): boolean {
  if (candidate.absenceOfAuthority !== contract.authorityAbsenceRule) return false;
  if (candidate.invalidConditions.some((condition) => contract.invalidConditions.includes(condition))) {
    return false;
  }
  if (candidate.claimsRuntime && !candidate.hasRuntimeEvidence) return false;
  return (
    contract.requiredEvidence.every((item) => candidate.evidence.includes(item)) &&
    contract.reviewedRootMinimum.every((root) => candidate.roots.includes(root))
  );
}

function validateSeededImapReceipt(
  contract: SeededImapReceiptContract,
  receipt: {
    evidence: string[];
    seededMessageAbsentAfterReread: boolean;
    deletionAuthority: string;
    negativeControlsPresentAfterReread: number;
    rejectedConditions: string[];
  },
): boolean {
  if (receipt.rejectedConditions.some((condition) => contract.rejectedIf.includes(condition))) {
    return false;
  }
  return (
    contract.requiredEvidence.every((item) => receipt.evidence.includes(item)) &&
    receipt.seededMessageAbsentAfterReread ===
      contract.positiveRequirement.seededMessageAbsentAfterReread &&
    receipt.deletionAuthority === contract.positiveRequirement.deletionAuthority &&
    receipt.negativeControlsPresentAfterReread >=
      contract.negativeRequirement.minimumRetainedNegativeControls &&
    contract.negativeRequirement.negativeControlsRemainPresent
  );
}

describe("crypto review report contracts", () => {
  it("decrypted_plaintext_spill_audit_apps_hub_and_ipc", () => {
    const contract = extractSpillAuditContract();
    expect(contract.schemaVersion).toBe(1);
    expect(new Set(contract.sourceRoots)).toEqual(
      new Set(["apps/osl-hub/src", "crates/ipc/src"]),
    );
    expect(contract.status).toBe("audit-definition/source-reviewed-only");

    expect(
      classifyPlaintextEvent(contract, {
        sink: "renderer_dto_memory",
        durable: false,
      }),
    ).toBe("pass");
    expect(
      classifyPlaintextEvent(contract, {
        sink: "ordinary_filesystem_path",
        durable: true,
        authority: "file_storage_key",
      }),
    ).toBe("fail");
    expect(
      classifyPlaintextEvent(contract, {
        sink: "renderer_dto_memory",
        durable: true,
      }),
    ).toBe("fail");
    expect(
      classifyPlaintextEvent(contract, {
        sink: "renderer_dto_memory",
        durable: false,
        debugFields: ["decrypted_plaintext"],
      }),
    ).toBe("fail");
    expect(
      classifyPlaintextEvent(contract, {
        sink: "renderer_dto_memory",
        durable: false,
        nonImageAttachmentOpen: true,
        completedBeforeRefusal: ["download"],
      }),
    ).toBe("fail");
    expect(
      classifyPlaintextEvent(contract, {
        sink: "renderer_dto_memory",
        durable: false,
        rnSessionPersistence: true,
        rnStore: "rn_session_store",
        rnSealer: "selected_non_plaintext_sealer",
      }),
    ).toBe("pass");
  });

  it("crypto_review_package_bundle_definition", () => {
    const contract = extractBundleContract();
    const validCandidate = {
      evidence: [...contract.requiredEvidence],
      roots: [...contract.reviewedRootMinimum],
      invalidConditions: [] as string[],
      claimsRuntime: false,
      hasRuntimeEvidence: false,
      absenceOfAuthority: "refuse" as const,
    };

    expect(validateReviewPackage(contract, validCandidate)).toBe(true);
    expect(
      validateReviewPackage(contract, {
        ...validCandidate,
        evidence: validCandidate.evidence.filter(
          (item) => item !== "negative_control_statement",
        ),
      }),
    ).toBe(false);
    expect(
      validateReviewPackage(contract, {
        ...validCandidate,
        invalidConditions: ["source_text_only_test"],
      }),
    ).toBe(false);
    expect(
      validateReviewPackage(contract, {
        ...validCandidate,
        claimsRuntime: true,
        hasRuntimeEvidence: false,
      }),
    ).toBe(false);
    expect(
      validateReviewPackage(contract, {
        ...validCandidate,
        absenceOfAuthority: "permit",
      }),
    ).toBe(false);
  });

  it("seeded_windows_imap_receipt_package_positive_and_negative", () => {
    const contract = extractSeededImapContract();
    const validReceipt = {
      evidence: [...contract.requiredEvidence],
      seededMessageAbsentAfterReread: true,
      deletionAuthority: "attended_imap_authority",
      negativeControlsPresentAfterReread: 1,
      rejectedConditions: [] as string[],
    };

    expect(validateSeededImapReceipt(contract, validReceipt)).toBe(true);
    expect(
      validateSeededImapReceipt(contract, {
        ...validReceipt,
        negativeControlsPresentAfterReread: 0,
      }),
    ).toBe(false);
    expect(
      validateSeededImapReceipt(contract, {
        ...validReceipt,
        seededMessageAbsentAfterReread: false,
      }),
    ).toBe(false);
    expect(
      validateSeededImapReceipt(contract, {
        ...validReceipt,
        evidence: validReceipt.evidence.filter(
          (item) => item !== "post_delete_imap_reread",
        ),
      }),
    ).toBe(false);
    expect(
      validateSeededImapReceipt(contract, {
        ...validReceipt,
        rejectedConditions: ["claims_live_provider_or_account_wide_deletion"],
      }),
    ).toBe(false);
  });

  it("test/integration/username-only-worker-scaffold.test.ts", () => {
    const contract = extractUsernameOnlyScaffoldContract();
    expect(contract.schemaVersion).toBe(1);
    expect(contract.test).toBe("test/integration/username-only-worker-scaffold.test.ts");
    // The website checkout is outside this repo, so it is pinned by repo name
    // plus an env-var locator rather than an absolute developer path: this
    // file ships publicly and must not leak a maintainer username or machine
    // layout (enforced by scripts/audit_public_release.py).
    expect(contract.websiteRepo).toEqual({
      repo: "oslprivacy-web",
      pathEnv: "OSL_WEBSITE_REPO",
      role: "static_pages_checkout",
      located: true,
    });
    expect(contract.websiteRepo.repo).not.toMatch(/[\\/]/u);
    expect(contract.websiteRepo.pathEnv).toMatch(/^[A-Z][A-Z0-9_]*$/u);
    expect(contract.workerScaffold).toEqual({
      repo: "this_worktree",
      route: "POST /v1/username-coverage",
      entrypoint: "keyserver-cf/src/endpoints/username-coverage.ts",
      router: "keyserver-cf/src/index.ts",
      responseVersion: 1,
    });
    expect(contract.requestContract).toEqual({
      exactBodyKeys: ["username"],
      acceptedUsernameExamples: ["alice.example_1"],
      refusedInputs: [
        "missing_username",
        "extra_provider",
        "credential_like_input",
        "unsupported_provider_binding",
        "discord_snowflake",
      ],
    });
    expect(contract.responseContract).toEqual({
      resultStatus: "not_scanned",
      signalsEmpty: true,
      forbiddenClaims: [
        "deletion",
        "private_mailbox_access",
        "browser_profile_access",
        "calibrated_risk_percentage",
      ],
    });
    expect(contract.status).toBe("scaffold-confirmed-test-proven-only");
  });
});
