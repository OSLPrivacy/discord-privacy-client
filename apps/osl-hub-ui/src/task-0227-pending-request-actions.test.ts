import { describe, expect, it } from "vitest";

describe("TASK 0227 - pending request actions", () => {
  it("renders Accept and Decline actions for one incoming request", () => {
    const requestMarkup = `<article class="inbox-row request-source"><span class="source-mark">icon</span><div><strong>Alice</strong><small>Verification needed before protected chat</small></div><button class="button compact" data-osl-request-accept="person-id-1" type="button">Accept</button><button class="button compact" data-osl-request-decline="person-id-1" type="button">Decline</button></article>`;

    expect(requestMarkup).toContain('class="inbox-row request-source"');
    expect(requestMarkup).toContain('data-osl-request-accept="person-id-1"');
    expect(requestMarkup).toContain('>Accept<');
    expect(requestMarkup).toContain('data-osl-request-decline="person-id-1"');
    expect(requestMarkup).toContain('>Decline<');
  });
});
