// The OSL Strip — the 44px chip bar that rides on top of a carrier app.
// Canon: design-export README "Screens / Views > 4. Strip".
//
// This module owns the DOM and the interactions; every honesty decision is
// derived in strip-state.ts. Two interaction rules are load-bearing:
//
//   * TOGGLE-TO-REVEAL. Clicking the eye reveals plaintext; clicking it again
//     restores cover text. If the control becomes unavailable while revealed,
//     it closes immediately and asks persistence to fail closed.
//
//   * DISABLED ≠ HIDDEN. An unavailable control stays on screen, keeps its
//     hover tooltip (which is why it uses aria-disabled, not the disabled
//     attribute — a disabled button swallows hover), and refuses activation.

import {
  deriveStripView,
  WHITELIST_MODIFIED_BUILD_NOTE,
  TIMER_HONESTY_LINE,
  type OslStripState,
  type OslStripView,
  type StripChipView,
} from "./strip-state";

export interface OslStripActions {
  /** True when the eye toggle reveals plaintext, false when it restores cover text. */
  readonly onRevealToggle: (revealed: boolean) => void;
  readonly onHome?: () => void;
  readonly onPlan?: () => void;
  /** `trusted` mirrors the activating event so two-step burns can require it. */
  readonly onBurn?: (trusted: boolean) => void;
  readonly onTimerSelect?: (seconds: number | null) => void;
  readonly onOnceToggle?: (armed: boolean) => void;
  readonly onLockToggle?: () => void;
  readonly onQuickSetting?: (id: string) => void;
  readonly onWhitelistToggle?: (personId: string) => void;
  readonly onWindowControl?: (control: "minimise" | "maximise" | "close") => void;
}

export interface OslStripOptions {
  readonly logoUrl: string;
  readonly actions: OslStripActions;
}

export interface OslStripHandle {
  readonly root: HTMLElement;
  update(state: OslStripState): void;
  /** Opens (rather than toggles) the quick-settings panel. Used by the last coach tip. */
  openQuickSettings(): void;
  destroy(): void;
}

type StripPanel = "quick" | "timer" | "whitelist" | null;

