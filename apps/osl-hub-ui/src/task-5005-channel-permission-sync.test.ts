import { describe, expect, it } from "vitest";

import {
  EnclaveChannelPermissionEditor,
  channelPermissionSyncState,
  enclaveChannelPermissionEditorMarkup,
  mountEnclaveChannelPermissionEditor,
  type EnclaveChannelPermissionEditorSnapshot,
} from "./osl-enclave-channel-permissions";
import { oslEnclavesSurfaceMarkup } from "./osl-enclaves";

function fixture(): EnclaveChannelPermissionEditorSnapshot {
  return {
    channel: {
      channelId: "channel-general",
      name: "general",
      topic: "Plans, notes, and everyday conversation",
      position: 3,
      categoryId: "category-community",
      categoryName: "Community",
    },
    permissions: [
      { key: "member.send", label: "Send messages", categoryAnswer: "allow" },
      { key: "member.files", label: "Send files", categoryAnswer: "deny" },
      { key: "mod.pin", label: "Pin messages", categoryAnswer: "allow" },
    ],
    overrides: {},
  };
}

describe("TASK 5005 channel permission sync state", () => {
  it("shows a zero-override channel as synced with its category", () => {
    const snapshot = fixture();
    const sync = channelPermissionSyncState(snapshot);
    const markup = enclaveChannelPermissionEditorMarkup(snapshot);

    expect(sync).toEqual({ kind: "synced", overrideCount: 0, label: "Synced with category" });
    expect(markup).toContain('data-permission-sync="synced"');
    expect(markup).toContain('data-override-count="0"');
    expect(markup).toContain("Synced with category");
    expect(markup).toContain("Match category");
    console.log(`TASK5005 initial label="${sync.label}" override_count=${sync.overrideCount}`);
  });

  it("shows two changed permissions, then one Match category action returns both", () => {
    const editor = new EnclaveChannelPermissionEditor(fixture());
    editor.setChannelAnswer("member.send", "deny");
    editor.setChannelAnswer("member.files", "allow");

    const changed = editor.syncState();
    const changedMarkup = enclaveChannelPermissionEditorMarkup(editor.snapshot());
    expect(changed).toEqual({
      kind: "changed",
      overrideCount: 2,
      label: "Changed from category · 2 permissions",
    });
    expect(changedMarkup).toContain('data-permission-sync="changed"');
    expect(changedMarkup).toContain('data-override-count="2"');
    expect(changedMarkup).not.toContain('data-permission-sync="synced"');

    editor.matchCategory();
    const matched = editor.syncState();
    expect(editor.snapshot().overrides).toEqual({});
    expect(matched).toEqual({ kind: "synced", overrideCount: 0, label: "Synced with category" });
    console.log(`TASK5005 changed label="${changed.label}" override_count=${changed.overrideCount}`);
    console.log(`TASK5005 match_category cleared=2 label="${matched.label}" override_count=${matched.overrideCount}`);
  });

  it("pressing the mounted Match category button clears both visible overrides", () => {
    const previousElement = globalThis.Element;
    class MatchCategoryButton {
      closest(selector: string): MatchCategoryButton | null {
        return selector === "[data-match-category]" ? this : null;
      }
    }
    Object.defineProperty(globalThis, "Element", { value: MatchCategoryButton, configurable: true });

    let click: ((event: Event) => void) | undefined;
    const root = {
      innerHTML: "",
      addEventListener(_type: "click", listener: (event: Event) => void) { click = listener; },
    };

    try {
      const editor = mountEnclaveChannelPermissionEditor(root, {
        ...fixture(),
        overrides: { "member.send": "deny", "member.files": "allow" },
      });
      expect(root.innerHTML).toContain("Changed from category · 2 permissions");

      click?.({ target: new MatchCategoryButton() } as unknown as Event);

      expect(editor.snapshot().overrides).toEqual({});
      expect(root.innerHTML).toContain("Synced with category");
      expect(root.innerHTML).toContain('data-override-count="0"');
      expect(root.innerHTML).not.toContain("Changed from category");
    } finally {
      Object.defineProperty(globalThis, "Element", { value: previousElement, configurable: true });
    }
  });

  it("never labels a real override as synced, including one-override grammar", () => {
    const snapshot = { ...fixture(), overrides: { "member.send": "deny" as const } };
    const sync = channelPermissionSyncState(snapshot);
    const markup = enclaveChannelPermissionEditorMarkup(snapshot);

    expect(sync.label).toBe("Changed from category · 1 permission");
    expect(sync.kind).toBe("changed");
    expect(markup).not.toContain('data-permission-sync="synced"');
  });

  it("category permission changes never change channel topic or position", () => {
    const editor = new EnclaveChannelPermissionEditor(fixture());
    editor.setChannelAnswer("member.send", "deny");
    const before = editor.snapshot().channel;

    editor.updateCategoryAnswers({
      "member.send": "allow",
      "member.files": "allow",
      "mod.pin": "deny",
    });

    const afterCategoryChange = editor.snapshot();
    expect(afterCategoryChange.channel).toEqual(before);
    expect(afterCategoryChange.channel.topic).toBe("Plans, notes, and everyday conversation");
    expect(afterCategoryChange.channel.position).toBe(3);
    expect(editor.syncState()).toMatchObject({ kind: "changed", overrideCount: 1 });

    editor.matchCategory();
    const afterMatch = editor.snapshot();
    expect(afterMatch.channel).toEqual(before);
    expect(editor.syncState()).toMatchObject({ kind: "synced", overrideCount: 0 });
    console.log(`TASK5005 identity topic="${afterMatch.channel.topic}" position=${afterMatch.channel.position} preserved=true`);
  });

  it("is rendered by the extracted Enclaves route when a channel is selected", () => {
    const markup = oslEnclavesSurfaceMarkup({
      statusTag: (label) => `<span>${label}</span>`,
      channelPermissions: fixture(),
    });

    expect(markup).toContain("#general permissions");
    expect(markup).toContain("Synced with category");
    expect(markup).toContain("Plans, notes, and everyday conversation");
    expect(markup).toContain("Position 3");
    expect(markup).not.toContain("style=");
  });
});
