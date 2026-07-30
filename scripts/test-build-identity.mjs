#!/usr/bin/env node
import assert from "node:assert/strict";
import test from "node:test";
import {
  DeploymentContractError,
  canonicalJson,
  createDeploymentIdentity,
  deploymentContract,
  deploymentIdentityDigest,
  sha256Hex,
  validateDeploymentIdentity,
  verifyLiveDeploymentObservation,
} from "./build-identity.mjs";

const SHA_A = "a".repeat(64);
const SHA_B = "b".repeat(64);
const SHA_C = "c".repeat(64);
const SHA_D = "d".repeat(64);
const SHA_E = "e".repeat(64);
const COMMIT = "1".repeat(40);
const TREE = "2".repeat(40);
const EMPTY_SHA256 =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

function baseFiles() {
  return [
    { path: "assets/app.js", role: "asset", sha256: SHA_D, sizeBytes: 4048 },
    { path: "_headers", role: "metadata", sha256: SHA_C, sizeBytes: 72 },
    { path: "build.json", role: "metadata", sha256: SHA_A, sizeBytes: 180 },
    { path: "index.html", role: "html", sha256: SHA_B, sizeBytes: 2048 },
    { path: "nested/status.html", role: "html", sha256: SHA_E, sizeBytes: 512 },
  ];
}

function source(overrides = {}) {
  return {
    commit: COMMIT,
    tree: TREE,
    clean: true,
    dirtyFingerprint: EMPTY_SHA256,
    ...overrides,
  };
}

