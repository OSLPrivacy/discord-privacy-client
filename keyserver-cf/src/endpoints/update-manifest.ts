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

/**
 * Per-version update signatures.
 *
 * Each entry is the contents of the .sig file produced by
 * `cargo tauri build` when TAURI_SIGNING_PRIVATE_KEY_PATH and
 * TAURI_SIGNING_PRIVATE_KEY_PASSWORD env vars are set.
 *
 * AFTER A NEW SIGNED BUILD, the operator workflow is:
 *   1. cargo tauri build (produces .msi + .msi.sig)
 *   2. Get-Content the .msi.sig file
 *   3. Paste the contents here as the value for the new
 *      version key
 *   4. git commit + npx wrangler deploy
 *   5. Upload the signed .msi to R2:
 *      npx wrangler r2 object put osl-installers/osl-privacy-X.Y.Z.msi
 *        --file="target/release/bundle/msi/OSL Privacy_X.Y.Z_x64_en-US.msi"
 *        --remote
 */
export const RELEASE_SIGNATURES: Record<string, string> = {
  "0.0.1":
    "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUNEdjRnI0NG10UkRyMS9MY0VYMmhIOUFvOUNYeXI4dnhuUTZNeWF6d29DeGpMd1BlV2NidTRNWnBJZE8vWDNHNmlGbHhkRHBHSUFiVU9tN3BBUjBFZzlCSHFKeFE0N1FVPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzc4OTAyOTAwCWZpbGU6T1NMIFByaXZhY3lfMC4wLjFfeDY0X2VuLVVTLm1zaQpoQXNoRi9xWlc5ZUFBeUxkYjZvcy9UeGdjOHVhcEZiSHAyc04wais0M0w0SGIveWkzaGszcjdrWG02VTEzVjl3V2puUGZtb08yYXpyc2pTQW9HL1FCUT09Cg==",
};

/// Update channels. `stable` = everyone; `beta` = a paid perk
/// (early access). There is exactly ONE real build today (0.0.1),
/// so both channels point at the same release — when a separate
/// beta build exists this is a one-line change (bump `beta.version`
/// + add its installer URL + add its signature to RELEASE_SIGNATURES).
///
/// Channel eligibility is NOT enforced here: a free user forcing
/// `?channel=beta` only ever gets a slightly-newer *build*, never a
/// paywalled capability (the actual paid features are gated in the
/// seal commands / server-side). Channel is a UX affordance, not a
/// security boundary — do not "harden" this into a fake auth check.
type Channel = "stable" | "beta";

interface ChannelRelease {
  version: string;
  url: string;
  notes: string;
}

const RELEASE_CHANNELS: Record<Channel, ChannelRelease> = {
  stable: {
    version: PRODUCTION_VERSION,
    url: INSTALLER_URL,
    notes: RELEASE_NOTES,
  },
  // No separate beta build yet — beta serves the same 0.0.1 as
  // stable. One-line swap when a real beta build ships.
  beta: {
    version: PRODUCTION_VERSION,
    url: INSTALLER_URL,
    notes: RELEASE_NOTES,
  },
};

/// Anything that isn't exactly "beta" (absent, "stable", garbage,
/// mixed case, ...) resolves to stable — the safe default.
export function parseChannel(raw: string | null): Channel {
  return raw === "beta" ? "beta" : "stable";
}

/// Look up the update signature for a given version. A missing entry
/// must NOT crash the endpoint: we warn and return "" so the manifest
/// still serves, and the client-side updater then fails signature
/// verification — the safe outcome (an unverifiable build is never
/// installed) rather than a hard 5xx that would also block legitimate
/// "no update" traffic. Exported so the fallback/warning path is
/// unit-testable (the handler itself only ever asks for the hardcoded
/// latest PRODUCTION_VERSION, whose key is always present).
export function signatureFor(version: string): string {
  const sig = RELEASE_SIGNATURES[version];
  if (sig === undefined) {
    console.warn(
      `[update-manifest] MISSING signature for version=${version} — returning empty (client will reject)`,
    );
    return "";
  }
  return sig;
}

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

  const channel = parseChannel(new URL(request.url).searchParams.get("channel"));
  const release = RELEASE_CHANNELS[channel];

  const current = parseSemver(currentVersion);
  if (!current) {
    console.log(
      `[update-manifest] reject malformed version ip=${ip} target=${target} arch=${arch} channel=${channel} current=${currentVersion}`,
    );
    return badRequest("current_version must be semver MAJOR.MINOR.PATCH");
  }

  const offered = parseSemver(release.version)!;

  // current >= offered → nothing newer to offer on this channel.
  if (compareSemver(current, offered) >= 0) {
    console.log(
      `[update-manifest] up-to-date ip=${ip} target=${target} arch=${arch} channel=${channel} current=${currentVersion} offered=${release.version}`,
    );
    return new Response(null, { status: 204 });
  }

  // current < offered → offer the channel's build. Platform key is
  // "<target>-<arch>" to match Tauri's client-side lookup
  // (`platforms["windows-x86_64"]`); OSL is Windows-only today but
  // we echo whatever the client asked for rather than hardcoding.
  const platformKey = `${target}-${arch}`;

  // Signature is keyed by the version we're offering. signatureFor()
  // warns + falls back to "" if absent so a missing entry degrades
  // safely instead of 5xx-ing.
  const signature = signatureFor(release.version);

  console.log(
    `[update-manifest] offer ip=${ip} platform=${platformKey} channel=${channel} current=${currentVersion} -> ${release.version}`,
  );
  return json({
    version: release.version,
    notes: release.notes,
    pub_date: new Date().toISOString(),
    platforms: {
      [platformKey]: {
        signature,
        url: release.url,
      },
    },
  });
}
