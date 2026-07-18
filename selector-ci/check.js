const DISCORD_APP = "https://discord.com/app";
const USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) " +
  "AppleWebKit/537.36 Chrome/125 Safari/537.36 OSL-Selector-CI/1.0";
const MAX_STYLESHEET_BYTES = 2 * 1024 * 1024;

// These are only class-name hints. OSL's runtime resolver leads with
// ARIA/role/structure strategies, but disappearance of these families
// is a useful early warning that Discord shipped a major UI rewrite.
const requiredFamilies = new Map([
  ["account panel", /\.panels_{1,2}[A-Za-z0-9_-]*/],
  ["guild rail", /\.guilds_{1,2}[A-Za-z0-9_-]*/],
  ["message composer", /\.channelTextArea_{1,2}[A-Za-z0-9_-]*/],
  ["channel toolbar", /\.toolbar_{1,2}[A-Za-z0-9_-]*/],
  ["header icon", /\.iconWrapper_{1,2}[A-Za-z0-9_-]*/],
  ["member row", /\.member_{1,2}[A-Za-z0-9_-]*/],
]);

async function fetchBounded(url, maxBytes) {
  const response = await fetch(url, {
    headers: { "user-agent": USER_AGENT, accept: "text/html,text/css;q=0.9" },
    redirect: "follow",
    signal: AbortSignal.timeout(20_000),
  });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  const declared = Number(response.headers.get("content-length") || 0);
  if (declared > maxBytes) throw new Error(`${url}: ${declared} bytes exceeds cap`);
  const body = await response.text();
  if (Buffer.byteLength(body) > maxBytes) {
    throw new Error(`${url}: response exceeds ${maxBytes}-byte cap`);
  }
  return { body, headers: response.headers };
}

function stylesheetUrls(html) {
  const urls = [];
  for (const match of html.matchAll(/<link\b[^>]*>/gi)) {
    const tag = match[0];
    if (!/\brel=["']stylesheet["']/i.test(tag)) continue;
    const href = tag.match(/\bhref=["']([^"']+\.css(?:\?[^"']*)?)["']/i)?.[1];
    if (href) urls.push(new URL(href, DISCORD_APP).href);
  }
  return [...new Set(urls)];
}

async function main() {
  const page = await fetchBounded(DISCORD_APP, 2 * 1024 * 1024);
  const urls = stylesheetUrls(page.body);
  if (urls.length === 0) throw new Error("Discord app exposed no linked stylesheets");

  const missing = new Map(requiredFamilies);
  let cursor = 0;
  let fetched = 0;
  const failures = [];
  async function worker() {
    while (missing.size > 0) {
      const index = cursor++;
      if (index >= urls.length) return;
      const url = urls[index];
      try {
        const { body } = await fetchBounded(url, MAX_STYLESHEET_BYTES);
        fetched++;
        for (const [name, pattern] of missing) {
          if (pattern.test(body)) missing.delete(name);
        }
      } catch (error) {
        failures.push(error instanceof Error ? error.message : String(error));
      }
    }
  }
  await Promise.all(Array.from({ length: 8 }, () => worker()));

  const buildId = page.headers.get("x-build-id") || "unknown";
  console.log(`Discord build: ${buildId}`);
  console.log(`Stylesheets inspected: ${fetched}/${urls.length}`);
  if (missing.size > 0) {
    const names = [...missing.keys()].join(", ");
    const suffix = failures.length > 0 ? ` (${failures.length} fetch failures)` : "";
    throw new Error(`selector families missing: ${names}${suffix}`);
  }
  console.log(`Selector families present: ${[...requiredFamilies.keys()].join(", ")}`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack : error);
  process.exitCode = 1;
});
