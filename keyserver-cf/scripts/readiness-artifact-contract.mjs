import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

export const READINESS_MANIFEST_FORMAT =
  "osl.keyserver.readiness-artifact.v1";

export const BRIDGE_ALIASES = Object.freeze({
  "./endpoints/control-inbox.js":
    "./src/readiness/bridge/control-inbox.ts",
  "./endpoints/healthz.js":
    "./src/readiness/bridge/healthz.ts",
  "./lib/control-inbox-sweep.js":
    "./src/readiness/bridge/control-inbox-sweep.ts",
});

export const BRIDGE_REQUIRED_TEXT = Object.freeze([
  "reserved derived identity namespace requires root proof verification",
  "paid checkout is unavailable until prepaid-code redemption is ready",
  "A-pre-0031-bridge",
  "control inbox unavailable during schema transition",
]);

export const MIGRATION_0031_SURFACES = Object.freeze([
  "worker_schema_capabilities",
  "control_inbox_sender_disposition",
  "control_inbox_sender_reconciliation_started",
  "delivery_status",
  "delivery_reason",
  "delivery_attempts",
  "sender_disabled_first_seen_at",
  "delivery_next_retry_at",
  "delivery_retain_until",
]);

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function requireObject(value, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
}

export function validateReadinessManifest(manifest) {
  requireObject(manifest, "manifest");
  if (manifest.format !== READINESS_MANIFEST_FORMAT) {
    throw new Error("unknown readiness manifest format");
  }
  if (manifest.artifact !== "A" && manifest.artifact !== "B") {
    throw new Error("readiness artifact must be A or B");
  }
  const source = requireObject(manifest.source, "manifest.source");
  const build = requireObject(manifest.build, "manifest.build");
  for (const [label, value] of [
    ["source.commit", source.commit],
    ["source.repository_tree", source.repository_tree],
    ["source.keyserver_tree", source.keyserver_tree],
  ]) {
    if (typeof value !== "string" || !/^[0-9a-f]{40}$/.test(value)) {
      throw new Error(`${label} is not a full git object id`);
    }
  }
  for (const [label, value] of [
    ["source.archive_sha256", source.archive_sha256],
    ["build.bundle_sha256", build.bundle_sha256],
  ]) {
    if (typeof value !== "string" || !/^[0-9a-f]{64}$/.test(value)) {
      throw new Error(`${label} is not a SHA-256`);
    }
  }
  const expectedBuild =
    manifest.artifact === "A"
      ? {
          role: "pre-0031-bridge",
          bundleFile: "artifact-a.bridge.mjs",
          aliases: BRIDGE_ALIASES,
        }
      : {
          role: "0031-aware-final",
          bundleFile: "artifact-b.final.mjs",
          aliases: {},
        };
  if (manifest.role !== expectedBuild.role) {
    throw new Error("manifest role does not match its artifact");
  }
  if (build.entrypoint !== "src/index.ts") {
    throw new Error("readiness entrypoint is not src/index.ts");
  }
  if (build.bundle_file !== expectedBuild.bundleFile) {
    throw new Error("bundle_file does not match its artifact");
  }
  if (
    JSON.stringify(build.aliases) !== JSON.stringify(expectedBuild.aliases)
  ) {
    throw new Error("build aliases do not match the artifact closure");
  }
  if (
    !Array.isArray(build.command) ||
    !build.command.includes("--dry-run") ||
    !build.command.includes("--minify") ||
    build.command.includes("--env")
  ) {
    throw new Error("build command is not the production-config dry run");
  }
  if (!Number.isSafeInteger(build.bundle_bytes) || build.bundle_bytes <= 0) {
    throw new Error("bundle_bytes must be nonzero");
  }
  const policy = requireObject(manifest.policy, "manifest.policy");
  if (
    manifest.artifact === "A" &&
    (policy.requires_0031 !== false ||
      policy.forbidden_after_reconciliation !== true)
  ) {
    throw new Error("artifact A policy is not fail closed");
  }
  if (
    manifest.artifact === "B" &&
    (policy.requires_0031 !== true ||
      policy.forbidden_after_reconciliation !== false)
  ) {
    throw new Error("artifact B policy is not fail closed");
  }
  return manifest;
}

export function verifyReadinessBundle(manifestValue, bundleBytes) {
  const manifest = validateReadinessManifest(manifestValue);
  if (bundleBytes.byteLength !== manifest.build.bundle_bytes) {
    throw new Error("bundle byte count does not match manifest");
  }
  if (sha256(bundleBytes) !== manifest.build.bundle_sha256) {
    throw new Error("bundle SHA-256 does not match manifest");
  }
  const text = new TextDecoder().decode(bundleBytes);
  if (manifest.artifact === "A") {
    for (const required of BRIDGE_REQUIRED_TEXT) {
      if (!text.includes(required)) {
        throw new Error(`bridge is missing required refusal: ${required}`);
      }
    }
    for (const forbidden of MIGRATION_0031_SURFACES) {
      if (text.includes(forbidden)) {
        throw new Error(`bridge contains migration 0031 surface: ${forbidden}`);
      }
    }
  } else {
    for (const required of MIGRATION_0031_SURFACES) {
      if (!text.includes(required)) {
        throw new Error(`final bundle is missing migration 0031 surface: ${required}`);
      }
    }
  }
  return manifest;
}

export async function readAndVerifyReadinessArtifact(
  manifestPath,
  bundlePath,
) {
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const bundle = await readFile(bundlePath);
  return verifyReadinessBundle(manifest, bundle);
}

/**
 * Evidence is the output of two read-only D1 marker queries normalized to
 * `null` when the capability table/row is absent. No boolean coercion is
 * accepted because a stale or malformed operator handoff must stop.
 */
export function validateReadinessSelection(manifestValue, evidenceValue) {
  const manifest = validateReadinessManifest(manifestValue);
  const evidence = requireObject(evidenceValue, "database evidence");
  const disposition = evidence.control_inbox_sender_disposition;
  const reconciliation = evidence.control_inbox_sender_reconciliation_started;
  if (![null, 1].includes(disposition)) {
    throw new Error("disposition marker must be exactly 1 or null");
  }
  if (![null, 1].includes(reconciliation)) {
    throw new Error("reconciliation marker must be exactly 1 or null");
  }
  if (reconciliation === 1 && disposition !== 1) {
    throw new Error("reconciliation evidence is internally inconsistent");
  }
  if (manifest.artifact === "A" && reconciliation === 1) {
    throw new Error(
      "artifact A is forbidden after control-inbox reconciliation starts",
    );
  }
  if (manifest.artifact === "B" && disposition !== 1) {
    throw new Error("artifact B requires the exact migration 0031 marker");
  }
  return true;
}
