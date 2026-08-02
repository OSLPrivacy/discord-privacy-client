export interface FocusSnapshot {
  selector: string;
  selectionStart: number | null;
  selectionEnd: number | null;
  selectionDirection: "forward" | "backward" | "none" | null;
}

type SelectionControl = HTMLElement & {
  selectionStart?: number | null;
  selectionEnd?: number | null;
  selectionDirection?: "forward" | "backward" | "none" | null;
  setSelectionRange?: (start: number, end: number, direction?: "forward" | "backward" | "none") => void;
};

function attributeSelector(attribute: "id" | "data-focus-key", value: string): string {
  return `[${attribute}="${value.replace(/\\/gu, "\\\\").replace(/"/gu, '\\"')}"]`;
}

function isFocusableElement(element: Element | null): element is HTMLElement {
  return element !== null && typeof (element as HTMLElement).focus === "function";
}

/** Captures enough browser state to put focus back after a wholesale render. */
export function captureFocusBeforeRender(root: ParentNode, activeElement: Element | null): FocusSnapshot | null {
  if (!isFocusableElement(activeElement) || !root.contains(activeElement)) return null;

  const focusKey = activeElement.getAttribute("data-focus-key");
  const selector = focusKey
    ? attributeSelector("data-focus-key", focusKey)
    : activeElement.id
      ? attributeSelector("id", activeElement.id)
      : null;
  if (!selector) return null;

  const control = activeElement as SelectionControl;
  return {
    selector,
    selectionStart: typeof control.selectionStart === "number" ? control.selectionStart : null,
    selectionEnd: typeof control.selectionEnd === "number" ? control.selectionEnd : null,
    selectionDirection: control.selectionDirection ?? null,
  };
}

/** Restores a captured descendant without affecting the route-heading focus guard. */
export function restoreFocusAfterRender(root: ParentNode, snapshot: FocusSnapshot | null): void {
  if (!snapshot) return;
  const control = root.querySelector<HTMLElement>(snapshot.selector) as SelectionControl | null;
  if (!control) return;

  control.focus({ preventScroll: true });
  if (
    snapshot.selectionStart !== null
    && snapshot.selectionEnd !== null
    && typeof control.setSelectionRange === "function"
  ) {
    control.setSelectionRange(snapshot.selectionStart, snapshot.selectionEnd, snapshot.selectionDirection ?? undefined);
  }
}

/** Runs a destructive render commit while retaining focus on a stable control key. */
export function retainFocusAcrossRender(
  root: ParentNode,
  activeElement: Element | null,
  commit: () => void,
): void {
  const snapshot = captureFocusBeforeRender(root, activeElement);
  commit();
  restoreFocusAfterRender(root, snapshot);
}