const ICONS = {
  tune: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" aria-hidden="true"><path d="M4 7 h9"></path><path d="M13 4 v6"></path><path d="M17 7 h3"></path><path d="M4 17 h3"></path><path d="M7 14 v6"></path><path d="M11 17 h9"></path></svg>`,
  flame: `<svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" stroke="none" aria-hidden="true"><path d="M12 4 c1.5 3 4.5 4.5 4.5 8.5 a4.5 4.5 0 0 1 -9 0 c0 -1.6 .7 -2.8 1.5 -3.8 c.2 1.3 1 1.8 1.5 2 c-.4 -2 .6 -4.5 1.5 -6.7z"></path></svg>`,
  shield: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3 8 5 4 6 v6 c0 4.5 3.4 7.6 8 9 4.6 -1.4 8 -4.5 8 -9 V6 l-4 -1 z"></path><path d="M9 12 l2 2 4 -4"></path></svg>`,
  clock: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="13" r="8"></circle><path d="M12 9 v4 l2.5 2"></path><path d="M9 2 h6"></path></svg>`,
  onceEye: `<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 12 s3.5 -7 10 -7 10 7 10 7 -3.5 7 -10 7 -10 -7 -10 -7z"></path><circle cx="12" cy="12" r="3"></circle><path d="M3 3 l18 18"></path></svg>`,
  lockClosed: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="11" width="16" height="10" rx="1.5"></rect><path d="M8 11 V7 a4 4 0 0 1 8 0 v4"></path></svg>`,
  lockOpen: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="11" width="16" height="10" rx="1.5"></rect><path d="M8 11 V7 a4 4 0 0 1 7.5 -1.9"></path></svg>`,
  eye: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 12 s3.5 -7 10 -7 10 7 10 7 -3.5 7 -10 7 -10 -7 -10 -7z"></path><circle cx="12" cy="12" r="3"></circle><path class="osl-strip__eye-slash" d="M3.5 3.5 l17 17"></path></svg>`,
  gear: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3.2"></circle><path d="M19.4 15 a1.65 1.65 0 0 0 .33 1.82 l.06.06 a2 2 0 1 1 -2.83 2.83 l-.06 -.06 a1.65 1.65 0 0 0 -1.82 -.33 1.65 1.65 0 0 0 -1 1.51 V21 a2 2 0 1 1 -4 0 v-.09 A1.65 1.65 0 0 0 9 19.4 a1.65 1.65 0 0 0 -1.82 .33 l-.06 .06 a2 2 0 1 1 -2.83 -2.83 l.06 -.06 a1.65 1.65 0 0 0 .33 -1.82 1.65 1.65 0 0 0 -1.51 -1 H3 a2 2 0 1 1 0 -4 h.09 A1.65 1.65 0 0 0 4.6 9 a1.65 1.65 0 0 0 -.33 -1.82 l-.06 -.06 a2 2 0 1 1 2.83 -2.83 l.06 .06 a1.65 1.65 0 0 0 1.82 .33 H9 a1.65 1.65 0 0 0 1 -1.51 V3 a2 2 0 1 1 4 0 v.09 a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82 -.33 l.06 -.06 a2 2 0 1 1 2.83 2.83 l-.06 .06 a1.65 1.65 0 0 0 -.33 1.82 V9 a1.65 1.65 0 0 0 1.51 1 H21 a2 2 0 1 1 0 4 h-.09 a1.65 1.65 0 0 0 -1.51 1 z"></path></svg>`,
  minimise: `<svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.1" aria-hidden="true"><path d="M1 6 h10"></path></svg>`,
  maximise: `<svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.1" aria-hidden="true"><rect x="1.3" y="1.3" width="9.4" height="9.4"></rect></svg>`,
  close: `<svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.1" aria-hidden="true"><path d="M1.5 1.5 l9 9 M10.5 1.5 l-9 9"></path></svg>`,
} as const;

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function applyChip(element: HTMLElement, view: StripChipView): void {
  element.dataset.tone = view.tone;
  element.title = view.title;
  if (view.disabled) element.setAttribute("aria-disabled", "true");
  else element.removeAttribute("aria-disabled");
  const face = element.querySelector<HTMLElement>(".osl-strip__face");
  if (face) face.textContent = view.face;
}

function activatable(element: HTMLElement): boolean {
  return element.getAttribute("aria-disabled") !== "true";
}

