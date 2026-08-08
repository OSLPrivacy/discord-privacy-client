import "./onboarding-identity.css";
import { continueButton } from "./onboarding-controls";

export type IdentityDiscoveryChoice = "public-name" | "private-link";

export interface PrivateContactLinkViewState {
  busy: boolean;
  linkValue: string | null;
  error: string;
}

function escapeMarkup(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/**
 * A public name is optional. The private branch is a first-class choice rather
 * than a quiet Skip link because it changes how other people can find this
 * account.
 */
export function identityChoiceMarkup(error = ""): string {
  const errorMarkup = error
    ? `<p class="identity-choice-error" id="identity-choice-error" role="alert">${escapeMarkup(error)}</p>`
    : `<p class="identity-choice-error" id="identity-choice-error" role="alert"></p>`;
  return `<section class="identity-choice-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1">How should people find you?</h1>
    <p class="identity-choice-lead">A public name can be searched. A private link works once and cannot be looked up.</p>
    <div class="identity-choice-grid">
      <form class="identity-choice-card" id="public-name-form" novalidate>
        <span class="identity-choice-kicker">Searchable</span>
        <strong>Use a public name</strong>
        <small>People can find this account by its exact public name.</small>
        <label for="public-name-input">Public name</label>
        <input id="public-name-input" name="publicName" minlength="3" maxlength="30" pattern="[a-z0-9](?:[a-z0-9_]{1,28}[a-z0-9])?" autocomplete="off" autocapitalize="none" spellcheck="false" aria-describedby="public-name-help identity-choice-error" placeholder="your_name" required/>
        <span id="public-name-help">3–30 lowercase letters, numbers, or underscores.</span>
        <button class="identity-choice-action" type="submit">Use public name</button>
      </form>
      <article class="identity-choice-card identity-choice-private">
        <span class="identity-choice-kicker">Private</span>
        <strong>No public name</strong>
        <small>Nobody can search for this account. Share a fresh one-use link with each person instead.</small>
        <button class="identity-choice-action" id="choose-no-public-name" type="button">Continue with no public name</button>
      </article>
    </div>
    ${errorMarkup}
  </section>`;
}

/**
 * The private page deliberately accepts only the opaque link value. The
 * backend response also carries the owning person ID for binding checks, but
 * this renderer has no parameter through which that searchable/stable value
 * could accidentally reach the page.
 */
export function privateContactLinkMarkup(state: PrivateContactLinkViewState): string {
  const result = state.linkValue
    ? `<div class="private-link-value" role="group" aria-labelledby="private-link-label">
        <span id="private-link-label">One-use private link</span>
        <code data-private-contact-link>${state.linkValue}</code>
      </div>
      <p class="private-link-use">It stops working after one person uses it. Create a different link for anyone else.</p>
      <button class="identity-choice-action private-link-copy" id="copy-private-contact-link" type="button">Copy private link</button>
      <div class="setup-footer onboarding-actions">${continueButton('id="continue-private-contact-link"', "identity-private-continue")}</div>`
    : state.busy
      ? `<p class="private-link-status" role="status" aria-live="polite">Creating a private link…</p>`
      : `<p class="private-link-status" role="alert">${escapeMarkup(state.error || "A private link could not be created. Nothing was published.")}</p>
        <button class="identity-choice-action" id="retry-private-contact-link" type="button">Try again</button>`;

  return `<section class="private-link-onboarding" aria-labelledby="route-heading" data-searchable-identifier="none">
    <span class="identity-choice-kicker">No public name</span>
    <h1 id="route-heading" tabindex="-1">Your private contact link</h1>
    <p class="identity-choice-lead">This account has no searchable identifier. People can add you only with a link you choose to share.</p>
    ${result}
  </section>`;
}
