import { describe, expect, it } from "vitest";
import {
  ACCESS_SOURCE_LABELS,
  CHANNEL_ACTIONS,
  CHANNEL_MODE_LABELS,
  SHIPPED_CHANNEL_MODES,
  collapseStorageKey,
  deletionRefusalWords,
  isVisualOnlyReorder,
  planChannelDeletion,
  readCollapsedCategories,
  renderDeleteChannelDialog,
  renderEnclaveLayout,
  renderMeasuredLimit,
  renderPermissionGrid,
  renderedChannelOrder,
  reorderChannel,
  toggleCollapsedCategory,
  writeCollapsedCategories,
  type AccessCell,
  type CategoryRow,
  type ChannelAction,
  type ChannelMode,
  type ChannelRow,
  type EnclaveLayoutModel,
  type LocalKeyValueStore,
} from "./osl-enclave-layout";

/** A device's local storage, so a "restart" is a new view over the same bytes. */
class MemoryStore implements LocalKeyValueStore {
  constructor(private readonly cells = new Map<string, string>()) {}

  getItem(key: string): string | null {
    return this.cells.has(key) ? (this.cells.get(key) as string) : null;
  }

  setItem(key: string, value: string): void {
    this.cells.set(key, value);
  }

  /** The same bytes, opened again: this is what a restart looks like here. */
  reopen(): MemoryStore {
    return new MemoryStore(new Map(this.cells));
  }

  keys(): string[] {
    return [...this.cells.keys()].sort();
  }
}

function cell(action: ChannelAction, allowed: boolean, source: AccessCell["source"], because: string): AccessCell {
  return { action, allowed, source, because };
}

function channel(
  channelId: string,
  name: string,
  mode: ChannelMode,
  messageCount = 0,
  access: readonly AccessCell[] = [
    cell("read", true, "inherited", "OPEN"),
    cell("post", true, "inherited", "OPEN"),
    cell("manage", false, "inherited", "OPEN"),
  ],
): ChannelRow {
  return { channelId, name, mode, messageCount, access };
}

function category(categoryId: string, name: string, channels: readonly ChannelRow[]): CategoryRow {
  return { categoryId, name, channels };
}

const model: EnclaveLayoutModel = {
  categories: [
    category("cat-commons", "Commons", [
      channel("ch-welcome", "welcome", "open"),
      channel("ch-notices", "notices", "read-only"),
      channel("ch-stewardship", "stewardship", "stewards"),
    ]),
    category("cat-field", "Field", [
      channel("ch-field-log", "field-log", "open"),
      channel("ch-dispatch", "dispatch", "read-only"),
    ]),
    category("cat-archive", "Archive", [channel("ch-cold", "cold-store", "open")]),
    category("cat-signals", "Signals", [
      channel("ch-relay", "relay", "stewards"),
      channel("ch-beacon", "beacon", "open"),
      channel("ch-quiet", "quiet", "read-only"),
    ]),
  ],
};

describe("enclave layout sidebar", () => {
  it("renders every category and channel with no two-category or five-channel ceiling", () => {
    const markup = renderEnclaveLayout(model, new Set());
    expect(model.categories).toHaveLength(4);
    expect(renderedChannelOrder(model)).toHaveLength(9);
    for (const group of model.categories) {
      expect(markup).toContain(`data-category-id="${group.categoryId}"`);
      for (const row of group.channels) {
        expect(markup).toContain(`data-channel-id="${row.channelId}"`);
        expect(markup).toContain(`#${row.name}`);
      }
    }
  });

  it("labels every shipped mode and no other", () => {
    const markup = renderEnclaveLayout(model, new Set());
    expect([...SHIPPED_CHANNEL_MODES]).toEqual(["open", "read-only", "stewards"]);
    for (const mode of SHIPPED_CHANNEL_MODES) {
      expect(markup).toContain(`data-mode="${mode}"`);
      expect(markup).toContain(CHANNEL_MODE_LABELS[mode]);
    }
    expect(CHANNEL_MODE_LABELS.stewards).toBe("STEWARDS");
    expect(CHANNEL_MODE_LABELS["read-only"]).toBe("READ ONLY");
  });

  it("renders the channels of an expanded category and hides a collapsed one", () => {
    const collapsed = renderEnclaveLayout(model, new Set(["cat-field"]));
    expect(collapsed).toContain('data-collapse-category="cat-field" aria-expanded="false"');
    expect(collapsed).toContain('data-collapse-category="cat-commons" aria-expanded="true"');
    expect(collapsed).not.toContain('data-channel-id="ch-dispatch"');
    expect(collapsed).toContain('data-channel-id="ch-welcome"');
  });
});

