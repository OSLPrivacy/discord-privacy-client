import { createReadStream } from "node:fs";
import { access } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const publicLogo = join(root, "..", "..", "apps", "osl-hub-ui", "src", "assets", "logo-mark.svg");
const host = "127.0.0.1";
const port = Number.parseInt(process.env.PORT ?? "4177", 10);
const files = new Map([
  ["/", join(root, "index.html")],
  ["/index.html", join(root, "index.html")],
  ["/styles.css", join(root, "styles.css")],
  ["/src/app.js", join(root, "src", "app.js")],
  ["/src/model.js", join(root, "src", "model.js")],
  ["/source/logo-mark.svg", publicLogo],
]);
const contentTypes = new Map([
  [".html", "text/html; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".svg", "image/svg+xml; charset=utf-8"],
]);

export function resolveRoute(url = "/") {
  let pathname;
  try {
    pathname = new URL(url, "http://localhost").pathname;
  } catch {
    return null;
  }
  const decoded = decodeURIComponent(pathname);
  if (decoded.includes("\0") || normalize(decoded).includes("..")) return null;
  return files.get(decoded) ?? null;
}

export function isAllowedHost(value) {
  return /^(?:127\.0\.0\.1|localhost)(?::\d{1,5})?$/i.test(String(value ?? ""));
}

export function createLogoLabServer() {
  return createServer(async (request, response) => {
    if (!isAllowedHost(request.headers.host)) {
      response.writeHead(421, { "content-type": "text/plain; charset=utf-8" });
      response.end("Local host required");
      return;
    }
    const path = resolveRoute(request.url);
    if (!path || (request.method !== "GET" && request.method !== "HEAD")) {
      response.writeHead(path ? 405 : 404, { "content-type": "text/plain; charset=utf-8" });
      response.end(path ? "Method not allowed" : "Not found");
      return;
    }
    try {
      await access(path);
      response.writeHead(200, {
        "content-type": contentTypes.get(extname(path)) ?? "application/octet-stream",
        "cache-control": "no-store",
        "content-security-policy": "default-src 'self'; img-src 'self' blob: data:; script-src 'self'; style-src 'self'; connect-src 'self'; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        "cross-origin-opener-policy": "same-origin",
        "referrer-policy": "no-referrer",
        "x-content-type-options": "nosniff",
        "x-frame-options": "DENY",
      });
      if (request.method === "HEAD") response.end();
      else createReadStream(path).pipe(response);
    } catch {
      response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
      response.end("Not found");
    }
  });
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  createLogoLabServer().listen(port, host, () => {
    process.stdout.write(`OSL Logo Lab: http://${host}:${port}\n`);
  });
}