function identity(overrides = {}) {
  return createDeploymentIdentity({
    source: source(overrides.source),
    files: overrides.files ?? baseFiles(),
    origin: overrides.origin ?? "https://osl.example",
    build: {
      nodeVersion: "v22.0.0",
      packageManager: "npm@10.0.0",
      ...overrides.build,
    },
  });
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function assertContractError(fn, pattern) {
  assert.throws(fn, (error) => {
    assert.ok(error instanceof DeploymentContractError);
    assert.match(error.message, pattern);
    return true;
  });
}

function observationFor(deploymentIdentity) {
  return {
    buildJson: {
      schemaVersion: deploymentIdentity.schemaVersion,
      commit: deploymentIdentity.source.commit,
      tree: deploymentIdentity.source.tree,
      manifestSha256: deploymentIdentity.artifact.manifestSha256,
    },
    resources: deploymentIdentity.live.requiredResources.map((resource) => ({
      path: resource.path,
      status: 200,
      sha256: resource.sha256,
    })),
    html: deploymentIdentity.artifact.files
      .filter((entry) => entry.role === "html")
      .map((entry) => ({
        path: `/${entry.path}`,
        commit: deploymentIdentity.source.commit,
        manifestSha256: deploymentIdentity.artifact.manifestSha256,
      })),
  };
}

test("deploymentContract freezes the exact live verification policy", () => {
  assert.equal(Object.isFrozen(deploymentContract), true);
  assert.equal(deploymentContract.schemaVersion, 1);
  assert.deepEqual(deploymentContract.build.command, ["npm", "run", "build"]);
  assert.equal(
    deploymentContract.liveVerification.retryPolicy,
    "caller-owned-no-automatic-retry",
  );
  assert.equal(deploymentContract.liveVerification.requireExactManifestResourcesOnly, true);
});

test("canonicalJson is stable and newline terminated", () => {
  assert.equal(
    canonicalJson({ b: 2, a: { d: true, c: ["x"] } }),
    '{"a":{"c":["x"],"d":true},"b":2}\n',
  );
});

test("sha256Hex hashes strings as UTF-8 bytes", () => {
  assert.equal(
    sha256Hex("osl"),
    "9efa71e4fb2432a9a9f08cb348af0559f1cc74324932cbb130c5b83ea15ac106",
  );
});

test("createDeploymentIdentity sorts files and binds the manifest digest", () => {
  const created = identity();
  assert.deepEqual(
    created.artifact.files.map((entry) => entry.path),
    ["_headers", "assets/app.js", "build.json", "index.html", "nested/status.html"],
  );
  assert.equal(
    created.artifact.manifestSha256,
    sha256Hex(canonicalJson(created.artifact.files)),
  );
  assert.deepEqual(
    created.live.requiredResources.map((entry) => entry.path),
    ["/_headers", "/assets/app.js", "/build.json", "/index.html", "/nested/status.html"],
  );
});

test("validateDeploymentIdentity accepts an exact generated identity", () => {
  const created = identity();
  assert.equal(validateDeploymentIdentity(created).commit, COMMIT);
});

test("deploymentIdentityDigest changes when a bound artifact changes", () => {
  const first = identity();
  const changed = identity({
    files: baseFiles().map((entry) =>
      entry.path === "assets/app.js" ? { ...entry, sha256: "f".repeat(64) } : entry,
    ),
  });
  assert.notEqual(deploymentIdentityDigest(first), deploymentIdentityDigest(changed));
});

test("dirty source state is refused", () => {
  assertContractError(
    () => identity({ source: { dirtyFingerprint: SHA_A } }),
    /dirtyFingerprint/,
  );
});

test("missing consent-like source cleanliness is refused rather than inferred", () => {
  const candidate = clone(identity());
  delete candidate.source.clean;
  assertContractError(() => validateDeploymentIdentity(candidate), /source fields are not exact/);
});

test("unknown root fields are refused", () => {
  const candidate = clone(identity());
  candidate.previewUrl = "https://preview.example";
  assertContractError(() => validateDeploymentIdentity(candidate), /deploymentIdentity fields/);
});

test("secret and account-identifying field names are refused anywhere", () => {
  const candidate = clone(identity());
  candidate.live.accountId = "acct_123";
  assertContractError(() => validateDeploymentIdentity(candidate), /accountId is forbidden/);
});

test("deployment origin cannot include credentials", () => {
  assertContractError(
    () => identity({ origin: "https://user:pass@osl.example" }),
    /origin must be an HTTPS origin/,
  );
});

test("deployment origin cannot include a path", () => {
  assertContractError(
    () => identity({ origin: "https://osl.example/prod" }),
    /origin must be an HTTPS origin/,
  );
});

test("uppercase SHA-256 is refused", () => {
  assertContractError(
    () =>
      identity({
        files: baseFiles().map((entry) =>
          entry.path === "build.json" ? { ...entry, sha256: "A".repeat(64) } : entry,
        ),
      }),
    /lowercase SHA-256/,
  );
});

test("path traversal in manifest files is refused", () => {
  assertContractError(
    () => identity({ files: [...baseFiles(), { path: "../admin.html", role: "html", sha256: SHA_A, sizeBytes: 1 }] }),
    /normalized relative POSIX path/,
  );
});

test("duplicate manifest files are refused", () => {
  const created = clone(identity());
  created.artifact.files.push({ ...created.artifact.files[0] });
  assertContractError(() => validateDeploymentIdentity(created), /duplicate path/);
});

test("unsorted manifest files are refused", () => {
  const created = clone(identity());
  created.artifact.files.reverse();
  assertContractError(() => validateDeploymentIdentity(created), /path-sorted/);
});

test("build.json must be present", () => {
  assertContractError(
    () => identity({ files: baseFiles().filter((entry) => entry.path !== "build.json") }),
    /missing build.json/,
  );
});

test("_headers must be present", () => {
  assertContractError(
    () => identity({ files: baseFiles().filter((entry) => entry.path !== "_headers") }),
    /missing _headers/,
  );
});

test("index.html must be present", () => {
  assertContractError(
    () => identity({ files: baseFiles().filter((entry) => entry.path !== "index.html") }),
    /missing index.html/,
  );
});

test("required file roles are exact", () => {
  assertContractError(
    () =>
      identity({
        files: baseFiles().map((entry) =>
          entry.path === "index.html" ? { ...entry, role: "asset" } : entry,
        ),
      }),
    /index.html must be html/,
  );
});

test("tampered manifest digest is refused", () => {
  const candidate = clone(identity());
  candidate.artifact.manifestSha256 = SHA_A;
  assertContractError(() => validateDeploymentIdentity(candidate), /manifestSha256/);
});

test("build command is exact", () => {
  const candidate = clone(identity());
  candidate.build.command = ["npm", "run", "deploy"];
  assertContractError(() => validateDeploymentIdentity(candidate), /exact production build command/);
});

test("production environment is exact", () => {
  const candidate = clone(identity());
  candidate.build.environment = "preview";
  assertContractError(() => validateDeploymentIdentity(candidate), /environment must be production/);
});

test("live required resources must match every manifest file exactly", () => {
  const candidate = clone(identity());
  candidate.live.requiredResources = candidate.live.requiredResources.filter(
    (resource) => resource.path !== "/assets/app.js",
  );
  assertContractError(() => validateDeploymentIdentity(candidate), /cover every manifest file/);
});

test("live required resources cannot include unknown files", () => {
  const candidate = clone(identity());
  candidate.live.requiredResources.push({ path: "/ghost.txt", sha256: SHA_A });
  assertContractError(() => validateDeploymentIdentity(candidate), /cover every manifest file/);
});

test("live required resource hashes must match the manifest", () => {
  const candidate = clone(identity());
  candidate.live.requiredResources[1].sha256 = SHA_A;
  assertContractError(() => validateDeploymentIdentity(candidate), /requiredResources must match/);
});

test("verifyLiveDeploymentObservation accepts exact live evidence", () => {
  const created = identity();
  assert.deepEqual(verifyLiveDeploymentObservation(created, observationFor(created)), {
    ok: true,
    deploymentIdentitySha256: deploymentIdentityDigest(created),
    checkedResources: created.live.requiredResources.length,
  });
});

test("live build.json commit must match", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.buildJson.commit = "3".repeat(40);
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /buildJson does not match/,
  );
});

test("live build.json manifest digest must match", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.buildJson.manifestSha256 = SHA_A;
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /buildJson does not match/,
  );
});

test("live resource set cannot omit a manifest resource", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.resources.pop();
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /resources must match/,
  );
});

test("live resource set cannot add an undeclared served file", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.resources.push({ path: "/undeclared.txt", status: 200, sha256: SHA_A });
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /resources must match/,
  );
});

test("live resource status must be 200", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.resources[0].status = 404;
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /resources differs/,
  );
});

test("live resource digest must match", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.resources[0].sha256 = SHA_A;
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /resources differs/,
  );
});

test("every manifest HTML file must be observed", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.html = observation.html.filter((entry) => entry.path !== "/nested/status.html");
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /cover every manifest HTML file/,
  );
});

test("nested HTML markers must match the same commit", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.html.find((entry) => entry.path === "/nested/status.html").commit = "3".repeat(40);
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /html marker differs/,
  );
});

test("HTML markers must match the same manifest digest", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.html[0].manifestSha256 = SHA_A;
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /html marker differs/,
  );
});

test("live observations use closed schemas", () => {
  const created = identity();
  const observation = observationFor(created);
  observation.autoRetry = true;
  assertContractError(
    () => verifyLiveDeploymentObservation(created, observation),
    /liveObservation fields are not exact/,
  );
});

test("canonical JSON refuses unsupported values", () => {
  assertContractError(() => canonicalJson({ unsupported: undefined }), /unsupported values/);
});
