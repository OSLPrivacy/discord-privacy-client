/**
 * First-use Strip coach tour.
 *
 * The overlay is deliberately attached to the real Strip host instead of a
 * screenshot or a separate demo. Each step resolves its target immediately
 * before it is drawn and remeasures it on layout changes, so the arrow keeps
 * landing inside the named live control at every window size.
 */

export type StripCoachStepId =
  | "lock"
  | "composer"
  | "reveal"
  | "view-once"
  | "timer"
  | "burn"
  | "verified-senders"
  | "your-plan"
  | "quick-settings";

export interface StripCoachStep {
  readonly id: StripCoachStepId;
  readonly title: string;
  readonly body: string;
  readonly targetSelector: string;
  readonly composerAbove?: boolean;
}

/** The owner-approved set. Its order is also the source of the visible counter. */
export const STRIP_COACH_STEPS: readonly StripCoachStep[] = Object.freeze([
  {
    id: "lock",
    title: "LOCK",
    body: "The lock shows whether this composer is protected. It stays honest when OSL cannot reach this conversation.",
    targetSelector: '[data-coach-anchor="lock"]',
  },
  {
    id: "composer",
    title: "COMPOSER",
    body: "Write protected text in this OSL composer. The carrier's own box remains separate.",
    targetSelector: "#protected-draft",
    composerAbove: true,
  },
  {
    id: "reveal",
    title: "REVEAL",
    body: "The eye is a click toggle: click it to switch between the protected message and its cover text.",
    targetSelector: '[data-coach-anchor="reveal"]',
  },
  {
    id: "view-once",
    title: "VIEW ONCE",
    body: "Choose view once before sending when a supported item should be limited to one open.",
    targetSelector: '[data-coach-anchor="view-once"]',
  },
  {
    id: "timer",
    title: "TIMER",
    body: "Set how long OSL keeps decrypting a protected message. This cannot stop a screenshot or a copy.",
    targetSelector: '[data-coach-anchor="timer"]',
  },
  {
    id: "burn",
    title: "BURN",
    body: "Burn removes this local protected chat and requests relay cleanup. It cannot retract carrier history or recipient copies.",
    targetSelector: '[data-coach-anchor="burn"]',
  },
  {
    id: "verified-senders",
    title: "VERIFIED SENDERS",
    body: "Review verified senders for this conversation before allowing protected delivery.",
    targetSelector: '[data-coach-anchor="verified-senders"]',
  },
  {
    id: "your-plan",
    title: "YOUR PLAN",
    body: "Your plan chip shows the entitlement currently available to this OSL profile.",
    targetSelector: '[data-coach-anchor="your-plan"]',
  },
  {
    id: "quick-settings",
    title: "QUICK SETTINGS",
    body: "Quick settings keeps sending choices close to the composer.",
    targetSelector: '[data-coach-anchor="quick-settings"]',
  },
]);

export interface StripCoachStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface StripCoachTourOptions {
  /** Stable carrier identity, e.g. discord. Dismissal never crosses carriers. */
  readonly carrierId: string;
  readonly storage?: StripCoachStorage;
  /** Opens quick settings without toggling it closed after the final tip. */
  readonly onFinishOpenQuickSettings: () => void;
}

export interface StripCoachTourHandle {
  startIfNeeded(): boolean;
  replay(): boolean;
  dismiss(): void;
  destroy(): void;
  readonly active: boolean;
}

export interface CoachRect {
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
}

export interface CoachBubblePlacement {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  /** The line end, always an interior point of the measured target rectangle. */
  readonly arrowTipX: number;
  readonly arrowTipY: number;
}

const CARD_WIDTH = 330;
const CARD_HEIGHT = 218;
const VIEWPORT_GUTTER = 12;
const TARGET_GAP = 16;

function limit(value: number, low: number, high: number): number {
  return Math.max(low, Math.min(high, value));
}

/**
 * Pure geometry makes the contract testable without a browser. The target is
 * passed in from getBoundingClientRect at draw time; there are no static
 * per-tip coordinates.
 */
