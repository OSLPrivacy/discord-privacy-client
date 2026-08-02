import { describe, expect, it } from "vitest";
import { retainFocusAcrossRender } from "./focus-retention";

type ControlKind = "input" | "checkbox" | "button";

class TestControl {
  readonly attributes = new Map<string, string>();
  selectionStart: number | null = null;
  selectionEnd: number | null = null;
  selectionDirection: "forward" | "backward" | "none" | null = null;

  constructor(
    readonly root: TestRoot,
    readonly id: string,
    readonly kind: ControlKind,
  ) {
    this.attributes.set("id", id);
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  focus(): void {
    this.root.activeElement = this;
  }

  setSelectionRange(start: number, end: number, direction: "forward" | "backward" | "none" = "none"): void {
    this.selectionStart = start;
    this.selectionEnd = end;
    this.selectionDirection = direction;
  }
}

class TestRoot {
  activeElement: TestControl | null = null;
  controls: TestControl[] = [];

  contains(element: unknown): boolean {
    return this.controls.includes(element as TestControl);
  }

  querySelector(selector: string): TestControl | null {
    const id = /^\[id="(.+)"\]$/u.exec(selector)?.[1];
    return this.controls.find((control) => control.id === id) ?? null;
  }

  render(kind: ControlKind): TestControl {
    const control = new TestControl(this, "persistent-control", kind);
    this.controls = [control];
    this.activeElement = null;
    return control;
  }
}

describe("TU-41 focus retention", () => {
  it.each(["input", "checkbox", "button"] as const)("keeps focus through a full render commit for a %s", (kind) => {
    const root = new TestRoot();
    const before = root.render(kind);
    before.focus();
    if (kind === "input") before.setSelectionRange(2, 4, "backward");

    let after: TestControl | null = null;
    retainFocusAcrossRender(
      root as unknown as ParentNode,
      before as unknown as Element,
      () => { after = root.render(kind); },
    );

    expect(after).not.toBe(before);
    expect(root.activeElement).toBe(after);
    if (kind === "input") {
      expect(after).toMatchObject({ selectionStart: 2, selectionEnd: 4, selectionDirection: "backward" });
    }
  });
});
