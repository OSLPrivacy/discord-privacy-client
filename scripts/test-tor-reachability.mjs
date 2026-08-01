#!/usr/bin/env node
/**
 * T11-T15: prove that a request made through Tor reaches the keyserver
 * without a Cloudflare challenge. The Cloudflare T1 skip rule is verified
 * separately in the zone because CF-IPCountry is a request header and is not
 * normally returned to the client.
 */
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

export const DEFAULT_TARGET = "https://keyserver.oslprivacy.com/v1/healthz";
export const DEFAULT_TOR_PROXY = "socks5h://127.0.0.1:9050";

function headerValue(headers, name) {
  return headers[name.toLowerCase()] ?? "";
}

export function assessTorResponse({ status, headers, body }, { expectedCountry } = {}) {
  if (status !== 200) throw new Error(`keyserver returned HTTP ${status}, expected 200`);

  const mitigated = headerValue(headers, "cf-mitigated");
  if (mitigated.toLowerCase() === "challenge") {
    throw new Error("Cloudflare returned a challenge to the Tor request");
  }
  if (!headerValue(headers, "content-type").toLowerCase().includes("application/json")) {
    throw new Error("keyserver response is not JSON; this may be a challenge page");
  }

  let json;
  try {
    json = JSON.parse(body);
  } catch {
    throw new Error("keyserver response is not valid JSON; this may be a challenge page");
  }
  if (json === null || Array.isArray(json) || typeof json !== "object") {
    throw new Error("keyserver JSON health response must be an object");
  }

  const observedCountry = headerValue(headers, "cf-ipcountry").toUpperCase();
  if (expectedCountry && observedCountry !== expectedCountry.toUpperCase()) {
    throw new Error(
      `expected cf-ipcountry ${expectedCountry.toUpperCase()}, received ${observedCountry || "no header"}`,
    );
  }
  return { json, observedCountry: observedCountry || null };
}

export function parseCurlResponse(stdout) {
  const normalized = stdout.replace(/\r\n/g, "\n");
  const finalResponse = normalized.split(/\n\n(?=HTTP\/)/).at(-1);
  const boundary = finalResponse.lastIndexOf("\n\n");
  if (boundary < 0) throw new Error("curl did not return an HTTP header block and body");
  const headerLines = finalResponse.slice(0, boundary).split("\n");
  const statusLine = headerLines.shift() ?? "";
  const match = /^HTTP\/\S+\s+(\d{3})\b/.exec(statusLine);
  if (!match) throw new Error(`invalid HTTP status line: ${statusLine}`);
  const headers = {};
  for (const line of headerLines) {
    const colon = line.indexOf(":");
    if (colon > 0) headers[line.slice(0, colon).toLowerCase()] = line.slice(colon + 1).trim();
  }
  return { status: Number(match[1]), headers, body: finalResponse.slice(boundary + 2) };
}

export async function probeTorReachability({
  target = DEFAULT_TARGET,
  proxy = DEFAULT_TOR_PROXY,
  expectedCountry,
  runCurl = (args) => execFileAsync("curl", args, { maxBuffer: 1024 * 1024 }),
} = {}) {
  const { stdout } = await runCurl([
    "--fail-with-body",
    "--silent",
    "--show-error",
    "--location",
    "--max-time", "45",
    "--proxy", proxy,
    "--dump-header", "-",
    target,
  ]);
  return assessTorResponse(parseCurlResponse(stdout), { expectedCountry });
}

function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--target") options.target = argv[++index];
    else if (value === "--proxy") options.proxy = argv[++index];
    else if (value === "--expect-country") options.expectedCountry = argv[++index];
    else if (value === "--help") {
      console.log("Usage: node scripts/test-tor-reachability.mjs [--target URL] [--proxy socks5h://HOST:PORT] [--expect-country T1]");
      process.exit(0);
    } else throw new Error(`unknown argument: ${value}`);
  }
  return options;
}

if (import.meta.main) {
  try {
    const result = await probeTorReachability(parseArgs(process.argv.slice(2)));
    console.log(`T11-T15 PASS: Tor reached ${DEFAULT_TARGET}; cf-ipcountry=${result.observedCountry ?? "not exposed"}`);
  } catch (error) {
    console.error(`T11-T15 FAIL: ${error.message}`);
    process.exitCode = 1;
  }
}