export function placeStripCoachBubble(
  target: CoachRect,
  viewport: { width: number; height: number },
  composerAbove = false,
): CoachBubblePlacement {
  const width = Math.min(CARD_WIDTH, Math.max(220, viewport.width - 2 * VIEWPORT_GUTTER));
  const height = Math.min(CARD_HEIGHT, Math.max(170, viewport.height - 2 * VIEWPORT_GUTTER));
  const targetCenterX = target.left + target.width / 2;
  const targetCenterY = target.top + target.height / 2;
  const placeAbove = composerAbove || targetCenterY > viewport.height / 2;
  const desiredY = placeAbove ? target.top - height - TARGET_GAP : target.top + target.height + TARGET_GAP;
  const x = limit(targetCenterX - width / 2, VIEWPORT_GUTTER, Math.max(VIEWPORT_GUTTER, viewport.width - width - VIEWPORT_GUTTER));
  const y = limit(desiredY, VIEWPORT_GUTTER, Math.max(VIEWPORT_GUTTER, viewport.height - height - VIEWPORT_GUTTER));
  // Offset one pixel into the real control, rather than stopping on its edge.
  const insetX = Math.max(1, Math.min(8, target.width / 2));
  const insetY = Math.max(1, Math.min(8, target.height / 2));
  return {
    x,
    y,
    width,
    height,
    arrowTipX: limit(targetCenterX, target.left + insetX, target.left + target.width - insetX),
    arrowTipY: placeAbove ? target.top + insetY : target.top + target.height - insetY,
  };
}

export function stripCoachDismissalKey(carrierId: string): string {
  return `osl-strip-coach-dismissed-v1:${carrierId}`;
}

export function stripCoachWasDismissed(storage: StripCoachStorage, carrierId: string): boolean {
  return storage.getItem(stripCoachDismissalKey(validCarrierId(carrierId))) === "dismissed";
}

export function dismissStripCoach(storage: StripCoachStorage, carrierId: string): void {
  storage.setItem(stripCoachDismissalKey(validCarrierId(carrierId)), "dismissed");
}

function safeStorage(storage: StripCoachStorage | undefined): StripCoachStorage | null {
  if (storage) return storage;
  try { return window.localStorage; } catch { return null; }
}

function validCarrierId(value: string): string {
  return value.replace(/[^a-z0-9_-]/giu, "-").slice(0, 80) || "unknown";
}

