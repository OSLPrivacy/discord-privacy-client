#!/usr/bin/env node
import { createHash } from "node:crypto";

const EMPTY_SHA256 =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const SHA256_RE = /^[0-9a-f]{64}$/;
const GIT_OBJECT_RE = /^[0-9a-f]{40}$/;
const DEPLOYMENT_SCHEMA_VERSION = 1;

const EXACT_BUILD_COMMAND = Object.freeze(["npm", "run", "build"]);
const REQUIRED_DIST_FILES = Object.freeze(["_headers", "build.json", "index.html"]);
const SENSITIVE_FIELD_RE =
  /(?:account(?:id|identifier)?|authorization|cookie|credential|email|handle|password|secret|token)/i;

export class DeploymentContractError extends Error {
  constructor(message) {
    super(message);
    this.name = "DeploymentContractError";
  }
}

function deepFreeze(value) {
  if (value && typeof value === "object" && !Object.isFrozen(value)) {
    for (const nested of Object.values(value)) {
      deepFreeze(nested);
    }
    Object.freeze(value);
  }
  return value;
}

export const deploymentContract = deepFreeze({
  schemaVersion: DEPLOYMENT_SCHEMA_VERSION,
  name: "osl-web-deployment-identity",
  purpose: "exact-live-verification",
  source: {
    cleanCheckoutRequired: true,
    dirtyFingerprintForCleanTree: EMPTY_SHA256,
  },
  build: {
    command: EXACT_BUILD_COMMAND,
    environment: "production",
  },
  artifact: {
    distPath: "dist",
    requiredFiles: REQUIRED_DIST_FILES,
    fileRoles: Object.freeze(["asset", "html", "metadata"]),
  },
  liveVerification: {
    retryPolicy: "caller-owned-no-automatic-retry",
    requiredBuildJsonPath: "/build.json",
    requiredHtmlEntryPath: "/index.html",
    requireEveryManifestHtmlPathStamped: true,
    requireExactManifestResourcesOnly: true,
  },
  forbiddenFieldPattern:
    "(account(id|identifier)?|authorization|cookie|credential|email|handle|password|secret|token)",
});

function fail(message) {
  throw new DeploymentContractError(message);
}

function exactObject(value, keys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  const missing = expected.filter((key) => !actual.includes(key));
  const unknown = actual.filter((key) => !expected.includes(key));
  if (missing.length > 0 || unknown.length > 0) {
    fail(
      `${label} fields are not exact; missing=${missing.join(",")} unknown=${unknown.join(",")}`,
    );
  }
  return value;
}

function rejectSensitiveFieldNames(value, label = "value") {
  if (!value || typeof value !== "object") return;
  if (Array.isArray(value)) {
    value.forEach((item, index) => rejectSensitiveFieldNames(item, `${label}[${index}]`));
    return;
  }
  for (const [key, nested] of Object.entries(value)) {
    if (SENSITIVE_FIELD_RE.test(key)) {
      fail(`${label}.${key} is forbidden by the deployment identity contract`);
    }
    rejectSensitiveFieldNames(nested, `${label}.${key}`);
  }
}

function requireString(value, label) {
  if (typeof value !== "string" || value.length === 0) {
    fail(`${label} must be a nonempty string`);
  }
  return value;
}

function requireSha256(value, label) {
  if (typeof value !== "string" || !SHA256_RE.test(value)) {
    fail(`${label} must be lowercase SHA-256`);
  }
  return value;
}

function requireGitObject(value, label) {
  if (typeof value !== "string" || !GIT_OBJECT_RE.test(value)) {
    fail(`${label} must be a full lowercase Git object id`);
  }
  return value;
}

function requireBoolean(value, label) {
  if (typeof value !== "boolean") {
    fail(`${label} must be a boolean`);
  }
  return value;
}

function requireSafeRelativePath(value, label) {
  const text = requireString(value, label);
  if (
    text.startsWith("/") ||
    text.startsWith("\\") ||
    text.includes("\\") ||
    text.split("/").some((part) => part === "" || part === "." || part === "..")
  ) {
    fail(`${label} must be a normalized relative POSIX path`);
  }
  return text;
}

function requireLivePath(value, label) {
  const text = requireString(value, label);
  if (
    !text.startsWith("/") ||
    text.includes("\\") ||
    text.includes("//") ||
    text.split("/").some((part, index) => index > 0 && (part === "." || part === ".."))
  ) {
    fail(`${label} must be an absolute normalized deployment path`);
  }
  return text;
}

