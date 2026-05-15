/// Resend email via direct REST API call. The Resend SDK works on
/// Workers but adds avoidable bundle weight — one `fetch` does the
/// job.

const RESEND_API = "https://api.resend.com/emails";

export interface SendLicenseEmailInput {
  to: string;
  licensePlaintext: string;
  /** URL of the Stripe Customer Portal session (already created). */
  billingPortalUrl?: string;
  /** Support inbox surfaced in the email signature. */
  supportEmail: string;
  /** Verified Resend sender, e.g. "OSL <licenses@oslprivacy.com>". */
  from: string;
}

export interface EmailDeliveryResult {
  /** Resend's email id on success, undefined on failure. */
  id?: string;
  /** Empty on success, populated on failure (logged, not propagated). */
  error?: string;
}

/**
 * Fire-and-best-effort license delivery. Returns the Resend
 * response shape but never throws — the webhook handler must
 * commit the license to D1 regardless of email outcome (so a
 * transient Resend outage doesn't lose the license; user can
 * recover via Customer Portal "resend").
 */
export async function sendLicenseEmail(
  apiKey: string,
  input: SendLicenseEmailInput,
  fetcher: typeof fetch = fetch,
): Promise<EmailDeliveryResult> {
  const html = licenseEmailHtml(input);
  const text = licenseEmailText(input);
  const body = {
    from: input.from,
    to: [input.to],
    subject: "Your OSL license key",
    html,
    text,
  };
  try {
    const res = await fetcher(RESEND_API, {
      method: "POST",
      headers: {
        authorization: `Bearer ${apiKey}`,
        "content-type": "application/json",
      },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      return { error: `Resend ${res.status}: ${await res.text()}` };
    }
    const j = (await res.json()) as { id?: string };
    return { id: j.id };
  } catch (err) {
    return { error: err instanceof Error ? err.message : String(err) };
  }
}

function licenseEmailText(input: SendLicenseEmailInput): string {
  const portalLine = input.billingPortalUrl
    ? `Manage your subscription: ${input.billingPortalUrl}\n\n`
    : "";
  return (
    `Welcome to OSL.\n\n` +
    `Your license key:\n\n` +
    `  ${input.licensePlaintext}\n\n` +
    `To activate your license:\n` +
    `  1. Open OSL Privacy\n` +
    `  2. Click the settings gear, then go to the Account page\n` +
    `  3. Paste your license key and click Validate & Save\n\n` +
    `That's it — your paid features unlock immediately and stay active automatically.\n\n` +
    `Save this email — you'll need the key to reinstall on a new device.\n\n` +
    portalLine +
    `Questions? Reply to this email or write to ${input.supportEmail}.\n\n` +
    `— The OSL team`
  );
}

function licenseEmailHtml(input: SendLicenseEmailInput): string {
  const portalLine = input.billingPortalUrl
    ? `<p style="margin:24px 0 0;font-size:14px;color:#5b6b80;">Manage your subscription: <a href="${escapeHtml(input.billingPortalUrl)}" style="color:#7c5cff;">customer portal</a></p>`
    : "";
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"></head>
<body style="margin:0;padding:0;background:#f4f6fa;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;color:#1a2332;">
  <table role="presentation" cellspacing="0" cellpadding="0" border="0" align="center" width="560" style="max-width:560px;background:#ffffff;margin:32px auto;border-radius:12px;border:1px solid #e4e8ef;">
    <tr><td style="padding:36px 40px 24px;">
      <h1 style="margin:0 0 20px;font-size:22px;font-weight:600;letter-spacing:-0.01em;">Welcome to OSL.</h1>
      <p style="margin:0 0 20px;font-size:15px;line-height:1.55;">Your license key:</p>
      <pre style="margin:0;padding:18px 22px;background:#0e1929;color:#dbe4f0;font-family:'SFMono-Regular',Menlo,Consolas,monospace;font-size:18px;letter-spacing:0.04em;border-radius:8px;text-align:center;">${escapeHtml(input.licensePlaintext)}</pre>
      <p style="margin:24px 0 8px;font-size:14px;color:#1a2332;font-weight:600;">To activate your license:</p>
      <ol style="margin:0 0 0 20px;padding:0;font-size:14px;line-height:1.6;color:#5b6b80;">
        <li>Open OSL Privacy</li>
        <li>Click the settings gear, then go to the <strong>Account</strong> page</li>
        <li>Paste your license key and click <strong>Validate &amp; Save</strong></li>
      </ol>
      <p style="margin:14px 0 0;font-size:14px;color:#5b6b80;">That's it — your paid features unlock immediately and stay active automatically.</p>
      <p style="margin:14px 0 0;font-size:14px;color:#5b6b80;">Save this email — you'll need the key to reinstall on a new device.</p>
      ${portalLine}
      <p style="margin:32px 0 0;font-size:13px;color:#8493a8;">Questions? Reply to this email or write to <a href="mailto:${escapeHtml(input.supportEmail)}" style="color:#7c5cff;">${escapeHtml(input.supportEmail)}</a>.</p>
      <p style="margin:18px 0 0;font-size:13px;color:#8493a8;">— The OSL team</p>
    </td></tr>
  </table>
</body></html>`;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
