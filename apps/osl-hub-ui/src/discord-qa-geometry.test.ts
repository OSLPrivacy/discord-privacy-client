import { describe, expect, it, vi } from "vitest";
import {
  createDiscordQaGeometryKeeper,
  DISCORD_QA_GEOMETRY_INTERVAL_MS,
  type DiscordQaGeometryDependencies,
} from "./discord-qa-geometry";
import type { NativeWindowHostAction } from "./services";

const resizedReceipt = (): NativeWindowHostAction => ({
  id: "discord",
  status: "resized",
  reason: "none",
  mode: "existingNativeCompanion",
  captureProtected: false,
});

function fixture(): {
  dependencies: DiscordQaGeometryDependencies;
  setActive(value: boolean): void;
  fire(): Promise<void>;
  resize: ReturnType<typeof vi.fn>;
  clearInterval: ReturnType<typeof vi.fn>;
} {
  let active = true;
  let callback: (() => void) | null = null;
  const resize = vi.fn().mockResolvedValue(resizedReceipt());
  const clearInterval = vi.fn();
  return {
    dependencies: {
      isActive: () => active,
      resize,
      setInterval: vi.fn((next: () => void) => {
        callback = next;
        return 17;
      }),
      clearInterval,
    },
    setActive: (value) => { active = value; },
    fire: async () => {
      callback?.();
      await Promise.resolve();
      await Promise.resolve();
    },
    resize,
    clearInterval,
  };
}

describe("Discord QA geometry keeper", () => {
  it("uses one modest interval and accepts only the exact borrowed Discord resize receipt", async () => {
    const test = fixture();
    const keeper = createDiscordQaGeometryKeeper(test.dependencies);

    keeper.start();
    expect(test.dependencies.setInterval).toHaveBeenCalledWith(expect.any(Function), DISCORD_QA_GEOMETRY_INTERVAL_MS);
    await test.fire();

    expect(test.resize).toHaveBeenCalledOnce();
    expect(keeper.running()).toBe(true);
  });

  it("stops without resizing once the QA host or route is no longer active", async () => {
    const test = fixture();
    const keeper = createDiscordQaGeometryKeeper(test.dependencies);
    keeper.start();

    test.setActive(false);
    await test.fire();

    expect(test.resize).not.toHaveBeenCalled();
    expect(test.clearInterval).toHaveBeenCalledWith(17);
    expect(keeper.running()).toBe(false);
  });

  it("stops on a rejected receipt and never overlaps resize requests", async () => {
    const test = fixture();
    let resolveResize: ((receipt: NativeWindowHostAction) => void) | undefined;
    test.resize.mockImplementationOnce(() => new Promise((resolve) => { resolveResize = resolve; }));
    const keeper = createDiscordQaGeometryKeeper(test.dependencies);
    keeper.start();

    await test.fire();
    await test.fire();
    expect(test.resize).toHaveBeenCalledOnce();

    resolveResize?.({ ...resizedReceipt(), id: "telegram" });
    await Promise.resolve();
    await Promise.resolve();
    expect(keeper.running()).toBe(false);
    expect(test.clearInterval).toHaveBeenCalledWith(17);
  });
});
