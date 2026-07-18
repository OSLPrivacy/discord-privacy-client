import {
  siDiscord,
  siGmail,
  siGmx,
  siInstagram,
  siMaildotcom,
  siMessenger,
  siProtonmail,
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
  proton: siProtonmail,
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
  if (id === "outlook") return outlookSvg();
  if (id === "yahoo") return yahooSvg();
  if (id === "aol") return aolSvg();
  const icon = providerIcons[id];
  const fallbackLabels: Record<string, string> = { fastmail: "Fastmail", yahoo: "Yahoo Mail" };
  return icon ? iconSvg(icon) : envelopeSvg(fallbackLabels[id] ?? "Mail");
}

function aolSvg(): string {
  return `<svg class="company-logo" viewBox="0 0 24 24" role="img" aria-label="AOL Mail"><path fill="currentColor" d="M2 17.5 7.3 6h2.4L15 17.5h-2.8l-1.1-2.7H5.8l-1.1 2.7H2Zm4.7-5h3.5L8.45 8.2 6.7 12.5Zm9.1 5V6h2.6v9.2H22v2.3h-6.2Z"/><circle cx="14.5" cy="11.8" r="2.8" fill="none" stroke="currentColor" stroke-width="2.2"/><circle cx="22" cy="16.2" r="1.3" fill="currentColor"/></svg>`;
}

function outlookSvg(): string {
  return `<svg class="company-logo" viewBox="0 0 24 24" role="img" aria-label="Microsoft Outlook"><path fill="currentColor" opacity=".72" d="M8 4h14v16H8z"/><path fill="currentColor" d="M2 6h11v12H2z"/><path fill="none" stroke="var(--panel, #fff)" stroke-width="1.8" d="M9.6 12c0 2-1 3.3-2.6 3.3S4.4 14 4.4 12 5.4 8.7 7 8.7 9.6 10 9.6 12Z"/><path fill="none" stroke="var(--panel, #fff)" stroke-width="1.4" d="m13 8 4.4 3.4L22 8"/></svg>`;
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
