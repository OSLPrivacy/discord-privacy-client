import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const read = (path: string): string => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");

describe("dedicated WhatsApp QA build surface", () => {
  it("routes the retained main window only to the dedicated local entry", () => {
    const config = JSON.parse(read("../../osl-hub/tauri.conf.json")) as { app: { windows: Array<{ label: string; url: string; focus: boolean }> } };
    expect(config.app.windows).toHaveLength(1);
    expect(config.app.windows[0]).toMatchObject({ label: "main", url: "whatsapp-qa.html", focus: false });
    expect(read("../whatsapp-qa.html")).toContain('src="/src/whatsapp-qa.ts"');
  });

  it("has one automatic readiness sentinel and no password or setup surface", () => {
    const source = read("./whatsapp-qa.ts");
    expect(source).toContain('id="whatsapp-qa-ready"');
    expect(source).toContain("readiness.unlocked && readiness.identityLoaded");
    expect(source).toContain("void claim()");
    expect(source).not.toMatch(/type="password"|<form|unlockHubPasswordGate|createHubOslIdentity|importHubOslIdentity|setupHubMainPassword/iu);
    expect(source).not.toMatch(/host-focus|host-resize|host-detach|Focus claimed window|Realign|Detach safely/iu);
  });

  it("has no browser, setup, install, provider, credential, or provider-send authority", () => {
    const source = read("./whatsapp-qa.ts");
    expect(source).not.toMatch(/https?:\/\//u);
    expect(source).not.toMatch(/\bfetch\s*\(|XMLHttpRequest|WebSocket|<iframe/iu);
    expect(source).not.toMatch(/createHubOslIdentity|importHubOslIdentity|setupHubMainPassword|installNativeApp|openServiceHost/iu);
    expect(source).not.toMatch(/prepare.*protected|encrypt|decryptHub|sendMessage|attachment.*invoke/iu);
    expect(source.match(/\binvoke(?:<[^>]+>)?\s*\(/g)).toHaveLength(4);
    expect(source).toContain('invoke("open_whatsapp_qa_protected_text", { coverText })');
    expect(source).toContain("does not read WhatsApp or your clipboard automatically");
    expect(source).toContain("OSL controls appear only after exact chat verification");
    expect(source).not.toMatch(/Protect text|Burn|Covertext|Image \+ caption|File \+ caption/u);
  });

  it("keeps screenshot calibration explicit, bounded, and fail closed", () => {
    const source = read("./whatsapp-qa.ts");
    const receipts = read("./whatsapp-visual-binding.ts");
    expect(source).toContain("Bind current chat");
    expect(source).toContain("begin_whatsapp_visual_binding");
    expect(source).toContain("confirm_whatsapp_visual_binding");
    expect(source).toContain("attested: true");
    expect(source).not.toContain("VITE_WHATSAPP_QA_AUTO_BIND");
    expect(source).toContain("if (!nativeWindowClaimed");
    expect(source).toContain("Protected controls remain locked");
    expect(receipts).toContain('"accountHeader"');
    expect(receipts).toContain('"chatHeader"');
    expect(receipts).toContain('"composer"');
    expect(receipts).toContain('"transcript"');
    expect(receipts).toContain("contentPersisted: false");
    expect(receipts).toContain("privateStorageRead: false");
    expect(receipts).toContain("foregroundChanged: false");
    const opened = read("./whatsapp-protected-open.ts");
    expect(opened).toContain("providerStorageRead: false");
    expect(opened).toContain("providerHistoryChanged: false");
  });
});
