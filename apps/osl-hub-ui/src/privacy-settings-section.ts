export type PrivacyProtectionPreset = "basic" | "balanced" | "maximum";

export interface PrivacySettingsSectionState {
  proActive: boolean;
  primary: { route: string; reviewTarget: string };
  scanBusy: boolean;
  hasScanResult: boolean;
  timer: string;
  protectionReviewOpen: boolean;
  protectionPreset: PrivacyProtectionPreset;
  screenshotProtectionEnabled: boolean;
  publicPostGuardCarrierPreviewMarkup: string;
  scrubCategoryChooserMarkup: string;
  privacyScanResultsMarkup: string;
}

export function privacyDestinationContent(
  state: PrivacySettingsSectionState,
  statusTag: (label: string, extraClass?: string) => string,
): string {
  const scanActions = `<div class="privacy-scan-actions"><label class="button primary ${state.scanBusy ? "disabled" : ""}" for="privacy-export-input">${state.scanBusy ? "Scanning..." : "Choose export"}</label><input id="privacy-export-input" class="sr-only" type="file" accept=".txt,.json,.csv,text/plain,application/json,text/csv" ${state.scanBusy ? "disabled" : ""}/>${state.hasScanResult ? `<button class="button" id="clear-privacy-scan" type="button">Clear results</button>` : ""}</div>`;
  const policyGroups = [
    ["Before I send", "Risk warnings, public-post checks, and attachment cleaning.", "On in Balanced"],
    ["After I send", `Message timers default to ${state.timer}; view-once media and retention reviews stay off until you choose them.`, "Review first"],
    ["Incoming content", "Link, scam, tracker, and file warnings run on this device when available.", "Local checks"],
    ["My history", "Manual scan and guided review for old messages, posts, and email exports.", "Free scan"],
    ["My exposure", "Old accounts, breach reminders, broker guidance, and privacy drift checks.", "Coming in stages"],
  ] as const;
  const tools = [
    ["History Cleanup", "Find old posts, messages, and email to review."],
    ["Attachment Guard", "Remove location, device, and document metadata before upload."],
    ["Email Privacy", "Block tracking pixels, identify redirect trackers, and sanitize links."],
    ["Exposure Inventory", "Show old accounts, breached identifiers, and public exposure."],
    ["Privacy Drift Watch", "Notice when an app changes settings, permissions, or connection state."],
    ["Scam Shield", "Warn about suspicious links, impersonation, and payment requests."],
    ["Data Removal", "Guide broker requests and verify results instead of counting requests as success."],
    ["Encrypted Capsule", "Send protected files or notes when the recipient does not use OSL."],
  ] as const;
  const policyCards = policyGroups.map(([name, detail, status]) => `<article class="privacy-policy-card"><span class="machine-fact">${statusTag(status)}</span><h3 class="machine-fact">${name}</h3><p>${detail}</p></article>`).join("");
  const toolRows = tools.map(([name, detail], index) => `<article class="setting-line privacy-tool-row"><span><strong class="machine-fact">${name}</strong><small>${detail}</small></span><span class="machine-fact">${statusTag(index === 0 ? "Available" : state.proActive ? "Pro planned" : "Pro")}</span></article>`).join("");
  const cleanupState = state.proActive ? "Manual queue planned" : "Pro manual queue";
  const protectionReview = state.protectionReviewOpen
    ? `<section class="privacy-review-card" data-privacy-protection-review><div><span class="privacy-local-mark machine-fact">PROTECTION REVIEW</span><h2 class="machine-fact">Review or change protection</h2><p>Check the Balanced policy, app exceptions, cleanup limits, and local warning choices before OSL changes anything.</p></div><button class="button compact" data-route="settings" data-settings="scrub" type="button">Open detailed review</button></section>`
    : "";
  const presetCopy: Record<PrivacyProtectionPreset, { title: string; detail: string }> = {
    basic: {
      title: "Basic",
      detail: "Account health, email tracker blocking, attachment metadata warnings, and exposure alerts.",
    },
    balanced: {
      title: "Balanced",
      detail: "Basic account health plus local before-send warnings, attachment cleaning, monthly cleanup review, and private OSL suggestions for verified contacts.",
    },
    maximum: {
      title: "Maximum",
      detail: "Balanced protection plus stricter public-post checks, optional VPN-required actions, and OSL protection required for chosen contacts.",
    },
  };
  const activePreset = presetCopy[state.protectionPreset];
  return `<main class="content-viewport privacy-destination" aria-labelledby="route-heading"><header class="destination-header"><div><p class="eyebrow machine-fact">Privacy</p><h1 class="machine-fact" id="route-heading" tabindex="-1">Privacy</h1><p>Review what OSL will do before it changes anything.</p></div><button class="button primary" data-privacy-primary-action data-route="${state.primary.route}" data-review-target="${state.primary.reviewTarget}" type="button">Review or change protection</button></header>${protectionReview}<section class="privacy-preset-panel" aria-labelledby="privacy-preset-title"><div><span class="privacy-local-mark machine-fact">ACTIVE PRESET</span><h2 class="machine-fact" id="privacy-preset-title">${activePreset.title}</h2><p>${activePreset.detail}</p></div><button class="button compact" data-change-protection-preset type="button">Change preset</button></section><section class="privacy-policy-stack" id="privacy-protection-review" aria-labelledby="privacy-policy-title"><header><div><h2 class="machine-fact" id="privacy-policy-title">Global policy</h2><p>Inherited from ${activePreset.title} until you make an exception.</p></div><span class="machine-fact">${statusTag("Deletion off")}</span></header><p class="privacy-policy-path">${activePreset.title} preset / app / account / conversation exception</p><div class="privacy-policy-grid">${policyCards}</div></section>${state.publicPostGuardCarrierPreviewMarkup}<section class="privacy-review-card manual-scrub-card"><div><span class="privacy-local-mark machine-fact">FREE · THIS DEVICE ONLY</span><h2 class="machine-fact">Recommended action</h2><h3 class="machine-fact">Review an export</h3><p>Choose a TXT, CSV, or JSON message export. OSL suggests items; you decide what to review. Nothing is deleted by this build.</p></div>${scanActions}</section>${state.scrubCategoryChooserMarkup}${state.privacyScanResultsMarkup}<section class="settings-list privacy-tools" aria-labelledby="privacy-tools-title"><header><h2 class="machine-fact" id="privacy-tools-title">Solo privacy tools</h2><p>Useful even when nobody else uses OSL.</p></header>${toolRows}</section><section class="settings-list privacy-limits" aria-labelledby="privacy-limits-title"><header><h2 class="machine-fact" id="privacy-limits-title">Proof and limits</h2><p>OSL refuses actions it cannot verify.</p></header><div class="setting-line"><span><strong class="machine-fact">Cleanup</strong><small>${cleanupState}; every batch must be scanned, shown, previewed, confirmed, executed, and checked.</small></span><span class="machine-fact">${statusTag("No auto delete")}</span></div><div class="setting-line"><span><strong class="machine-fact">Service messages</strong><small>Apps, people, exports, backups, and opened copies may retain content.</small></span><span class="machine-fact">${statusTag("Limit shown")}</span></div><div class="setting-line"><span><strong class="machine-fact">Window protection</strong><small>Applied to OSL's own window when available. Cameras, malware, and modified recipients can still capture content.</small></span><span class="machine-fact">${statusTag(state.screenshotProtectionEnabled ? "Active" : "Unavailable")}</span></div></section></main>`;
}
