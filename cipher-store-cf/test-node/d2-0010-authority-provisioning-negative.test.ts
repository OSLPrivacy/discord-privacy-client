import { describe, expect, it } from "vitest";
import {
  D2_AUTHORITY_REGISTRY_CANDIDATE_FORMAT,
  D2_AUTHORITY_ROLES,
  D2_AUTHORITY_SIGNING_DOMAIN,
  D2_AUTHORITY_UNSIGNED_PLAN_FORMAT,
  buildUnsignedD2AuthorityPlan,
  verifyAndImportReviewedD2AuthorityReceipt,
} from "../scripts/d2-0010-authority-provisioning.js";
import {
  D2_AUTHORITY_FIXTURE_ACCOUNT_SHA256,
  D2_AUTHORITY_FIXTURE_NOW_MS,
  D2_AUTHORITY_FIXTURE_WORKER_VERSION_ID,
  cloneD2AuthorityFixtureRequest,
  makeValidD2AuthorityProvisioningFixture,
  signD2AuthorityPlanForFixture,
  type D2AuthorityProvisioningFixture,
  type D2AuthorityReviewedReceiptFixture,
} from "./helpers/d2-0010-authority-provisioning-fixtures.js";

type FixtureRequest = D2AuthorityProvisioningFixture["request"];
type FixtureReceipt = D2AuthorityReviewedReceiptFixture;
type RequestMutation = (request: FixtureRequest) => void;
type ReceiptMutation = (receipt: FixtureReceipt) => void;

async function expectBuildRefusal(
  fixture: D2AuthorityProvisioningFixture,
  mutate: RequestMutation,
): Promise<void> {
  const request = cloneD2AuthorityFixtureRequest(fixture);
  mutate(request);
  await expect(
    Promise.resolve().then(() =>
      buildUnsignedD2AuthorityPlan(request, D2_AUTHORITY_FIXTURE_NOW_MS)
    ),
  ).rejects.toThrow();
}

async function signedPlanAndReceipt(
  fixture: D2AuthorityProvisioningFixture,
) {
  const plan = await buildUnsignedD2AuthorityPlan(
    cloneD2AuthorityFixtureRequest(fixture),
    D2_AUTHORITY_FIXTURE_NOW_MS,
  );
  const receipt = await signD2AuthorityPlanForFixture(
    plan,
    fixture.privateKeysByProducerId,
  );
  return { plan, receipt };
}

async function expectImportRefusal(
  fixture: D2AuthorityProvisioningFixture,
  mutate: ReceiptMutation,
  nowMs = D2_AUTHORITY_FIXTURE_NOW_MS + 2_000,
): Promise<void> {
  const { plan, receipt } = await signedPlanAndReceipt(fixture);
  mutate(receipt);
  await expect(
    Promise.resolve().then(() =>
      verifyAndImportReviewedD2AuthorityReceipt(plan, receipt, nowMs)
    ),
  ).rejects.toThrow();
}

