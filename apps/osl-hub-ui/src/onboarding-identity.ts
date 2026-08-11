import type { HubPrivateContactLink } from "./adapters";

export type IdentityDiscoveryChoice = "public-name" | "private-link";
export type RenderedPrivateContactLink = HubPrivateContactLink & { terminalState: "live" | "revoked" };

export interface PrivateContactLinkActions {
  create(): Promise<HubPrivateContactLink | null>;
  revoke(link: HubPrivateContactLink): Promise<boolean>;
}

function escapeMarkup(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;").replaceAll("'", "&#39;");
}

export function identityChoiceMarkup(error = ""): string {
  return `<section class="identity-choice-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1">How should people find you?</h1>
    <p>A public name can be searched. Private links are finite, one-use, and can be revoked.</p>
    <form id="public-name-form"><label for="public-name-input">Public name</label><input id="public-name-input" name="publicName" minlength="3" maxlength="30"/><button type="submit">Use public name</button></form>
    <button id="choose-no-public-name" type="button">Continue with no public name</button>
    <p role="alert">${escapeMarkup(error)}</p>
  </section>`;
}

function exactExpiry(link: RenderedPrivateContactLink): string {
  return new Date(link.expiresAtUnixSeconds * 1000).toISOString();
}

export function privateContactLinkMarkup(
  links: readonly RenderedPrivateContactLink[],
  busy: boolean,
  error = "",
): string {
  const rows = links.map((link) => {
    const expiry = exactExpiry(link);
    const revoked = link.terminalState === "revoked";
    return `<article class="private-link-card" data-private-link-state="${link.terminalState}">
      <code data-private-contact-link>${escapeMarkup(link.linkValue)}</code>
      <p>Expires exactly <time data-private-link-expiry data-expires-at-unix-seconds="${link.expiresAtUnixSeconds}" datetime="${expiry}">${expiry}</time></p>
      <button data-revoke-private-contact-link="${escapeMarkup(link.linkValue)}" type="button" ${revoked ? "disabled" : ""}>${revoked ? "Revoked" : "Revoke now"}</button>
    </article>`;
  }).join("");
  return `<section class="private-link-onboarding" aria-labelledby="route-heading" data-searchable-identifier="none">
    <h1 id="route-heading" tabindex="-1">Your private contact links</h1>
    <p>This account has no searchable identifier. Each link works once, expires at the exact service time shown, and can be revoked immediately.</p>
    <div data-private-contact-link-list>${rows}</div>
    <p role="alert">${escapeMarkup(error)}</p>
    <button id="create-another-private-contact-link" type="button" ${busy ? "disabled" : ""}>${busy ? "Creating…" : links.length === 0 ? "Create private link" : "Create another private link"}</button>
    <button id="continue-private-contact-link" type="button" ${links.length === 0 ? "disabled" : ""}>Continue</button>
  </section>`;
}

/** State/controller used by the shipping onboarding page and its rendered controls. */
export class PrivateContactLinkPageController {
  readonly links: RenderedPrivateContactLink[] = [];
  busy = false;
  error = "";

  constructor(private readonly actions: PrivateContactLinkActions) {}

  async create(): Promise<boolean> {
    if (this.busy) return false;
    this.busy = true;
    this.error = "";
    const issued = await this.actions.create();
    this.busy = false;
    if (!issued || this.links.some((link) => link.linkValue === issued.linkValue)) {
      this.error = "OSL could not create a fresh private link. Nothing was published.";
      return false;
    }
    this.links.push({ ...issued, terminalState: "live" });
    return true;
  }

  async revoke(linkValue: string): Promise<boolean> {
    const link = this.links.find((candidate) => candidate.linkValue === linkValue);
    if (!link || link.terminalState !== "live") return false;
    const revoked = await this.actions.revoke(link);
    if (!revoked) {
      this.error = "OSL could not confirm revocation. The link is still shown as live.";
      return false;
    }
    link.terminalState = "revoked";
    return true;
  }

  render(): string {
    return privateContactLinkMarkup(this.links, this.busy, this.error);
  }
}
