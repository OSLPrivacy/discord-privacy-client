/**
 * Focused Worker entry used by the 0310a executable acceptance and mutation
 * services. It dispatches the exact production handlers installed in
 * `index.ts`; keeping this tiny entry avoids importing unrelated routes while
 * they are being edited by other build lanes.
 */
import type { Env } from "./env.js";
import {
  handlePrivateContactLinkIssue,
  handlePrivateContactLinkRedeem,
  handlePrivateContactLinkRevoke,
  handlePrivateContactLinkStatus,
} from "./endpoints/private-contact-links.js";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const path = new URL(request.url).pathname;
    if (request.method !== "POST") return new Response("method not allowed", { status: 405 });
    if (path === "/v1/private-contact-links/issue") return handlePrivateContactLinkIssue(request, env);
    if (path === "/v1/private-contact-links/redeem") return handlePrivateContactLinkRedeem(request, env);
    if (path === "/v1/private-contact-links/revoke") return handlePrivateContactLinkRevoke(request, env);
    if (path === "/v1/private-contact-links/status") return handlePrivateContactLinkStatus(request, env);
    return new Response("not found", { status: 404 });
  },
};
