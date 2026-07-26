/// The `/v/<id>` landing page.
///
/// ## The one property this file must never lose
///
/// The response is **byte-identical for every id** -- ids that exist,
/// ids that never existed, ids that expired, ids already viewed. Same
/// status (always 200), same `Content-Length`, same `ETag`, same bytes.
/// Nothing about an id is knowable from this route.
///
/// That is what stops Discord's unfurler (and every other crawler)
/// from being an existence oracle, and -- combined with the fact that
/// ciphertext is only released by a POST carrying a gesture header --
/// what stops a crawler from burning a view before the human clicks.
/// The page therefore embeds no id: it reads the id from
/// `location.pathname` and the key from `location.hash` at runtime.
///
/// ## Key custody
///
/// The AES-256-GCM key arrives in the URL **fragment**. Browsers do not
/// transmit fragments -- not in the request line, not in `Referer`, not
/// across redirects. The Worker never sees it and has no column to put
/// it in. Decryption is native WebCrypto; this page ships **zero JS
/// crypto**.
///
/// The CSP is `default-src 'self'` with per-directive tightening and
/// **no third-party subresources at all** -- script and style are
/// inline and pinned by SHA-256 hash, the favicon is an empty `data:`
/// URL, and `connect-src 'self'` means the only request this page can
/// ever make is back to its own origin. There is no outbound channel
/// through which the fragment could leak.
///
/// ## Honesty
///
/// Everything the page does about screenshots is **deterrence**. There
/// is no web API for screenshot prevention. The page says so, in those
/// words, above the reveal button. The strings "screenshot-protected",
/// "secure view", "cannot be saved" and "disappears forever" are banned
/// and a test enforces their absence.

const LANDING_STYLE = `
:root { color-scheme: dark; }
* { box-sizing: border-box; }
body {
  margin: 0;
  padding: 2rem 1.25rem 3rem;
  background: #0e1013;
  color: #e6e8ea;
  font: 15px/1.55 ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  -webkit-user-select: none;
  user-select: none;
}
main { max-width: 46rem; margin: 0 auto; }
h1 { font-size: 1.3rem; margin: 0 0 .25rem; letter-spacing: -.01em; }
h2 { font-size: .82rem; margin: 1.4rem 0 .4rem; text-transform: uppercase;
     letter-spacing: .08em; color: #8b949e; font-weight: 600; }
p { margin: .5rem 0; }
.lede { color: #d7dbe0; font-size: 1rem; }
.warn {
  border: 1px solid #4a3a12; background: #1b1608; color: #f0d9a0;
  border-radius: 8px; padding: .75rem .9rem; margin: 1rem 0;
}
.warn strong { color: #ffe6a8; }
.muted { color: #8b949e; font-size: .86rem; }
ul { margin: .4rem 0 .4rem 1.1rem; padding: 0; }
li { margin: .28rem 0; color: #b9c0c8; font-size: .88rem; }
.tag {
  display: inline-block; font-size: .68rem; letter-spacing: .06em;
  text-transform: uppercase; border-radius: 4px; padding: .05rem .35rem;
  margin-right: .4rem; border: 1px solid #3a4048; color: #9aa4ae;
}
.tag.det { border-color: #5a4a1a; color: #d8bd74; }
.tag.pre { border-color: #1f4d33; color: #74d8a0; }
button {
  font: inherit; font-weight: 600; cursor: pointer;
  background: #1f6feb; color: #fff; border: 0; border-radius: 8px;
  padding: .6rem 1.2rem; margin-top: .6rem;
}
button:disabled { background: #2a2f36; color: #6b7280; cursor: default; }
button:focus-visible { outline: 2px solid #8ab4ff; outline-offset: 2px; }
#stage { margin: 1.2rem 0; }
canvas {
  max-width: 100%; height: auto; display: block;
  background: #000; border: 1px solid #262b31; border-radius: 8px;
  -webkit-user-select: none; user-select: none; -webkit-touch-callout: none;
  pointer-events: none;
}
#status { min-height: 1.4rem; color: #b9c0c8; font-size: .9rem; }
#status.err { color: #f0a0a0; }
label.opt { display: block; margin-top: .9rem; font-size: .86rem; color: #b9c0c8; }
label.opt input { margin-right: .4rem; }
footer { margin-top: 2rem; border-top: 1px solid #21262d; padding-top: .9rem; }
[hidden] { display: none !important; }
`;