function requireOrigin(value, label) {
  const text = requireString(value, label);
  let parsed;
  try {
    parsed = new URL(text);
  } catch {
    fail(`${label} must be an HTTPS origin`);
  }
  if (
    parsed.protocol !== "https:" ||
    parsed.pathname !== "/" ||
    parsed.search !== "" ||
    parsed.hash !== "" ||
    parsed.username !== "" ||
    parsed.password !== ""
  ) {
    fail(`${label} must be an HTTPS origin without credentials, path, query, or fragment`);
  }
  return parsed.origin;
}

function requireInteger(value, label, { minimum = 0 } = {}) {
  if (!Number.isInteger(value) || value < minimum) {
    fail(`${label} must be an integer >= ${minimum}`);
  }
  return value;
}

function validateSource(source, label = "deploymentIdentity.source") {
  exactObject(source, ["commit", "tree", "clean", "dirtyFingerprint"], label);
  requireGitObject(source.commit, `${label}.commit`);
  requireGitObject(source.tree, `${label}.tree`);
  if (requireBoolean(source.clean, `${label}.clean`) !== true) {
    fail(`${label}.clean must be true`);
  }
  if (source.dirtyFingerprint !== EMPTY_SHA256) {
    fail(`${label}.dirtyFingerprint must be the empty status digest`);
  }
  return source;
}

function validateBuild(build, label = "deploymentIdentity.build") {
  exactObject(build, ["command", "environment", "nodeVersion", "packageManager"], label);
  if (
    !Array.isArray(build.command) ||
    build.command.length !== EXACT_BUILD_COMMAND.length ||
    build.command.some((part, index) => part !== EXACT_BUILD_COMMAND[index])
  ) {
    fail(`${label}.command is not the exact production build command`);
  }
  if (build.environment !== "production") {
    fail(`${label}.environment must be production`);
  }
  requireString(build.nodeVersion, `${label}.nodeVersion`);
  requireString(build.packageManager, `${label}.packageManager`);
  return build;
}

function validateFileEntry(entry, label) {
  exactObject(entry, ["path", "role", "sha256", "sizeBytes"], label);
  requireSafeRelativePath(entry.path, `${label}.path`);
  if (!deploymentContract.artifact.fileRoles.includes(entry.role)) {
    fail(`${label}.role is not allowed`);
  }
  requireSha256(entry.sha256, `${label}.sha256`);
  requireInteger(entry.sizeBytes, `${label}.sizeBytes`);
  return entry;
}

function validateArtifact(artifact, label = "deploymentIdentity.artifact") {
  exactObject(artifact, ["distPath", "files", "manifestSha256"], label);
  if (artifact.distPath !== deploymentContract.artifact.distPath) {
    fail(`${label}.distPath must be ${deploymentContract.artifact.distPath}`);
  }
  requireSha256(artifact.manifestSha256, `${label}.manifestSha256`);
  if (!Array.isArray(artifact.files) || artifact.files.length === 0) {
    fail(`${label}.files must be a nonempty array`);
  }
  const seen = new Set();
  const files = artifact.files.map((entry, index) =>
    validateFileEntry(entry, `${label}.files[${index}]`),
  );
  for (const entry of files) {
    if (seen.has(entry.path)) fail(`${label}.files contains a duplicate path`);
    seen.add(entry.path);
  }
  const sorted = [...files].sort((left, right) => left.path.localeCompare(right.path));
  if (files.some((entry, index) => entry.path !== sorted[index].path)) {
    fail(`${label}.files must be path-sorted`);
  }
  for (const path of deploymentContract.artifact.requiredFiles) {
    if (!seen.has(path)) fail(`${label}.files is missing ${path}`);
  }
  if (files.find((entry) => entry.path === "build.json")?.role !== "metadata") {
    fail(`${label}.files build.json must be metadata`);
  }
  if (files.find((entry) => entry.path === "index.html")?.role !== "html") {
    fail(`${label}.files index.html must be html`);
  }
  if (files.find((entry) => entry.path === "_headers")?.role !== "metadata") {
    fail(`${label}.files _headers must be metadata`);
  }
  const expectedManifestSha256 = manifestDigest(files);
  if (artifact.manifestSha256 !== expectedManifestSha256) {
    fail(`${label}.manifestSha256 does not match the canonical file manifest`);
  }
  return artifact;
}

