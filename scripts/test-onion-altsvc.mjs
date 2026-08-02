#!/usr/bin/env node
/** T11-D5 / T11-T16: record Alt-Svc headers as seen through Tor. */
import { execFile } from "node:child_process";
import { promisify } from "node:util";
const run = promisify(execFile);
export const targets = ["https://keyserver.oslprivacy.com/v1/healthz", "https://oslprivacy.com/"];

export function altSvc(headers) {
  return headers.split(/\r?\n/).filter((line) => /^alt-svc:/i.test(line)).map((line) => line.slice(line.indexOf(":") + 1).trim());
}
export async function probe(target, headers = [], proxy = "socks5h://127.0.0.1:9050", exec = run) {
  const { stdout } = await exec("curl", ["--silent", "--show-error", "--max-time", "45", "--proxy", proxy, "--dump-header", "-", "--output", "/dev/null", ...headers.flatMap((value) => ["--header", value]), target]);
  return { target, headers, alt_svc: altSvc(stdout) };
}
if (import.meta.main) {
  try {
    const results = [];
    for (const target of targets) for (const headers of [[], ["User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0", "Accept: text/html,application/xhtml+xml"]]) results.push(await probe(target, headers));
    console.log(JSON.stringify({ measured_at: new Date().toISOString(), results }, null, 2));
  } catch (error) { console.error(`T11-T16 BLOCKED: ${error.message}`); process.exitCode = 1; }
}
