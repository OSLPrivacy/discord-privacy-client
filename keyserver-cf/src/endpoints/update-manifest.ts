/// GET /v1/update-manifest/:target/:arch/:current_version
///
/// Tauri-updater dynamic manifest endpoint. Tauri interpolates the
/// path placeholders at runtime, e.g.
///   /v1/update-manifest/windows/x86_64/0.0.1
///
/// G3.1 scope: no NEWER build exists yet, so the only real job is
/// to answer "no update" for anyone already on the production
/// version (0.0.1+) and to hand the 0.0.1 installer to anyone on
/// an older version (lets us exercise the flow before 0.0.2 ships).
/// Signing arrives in G3.2 — `signature` is an empty placeholder.
///
/// Public (no auth — anyone running OSL checks for updates),
/// rate-limited, and CORS-wrapped to mirror /v1/checkout-session
/// (cosmetic for the Tauri HTTP client, which is not a browser).

import type { Env } from "../env.js";
import { badRequest, json, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

/// The version currently hosted at installers.oslprivacy.com.
/// Bump this (and add the matching .msi) when a new build ships;
/// the client-side semver compare in Tauri does the rest.
const PRODUCTION_VERSION = "0.0.1";
const INSTALLER_URL =
  "https://installers.oslprivacy.com/osl-privacy-0.0.1.msi";
const RELEASE_NOTES = "OSL Privacy 0.0.1 — beta build.";

interface Semver {
  major: number;
  minor: number;
  patch: number;
}

/// Minimal semver parser — MAJOR.MINOR.PATCH, optional leading "v",
/// prerelease/build metadata ignored (our versions are plain). No
/// external dependency: keeps the Worker bundle lean and avoids a
/// supply-chain surface for a dozen lines of arithmetic.
function parseSemver(input: string): Semver | null {
  const cleaned = input.startsWith("v") ? input.slice(1) : input;
  const core = cleaned.split(/[-+]/)[0] ?? "";
  const parts = core.split(".");
  if (parts.length !== 3) return null;
  const nums = parts.map((p) => Number(p));
  if (nums.some((n) => !Number.isInteger(n) || n < 0)) return null;
  return { major: nums[0]!, minor: nums[1]!, patch: nums[2]! };
}

/// -1 if a < b, 0 if equal, 1 if a > b.
function compareSemver(a: Semver, b: Semver): number {
  if (a.major !== b.major) return a.major < b.major ? -1 : 1;
  if (a.minor !== b.minor) return a.minor < b.minor ? -1 : 1;
  if (a.patch !== b.patch) return a.patch < b.patch ? -1 : 1;
  return 0;
}

export async function handleUpdateManifest(
  request: Request,
  env: Env,
  target: string,
  arch: string,
  currentVersion: string,
): Promise<Response> {
  const ip = callerIp(request);
  const rl = await checkRateLimit(env, ip, 10);
  if (!rl.ok) return tooMany(rl.retryAfter);

  const current = parseSemver(currentVersion);
  if (!current) {
    console.log(
      `[update-manifest] reject malformed version ip=${ip} target=${target} arch=${arch} current=${currentVersion}`,
    );
    return badRequest("current_version must be semver MAJOR.MINOR.PATCH");
  }

  const production = parseSemver(PRODUCTION_VERSION)!;

  // current >= production → nothing newer to offer.
  if (compareSemver(current, production) >= 0) {
    console.log(
      `[update-manifest] up-to-date ip=${ip} target=${target} arch=${arch} current=${currentVersion} production=${PRODUCTION_VERSION}`,
    );
    return new Response(null, { status: 204 });
  }

  // current < production → offer the production build. Platform key
  // is "<target>-<arch>" to match Tauri's client-side lookup
  // (`platforms["windows-x86_64"]`); OSL is Windows-only today but
  // we echo whatever the client asked for rather than hardcoding.
  const platformKey = `${target}-${arch}`;
  console.log(
    `[update-manifest] offer ip=${ip} platform=${platformKey} current=${currentVersion} -> ${PRODUCTION_VERSION}`,
  );
  return json({
    version: PRODUCTION_VERSION,
    notes: RELEASE_NOTES,
    pub_date: new Date().toISOString(),
    platforms: {
      [platformKey]: {
        // G3.2 fills this with a real minisign/ed25519 signature.
        signature: "",
        url: INSTALLER_URL,
      },
    },
  });
}
