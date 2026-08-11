#!/usr/bin/env node
import { createHash, verify as verifySignature } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export const MAX_PROBE_GAP_SECONDS = 60 * 60;
export const TIER_REQUIREMENTS = Object.freeze({
  Free: Object.freeze({ survival: 6 * 24 * 60 * 60, unavailableFloor: 7 * 24 * 60 * 60 }),
  Pro: Object.freeze({ survival: 29 * 24 * 60 * 60, unavailableFloor: 30 * 24 * 60 * 60 }),
});
export const REQUIRED_MUTATIONS = Object.freeze([
  "Free:premature-clock-advance",
  "Pro:premature-clock-advance",
  "Free:early-delete",
  "Pro:early-delete",
  "Free:delete-reupload-identical",
  "Pro:delete-reupload-identical",
]);

function fail(message) {
  throw new Error(`TASK 6211 FAIL: ${message}`);
}

function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

export function signableEvidence(evidence) {
  const { observer_signature: _signature, ...unsigned } = evidence;
  return Buffer.from(canonical(unsigned));
}

function required(condition, message) {
  if (!condition) fail(message);
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function verifyObserver(evidence, observerPublicKeyPem) {
  required(observerPublicKeyPem?.includes("PUBLIC KEY"), "independent observer public key absent");
  required(typeof evidence.observer_signature === "string", "independent observer signature absent");
  const signature = Buffer.from(evidence.observer_signature, "base64");
  required(
    verifySignature(null, signableEvidence(evidence), observerPublicKeyPem, signature),
    "independent observer signature invalid",
  );
}

function verifyMutation(evidence) {
  const tier = evidence.tier;
  const object = evidence.object_id;
  const generation = evidence.provider_generation;
  required(tier === "Free" || tier === "Pro", "mutation tier absent");
  required(/^[0-9a-f]{32}$/.test(object ?? ""), `tier=${tier} mutation object absent`);
  required(typeof generation === "string" && generation.length > 0, `tier=${tier} object=${object} provider generation absent`);
  required(evidence.throwaway_deployed === true, `tier=${tier} object=${object} mutation was not deployed`);
  required(evidence.discarded === true, `tier=${tier} object=${object} throwaway state not discarded`);
  required(evidence.control?.status === 200 && evidence.control?.hash_match === true,
    `tier=${tier} object=${object} unmodified same-tier control is not readable`);

  if (evidence.kind === "premature-clock-advance") {
    const reported = evidence.reported_elapsed_seconds;
    const measured = evidence.independent_elapsed_seconds;
    required(Number.isFinite(reported) && Number.isFinite(measured), `tier=${tier} object=${object} elapsed evidence absent`);
    if (reported >= TIER_REQUIREMENTS[tier].survival && measured < TIER_REQUIREMENTS[tier].survival) {
      fail(`tier=${tier} object=${object} reported versus independently measured elapsed time reported=${reported} measured=${measured} control_status=200`);
    }
    fail(`tier=${tier} object=${object} premature clock-advance attack absent`);
  }

  const event = evidence.audit_event;
  const interval = evidence.missing_interval_seconds;
  required(typeof event === "string" && event.length > 0, `tier=${tier} object=${object} audit event absent`);
  required(Number.isFinite(interval) && interval > 0, `tier=${tier} object=${object} missing interval absent`);
  if (evidence.kind === "early-delete") {
    fail(`tier=${tier} object=${object} audit_event=${event} generation=${generation} missing_interval_seconds=${interval} control_status=200`);
  }
  if (evidence.kind === "delete-reupload-identical") {
    required(evidence.replacement_generation !== generation,
      `tier=${tier} object=${object} delete/re-upload generation did not change`);
    fail(`tier=${tier} object=${object} audit_event=${event} generation=${generation} replacement_generation=${evidence.replacement_generation} missing_interval_seconds=${interval} control_status=200`);
  }
  fail(`tier=${tier} object=${object} unknown boundary mutation ${evidence.kind ?? "absent"}`);
}

function verifyTier(tierEvidence, tier) {
  required(tierEvidence, `${tier} tier starved`);
  const requirement = TIER_REQUIREMENTS[tier];
  const object = tierEvidence.object_id;
  required(tierEvidence.tier === tier, `${tier} tier label mismatch`);
  required(/^[0-9a-f]{32}$/.test(object ?? ""), `${tier} object id absent`);
  required(/^[0-9a-f]{64}$/.test(tierEvidence.bytes_sha256 ?? ""), `${tier} object=${object} frozen byte hash absent`);
  required(typeof tierEvidence.provider_generation === "string" && tierEvidence.provider_generation.length > 0,
    `${tier} object=${object} immutable provider generation absent`);
  required(typeof tierEvidence.upload_receipt === "string" && tierEvidence.upload_receipt.length > 0,
    `${tier} object=${object} signed upload receipt absent`);
  required(typeof tierEvidence.upload_receipt_signature === "string" && tierEvidence.upload_receipt_signature.length > 0,
    `${tier} object=${object} signed upload receipt signature absent`);
  required(Number.isSafeInteger(tierEvidence.start?.wall_unix_ms), `${tier} object=${object} external wall start absent`);
  required(/^\d+$/.test(tierEvidence.start?.monotonic_ns ?? ""), `${tier} object=${object} monotonic start absent`);

  const offsets = tierEvidence.schedule_offsets_seconds;
  const probes = tierEvidence.probes;
  required(Array.isArray(offsets) && offsets.length > 1, `${tier} object=${object} precommitted schedule absent`);
  required(Array.isArray(probes) && probes.length === offsets.length, `${tier} object=${object} scheduled read starved`);
  const startNs = BigInt(tierEvidence.start.monotonic_ns);
  let previousElapsed = 0;
  for (let index = 0; index < probes.length; index += 1) {
    const probe = probes[index];
    const expectedOffset = offsets[index];
    required(probe.scheduled_offset_seconds === expectedOffset,
      `${tier} object=${object} scheduled read=${index} does not match precommit`);
    const elapsed = Number((BigInt(probe.monotonic_ns) - startNs) / 1_000_000_000n);
    required(elapsed >= expectedOffset, `${tier} object=${object} read=${index} ran before its scheduled monotonic instant`);
    const gap = index === 0 ? elapsed : elapsed - previousElapsed;
    required(gap >= 0 && gap <= MAX_PROBE_GAP_SECONDS,
      `${tier} object=${object} observation gap=${gap} seconds exceeds 3600`);
    required(probe.status === 200, `${tier} object=${object} scheduled read=${index} unavailable at elapsed=${elapsed}`);
    required(probe.shipping_read_route === true, `${tier} object=${object} scheduled read=${index} bypassed shipping read route`);
    required(probe.sha256 === tierEvidence.bytes_sha256, `${tier} object=${object} scheduled read=${index} byte hash mismatch`);
    required(probe.provider_generation === tierEvidence.provider_generation,
      `${tier} object=${object} scheduled read=${index} immutable generation changed from ${tierEvidence.provider_generation} to ${probe.provider_generation}`);
    required(probe.provider_audit?.history_complete === true,
      `${tier} object=${object} scheduled read=${index} provider audit-history gap`);
    required(probe.provider_audit?.generation === tierEvidence.provider_generation,
      `${tier} object=${object} scheduled read=${index} provider audit generation mismatch`);
    required(probe.provider_audit?.delete === false && probe.provider_audit?.replace === false
      && probe.provider_audit?.restore === false && probe.provider_audit?.availability_gap === false,
    `${tier} object=${object} scheduled read=${index} provider audit event or availability gap`);
    previousElapsed = elapsed;
  }
  required(previousElapsed >= requirement.survival,
    `${tier} object=${object} actual-duration endpoint measured=${previousElapsed} required=${requirement.survival}`);
  if (tierEvidence.first_unavailable_elapsed_seconds !== null) {
    required(tierEvidence.first_unavailable_elapsed_seconds >= requirement.unavailableFloor,
      `${tier} object=${object} first unavailable elapsed=${tierEvidence.first_unavailable_elapsed_seconds} before floor=${requirement.unavailableFloor}`);
  }
  return { tier, object, probes: probes.length, elapsed: previousElapsed, generation: tierEvidence.provider_generation };
}

export function verifyEvidence(evidence, observerPublicKeyPem) {
  required(evidence?.format === "osl.task6211.elapsed-retention.v1", "format absent or wrong");
  verifyObserver(evidence, observerPublicKeyPem);
  if (evidence.campaign_kind !== "positive") return verifyMutation(evidence);

  required(evidence.origin === "deployed-production", "deployed production origin absent");
  required(evidence.relay?.shipping === true && /^https:\/\//.test(evidence.relay?.url ?? ""),
    "shipping deployed relay route absent");
  required(typeof evidence.relay?.deployment_version === "string" && evidence.relay.deployment_version.length > 0,
    "deployed relay version absent");
  required(evidence.shipping_client?.route === "CipherStoreClient::upload_attachment_file/fetch_attachment_to_writer",
    "shipping client route absent");
  required(/^[0-9a-f]{64}$/.test(evidence.shipping_client?.binary_sha256 ?? ""), "shipping client binary hash absent");
  required(evidence.storage?.provider === "Cloudflare R2" && evidence.storage?.production === true,
    "real production storage absent");
  required(evidence.schedule?.origin === "external-observer-precommit", "implementation-authored schedule prohibited");
  required(evidence.schedule?.committed_before_upload === true, "schedule was not committed before upload");
  required(evidence.schedule?.max_gap_seconds <= MAX_PROBE_GAP_SECONDS, "schedule permits gap longer than 3600 seconds");
  required(evidence.tiers?.Free, "Free tier starved");
  required(evidence.tiers?.Pro, "Pro tier starved");
  required(evidence.schedule?.commitment_sha256 === sha256(canonical({
    Free: evidence.tiers?.Free?.schedule_offsets_seconds,
    Pro: evidence.tiers?.Pro?.schedule_offsets_seconds,
  })), "precommitted schedule hash mismatch");
  required(evidence.time_sources?.wall?.kind === "external-signed-wall"
    && typeof evidence.time_sources.wall.proof_digest === "string", "independent wall-clock source absent");
  required(evidence.time_sources?.monotonic?.kind === "kernel-clock-boottime"
    && evidence.time_sources.monotonic.continuous === true
    && evidence.time_sources.monotonic.relay_settable === false
    && evidence.time_sources.monotonic.storage_settable === false
    && typeof evidence.time_sources.monotonic.boot_id === "string",
  "independent monotonic elapsed-time counter absent");

  const free = verifyTier(evidence.tiers?.Free, "Free");
  const pro = verifyTier(evidence.tiers?.Pro, "Pro");
  required(free.object !== pro.object, "Free and Pro objects are not distinct");
  required(evidence.tiers.Free.bytes_sha256 !== evidence.tiers.Pro.bytes_sha256,
    "Free and Pro attachments are not distinct fresh random bytes");

  const receipts = evidence.boundary_mutation_receipts;
  required(Array.isArray(receipts), "boundary mutation receipt census absent");
  for (const requiredMutation of REQUIRED_MUTATIONS) {
    const receipt = receipts.find((candidate) => `${candidate.tier}:${candidate.kind}` === requiredMutation);
    required(receipt, `boundary mutation ${requiredMutation} starved`);
    required(receipt.exit_code === 1, `boundary mutation ${requiredMutation} did not exit 1`);
    required(receipt.control_status === 200, `boundary mutation ${requiredMutation} positive control unreadable`);
    required(receipt.discarded === true, `boundary mutation ${requiredMutation} throwaway state not discarded`);
    required(typeof receipt.failure === "string" && receipt.failure.includes(`tier=${receipt.tier}`),
      `boundary mutation ${requiredMutation} failure did not name tier`);
  }
  return { free, pro, mutations: REQUIRED_MUTATIONS.length };
}

function parseCli(argv) {
  const evidenceIndex = argv.indexOf("--evidence");
  const keyIndex = argv.indexOf("--observer-key");
  if (evidenceIndex === -1 || keyIndex === -1 || !argv[evidenceIndex + 1] || !argv[keyIndex + 1]) {
    fail("usage: --evidence <json> --observer-key <pem>");
  }
  return { evidence: resolve(argv[evidenceIndex + 1]), observerKey: resolve(argv[keyIndex + 1]) };
}

if (process.argv[1] && resolve(process.argv[1]) === new URL(import.meta.url).pathname) {
  try {
    const paths = parseCli(process.argv.slice(2));
    const evidence = JSON.parse(readFileSync(paths.evidence, "utf8"));
    const result = verifyEvidence(evidence, readFileSync(paths.observerKey, "utf8"));
    process.stdout.write(`TASK 6211 PASS: Free probes=${result.free.probes} elapsed=${result.free.elapsed}; Pro probes=${result.pro.probes} elapsed=${result.pro.elapsed}; mutations=${result.mutations}\n`);
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