function filePathToLivePath(pathValue) {
  return `/${pathValue}`;
}

function expectedLiveResources(files) {
  return files.map((entry) =>
    Object.freeze({
      path: filePathToLivePath(entry.path),
      sha256: entry.sha256,
    }),
  );
}

function validateLive(live, artifact, label = "deploymentIdentity.live") {
  exactObject(live, ["origin", "requiredResources"], label);
  requireOrigin(live.origin, `${label}.origin`);
  if (!Array.isArray(live.requiredResources)) {
    fail(`${label}.requiredResources must be an array`);
  }
  const expected = expectedLiveResources(artifact.files);
  if (live.requiredResources.length !== expected.length) {
    fail(`${label}.requiredResources must cover every manifest file exactly once`);
  }
  for (const [index, resource] of live.requiredResources.entries()) {
    exactObject(resource, ["path", "sha256"], `${label}.requiredResources[${index}]`);
    requireLivePath(resource.path, `${label}.requiredResources[${index}].path`);
    requireSha256(resource.sha256, `${label}.requiredResources[${index}].sha256`);
    if (
      resource.path !== expected[index].path ||
      resource.sha256 !== expected[index].sha256
    ) {
      fail(`${label}.requiredResources must match the path-sorted manifest`);
    }
  }
  return live;
}

export function canonicalJson(value) {
  return `${JSON.stringify(canonicalize(value))}\n`;
}

function canonicalize(value) {
  if (value === null || typeof value === "string" || typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) fail("canonical JSON cannot encode non-finite numbers");
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((item) => canonicalize(item));
  }
  if (value && typeof value === "object") {
    const output = {};
    for (const key of Object.keys(value).sort()) {
      const nested = value[key];
      if (nested === undefined || typeof nested === "function") {
        fail("canonical JSON cannot encode unsupported values");
      }
      output[key] = canonicalize(nested);
    }
    return output;
  }
  fail("canonical JSON cannot encode unsupported values");
}

export function sha256Hex(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function manifestDigest(files) {
  return sha256Hex(canonicalJson(files));
}

export function createDeploymentIdentity(input = {}) {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    fail("deploymentIdentityInput must be an object");
  }
  const inputKeys = Object.keys(input);
  const allowedInputKeys = ["build", "files", "origin", "source"];
  const missingInputKeys = ["files", "origin", "source"].filter(
    (key) => !inputKeys.includes(key),
  );
  const unknownInputKeys = inputKeys.filter((key) => !allowedInputKeys.includes(key));
  if (missingInputKeys.length > 0 || unknownInputKeys.length > 0) {
    fail(
      `deploymentIdentityInput fields are not exact; missing=${missingInputKeys.join(",")} unknown=${unknownInputKeys.join(",")}`,
    );
  }
  const { source, files, origin, build = {} } = input;
  rejectSensitiveFieldNames({ source, files, origin, build }, "deploymentIdentityInput");
  if (!Array.isArray(files)) {
    fail("deploymentIdentityInput.files must be an array");
  }
  const buildInput = build ?? {};
  const normalizedFiles = [...files]
    .map((entry) => validateFileEntry({ ...entry }, "deploymentIdentityInput.files[]"))
    .sort((left, right) => left.path.localeCompare(right.path));
  const artifact = {
    distPath: deploymentContract.artifact.distPath,
    files: normalizedFiles,
    manifestSha256: manifestDigest(normalizedFiles),
  };
  const identity = {
    schemaVersion: DEPLOYMENT_SCHEMA_VERSION,
    source: { ...source },
    build: {
      command: [...EXACT_BUILD_COMMAND],
      environment: deploymentContract.build.environment,
      nodeVersion: buildInput.nodeVersion ?? process.version,
      packageManager: buildInput.packageManager ?? "npm",
    },
    artifact,
    live: {
      origin: requireOrigin(origin, "deploymentIdentityInput.origin"),
      requiredResources: expectedLiveResources(normalizedFiles),
    },
  };
  validateDeploymentIdentity(identity);
  return deepFreeze(identity);
}

