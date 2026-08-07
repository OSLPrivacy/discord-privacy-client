import { describe, expect, it } from "vitest";
import { randomBytes } from "crypto";
import { mkdirSync, writeFileSync } from "fs";
import { join } from "path";
import { inviteLinkActionsMarkup, type InviteLinkActionsModel, type OneUseInviteLink } from "./invite-link-actions";

const ONE_USE_INVITE_LINK_PREFIX = "https://invite.osl.local/one-use/";
const ONE_USE_INVITE_LINK_TTL_SECONDS = 60 * 60 * 24;

function base64url(bytes: Buffer): string {
  return bytes.toString("base64url");
}

/**
 * Mirrors `fresh_one_use_invite_link` (apps/osl-hub/src/security.rs): a fresh
 * 16-byte id and a fresh 32-byte token, both drawn from the OS CSPRNG, joined
 * into the same `https://invite.osl.local/one-use/<id>.<token>` shape the
 * `create_one_use_invite_link` command produces. Generated here, at test run
 * time, from real randomness — not a fixed string checked into the repo.
 */
function freshOneUseInviteLink(recipientLabel: string, now: number): OneUseInviteLink {
  const inviteId = base64url(randomBytes(16));
  const token = base64url(randomBytes(32));
  return {
    inviteId,
    link: `${ONE_USE_INVITE_LINK_PREFIX}${inviteId}.${token}`,
    recipientLabel,
    expiresAt: now + ONE_USE_INVITE_LINK_TTL_SECONDS,
  };
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

describe("TASK 0220 invite-link actions", () => {
  it("generates two fresh links that differ, proving the fixture link is not a fixed string", () => {
    const now = 1_700_000_000;
    const first = freshOneUseInviteLink("No OSL name 0220 A", now);
    const second = freshOneUseInviteLink("No OSL name 0220 B", now);

    expect(first.link).not.toBe(second.link);
    expect(first.link.startsWith(ONE_USE_INVITE_LINK_PREFIX)).toBe(true);
    console.log(`TASK0220_LINK_1=${first.link}`);
    console.log(`TASK0220_LINK_2=${second.link}`);
  });

  it("renders all four actions -- create, copy, share, paste -- and one freshly generated link", () => {
    const now = 1_700_000_000;
    const freshLink = freshOneUseInviteLink("No OSL name 0220", now);

    const model: InviteLinkActionsModel = {
      link: freshLink,
      creating: false,
      pasteValue: "",
    };

    const markup = inviteLinkActionsMarkup(model, escapeHtml);

    const createCount = (markup.match(/data-invite-link-create/g) || []).length;
    const copyCount = (markup.match(/data-invite-link-copy/g) || []).length;
    const shareCount = (markup.match(/data-invite-link-share/g) || []).length;
    const pasteFormCount = (markup.match(/data-invite-link-paste-form/g) || []).length;
    const pasteInputCount = (markup.match(/data-invite-link-paste-input/g) || []).length;

    console.log(`TASK0220_CREATE_COUNT=${createCount}`);
    console.log(`TASK0220_COPY_COUNT=${copyCount}`);
    console.log(`TASK0220_SHARE_COUNT=${shareCount}`);
    console.log(`TASK0220_PASTE_FORM_COUNT=${pasteFormCount}`);
    console.log(`TASK0220_PASTE_INPUT_COUNT=${pasteInputCount}`);
    console.log(`TASK0220_RENDERED_LINK=${freshLink.link}`);

    expect(createCount).toBe(1);
    expect(copyCount).toBe(1);
    expect(shareCount).toBe(1);
    expect(pasteFormCount).toBe(1);
    expect(pasteInputCount).toBe(1);

    // The rendered link is the exact fresh link, unmodified.
    expect(markup).toContain(freshLink.link);
    // Copy and Share are usable once a link exists.
    expect(markup).not.toMatch(/data-invite-link-copy disabled/u);
    expect(markup).not.toMatch(/data-invite-link-share disabled/u);

    const fixtureDir = join(process.cwd(), "screenshots", "artifacts");
    mkdirSync(fixtureDir, { recursive: true });
    const fixturePath = join(fixtureDir, "task-0220-invite-link-actions-fixture.html");
    const html = `<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>TASK 0220 Invite Link Actions Fixture</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; padding: 20px; background: #f5f5f5; }
    .invite-link-actions { max-width: 480px; margin: 0 auto; background: white; border-radius: 8px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); padding: 20px; display: flex; flex-direction: column; gap: 16px; }
    .invite-link-create-row { display: flex; }
    .invite-link-display { display: flex; flex-direction: column; gap: 8px; padding: 12px; background: #fafafa; border-radius: 6px; }
    .invite-link-value { word-break: break-all; font-size: 13px; }
    .invite-link-empty { color: #666; font-size: 13px; margin: 0; }
    .invite-link-share-row { display: flex; gap: 8px; }
    .invite-link-paste-form { display: flex; flex-direction: column; gap: 8px; }
    .invite-link-paste-form label { display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
    .invite-link-paste-form input { padding: 8px; border: 1px solid #d0d0d0; border-radius: 4px; font-size: 13px; }
    .button { padding: 8px 16px; border: none; border-radius: 4px; font-weight: 600; font-size: 13px; cursor: pointer; background: #e0e0e0; }
    .button.primary { background: #0066cc; color: white; }
    .button:disabled { background: #eee; color: #999; cursor: not-allowed; }
  </style>
</head>
<body>
  ${markup}
</body>
</html>`;
    writeFileSync(fixturePath, html, "utf-8");
    console.log(`TASK0220_FIXTURE_PATH=${fixturePath}`);
  });

  it("disables create while a create is in flight, and disables copy/share/paste-submit with no link and no pasted value", () => {
    const creating = inviteLinkActionsMarkup({ link: null, creating: true, pasteValue: "" }, escapeHtml);
    expect(creating).toContain('data-invite-link-create disabled');
    expect(creating).toContain('data-invite-link-copy disabled');
    expect(creating).toContain('data-invite-link-share disabled');
    expect(creating).toContain('data-invite-link-paste-submit disabled');
    expect(creating).toContain("No invite link yet.");

    const pasted = inviteLinkActionsMarkup(
      { link: null, creating: false, pasteValue: "https://invite.osl.local/one-use/abc.def" },
      escapeHtml,
    );
    expect(pasted).not.toMatch(/data-invite-link-paste-submit disabled/u);
  });

  it("escapes a hostile link and recipient label rather than trusting their shape", () => {
    const hostile: OneUseInviteLink = {
      inviteId: "id",
      link: 'https://invite.osl.local/one-use/<img src=x onerror="boom">',
      recipientLabel: "<script>boom()</script>",
      expiresAt: 0,
    };
    const markup = inviteLinkActionsMarkup({ link: hostile, creating: false, pasteValue: "" }, escapeHtml);

    expect(markup).not.toContain("<img");
    expect(markup).not.toContain("<script>");
  });
});
