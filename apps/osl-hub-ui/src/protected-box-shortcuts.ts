export const PROTECTED_TEXT_BOX_RULE = "local-edit-only-no-clipboard-shortcuts";
export const COVER_MESSAGE_BOX_RULE = "public-cover-message-no-private-paste";

export const PROTECTED_TEXT_SHORTCUTS = ["Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+Z", "Ctrl+A"] as const;
export type ProtectedTextShortcut = (typeof PROTECTED_TEXT_SHORTCUTS)[number];

type ShortcutEvent = {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  preventDefault(): void;
};

type ListenerTarget = {
  addEventListener(type: string, listener: (event: never) => void): void;
};

type ShortcutRoot = {
  querySelectorAll(selector: string): Iterable<ListenerTarget>;
};

const protectedShortcutKeys = new Set(["a", "c", "v", "x", "z"]);

export function isProtectedTextShortcut(event: Pick<ShortcutEvent, "key" | "ctrlKey" | "metaKey" | "altKey">): boolean {
  return (event.ctrlKey === true || event.metaKey === true)
    && event.altKey !== true
    && protectedShortcutKeys.has(event.key.toLowerCase());
}

export function preventProtectedTextShortcut(event: ShortcutEvent): boolean {
  if (!isProtectedTextShortcut(event)) return false;
  event.preventDefault();
  return true;
}

export function bindProtectedTextBoxShortcutGuards(root: ShortcutRoot): number {
  let protectedBoxCount = 0;
  for (const box of root.querySelectorAll(`[data-osl-protected-box-rule="${PROTECTED_TEXT_BOX_RULE}"]`)) {
    protectedBoxCount += 1;
    box.addEventListener("keydown", (event: ShortcutEvent) => { preventProtectedTextShortcut(event); });
    for (const type of ["copy", "cut", "paste"] as const) {
      box.addEventListener(type, (event: { preventDefault(): void }) => { event.preventDefault(); });
    }
  }
  for (const box of root.querySelectorAll(`[data-osl-cover-message-box="${COVER_MESSAGE_BOX_RULE}"]`)) {
    box.addEventListener("paste", (event: { preventDefault(): void }) => { event.preventDefault(); });
  }
  return protectedBoxCount;
}