describe("collapse state is this member's, on this device", () => {
  it("keeps two members on one device apart and survives a restart", () => {
    const store = new MemoryStore();
    toggleCollapsedCategory(store, "member-ada", "cat-commons");
    toggleCollapsedCategory(store, "member-bo", "cat-field");
    toggleCollapsedCategory(store, "member-bo", "cat-archive");

    expect([...readCollapsedCategories(store, "member-ada")]).toEqual(["cat-commons"]);
    expect([...readCollapsedCategories(store, "member-bo")].sort()).toEqual([
      "cat-archive",
      "cat-field",
    ]);
    expect([...readCollapsedCategories(store, "member-cy")]).toEqual([]);
    expect(store.keys()).toEqual([
      collapseStorageKey("member-ada"),
      collapseStorageKey("member-bo"),
    ]);

    const afterRestart = store.reopen();
    expect([...readCollapsedCategories(afterRestart, "member-ada")]).toEqual(["cat-commons"]);
    expect([...readCollapsedCategories(afterRestart, "member-bo")].sort()).toEqual([
      "cat-archive",
      "cat-field",
    ]);
    expect([...readCollapsedCategories(afterRestart, "member-cy")]).toEqual([]);
  });

  it("folds a category open again without disturbing the other member", () => {
    const store = new MemoryStore();
    writeCollapsedCategories(store, "member-ada", new Set(["cat-commons"]));
    writeCollapsedCategories(store, "member-bo", new Set(["cat-commons"]));
    expect(toggleCollapsedCategory(store, "member-ada", "cat-commons")).toBe(false);
    expect([...readCollapsedCategories(store, "member-ada")]).toEqual([]);
    expect([...readCollapsedCategories(store, "member-bo")]).toEqual(["cat-commons"]);
  });

  it("treats damaged local state as nothing collapsed rather than throwing", () => {
    const store = new MemoryStore();
    store.setItem(collapseStorageKey("member-ada"), "{not json");
    expect([...readCollapsedCategories(store, "member-ada")]).toEqual([]);
  });
});

describe("reordering is authoritative, never visual", () => {
  it("renders whatever order the backend returned", () => {
    const moved: EnclaveLayoutModel = {
      categories: [
        category("cat-commons", "Commons", [
          channel("ch-notices", "notices", "read-only"),
          channel("ch-welcome", "welcome", "open"),
        ]),
      ],
    };
    const port = { reorderChannel: () => moved };
    const next = reorderChannel(port, "ch-notices", 0);
    expect(renderedChannelOrder(next)).toEqual(["ch-notices", "ch-welcome"]);
    const markup = renderEnclaveLayout(next, new Set());
    expect(markup.indexOf("ch-notices")).toBeLessThan(markup.indexOf("ch-welcome"));
  });

  it("leaves the list where it was when the backend refused the reorder", () => {
    const before: EnclaveLayoutModel = {
      categories: [
        category("cat-commons", "Commons", [
          channel("ch-welcome", "welcome", "open"),
          channel("ch-notices", "notices", "read-only"),
        ]),
      ],
    };
    // A refused reorder returns the model unchanged; the view must not have
    // moved anything on its own in the meantime.
    const port = { reorderChannel: () => before };
    const next = reorderChannel(port, "ch-notices", 0);
    expect(renderedChannelOrder(next)).toEqual(["ch-welcome", "ch-notices"]);
  });

  it("detects a rendered order the model does not have", () => {
    const authoritative = renderedChannelOrder(model);
    expect(isVisualOnlyReorder(model, authoritative)).toBe(false);
    const permuted = [...authoritative];
    [permuted[0], permuted[1]] = [permuted[1], permuted[0]];
    expect(isVisualOnlyReorder(model, permuted)).toBe(true);
    expect(isVisualOnlyReorder(model, authoritative.slice(1))).toBe(true);
  });
});

describe("the permission grid shows inherited apart from overridden", () => {
  const rows = [
    {
      roleId: "role-steward",
      roleName: "Stewards",
      authority: true,
      cells: [
        cell("read", true, "inherited", "STEWARDS"),
        cell("post", true, "inherited", "STEWARDS"),
        cell("manage", true, "inherited", "STEWARDS"),
      ],
    },
    {
      roleId: "role-guest",
      roleName: "Guest",
      authority: false,
      cells: [
        cell("read", true, "overridden", "Guest is allowed to read here"),
        cell("post", false, "overridden", "Guest is denied posting here"),
        cell("manage", false, "inherited", "STEWARDS"),
      ],
    },
  ];

  it("renders a cell per role and action with its source", () => {
    const markup = renderPermissionGrid(rows);
    expect([...CHANNEL_ACTIONS]).toEqual(["read", "post", "manage"]);
    for (const row of rows) {
      expect(markup).toContain(`data-role-id="${row.roleId}"`);
      for (const each of row.cells) {
        expect(markup).toContain(`data-action="${each.action}" data-allowed="${each.allowed}" data-source="${each.source}"`);
      }
    }
    expect(markup).toContain(ACCESS_SOURCE_LABELS.inherited);
    expect(markup).toContain(ACCESS_SOURCE_LABELS.overridden);
    expect(markup).toContain("Guest is denied posting here");
    expect(markup).toContain("Carries authority");
  });

  it("refuses to render a row that is missing an action rather than guessing", () => {
    expect(() => renderPermissionGrid([{ ...rows[1], cells: rows[1].cells.slice(0, 2) }]))
      .toThrow(/missing manage/);
  });
});

