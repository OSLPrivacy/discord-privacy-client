export interface HomeProtectionPanelData {
  protection: string;
  messageProtection: string;
  verification: string;
  reviewSettings: string;
  learnMore: string;
}

export function homeProtectionPanelMarkup(data: HomeProtectionPanelData): string {
  return `<section class="home-protection-panel" aria-labelledby="protection-panel-title">
    <h1 id="protection-panel-title">Protection</h1>
    <p>${data.protection} keeps your messages private on this device.</p>
    <dl>
      <div><dt>Message protection</dt><dd>${data.messageProtection}</dd></div>
      <div><dt>Verification</dt><dd>${data.verification}</dd></div>
    </dl>
    <p>Review the choices that protect your conversations and connected apps.</p>
    <button type="button">${data.reviewSettings}</button>
    <button type="button">${data.learnMore}</button>
  </section>`;
}