// No template literals and no backticks inside this string: it is
// embedded verbatim into a TS template literal below.
const LANDING_SCRIPT = `
"use strict";
(function () {
  var AAD = "OSL-VIEW-ONCE-LINK-v1";
  var MAGIC = "OSLV1";
  var NONCE_BYTES = 12;

  var byId = function (id) { return document.getElementById(id); };
  var stage = byId("stage");
  var canvas = byId("c");
  var ctx = canvas.getContext("2d");
  var revealBtn = byId("reveal");
  var statusEl = byId("status");
  var sliceBox = byId("slice");
  var hostEl = byId("host");
  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)");

  var linkId = "";
  var token = "";
  var keyB64 = "";
  var revealed = false;
  var burned = false;
  var released = false;
  var buf = null;
  var bctx = null;
  var rafId = 0;
  var slicePhase = 0;

  hostEl.textContent = location.hostname;

  function say(text, isError) {
    statusEl.textContent = text;
    statusEl.className = isError ? "err" : "";
  }

  // The fragment is the only place the key ever exists. Browsers never
  // transmit it. Read it once, then strip it from the address bar so it
  // does not survive in history, a bookmark, or a shoulder-surf.
  function readFragment() {
    var raw = location.hash.replace(/^#/, "");
    var out = {};
    var pairs = raw.split("&");
    for (var i = 0; i < pairs.length; i++) {
      var eq = pairs[i].indexOf("=");
      if (eq > 0) out[pairs[i].slice(0, eq)] = pairs[i].slice(eq + 1);
    }
    return out;
  }

  function b64uToBytes(value) {
    if (!/^[A-Za-z0-9_-]+$/.test(value)) return null;
    var s = value.replace(/-/g, "+").replace(/_/g, "/");
    while (s.length % 4) s += "=";
    try {
      var bin = atob(s);
      var out = new Uint8Array(bin.length);
      for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
      return out;
    } catch (e) { return null; }
  }

  function blank() {
    if (buf) { buf.width = 1; buf.height = 1; buf = null; bctx = null; }
    if (rafId) { cancelAnimationFrame(rafId); rafId = 0; }
    try { ctx.clearRect(0, 0, canvas.width, canvas.height); } catch (e) {}
    canvas.width = 1;
    canvas.height = 1;
    stage.hidden = true;
  }

  // Burn is best-effort from the client. The authoritative destruction
  // is server-side at reserved_until; this only makes it happen sooner.
  function burn(reason) {
    if (burned) return;
    burned = true;
    blank();
    revealBtn.disabled = true;
    say(reason, false);
    if (!released || !linkId || !token) { token = ""; keyB64 = ""; return; }
    var body = JSON.stringify({ t: token });
    token = ""; keyB64 = "";
    try {
      fetch("/v/" + linkId + "/burn", {
        method: "POST",
        keepalive: true,
        credentials: "omit",
        cache: "no-store",
        referrerPolicy: "no-referrer",
        headers: { "content-type": "application/json" },
        body: body
      })["catch"](function () {});
    } catch (e) {}
  }

  function armDeterrence() {
    // Every listener below is DETERRENCE. None of it can stop a
    // screenshot, a screen recorder, or a camera pointed at the screen.
    document.addEventListener("visibilitychange", function () {
      if (document.visibilityState !== "visible") burn("Hidden. The link has been used.");
    });
    window.addEventListener("blur", function () {
      burn("Focus left the page. The link has been used.");
    });
    window.addEventListener("keyup", function (e) {
      if (e.key === "PrintScreen") burn("The link has been used.");
    });
    window.addEventListener("pagehide", function () { burn("The link has been used."); });
  }

  function fitCanvas(w, h) {
    canvas.width = w;
    canvas.height = h;
    buf = document.createElement("canvas");
    buf.width = w;
    buf.height = h;
    bctx = buf.getContext("2d");
  }

  function present() {
    if (burned || !buf) return;
    if (!sliceBox.checked || sliceBox.disabled) {
      ctx.drawImage(buf, 0, 0);
      return;
    }
    // Experimental temporal slicing. Honours prefers-reduced-motion by
    // being force-disabled above. It leaks a large fraction of the
    // image per still screenshot and is defeated outright by any screen
    // recorder; it is offered only because it was asked for.
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(buf, 0, 0);
    ctx.globalCompositeOperation = "destination-out";
    ctx.fillStyle = "#000";
    for (var y = slicePhase * 6; y < canvas.height; y += 12) {
      ctx.fillRect(0, y, canvas.width, 6);
    }
    ctx.globalCompositeOperation = "source-over";
    slicePhase = slicePhase ? 0 : 1;
    rafId = requestAnimationFrame(present);
  }

  function drawText(text) {
    var width = 960;
    var pad = 28;
    var lineHeight = 26;
    var probe = document.createElement("canvas").getContext("2d");
    probe.font = "17px ui-sans-serif, system-ui, sans-serif";
    var lines = [];
    var paras = text.split("\\n");
    for (var p = 0; p < paras.length; p++) {
      var words = paras[p].split(/\\s+/);
      var line = "";
      for (var w = 0; w < words.length; w++) {
        var next = line ? line + " " + words[w] : words[w];
        if (probe.measureText(next).width > width - pad * 2 && line) {
          lines.push(line);
          line = words[w];
        } else { line = next; }
      }
      lines.push(line);
    }
    var height = Math.max(120, pad * 2 + lines.length * lineHeight);
    fitCanvas(width, height);
    bctx.fillStyle = "#000";
    bctx.fillRect(0, 0, width, height);
    bctx.fillStyle = "#f2f4f6";
    bctx.font = "17px ui-sans-serif, system-ui, sans-serif";
    bctx.textBaseline = "top";
    for (var i = 0; i < lines.length; i++) {
      bctx.fillText(lines[i], pad, pad + i * lineHeight);
    }
  }

  // Images are decoded via createImageBitmap on an in-memory Blob:
  // no <img> element, no object URL, nothing with a src a browser
  // would offer to save. Deterrence, not prevention.
  function drawImage(bytes, mime) {
    return createImageBitmap(new Blob([bytes], { type: mime })).then(function (bmp) {
      var scale = Math.min(1, 1400 / bmp.width, 1000 / bmp.height);
      var w = Math.max(1, Math.round(bmp.width * scale));
      var h = Math.max(1, Math.round(bmp.height * scale));
      fitCanvas(w, h);
      bctx.drawImage(bmp, 0, 0, w, h);
      bmp.close();
    });
  }

  function parseFrame(pt) {
    var dec = new TextDecoder();
    if (pt.length < 8) throw new Error("frame");
    if (dec.decode(pt.subarray(0, 5)) !== MAGIC) throw new Error("frame");
    var kind = pt[5];
    var mimeLen = (pt[6] << 8) | pt[7];
    var at = 8 + mimeLen;
    if (pt.length < at + 4) throw new Error("frame");
    var mime = dec.decode(pt.subarray(8, at));
    var bodyLen = ((pt[at] << 24) >>> 0) + (pt[at + 1] << 16) + (pt[at + 2] << 8) + pt[at + 3];
    var body = pt.subarray(at + 4, at + 4 + bodyLen);
    if (body.length !== bodyLen) throw new Error("frame");
    return { kind: kind, mime: mime, body: body };
  }

  function reveal() {
    if (revealed || burned) return;
    revealed = true;
    revealBtn.disabled = true;
    say("Opening...", false);
    fetch("/v/" + linkId + "/fetch", {
      method: "POST",
      credentials: "omit",
      cache: "no-store",
      referrerPolicy: "no-referrer",
      // Set only on this path, which is reachable only from a trusted
      // user gesture. A crawler that executes JS still cannot click.
      headers: { "content-type": "application/json", "x-osl-gesture": "1" },
      body: JSON.stringify({ t: token })
    }).then(function (res) {
      if (!res.ok) throw new Error("gone");
      released = true;
      return res.arrayBuffer();
    }).then(function (raw) {
      var all = new Uint8Array(raw);
      if (all.length <= NONCE_BYTES) throw new Error("gone");
      var iv = all.subarray(0, NONCE_BYTES);
      var ct = all.subarray(NONCE_BYTES);
      var keyBytes = b64uToBytes(keyB64);
      if (!keyBytes || keyBytes.length !== 32) throw new Error("key");
      return crypto.subtle.importKey("raw", keyBytes, { name: "AES-GCM" }, false, ["decrypt"])
        .then(function (key) {
          return crypto.subtle.decrypt(
            { name: "AES-GCM", iv: iv, tagLength: 128, additionalData: new TextEncoder().encode(AAD) },
            key,
            ct
          );
        });
    }).then(function (ptBuf) {
      keyB64 = "";
      var frame = parseFrame(new Uint8Array(ptBuf));
      armDeterrence();
      stage.hidden = false;
      if (frame.kind === 1) {
        return drawImage(frame.body, frame.mime || "image/png").then(function () {
          present();
          say("Open now. The link stops working when you close this page, or in 60 seconds.", false);
        });
      }
      drawText(new TextDecoder().decode(frame.body));
      present();
      say("Open now. The link stops working when you close this page, or in 60 seconds.", false);
      return null;
    })["catch"](function () {
      blank();
      say("This link is not available. It may already have been opened, or it may have expired.", true);
    });
  }

  function onReveal(e) {
    // isTrusted is false for every synthetic event, so an automated
    // crawler cannot reach the fetch even if it executes this script.
    if (!e.isTrusted) return;
    if (e.type === "keydown" && e.key !== "Enter" && e.key !== " ") return;
    if (e.type === "keydown") e.preventDefault();
    if (sliceBox.checked && !sliceBox.disabled) {
      var ok = window.confirm(
        "Flicker masking rapidly alternates a high-contrast pattern. This can trigger seizures in people with photosensitive epilepsy. It does not stop screen recording. Turn it on anyway?"
      );
      if (!ok) { sliceBox.checked = false; return; }
    }
    reveal();
  }

  function init() {
    var path = location.pathname.replace(/\\/+$/, "").split("/");
    linkId = path[path.length - 1] || "";
    var frag = readFragment();
    keyB64 = frag.k || "";
    token = frag.t || "";
    try { history.replaceState(null, "", location.pathname); } catch (e) {}

    if (reduce.matches) {
      sliceBox.checked = false;
      sliceBox.disabled = true;
      byId("slicelabel").textContent =
        "Flicker masking (experimental) - disabled because your system asks for reduced motion.";
    }

    if (!/^[0-9a-f]{32}$/.test(linkId) || !/^[0-9a-f]{32}$/.test(token) || !keyB64) {
      revealBtn.disabled = true;
      say("This link is not available. It may already have been opened, or it may have expired.", true);
      return;
    }
    revealBtn.addEventListener("pointerdown", onReveal);
    revealBtn.addEventListener("keydown", onReveal);
    sliceBox.addEventListener("change", function () {
      if (rafId) { cancelAnimationFrame(rafId); rafId = 0; }
      present();
    });
    say("", false);
  }

  ["contextmenu", "dragstart", "selectstart", "copy", "cut"].forEach(function (name) {
    document.addEventListener(name, function (e) { e.preventDefault(); });
  });

  init();
})();
`;