describe("offline D2 authority provisioning refusal boundary", () => {
  it("builds a deterministic, unsigned, non-authorizing review plan", async () => {
    const fixture = await makeValidD2AuthorityProvisioningFixture();
    const first = await buildUnsignedD2AuthorityPlan(
      cloneD2AuthorityFixtureRequest(fixture),
      D2_AUTHORITY_FIXTURE_NOW_MS,
    );
    const second = await buildUnsignedD2AuthorityPlan(
      cloneD2AuthorityFixtureRequest(fixture),
      D2_AUTHORITY_FIXTURE_NOW_MS,
    );

    expect(first).toEqual(second);
    expect(first).toMatchObject({
      format: D2_AUTHORITY_UNSIGNED_PLAN_FORMAT,
      production_authorized: false,
      shipping_registry_mutated: false,
      signatures: [],
    });
    expect(first.plan_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(first.signing_requests).toHaveLength(4);
    expect(first.signing_requests.map((request) => request.role).sort())
      .toEqual([...D2_AUTHORITY_ROLES].sort());
    for (const request of first.signing_requests) {
      expect(request.algorithm).toBe("Ed25519");
      expect(request.signature_domain).toBe(D2_AUTHORITY_SIGNING_DOMAIN);
      expect(request.plan_sha256).toBe(first.plan_sha256);
      expect(request.required_review_fields).toEqual([
        "review_id",
        "review_artifact_sha256",
        "reviewed_at_ms",
        "expires_at_ms",
        "decision",
      ]);
    }
  });

  it("imports four independently signed roles only as a non-shipping candidate", async () => {
    const fixture = await makeValidD2AuthorityProvisioningFixture();
    const { plan, receipt } = await signedPlanAndReceipt(fixture);
    const candidate = await verifyAndImportReviewedD2AuthorityReceipt(
      plan,
      receipt,
      D2_AUTHORITY_FIXTURE_NOW_MS + 2_000,
    );

    expect(candidate).toMatchObject({
      format: D2_AUTHORITY_REGISTRY_CANDIDATE_FORMAT,
      plan_sha256: plan.plan_sha256,
      review_id: receipt.review_id,
      review_artifact_sha256: receipt.review_artifact_sha256,
      production_authorized: false,
      shipping_registry_mutated: false,
      requires_source_import: true,
    });
    expect(Object.keys(candidate.producers)).toHaveLength(4);
    expect(Object.values(candidate.producers).map((producer) => producer.role).sort())
      .toEqual([...D2_AUTHORITY_ROLES].sort());
  });

  it.each<readonly [string, RequestMutation]>([
    ["an empty producer list", (request) => {
      request.producers = [];
    }],
    ["a missing producer role", (request) => {
      request.producers.pop();
    }],
    ["a duplicate producer id", (request) => {
      request.producers[1]!.producer_id = request.producers[0]!.producer_id;
    }],
    ["a duplicate public key", (request) => {
      request.producers[1]!.public_key_raw_base64url =
        request.producers[0]!.public_key_raw_base64url;
    }],
    ["a duplicate role", (request) => {
      request.producers[1]!.role = request.producers[0]!.role;
    }],
    ["an Ed25519 public key with the wrong byte length", (request) => {
      request.producers[0]!.public_key_raw_base64url = "AA";
    }],
    ["a zero key epoch", (request) => {
      request.producers[0]!.key_epoch = 0;
    }],
    ["a key not yet valid for the plan", (request) => {
      request.producers[0]!.valid_from_ms = request.created_at_ms + 1;
    }],
    ["a key that expires before the plan", (request) => {
      request.producers[0]!.valid_through_ms = request.expires_at_ms - 1;
    }],
    ["a key revoked before the plan was created", (request) => {
      request.producers[0]!.revoked_at_ms = request.created_at_ms - 1;
    }],
  ])("refuses %s", async (_label, mutate) => {
    await expectBuildRefusal(
      await makeValidD2AuthorityProvisioningFixture(),
      mutate,
    );
  });

  it.each<readonly [string, RequestMutation]>([
    ["partial traffic", (request) => {
      request.active_deployment.traffic_percentage = 99;
    }],
    ["a deployment from another account", (request) => {
      request.active_deployment.account_sha256 = "9".repeat(64);
    }],
    ["a deployment source outside the reviewed release snapshot", (request) => {
      request.active_deployment.source.manifest_sha256 = "9".repeat(64);
    }],
    ["a readback from another Worker version", (request) => {
      request.readback_roots.worker_version_id =
        "22222222-2222-4222-8222-222222222222";
    }],
  ])("refuses invalid active deployment binding: %s", async (_label, mutate) => {
    await expectBuildRefusal(
      await makeValidD2AuthorityProvisioningFixture(),
      mutate,
    );
  });

  it.each<readonly [string, RequestMutation]>([
    ["an event for another Worker version", (request) => {
      request.provider_event_identity.worker_version_id =
        "22222222-2222-4222-8222-222222222222";
    }],
    ["an event observed before the active deployment", (request) => {
      request.provider_event_identity.observed_at_ms =
        request.active_deployment.activated_at_ms - 1;
    }],
    ["an invalid provider event digest", (request) => {
      request.provider_event_identity.event_sha256 = "";
    }],
  ])("refuses invalid provider-event identity: %s", async (_label, mutate) => {
    await expectBuildRefusal(
      await makeValidD2AuthorityProvisioningFixture(),
      mutate,
    );
  });

  it.each<readonly [string, RequestMutation]>([
    ["D1", (request) => {
      request.readback_roots.d1_sha256 = "";
    }],
    ["R2", (request) => {
      request.readback_roots.r2_sha256 = "not-a-digest";
    }],
    ["quota", (request) => {
      request.readback_roots.quota_sha256 = "0".repeat(63);
    }],
  ])("refuses an invalid %s readback root", async (_label, mutate) => {
    await expectBuildRefusal(
      await makeValidD2AuthorityProvisioningFixture(),
      mutate,
    );
  });

  it.each<readonly [string, RequestMutation]>([
    ["admitted commit", (request) => {
      request.admitted_product_client.commit_sha = "a".repeat(39);
    }],
    ["admitted tree", (request) => {
      request.admitted_product_client.tree_sha = "g".repeat(40);
    }],
    ["admitted contract", (request) => {
      request.admitted_product_client.contract_sha256 = "a".repeat(63);
    }],
    ["admission-contract commit", (request) => {
      request.admission_contract.commit_sha = "";
    }],
    ["admission-contract tree", (request) => {
      request.admission_contract.tree_sha = "A".repeat(40);
    }],
    ["admission-contract digest", (request) => {
      request.admission_contract.contract_sha256 = "f".repeat(65);
    }],
  ])("refuses a malformed %s identity", async (_label, mutate) => {
    await expectBuildRefusal(
      await makeValidD2AuthorityProvisioningFixture(),
      mutate,
    );
  });

  it("refuses an already expired request and a stale unsigned plan", async () => {
    const fixture = await makeValidD2AuthorityProvisioningFixture();
    const request = cloneD2AuthorityFixtureRequest(fixture);
    request.expires_at_ms = D2_AUTHORITY_FIXTURE_NOW_MS - 1;
    await expect(
      Promise.resolve().then(() =>
        buildUnsignedD2AuthorityPlan(request, D2_AUTHORITY_FIXTURE_NOW_MS)
      ),
    ).rejects.toThrow();

    const { plan, receipt } = await signedPlanAndReceipt(fixture);
    await expect(
      Promise.resolve().then(() =>
        verifyAndImportReviewedD2AuthorityReceipt(
          plan,
          receipt,
          plan.expires_at_ms + 1,
        )
      ),
    ).rejects.toThrow();
  });

  it.each<readonly [string, ReceiptMutation]>([
    ["an empty signature", (receipt) => {
      receipt.signatures[0]!.signature_base64url = "";
    }],
    ["a missing signature", (receipt) => {
      receipt.signatures.pop();
    }],
    ["a duplicate signature", (receipt) => {
      receipt.signatures[1] = structuredClone(receipt.signatures[0]!);
    }],
    ["the wrong role", (receipt) => {
      receipt.signatures[0]!.role = receipt.signatures[1]!.role;
    }],
    ["the wrong plan digest", (receipt) => {
      receipt.plan_sha256 = "9".repeat(64);
    }],
    ["a review artifact changed after signing", (receipt) => {
      receipt.review_artifact_sha256 = "9".repeat(64);
    }],
    ["a review identity changed after signing", (receipt) => {
      receipt.review_id = "9".repeat(64);
    }],
    ["a review time changed after signing", (receipt) => {
      receipt.reviewed_at_ms += 1;
    }],
    ["a cryptographically wrong signature", (receipt) => {
      receipt.signatures[0]!.signature_base64url =
        Buffer.alloc(64, 0).toString("base64url");
    }],
    ["a zero signature epoch", (receipt) => {
      receipt.signatures[0]!.key_epoch = 0;
    }],
    ["a stale nonzero signature epoch", (receipt) => {
      receipt.signatures[1]!.key_epoch -= 1;
    }],
  ])("refuses reviewed evidence with %s", async (_label, mutate) => {
    await expectImportRefusal(
      await makeValidD2AuthorityProvisioningFixture(),
      mutate,
    );
  });

  it("refuses an expired external review receipt", async () => {
    const fixture = await makeValidD2AuthorityProvisioningFixture();
    const { plan, receipt } = await signedPlanAndReceipt(fixture);
    await expect(
      Promise.resolve().then(() =>
        verifyAndImportReviewedD2AuthorityReceipt(
          plan,
          receipt,
          receipt.expires_at_ms + 1,
        )
      ),
    ).rejects.toThrow();
  });

  it("does not accept evidence for a different account or Worker snapshot", async () => {
    const fixture = await makeValidD2AuthorityProvisioningFixture();
    expect(fixture.request.account_sha256)
      .toBe(D2_AUTHORITY_FIXTURE_ACCOUNT_SHA256);
    expect(fixture.request.active_deployment.worker_version_id)
      .toBe(D2_AUTHORITY_FIXTURE_WORKER_VERSION_ID);

    await expectBuildRefusal(fixture, (request) => {
      request.account_sha256 = "9".repeat(64);
    });
  });
});