describe("deleting a channel cannot orphan its messages", () => {
  const doomed = channel("ch-doomed", "doomed", "open", 3);
  const haven = channel("ch-haven", "haven", "open");

  it("refuses a nonempty channel with neither a destination nor a burn", () => {
    const plan = planChannelDeletion({ channel: doomed, destinations: [haven] });
    expect(plan.ok).toBe(false);
    expect(deletionRefusalWords(plan)).toBe(
      '"doomed" still holds 3 messages. Choose a channel to move them to, or burn them on purpose.',
    );
    expect(renderDeleteChannelDialog({ channel: doomed, destinations: [haven] }))
      .toContain("data-delete-confirm disabled");
  });

  it("accepts a surviving destination", () => {
    const plan = planChannelDeletion({
      channel: doomed,
      destinations: [haven],
      chosenDestinationId: "ch-haven",
    });
    expect(plan).toEqual({ ok: true, disposition: { kind: "move", channelId: "ch-haven" } });
  });

  it("refuses a destination that is missing or is the channel itself", () => {
    expect(planChannelDeletion({
      channel: doomed,
      destinations: [haven],
      chosenDestinationId: "ch-gone",
    })).toEqual({ ok: false, reason: "destination-missing" });
    expect(planChannelDeletion({
      channel: doomed,
      destinations: [haven],
      chosenDestinationId: "ch-doomed",
    })).toEqual({ ok: false, reason: "destination-is-the-channel" });
  });

  it("requires the burn to be confirmed on purpose", () => {
    expect(planChannelDeletion({ channel: doomed, destinations: [haven], burnRequested: true }))
      .toEqual({ ok: false, reason: "burn-not-confirmed" });
    expect(planChannelDeletion({
      channel: doomed,
      destinations: [haven],
      burnRequested: true,
      burnConfirmed: true,
    })).toEqual({ ok: true, disposition: { kind: "burn" } });
  });

  it("lets an empty channel go without a destination", () => {
    expect(planChannelDeletion({ channel: haven, destinations: [doomed] }))
      .toEqual({ ok: true, disposition: { kind: "none" } });
  });
});

describe("the layout is reachable from the shipped Enclaves surface", () => {
  it("renders the converged tree and the measured limit on the Enclaves route", async () => {
    const { oslEnclavesSurfaceMarkup } = await import("./osl-enclaves");
    const markup = oslEnclavesSurfaceMarkup({
      statusTag: (label) => `<span>${label}</span>`,
      layout: model,
      collapsedCategories: new Set(["cat-archive"]),
      measuredLimit: {
        budgetBytes: 49152,
        categories: 192,
        channels: 156,
        logBytes: 48844,
        bytesPerChannel: 311,
      },
    });
    expect(markup).toContain(">OSL Enclaves</h1>");
    expect(markup).toContain('data-category-id="cat-commons"');
    expect(markup).toContain('data-collapse-category="cat-archive" aria-expanded="false"');
    expect(markup).not.toContain('data-channel-id="ch-cold"');
    expect(markup).toContain("192 categories and 156 channels");
    expect(markup).not.toContain("style=");
  });

  it("renders unchanged when no Enclave is open", async () => {
    const { oslEnclavesSurfaceMarkup } = await import("./osl-enclaves");
    const markup = oslEnclavesSurfaceMarkup({ statusTag: (label) => `<span>${label}</span>` });
    expect(markup).not.toContain("enclave-layout");
    expect(markup).not.toContain("enclave-measured-limit");
  });
});

describe("the resource limit shown is the one that was measured", () => {
  it("renders the measured numbers and calls them measured", () => {
    const markup = renderMeasuredLimit({
      budgetBytes: 49152,
      categories: 192,
      channels: 156,
      logBytes: 48844,
      bytesPerChannel: 311,
    });
    expect(markup).toContain("Measured on this device");
    expect(markup).toContain("192 categories and 156 channels");
    expect(markup).toContain("measured limit, not a fixed ceiling");
    expect(markup).not.toContain("at most two categories");
  });

  it("refuses to invent a limit it was not given", () => {
    expect(() => renderMeasuredLimit({
      budgetBytes: Number.NaN,
      categories: 2,
      channels: 5,
      logBytes: 0,
      bytesPerChannel: 0,
    })).toThrow(/needs a real budgetBytes/);
  });
});