const LANDING_BODY = `<main>
<h1>One-time link</h1>
<p class="lede">This opens once. When you close it, the link stops working. Your screen isn't protected.</p>

<div class="warn">
<p><strong>What this page can promise:</strong> the link dies after one view, or 60&nbsp;seconds, whichever is first. The content is decrypted here in your browser with a key that was in the link itself and was never sent to the server.</p>
<p><strong>What it cannot promise:</strong> nothing here stops a screenshot, a screen recorder, or a phone camera. No web browser can do that. Everything below is deterrence, not prevention.</p>
</div>

<button id="reveal" type="button">Show it once</button>
<label class="opt" for="slice"><input type="checkbox" id="slice"><span id="slicelabel">Flicker masking (experimental) &mdash; may trigger seizures, and does not stop screen recording.</span></label>
<p id="status" role="status" aria-live="polite"></p>

<div id="stage" hidden><canvas id="c" width="1" height="1" aria-label="One-time content, drawn as an image."></canvas></div>

<h2>What is actually happening</h2>
<ul>
<li><span class="tag pre">prevention</span>The decryption key travels in the URL fragment, which browsers never send to a server. This server holds ciphertext and cannot decrypt it.</li>
<li><span class="tag pre">prevention</span>The content is released once. After 60&nbsp;seconds the server destroys it whether or not this page confirms.</li>
<li><span class="tag det">deterrent</span>The page blanks and burns the link when it is hidden, loses focus, or sees a PrintScreen key release.</li>
<li><span class="tag det">deterrent</span>Content is drawn to a canvas &mdash; no image element, no object URL, nothing with a "save image as".</li>
<li><span class="tag det">deterrent</span>Right-click, drag, selection and copy are suppressed.</li>
<li><span class="tag pre">prevention</span>Nothing is cached: the content response is no-store, and the page is excluded from search indexes.</li>
</ul>
<p class="muted">Because content is drawn to a canvas, text on this page cannot be selected and is not readable by a screen reader. That is a real accessibility cost of the deterrence above, and we would rather say so than hide it.</p>

<footer>
<p class="muted">Report abuse: abuse@<span id="host"></span></p>
<p class="muted">Sent with OSL. This page holds no account, sets no cookie, and records no IP address, browser or referrer.</p>
</footer>
</main>`;