export function validateDeploymentIdentity(identity) {
  rejectSensitiveFieldNames(identity, "deploymentIdentity");
  exactObject(
    identity,
    ["schemaVersion", "artifact", "build", "live", "source"],
    "deploymentIdentity",
  );
  if (identity.schemaVersion !== DEPLOYMENT_SCHEMA_VERSION) {
    fail(`deploymentIdentity.schemaVersion must be exactly ${DEPLOYMENT_SCHEMA_VERSION}`);
  }
  const source = validateSource(identity.source);
  validateBuild(identity.build);
  const artifact = validateArtifact(identity.artifact);
  validateLive(identity.live, artifact);
  return {
    commit: source.commit,
    tree: source.tree,
    manifestSha256: artifact.manifestSha256,
    digest: deploymentIdentityDigest(identity),
  };
}

export function deploymentIdentityDigest(identity) {
  validateDeploymentIdentityWithoutDigest(identity);
  return sha256Hex(canonicalJson(identity));
}

function validateDeploymentIdentityWithoutDigest(identity) {
  rejectSensitiveFieldNames(identity, "deploymentIdentity");
  exactObject(
    identity,
    ["schemaVersion", "artifact", "build", "live", "source"],
    "deploymentIdentity",
  );
  if (identity.schemaVersion !== DEPLOYMENT_SCHEMA_VERSION) {
    fail(`deploymentIdentity.schemaVersion must be exactly ${DEPLOYMENT_SCHEMA_VERSION}`);
  }
  validateSource(identity.source);
  validateBuild(identity.build);
  const artifact = validateArtifact(identity.artifact);
  validateLive(identity.live, artifact);
}

export function verifyLiveDeploymentObservation(identity, observation) {
  const validated = validateDeploymentIdentity(identity);
  rejectSensitiveFieldNames(observation, "liveObservation");
  exactObject(observation, ["buildJson", "html", "resources"], "liveObservation");
  exactObject(
    observation.buildJson,
    ["commit", "manifestSha256", "schemaVersion", "tree"],
    "liveObservation.buildJson",
  );
  if (observation.buildJson.schemaVersion !== DEPLOYMENT_SCHEMA_VERSION) {
    fail("liveObservation.buildJson.schemaVersion is not exact");
  }
  if (
    observation.buildJson.commit !== validated.commit ||
    observation.buildJson.tree !== validated.tree ||
    observation.buildJson.manifestSha256 !== validated.manifestSha256
  ) {
    fail("liveObservation.buildJson does not match the deployment identity");
  }
  verifyObservedResources(identity.live.requiredResources, observation.resources);
  verifyObservedHtml(identity, observation.html);
  return {
    ok: true,
    deploymentIdentitySha256: validated.digest,
    checkedResources: identity.live.requiredResources.length,
  };
}

function verifyObservedResources(expectedResources, observedResources) {
  if (!Array.isArray(observedResources)) {
    fail("liveObservation.resources must be an array");
  }
  if (observedResources.length !== expectedResources.length) {
    fail("liveObservation.resources must match the manifest resource set exactly");
  }
  for (const [index, observed] of observedResources.entries()) {
    exactObject(observed, ["path", "sha256", "status"], `liveObservation.resources[${index}]`);
    requireLivePath(observed.path, `liveObservation.resources[${index}].path`);
    requireSha256(observed.sha256, `liveObservation.resources[${index}].sha256`);
    requireInteger(observed.status, `liveObservation.resources[${index}].status`, {
      minimum: 100,
    });
    const expected = expectedResources[index];
    if (
      observed.status !== 200 ||
      observed.path !== expected.path ||
      observed.sha256 !== expected.sha256
    ) {
      fail("liveObservation.resources differs from the exact manifest resource set");
    }
  }
}

function verifyObservedHtml(identity, observedHtml) {
  if (!Array.isArray(observedHtml)) {
    fail("liveObservation.html must be an array");
  }
  const expectedHtml = identity.artifact.files
    .filter((entry) => entry.role === "html")
    .map((entry) => filePathToLivePath(entry.path));
  if (observedHtml.length !== expectedHtml.length) {
    fail("liveObservation.html must cover every manifest HTML file exactly once");
  }
  for (const [index, observed] of observedHtml.entries()) {
    exactObject(
      observed,
      ["commit", "manifestSha256", "path"],
      `liveObservation.html[${index}]`,
    );
    requireLivePath(observed.path, `liveObservation.html[${index}].path`);
    if (
      observed.path !== expectedHtml[index] ||
      observed.commit !== identity.source.commit ||
      observed.manifestSha256 !== identity.artifact.manifestSha256
    ) {
      fail("liveObservation.html marker differs from the deployment identity");
    }
  }
}