/** Mount a measured, persisted coach tour over the actual Strip controls. */
export function createStripCoachTour(root: HTMLElement, options: StripCoachTourOptions): StripCoachTourHandle {
  const storage = safeStorage(options.storage);
  let index = 0;
  let layer: HTMLElement | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let mutationObserver: MutationObserver | null = null;
  let positionFrame: number | null = null;

  const isDismissed = (): boolean => {
    try { return storage ? stripCoachWasDismissed(storage, options.carrierId) : false; } catch { return false; }
  };
  const persistDismissal = (): void => {
    try { if (storage) dismissStripCoach(storage, options.carrierId); } catch { /* the visible tour remains usable */ }
  };
  const targetFor = (step: StripCoachStep): HTMLElement | null => {
    // Composer lives next to the Strip, while every other target belongs to it.
    return root.querySelector<HTMLElement>(step.targetSelector)
      ?? document.querySelector<HTMLElement>(step.targetSelector);
  };
  const clearObservers = (): void => {
    if (positionFrame !== null) cancelAnimationFrame(positionFrame);
    positionFrame = null;
    resizeObserver?.disconnect();
    resizeObserver = null;
    mutationObserver?.disconnect();
    mutationObserver = null;
    window.removeEventListener("resize", schedulePosition, true);
    window.removeEventListener("scroll", schedulePosition, true);
  };
  const close = (): void => {
    clearObservers();
    layer?.remove();
    layer = null;
  };
  const finish = (): void => {
    persistDismissal();
    close();
    options.onFinishOpenQuickSettings();
  };
  const skip = (): void => {
    persistDismissal();
    close();
  };
  const next = (): void => {
    if (index === STRIP_COACH_STEPS.length - 1) finish();
    else { index += 1; renderStep(); }
  };
  const schedulePosition = (): void => {
    if (!layer || positionFrame !== null) return;
    positionFrame = requestAnimationFrame(() => {
      positionFrame = null;
      positionStep();
    });
  };
  const positionStep = (): void => {
    if (!layer) return;
    const step = STRIP_COACH_STEPS[index];
    const target = targetFor(step);
    const foreign = layer.querySelector<SVGForeignObjectElement>("[data-strip-coach-card]");
    const arrow = layer.querySelector<SVGLineElement>("[data-strip-coach-arrow]");
    if (!target || !foreign || !arrow) return;
    const rect = target.getBoundingClientRect();
    const placement = placeStripCoachBubble(rect, { width: window.innerWidth, height: window.innerHeight }, step.composerAbove);
    foreign.setAttribute("x", String(placement.x));
    foreign.setAttribute("y", String(placement.y));
    foreign.setAttribute("width", String(placement.width));
    foreign.setAttribute("height", String(placement.height));
    const card = foreign.getBoundingClientRect();
    const fromX = limit(placement.arrowTipX, card.left + 22, card.right - 22);
    const fromY = placement.arrowTipY > card.bottom ? card.bottom : card.top;
    arrow.setAttribute("x1", String(fromX));
    arrow.setAttribute("y1", String(fromY));
    arrow.setAttribute("x2", String(placement.arrowTipX));
    arrow.setAttribute("y2", String(placement.arrowTipY));
    layer.dataset.coachTarget = step.id;
    layer.dataset.coachArrowTipX = String(placement.arrowTipX);
    layer.dataset.coachArrowTipY = String(placement.arrowTipY);
    layer.dataset.coachTargetLeft = String(rect.left);
    layer.dataset.coachTargetTop = String(rect.top);
    layer.dataset.coachTargetRight = String(rect.right);
    layer.dataset.coachTargetBottom = String(rect.bottom);
  };
  const observePosition = (): void => {
    const target = targetFor(STRIP_COACH_STEPS[index]);
    if (!target || !layer) return;
    resizeObserver = new ResizeObserver(schedulePosition);
    resizeObserver.observe(target);
    resizeObserver.observe(root);
    mutationObserver = new MutationObserver((records) => {
      // Our own telemetry attributes are mutations too; observing them would
      // schedule an unnecessary frame forever. Only a live control/layout
      // mutation outside the coach needs another measured placement.
      if (records.some((record) => !layer?.contains(record.target))) schedulePosition();
    });
    mutationObserver.observe(document.body, { attributes: true, childList: true, subtree: true });
    window.addEventListener("resize", schedulePosition, true);
    window.addEventListener("scroll", schedulePosition, true);
  };
  const renderStep = (): void => {
    close();
    const step = STRIP_COACH_STEPS[index];
    if (!targetFor(step)) return;
    const shell = document.createElement("aside");
    shell.className = "osl-strip-coach";
    shell.setAttribute("data-osl-strip-coach", "true");
    shell.setAttribute("aria-live", "polite");
    shell.innerHTML = `<svg class="osl-strip-coach__svg" aria-hidden="true"><defs><marker id="osl-strip-coach-arrow-head" markerWidth="8" markerHeight="8" refX="6" refY="3" orient="auto"><path d="M0,0 L0,6 L6,3 z"/></marker></defs><line data-strip-coach-arrow marker-end="url(#osl-strip-coach-arrow-head)"/></svg><svg class="osl-strip-coach__card-svg"><foreignObject data-strip-coach-card><section xmlns="http://www.w3.org/1999/xhtml" class="osl-strip-coach__card" role="dialog" aria-label="${step.title}"><div class="osl-strip-coach__kicker">TIP ${index + 1} OF ${STRIP_COACH_STEPS.length}</div><h2>${step.title}</h2><p>${step.body}</p><div class="osl-strip-coach__actions"><button type="button" data-strip-coach-skip>SKIP</button><button type="button" data-strip-coach-next>${index === STRIP_COACH_STEPS.length - 1 ? "OPEN SETTINGS" : "NEXT"}</button></div></section></foreignObject></svg>`;
    document.body.appendChild(shell);
    layer = shell;
    shell.querySelector<HTMLButtonElement>("[data-strip-coach-skip]")?.addEventListener("click", skip);
    shell.querySelector<HTMLButtonElement>("[data-strip-coach-next]")?.addEventListener("click", next);
    observePosition();
    schedulePosition();
  };
  const start = (ignoreDismissal: boolean): boolean => {
    if ((!ignoreDismissal && isDismissed()) || layer || !targetFor(STRIP_COACH_STEPS[0])) return false;
    index = 0;
    renderStep();
    return layer !== null;
  };

  return {
    startIfNeeded: () => start(false),
    replay: () => start(true),
    dismiss: skip,
    destroy: close,
    get active(): boolean { return layer !== null; },
  };
}
