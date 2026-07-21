import {
  siDiscord,
  siGmail,
  siGmx,
  siInstagram,
  siMaildotcom,
  siMessenger,
  siSignal,
  siSlack,
  siSnapchat,
  siTelegram,
  siTuta,
  siWhatsapp,
  siX,
  siZoho,
} from "simple-icons";
import type { SimpleIcon } from "simple-icons";
import type { ServiceId } from "./services";

const serviceIcons: Partial<Record<ServiceId | "signal", SimpleIcon>> = {
  discord: siDiscord,
  telegram: siTelegram,
  instagram: siInstagram,
  snapchat: siSnapchat,
  x: siX,
  messenger: siMessenger,
  signal: siSignal,
  whatsapp: siWhatsapp,
  slack: siSlack,
};

const providerIcons: Record<string, SimpleIcon> = {
  gmail: siGmail,
  tuta: siTuta,
  zoho: siZoho,
  gmx: siGmx,
  maildotcom: siMaildotcom,
};

export function serviceLogo(id: ServiceId | "signal"): string {
  if (id === "email") return envelopeSvg("Email");
  if (id === "teams") return teamsSvg();
  if (id === "linkedin") return linkedInSvg();
  const icon = serviceIcons[id];
  return icon ? iconSvg(icon) : envelopeSvg(id);
}

export function providerLogo(id: string): string {
  if (id === "icloud") return iCloudMailSvg();
  if (id === "outlook") return outlookSvg();
  if (id === "proton") return protonMailSvg();
  if (id === "yahoo") return yahooSvg();
  if (id === "aol") return aolSvg();
  const icon = providerIcons[id];
  const fallbackLabels: Record<string, string> = { fastmail: "Fastmail", yahoo: "Yahoo Mail" };
  return icon ? iconSvg(icon) : envelopeSvg(fallbackLabels[id] ?? "Mail");
}

function iCloudMailSvg(): string {
  return `<svg class="company-logo provider-logo provider-logo-icloud" viewBox="0 0 24 24" role="img" aria-label="iCloud Mail"><defs><linearGradient id="icloud-mail-gradient" x1="4" y1="3" x2="20" y2="21" gradientUnits="userSpaceOnUse"><stop stop-color="#62c8ff"/><stop offset="1" stop-color="#1688f8"/></linearGradient></defs><path fill="url(#icloud-mail-gradient)" d="M7.2 19.4h10.2a4.6 4.6 0 0 0 .8-9.1A6.5 6.5 0 0 0 5.9 8.7a5.4 5.4 0 0 0 1.3 10.7Z"/><path fill="none" stroke="#fff" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.45" d="M8.1 12.2h7.8v4.6H8.1zM8.4 12.6l3.6 2.5 3.6-2.5"/></svg>`;
}

function aolSvg(): string {
  return `<svg class="company-logo" viewBox="0 0 24 24" role="img" aria-label="AOL Mail"><path fill="currentColor" d="M2 17.5 7.3 6h2.4L15 17.5h-2.8l-1.1-2.7H5.8l-1.1 2.7H2Zm4.7-5h3.5L8.45 8.2 6.7 12.5Zm9.1 5V6h2.6v9.2H22v2.3h-6.2Z"/><circle cx="14.5" cy="11.8" r="2.8" fill="none" stroke="currentColor" stroke-width="2.2"/><circle cx="22" cy="16.2" r="1.3" fill="currentColor"/></svg>`;
}

