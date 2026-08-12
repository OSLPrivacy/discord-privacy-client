import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  CoachTipController,
  COACH_TIP_CATALOG_VERSION,
  COACH_TIP_STATE_VERSION,
  coachTipCatalog,
  encryptedCoachTipProfileStore,
} from "./coach-tips";

class MemoryStorage implements Storage {
  readonly values = new Map<string, string>();
  get length(): number { return this.values.size; }
  clear(): void { this.values.clear(); }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

async function makeController(storage = new MemoryStorage(), fill = 7): Promise<{ controller: CoachTipController; storage: MemoryStorage }> {
  const controller = new CoachTipController(await encryptedCoachTipProfileStore(
    storage,
    new Uint8Array(32).fill(fill),
    (bytes) => { bytes.fill(fill + 1); return bytes; },
  ));
  await controller.load();
  return { controller, storage };
}

describe("task 6854 contextual coach tips", () => {
  it("renders only in an eligible context and keeps the coached control outside the note", async () => {
    const { controller } = await makeController();
    for (const tip of coachTipCatalog) {
      const markup = controller.markup(tip.id, tip.context, [tip.controlSelector]);
      expect(markup).toContain(`data-coach-tip="${tip.id}"`);
      expect(markup).toContain(`data-coach-tip-dismiss="${tip.id}"`);
      expect(markup).toContain("data-coach-tip-dismiss-all");
      expect(markup).not.toContain(tip.controlSelector);
      expect(controller.markup(tip.id, tip.context, [])).toBe("");
    }
  });

  it("persists only versioned explicit dismissals in opaque AES-GCM profile state", async () => {
    const { controller, storage } = await makeController();
    await controller.dismiss("protect-message");

    const observerText = JSON.stringify([...storage.values.entries()]);
    expect(observerText).toContain("AES-256-GCM");
    for (const forbidden of ["coach", "tip", "dismiss", "protect-message", "impression", "behavior", "usage", "click"]) {
      expect(observerText.toLowerCase()).not.toContain(forbidden);
    }

    const restarted = (await makeController(storage)).controller;
    expect(restarted.markup("protect-message", "protected-workspace", ["#local-protected-toggle"])).toBe("");
    expect(restarted.markup("private-scan", "privacy-scan", ["[data-scrub-route-scan]"])).not.toBe("");
    expect(COACH_TIP_STATE_VERSION).toBe(1);
    expect(COACH_TIP_CATALOG_VERSION).toBe(1);
  });

  it("supports dismiss-all, reset, encrypted sync copy, and independent profiles", async () => {
    const profileA = await makeController();
    await profileA.controller.dismissAll();
    expect(coachTipCatalog.every((tip) => profileA.controller.markup(tip.id, tip.context, [tip.controlSelector]) === "")).toBe(true);

    const syncedStorage = new MemoryStorage();
    for (const [key, value] of profileA.storage.values) syncedStorage.setItem(key, value);
    const syncedA = (await makeController(syncedStorage)).controller;
    expect(coachTipCatalog.every((tip) => syncedA.markup(tip.id, tip.context, [tip.controlSelector]) === "")).toBe(true);

    const profileB = (await makeController(new MemoryStorage(), 19)).controller;
    expect(coachTipCatalog.every((tip) => profileB.markup(tip.id, tip.context, [tip.controlSelector]) !== "")).toBe(true);

    await syncedA.reset();
    expect(coachTipCatalog.every((tip) => syncedA.markup(tip.id, tip.context, [tip.controlSelector]) !== "")).toBe(true);
  });

  it("does not create a session-only dismissal when encrypted persistence refuses", async () => {
    const controller = new CoachTipController({
      getItem: async () => null,
      setItem: async () => { throw new Error("sealed profile unavailable"); },
    });
    await controller.load();
    await expect(controller.dismiss("protect-message")).rejects.toThrow("sealed profile unavailable");
    expect(controller.markup("protect-message", "protected-workspace", ["#local-protected-toggle"])).not.toBe("");
  });

  it("is wired into each shipping context and exposes accessible explicit preference controls", () => {
    const main = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");
    const styles = readFileSync(fileURLToPath(new URL("./styles.css", import.meta.url)), "utf8");
    expect(main).toContain('coachTipMarkup("protect-message", "protected-workspace", ["#local-protected-toggle"])');
    expect(main).toContain('coachTipMarkup("private-scan", "privacy-scan", ["[data-scrub-route-scan]"])');
    expect(main).toContain('coachTipMarkup("switch-profile", "profile-picker", ["[data-switch-identity]"])');
    expect(main).toContain("bindCoachTipControls(document, render)");
    expect(main).toContain("coachTipSettingsMarkup()");
    expect(styles).toContain(".coach-tip");
    expect(styles).toContain(":focus-visible");
  });
});
