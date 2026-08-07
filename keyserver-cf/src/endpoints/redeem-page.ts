import type { Env } from "../env.js";
import { handleLicenseRedeem } from "./license-redeem.js";

const HTML_HEADERS = {
  "content-type": "text/html; charset=utf-8",
  "cache-control": "no-store",
  "x-content-type-options": "nosniff",
  "referrer-policy": "no-referrer",
};

interface LicenseRedeemBody {
  status?: string;
  redeemed_at?: number;
  expires_at?: number;
  checksum_ok?: boolean;
  error?: string;
}

export async function handleRedeemPage(
  request: Request,
  env: Env,
  pathCode: string | null = null,
): Promise<Response> {
  const url = new URL(request.url);
  const code = url.searchParams.get("code") ?? pathCode;
  if (!code) {
    return renderRedeemPage({
      ok: false,
      title: "Redemption code missing",
      message: "Open the redemption link again or paste your Pro code into OSL.",
      statusCode: 400,
    });
  }

  const redeemHeaders = new Headers({
    "content-type": "application/json",
  });
  const caller = request.headers.get("cf-connecting-ip");
  if (caller) redeemHeaders.set("cf-connecting-ip", caller);
  const redeemResponse = await handleLicenseRedeem(
    new Request("https://internal.oslprivacy.test/v1/license/redeem", {
      method: "POST",
      headers: redeemHeaders,
      body: JSON.stringify({ license_key: code }),
    }),
    env,
  );

  let body: LicenseRedeemBody;
  try {
    body = await redeemResponse.json() as LicenseRedeemBody;
  } catch {
    body = { error: "The redemption service returned an unreadable response." };
  }

  if (redeemResponse.status === 200 && body.status === "ACTIVE") {
    return renderRedeemPage({
      ok: true,
      title: "OSL Pro redeemed",
      message: "Your one-month Pro code is active.",
      status: body.status,
      redeemedAt: body.redeemed_at,
      expiresAt: body.expires_at,
    });
  }

  return renderRedeemPage({
    ok: false,
    title: "Redemption failed",
    message: failureMessage(body),
    status: body.status,
    statusCode: redeemResponse.status === 404 ? 200 : redeemResponse.status,
  });
}

function failureMessage(body: LicenseRedeemBody): string {
  if (body.error) return body.error;
  switch (body.status) {
    case "REVOKED":
      return "This Pro code has been revoked.";
    case "EXPIRED":
      return "This Pro code has expired.";
    case "UNKNOWN":
      return body.checksum_ok === false
        ? "That Pro code is not valid. Check the code and try again."
        : "That Pro code could not be redeemed.";
    default:
      return "That Pro code could not be redeemed.";
  }
}

function renderRedeemPage(input: {
  ok: boolean;
  title: string;
  message: string;
  status?: string;
  statusCode?: number;
  redeemedAt?: number;
  expiresAt?: number;
}): Response {
  const statusText = input.ok ? "success" : "failure";
  const rows = [
    input.status ? `<p>Status: <strong>${escapeHtml(input.status)}</strong></p>` : "",
    typeof input.redeemedAt === "number"
      ? `<p>Redeemed: <time datetime="${isoTime(input.redeemedAt)}">${isoTime(input.redeemedAt)}</time></p>`
      : "",
    typeof input.expiresAt === "number"
      ? `<p>Expires: <time datetime="${isoTime(input.expiresAt)}">${isoTime(input.expiresAt)}</time></p>`
      : "",
  ].join("");
  return new Response(
    `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(input.title)}</title>
  <style>
    :root { color-scheme: light dark; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: Canvas; color: CanvasText; }
    main { width: min(34rem, calc(100vw - 2rem)); border: 1px solid color-mix(in srgb, CanvasText 20%, transparent); border-radius: 8px; padding: 2rem; }
    .status { font-size: 0.8rem; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: ${input.ok ? "#0f7a3b" : "#b42318"}; }
    h1 { margin: 0.5rem 0 1rem; font-size: clamp(1.75rem, 4vw, 2.25rem); line-height: 1.1; }
    p { line-height: 1.5; }
  </style>
</head>
<body>
  <main>
    <div class="status">Redemption ${statusText}</div>
    <h1>${escapeHtml(input.title)}</h1>
    <p>${escapeHtml(input.message)}</p>
    ${rows}
  </main>
</body>
</html>`,
    {
      status: input.statusCode ?? 200,
      headers: HTML_HEADERS,
    },
  );
}

function isoTime(seconds: number): string {
  return new Date(seconds * 1000).toISOString();
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => {
    switch (char) {
      case "&":
        return "&amp;";
      case "<":
        return "&lt;";
      case ">":
        return "&gt;";
      case "\"":
        return "&quot;";
      default:
        return "&#39;";
    }
  });
}