export function createOslStrip(host: HTMLElement, options: OslStripOptions): OslStripHandle {
  const { actions } = options;
  let state: OslStripState | null = null;
  let view: OslStripView | null = null;
  let openPanel: StripPanel = null;

  host.innerHTML = `
  <div class="osl-strip" role="toolbar" aria-label="OSL strip" data-osl-strip data-reveal-visible="false">
    <div class="osl-strip__cluster osl-strip__cluster--left">
      <button class="osl-strip__logo" type="button" data-chip="home" aria-label="OSL — back to home"><img src="${escapeHtml(options.logoUrl)}" alt=""/></button>
      <button class="osl-strip__chip" type="button" data-chip="plan" data-coach-anchor="your-plan"><span class="osl-strip__face"></span></button>
      <button class="osl-strip__chip osl-strip__chip--icon" type="button" data-chip="quick" data-coach-anchor="quick-settings" aria-haspopup="menu" aria-expanded="false" aria-label="Quick settings">${ICONS.tune}</button>
      <button class="osl-strip__chip osl-strip__chip--icon" type="button" data-chip="burn" data-coach-anchor="burn" aria-label="Burn">${ICONS.flame}</button>
    </div>
    <div class="osl-strip__spring"></div>
    <div class="osl-strip__cluster osl-strip__cluster--right">
      <button class="osl-strip__chip" type="button" data-chip="whitelist" data-coach-anchor="verified-senders" aria-haspopup="menu" aria-expanded="false">${ICONS.shield}<span class="osl-strip__face"></span></button>
      <span class="osl-strip__divider" aria-hidden="true"></span>
      <button class="osl-strip__chip" type="button" data-chip="timer" data-coach-anchor="timer" aria-haspopup="menu" aria-expanded="false">${ICONS.clock}<span class="osl-strip__face"></span></button>
      <button class="osl-strip__chip" type="button" data-chip="once" data-coach-anchor="view-once">${ICONS.onceEye}<span class="osl-strip__face"></span></button>
      <span class="osl-strip__divider" aria-hidden="true"></span>
      <button class="osl-strip__chip osl-strip__chip--icon" type="button" data-chip="lock" data-coach-anchor="lock"></button>
      <button class="osl-strip__chip osl-strip__chip--icon" type="button" data-chip="eye" data-coach-anchor="reveal" aria-pressed="false" aria-label="Click to see the real message">${ICONS.eye}</button>
      <span class="osl-strip__divider osl-strip__divider--wc" aria-hidden="true"></span>
      <button class="osl-strip__wc" type="button" data-window-control="minimise" aria-label="Minimise">${ICONS.minimise}</button>
      <button class="osl-strip__wc" type="button" data-window-control="maximise" aria-label="Maximise">${ICONS.maximise}</button>
      <button class="osl-strip__wc osl-strip__wc--close" type="button" data-window-control="close" aria-label="Close">${ICONS.close}</button>
    </div>
    <div class="osl-strip__panels"></div>
  </div>`;

  const root = host.querySelector<HTMLElement>("[data-osl-strip]");
  if (!root) throw new Error("OSL strip failed to mount");
  const chip = (name: string): HTMLElement => {
    const element = root.querySelector<HTMLElement>(`[data-chip="${name}"]`);
    if (!element) throw new Error(`OSL strip chip '${name}' is missing`);
    return element;
  };
  const homeChip = chip("home");
  const planChip = chip("plan");
  const quickChip = chip("quick");
  const burnChip = chip("burn");
  const whitelistChip = chip("whitelist");
  const timerChip = chip("timer");
  const onceChip = chip("once");
  const lockChip = chip("lock");
  const eyeChip = chip("eye");
  const panels = root.querySelector<HTMLElement>(".osl-strip__panels");
  if (!panels) throw new Error("OSL strip panel layer is missing");

  // ---- Toggle-to-reveal --------------------------------------------------

  eyeChip.addEventListener("click", () => {
    if (!view || view.eye.disabled) return;
    actions.onRevealToggle(!view.eye.pressed);
  });

  // ---- Popovers ----------------------------------------------------------

  function closePanel(): void {
    openPanel = null;
    renderPanels();
  }

  function togglePanel(panel: Exclude<StripPanel, null>): void {
    openPanel = openPanel === panel ? null : panel;
    renderPanels();
  }

  function renderPanels(): void {
    quickChip.dataset.open = String(openPanel === "quick");
    quickChip.setAttribute("aria-expanded", String(openPanel === "quick"));
    timerChip.dataset.open = String(openPanel === "timer");
    timerChip.setAttribute("aria-expanded", String(openPanel === "timer"));
    whitelistChip.dataset.open = String(openPanel === "whitelist");
    whitelistChip.setAttribute("aria-expanded", String(openPanel === "whitelist"));
    if (!state || openPanel === null) { panels!.innerHTML = ""; return; }
    if (openPanel === "quick") { panels!.innerHTML = quickPanelMarkup(); wireQuickPanel(); return; }
    if (openPanel === "timer") { panels!.innerHTML = timerPanelMarkup(); wireTimerPanel(); return; }
    panels!.innerHTML = whitelistPanelMarkup();
    wireWhitelistPanel();
  }

  function quickPanelMarkup(): string {
    const rows = state!.quickSettings.map((row) => {
      const disabled = row.available ? "" : ` aria-disabled="true" title="${escapeHtml(row.reason ?? "")}"`;
      const right = row.kind === "toggle"
        ? `<span class="osl-strip__qs-toggle" aria-hidden="true"><span></span></span>`
        : row.kind === "gear"
          ? ICONS.gear
          : row.value !== undefined
            ? `<span class="osl-strip__qs-value">${escapeHtml(row.value)}</span>`
            : "";
      return `<button class="osl-strip__qs-row" type="button" data-qs="${escapeHtml(row.id)}" data-on="${row.on === true}"${disabled}><span class="osl-strip__qs-name">${escapeHtml(row.name)}</span>${right}</button>`;
    }).join("");
    return `<div class="osl-strip__popup osl-strip__popup--quick" role="menu" aria-label="Quick settings"><div class="osl-strip__popup-heading">Quick settings</div>${rows}</div><button class="osl-strip__backdrop" type="button" data-strip-backdrop aria-label="Close quick settings"></button>`;
  }

  function wireQuickPanel(): void {
    for (const row of panels!.querySelectorAll<HTMLElement>("[data-qs]")) {
      row.addEventListener("click", () => {
        if (!activatable(row)) return;
        actions.onQuickSetting?.(row.dataset.qs ?? "");
      });
    }
    wireBackdrop();
  }

  function timerPanelMarkup(): string {
    const timer = state!.timer;
    const presets = timer.presets.map((preset) => {
      const selected = preset.seconds === timer.seconds
        || (preset.seconds === null && (timer.seconds === null || timer.seconds === 0));
      const disabled = preset.available ? "" : ` aria-disabled="true" title="${escapeHtml(preset.reason ?? "")}"`;
      return `<button class="osl-strip__timer-preset" type="button" data-preset-seconds="${preset.seconds ?? "off"}" data-selected="${selected}"${disabled}>${escapeHtml(preset.label)}</button>`;
    }).join("");
    const fact = timer.factLine ? `<div class="osl-strip__timer-fact">${escapeHtml(timer.factLine)}</div>` : "";
    return `<div class="osl-strip__popup osl-strip__popup--timer" role="menu" aria-label="Message timer"><div class="osl-strip__timer-title">How long does this message last?</div><div class="osl-strip__timer-sub">Applied to every protected message you send here.</div><div class="osl-strip__timer-presets">${presets}</div>${fact}<div class="osl-strip__timer-honesty">${escapeHtml(TIMER_HONESTY_LINE)}</div></div><button class="osl-strip__backdrop" type="button" data-strip-backdrop aria-label="Close the timer"></button>`;
  }

  function wireTimerPanel(): void {
    for (const preset of panels!.querySelectorAll<HTMLElement>("[data-preset-seconds]")) {
      preset.addEventListener("click", () => {
        if (!activatable(preset)) return;
        const raw = preset.dataset.presetSeconds;
        actions.onTimerSelect?.(raw === "off" ? null : Number(raw));
        closePanel();
      });
    }
    wireBackdrop();
  }

  function whitelistPanelMarkup(): string {
    const roster = state!.whitelist.roster ?? [];
    const rows = roster.map((person) => {
      const action = person.allowed ? "ALLOWED" : person.matched ? "ALLOW" : "VERIFY FIRST";
      const actionDisabled = !person.allowed && !person.matched
        ? ` aria-disabled="true" title="Verify this person before allowing them"`
        : "";
      return `<div class="osl-strip__wl-row"><span class="osl-strip__wl-name">${escapeHtml(person.name)}<span class="osl-strip__wl-build" data-build="${person.build}">${escapeHtml(person.buildLabel)}</span></span><button class="osl-strip__wl-action" type="button" data-wl-person="${escapeHtml(person.id)}" data-allowed="${person.allowed}"${actionDisabled}>${action}</button></div>`;
    }).join("");
    return `<div class="osl-strip__popup osl-strip__popup--whitelist" role="menu" aria-label="Whitelist"><div class="osl-strip__wl-head"><span class="osl-strip__wl-title">Whitelist</span><span class="osl-strip__wl-scope">${escapeHtml(state!.roomLabel)}</span></div>${rows}<div class="osl-strip__wl-note">${escapeHtml(WHITELIST_MODIFIED_BUILD_NOTE)}</div></div><button class="osl-strip__backdrop" type="button" data-strip-backdrop aria-label="Close the whitelist"></button>`;
  }

  function wireWhitelistPanel(): void {
    for (const action of panels!.querySelectorAll<HTMLElement>("[data-wl-person]")) {
      action.addEventListener("click", () => {
        if (!activatable(action)) return;
        actions.onWhitelistToggle?.(action.dataset.wlPerson ?? "");
      });
    }
    wireBackdrop();
  }

  function wireBackdrop(): void {
    panels!.querySelector<HTMLElement>("[data-strip-backdrop]")?.addEventListener("click", closePanel);
  }

  // ---- Plain activations -------------------------------------------------

  homeChip.addEventListener("click", () => { if (activatable(homeChip)) actions.onHome?.(); });
  planChip.addEventListener("click", () => { if (activatable(planChip)) actions.onPlan?.(); });
  quickChip.addEventListener("click", () => togglePanel("quick"));
  burnChip.addEventListener("click", (event) => {
    if (activatable(burnChip)) actions.onBurn?.(event.isTrusted);
  });
  whitelistChip.addEventListener("click", () => {
    if (!activatable(whitelistChip)) return;
    if (state?.whitelist.roster) togglePanel("whitelist");
  });
  timerChip.addEventListener("click", () => { if (activatable(timerChip)) togglePanel("timer"); });
  onceChip.addEventListener("click", () => {
    if (activatable(onceChip) && state) actions.onOnceToggle?.(!state.once.armed);
  });
  lockChip.addEventListener("click", () => { if (activatable(lockChip)) actions.onLockToggle?.(); });
  for (const control of root.querySelectorAll<HTMLElement>("[data-window-control]")) {
    control.addEventListener("click", () => {
      actions.onWindowControl?.(control.dataset.windowControl as "minimise" | "maximise" | "close");
    });
  }

  // ---- Update ------------------------------------------------------------

  function update(next: OslStripState): void {
    state = next;
    view = deriveStripView(next);
    root!.dataset.roomProven = String(view.roomProven);
    applyChip(homeChip, view.home);
    applyChip(planChip, view.plan);
    applyChip(quickChip, view.quick);
    applyChip(burnChip, view.burn);
    applyChip(whitelistChip, view.whitelist);
    applyChip(timerChip, view.timer);
    applyChip(onceChip, view.once);
    applyChip(lockChip, view.lock);
    lockChip.innerHTML = next.lock.state === "on" ? ICONS.lockClosed : ICONS.lockOpen;
    lockChip.setAttribute("aria-label", view.lock.title);
    // A lost session or room cannot leave plaintext on screen. The persistence
    // layer receives the close request and retries it if native storage refuses.
    if (next.reveal.revealed && view.eye.disabled) actions.onRevealToggle(false);
    applyChip(eyeChip, view.eye);
    eyeChip.setAttribute("aria-pressed", String(view.eye.pressed === true));
    eyeChip.setAttribute("aria-label", view.eye.title);
    root!.dataset.revealVisible = String(view.eye.pressed === true && !view.eye.disabled);
    eyeChip.querySelector<SVGElement>(".osl-strip__eye-slash")?.setAttribute(
      "visibility",
      view.eye.pressed && !view.eye.disabled ? "hidden" : "visible",
    );
    const windowControls = root!.querySelectorAll<HTMLElement>("[data-window-control], .osl-strip__divider--wc");
    for (const control of windowControls) control.hidden = !next.windowControls;
    if (openPanel === "whitelist" && !next.whitelist.roster) openPanel = null;
    if (openPanel !== null && !view.roomProven && openPanel !== "quick") openPanel = null;
    renderPanels();
  }

  function destroy(): void {
    host.innerHTML = "";
  }

  function openQuickSettings(): void {
    openPanel = "quick";
    renderPanels();
  }

  return { root, update, openQuickSettings, destroy };
}