const LANDING_HTML = `<!doctype html><html lang="en"><head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow, noarchive, nosnippet, noimageindex">
<meta name="referrer" content="no-referrer">
<link rel="icon" href="data:,">
<title>One-time link</title>
<style>${LANDING_STYLE}</style>
</head><body>
${LANDING_BODY}
<script>${LANDING_SCRIPT}</script>
</body></html>
`;

export const LANDING_BYTES: Uint8Array = new TextEncoder().encode(LANDING_HTML);

/// Exposed for tests that assert the page contains no banned claim and
/// no DevTools detection.
export function landingSource(): string {
  return LANDING_HTML;
}

async function sha256Base64(input: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(input),
  );
  let bin = "";
  for (const b of new Uint8Array(digest)) bin += String.fromCharCode(b);
  return btoa(bin);
}

let cachedHeaders: Promise<Record<string, string>> | null = null;

async function buildHeaders(): Promise<Record<string, string>> {
  const scriptHash = await sha256Base64(LANDING_SCRIPT);
  const styleHash = await sha256Base64(LANDING_STYLE);
  let bodyDigest = "";
  for (const b of new Uint8Array(
    await crypto.subtle.digest("SHA-256", LANDING_BYTES),
  ).slice(0, 16)) {
    bodyDigest += b.toString(16).padStart(2, "0");
  }
  const csp = [
    "default-src 'self'",
    "script-src 'sha256-" + scriptHash + "'",
    "style-src 'sha256-" + styleHash + "'",
    // The only request this page may make is back to its own origin.
    "connect-src 'self'",
    "img-src data:",
    "font-src 'none'",
    "media-src 'none'",
    "object-src 'none'",
    "frame-src 'none'",
    "child-src 'none'",
    "worker-src 'none'",
    "manifest-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
    "frame-ancestors 'none'",
    "upgrade-insecure-requests",
  ].join("; ");
  return {
    "content-type": "text/html; charset=utf-8",
    "content-length": String(LANDING_BYTES.byteLength),
    // Constant across every id -- part of the no-existence-oracle
    // guarantee, and it lets a crawler's fetch be served from cache.
    etag: '"' + bodyDigest + '"',
    "cache-control": "public, max-age=300",
    "content-security-policy": csp,
    "referrer-policy": "no-referrer",
    "x-content-type-options": "nosniff",
    "x-frame-options": "DENY",
    "x-robots-tag": "noindex, nofollow, noarchive, nosnippet, noimageindex",
    "cross-origin-opener-policy": "same-origin",
    "cross-origin-resource-policy": "same-origin",
    "permissions-policy":
      "camera=(), microphone=(), geolocation=(), interest-cohort=()",
  };
}

export async function landingHeaders(): Promise<Record<string, string>> {
  if (!cachedHeaders) cachedHeaders = buildHeaders();
  return { ...(await cachedHeaders) };
}

/// The landing response. Identical bytes, identical headers, HTTP 200 --
/// for every id, existing or not, expired or not, viewed or not.
export async function handleLanding(): Promise<Response> {
  return new Response(LANDING_BYTES, {
    status: 200,
    headers: await landingHeaders(),
  });
}

export function handleRobots(): Response {
  return new Response("User-agent: *\nDisallow: /\n", {
    status: 200,
    headers: {
      "content-type": "text/plain; charset=utf-8",
      "cache-control": "public, max-age=86400",
      "x-robots-tag": "noindex, nofollow",
    },
  });
}
