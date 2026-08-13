import { describe, expect, it } from "vitest";
import {
  captureRetainedSurface,
  retainedRouteRegistry,
  retainedSurfaceHtml,
  retainedSurfaceModel,
  routeStateKey,
  type RetainedPage,
} from "./retained-redesign-6872";

const retainedPage: RetainedPage = {
  page_id: "apps/osl-hub-ui/screenshots/evidence/example.png",
  source_image: {
    path: "apps/osl-hub-ui/screenshots/evidence/example.png",
    commit: "0".repeat(40),
    blob: "1".repeat(40),
    width: 1280,
    height: 800,
  },
  route: "/friends",
  state: "empty",
  disposition: "retained",
  required_windows_widths: [1280],
  capability_feed: "friend-authority",
};

describe("TASK 6872 retained redesign renderer", () => {
  it("defaults every missing capability to a visible unavailable state", () => {
    const model = retainedSurfaceModel(retainedPage);
    expect(model.observation).toMatchObject({ origin: "subsystem", status: "unavailable" });
    expect(model.actionEnabled).toBe(false);
    const html = retainedSurfaceHtml(model, "high-contrast");
    expect(html).toContain('data-capability-feed="friend-authority"');
    expect(html).toContain('data-capability-status="unavailable"');
    expect(html).toContain("disabled");
    expect(html).not.toMatch(/<img\b|background-image/u);
  });

  it("emits deterministic visual and accessibility capture facts", () => {
    const first = captureRetainedSurface(retainedPage, 1280, "dark");
    const second = captureRetainedSurface(retainedPage, 1280, "dark");
    expect(second).toEqual(first);
    expect(first.visual.paint_commands).toHaveLength(4);
    expect(first.accessibility.nodes.map((node) => node.role)).toEqual([
      "navigation",
      "main",
      "heading",
      "status",
      "button",
    ]);
  });

  it("keeps deleted and Chats/Strip-owned page states unreachable", () => {
    const deleted = { ...retainedPage, page_id: "deleted", state: "decoy", disposition: "deleted-d1" as const };
    const chats = { ...retainedPage, page_id: "chat", state: "composer", disposition: "chats-strip-follow-up" as const };
    const registry = retainedRouteRegistry([retainedPage, deleted, chats]);
    expect(registry).toEqual(new Set([routeStateKey(retainedPage)]));
    expect(() => retainedSurfaceModel(deleted)).toThrow("page is not retained");
    expect(() => retainedSurfaceModel(chats)).toThrow("page is not retained");
  });
});