function outlookSvg(): string {
  return `<svg class="company-logo provider-logo provider-logo-outlook" viewBox="0 0 24 24" role="img" aria-label="Microsoft Outlook"><path fill="#1490df" d="M9 3h12v7H9z"/><path fill="#0f78d4" d="M9 10h12v11H9z"/><path fill="#35a7e8" d="m9 10 6 4 6-4v8.8c0 1.2-1 2.2-2.2 2.2H9z"/><path fill="#0a5ea8" d="M2 5.5h11v13H2z"/><path fill="#fff" d="M7.5 8.2c2 0 3.3 1.5 3.3 3.8s-1.3 3.8-3.3 3.8-3.3-1.5-3.3-3.8 1.3-3.8 3.3-3.8Zm0 1.9c-.8 0-1.3.7-1.3 1.9s.5 1.9 1.3 1.9 1.3-.7 1.3-1.9-.5-1.9-1.3-1.9Z"/></svg>`;
}

function protonMailSvg(): string {
  return `<svg class="company-logo provider-logo provider-logo-proton" viewBox="0 0 24 24" role="img" aria-label="Proton Mail"><path fill="#6d4aff" d="M2 6.7 8.2 12a2.3 2.3 0 0 0 3-.1l5-4.5v13.1H4.5A2.5 2.5 0 0 1 2 18V6.7Z"/><path fill="#8b6cff" d="M2.7 3.2a.7.7 0 0 0-.7.7v1.3l6.9 5.9a1.2 1.2 0 0 0 1.6 0l1.8-1.6a2.7 2.7 0 0 1-1.2-.6L4.2 3.2H2.7Z"/><path fill="#b6a7ff" d="M21.3 3.2h-1.1l-3 2.6v14.7h2.3A2.5 2.5 0 0 0 22 18V3.9a.7.7 0 0 0-.7-.7Z"/></svg>`;
}

function yahooSvg(): string {
  return `<svg class="company-logo" viewBox="0 0 24 24" role="img" aria-label="Yahoo Mail"><path fill="currentColor" d="m2 5 5.2 8v6h3.2v-6L15.5 5h-3.6L8.8 10.3 5.7 5H2Zm14.8 0h3.5l-.9 9.2h-2.1L16.8 5Zm.2 11.7h3v3h-3v-3Z"/></svg>`;
}

function teamsSvg(): string {
  return `<svg class="company-logo" viewBox="0 0 24 24" role="img" aria-label="Microsoft Teams"><circle cx="18.5" cy="5.2" r="2.3" fill="currentColor" opacity=".75"/><path fill="currentColor" opacity=".75" d="M15 9h7v7.5c0 2-1.6 3.5-3.5 3.5S15 18.5 15 16.5V9Z"/><path fill="currentColor" d="M7 4h9v14.5A3.5 3.5 0 0 1 12.5 22H7V4Z"/><path fill="var(--panel, #fff)" d="M8.7 7h5.8v2H12.6v7h-2.1V9H8.7V7Z"/><circle cx="5" cy="7" r="3" fill="currentColor" opacity=".55"/><path fill="currentColor" opacity=".55" d="M1 11h6v6a3 3 0 0 1-6 0v-6Z"/></svg>`;
}

function linkedInSvg(): string {
  return `<svg class="company-logo" viewBox="0 0 24 24" role="img" aria-label="LinkedIn"><path fill="currentColor" d="M3 8.2h4V21H3V8.2ZM5 2.5A2.3 2.3 0 1 1 5 7a2.3 2.3 0 0 1 0-4.5ZM9.2 8.2H13V10c.9-1.4 2.4-2.3 4.4-2.3 4 0 4.8 2.6 4.8 6V21h-4v-6.5c0-1.6 0-3.6-2.2-3.6s-2.6 1.7-2.6 3.5V21h-4V8.2Z"/></svg>`;
}

function iconSvg(icon: SimpleIcon): string {
  return `<svg class="company-logo" viewBox="0 0 24 24" role="img" aria-label="${icon.title}"><path fill="currentColor" d="${icon.path}"/></svg>`;
}

function envelopeSvg(label: string): string {
  return `<svg class="company-logo" viewBox="0 0 24 24" role="img" aria-label="${label}"><path fill="none" stroke="currentColor" stroke-width="1.8" d="M3.5 6.5h17v11h-17zM4 7l8 6 8-6"/></svg>`;
}
