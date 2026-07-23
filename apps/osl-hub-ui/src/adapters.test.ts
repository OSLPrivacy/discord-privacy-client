import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  activateLocalLoopbackContext,
  activateManualPeerContext,
  activateNativeManualPeerContext,
  decryptHubCapsule,
  decryptLocalProtectedText,
  copyHubFriendInvite,
  isHubPlaintext,
  isLocalProtectedPlaintext,
  openHubAttachment,
  openPeerProseText,
  parseDecryptedLocalProtectedText,
  parseDecryptedHubPlaintext,
  parseFriendProfile,
  parseHubPerson,
  parseHubServiceBurnReadiness,
  parseHubServiceBurnResult,
  parseFullCleanup,
  parseLocalPrivacyScan,
  parseLocalLoopbackContext,
  parseLocalProtectedText,
  parseManualPeerContext,
  parseNotifications,
  parseOpenedHubAttachment,
  parseOpenedPeerProseText,
  parsePreparedEncryptedText,
  parsePreparedHubAttachment,
  parsePreparedPeerProseText,
  prepareHubAttachment,
  prepareLocalProtectedText,
  preparePeerProseText,
  prepareEncryptedText,
  setHubFriendNickname,
  setActiveHubFriendPermission,
  setLocalProtectedSheetOpen,
  setNativeDiscordProtectedOverlayOpen,
  setNativeDiscordProtectedOverlayOpenForQa,
} from "./adapters";

describe("optional OSL Privacy adapters", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("validates friend and notification responses exactly", () => {
    const profile = {
      friendCode: "OSLFR1.ABCDEFGHIJKLMNOP",
      oslUserId: "osl-user-1",
      safetyNumber: "1234 5678",
    };
    expect(parseFriendProfile(profile)).toEqual(profile);
    expect(parseFriendProfile({ ...profile, friendCode: "bad code", extra: true })).toBeNull();
    expect(parseNotifications([{ id: "1", title: "Update", detail: "Ready", createdAt: "2026-07-16" }])).toHaveLength(1);
    expect(parseNotifications([{ id: "1", title: "<script>", detail: "Ready", createdAt: "now" }])).toBeNull();
  });

  it("copies a friend invite through the argument-free native command", async () => {
    const friendCode = "OSLFR1.ABCDEFGHIJKLMNOP";
    mocks.invoke.mockResolvedValueOnce(undefined);
    await expect(copyHubFriendInvite(friendCode)).resolves.toBe(true);
    expect(mocks.invoke).toHaveBeenCalledWith("copy_hub_friend_invite");

    mocks.invoke.mockClear();
    await expect(copyHubFriendInvite("not-an-invite")).resolves.toBe(false);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("uses browser clipboard only outside the native runtime", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    mocks.isTauriRuntime.mockReturnValue(false);
    const friendCode = "OSLFR1.ABCDEFGHIJKLMNOP";
    await expect(copyHubFriendInvite(friendCode)).resolves.toBe(true);
    expect(writeText).toHaveBeenCalledWith(friendCode);
    expect(mocks.invoke).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("accepts only bounded local friend labels and explicit local scopes", async () => {
    const person = {
      personId: "hub-person-abc",
      oslUserId: "osl-opaque-identity",
      alias: "Rose",
      safetyNumber: "1234 5678",
      safetyNumberVerified: true,
      whitelistCount: 2,
      whitelistedScopes: [
        { kind: "dm", contextId: null },
        { kind: "group", contextId: "opaque-conversation" },
      ],
      whitelistedScopesTruncated: false,
      pendingKeyChange: false,
    };
    expect(parseHubPerson(person)).toEqual(person);
    expect(parseHubPerson({ ...person, linkedInstagram: "@rose" })).toBeNull();
    expect(parseHubPerson({ ...person, whitelistedScopes: [{ kind: "instagram", contextId: "rose" }] })).toBeNull();

    mocks.invoke.mockResolvedValueOnce({ ...person, alias: "Bestie" });
    await expect(setHubFriendNickname(person.personId, "  Bestie  ")).resolves.toMatchObject({ alias: "Bestie" });
    expect(mocks.invoke).toHaveBeenCalledWith("set_hub_friend_nickname", { personId: person.personId, nickname: "Bestie" });

    mocks.invoke.mockClear();
    await expect(setHubFriendNickname(person.personId, `bad\u{202e}name`)).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("never upgrades local loopback protection into E2EE", () => {
    const result = parseLocalProtectedText({
      capsule: "OSL1.local.capsule",
      localMessageId: "local-1",
      protection: "local_protected_loopback",
      personToPersonE2ee: false,
      statePersisted: true,
      viewOnce: true,
    });
    expect(result?.personToPersonE2ee).toBe(false);
    expect(result?.viewOnce).toBe(true);
    expect(parseLocalProtectedText({ ...result, personToPersonE2ee: true })).toBeNull();
    expect(parseLocalProtectedText({ ...result, viewOnce: "yes" })).toBeNull();
  });

  it("keeps view-once explicitly local and parses consumption truth exactly", async () => {
    const prepared = {
      capsule: "TE9DQUw=",
      localMessageId: "local-1",
      protection: "local_protected_loopback",
      personToPersonE2ee: false,
      statePersisted: true,
      viewOnce: true,
    } as const;
    const opened = {
      plaintext: "one look",
      localMessageId: "local-1",
      protection: "local_protected_loopback",
      personToPersonE2ee: false,
      contextVerified: true,
      viewOnceConsumed: true,
    } as const;
    mocks.invoke.mockResolvedValueOnce(prepared).mockResolvedValueOnce(opened);

    await expect(prepareLocalProtectedText("context-token", "one look", true)).resolves.toEqual(prepared);
    await expect(decryptLocalProtectedText("context-token", "TE9DQUw=")).resolves.toEqual(opened);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "prepare_local_protected_text_with_policy", {
      contextToken: "context-token", plaintext: "one look", viewOnce: true,
    });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "decrypt_local_protected_capsule", {
      contextToken: "context-token", capsule: "TE9DQUw=",
    });
    expect(parseDecryptedLocalProtectedText({ ...opened, screenshotPrevented: true })).toBeNull();
    expect(parseDecryptedLocalProtectedText({ ...opened, contextVerified: false })).toBeNull();
  });

  it("binds only an exact opaque local loopback context", async () => {
    const lease = {
      contextToken: "ctx-1-abcdef",
      serviceId: "discord",
      accountId: "account-1",
      conversationId: "local-00112233445566778899aabbccddeeff",
    };
    mocks.invoke.mockResolvedValueOnce(lease).mockResolvedValueOnce(true);

    await expect(activateLocalLoopbackContext(
      lease.serviceId,
      lease.accountId,
      lease.conversationId,
    )).resolves.toEqual(lease);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "activate_local_loopback_context", {
      serviceId: lease.serviceId,
      accountId: lease.accountId,
      conversationId: lease.conversationId,
    });
    await expect(setLocalProtectedSheetOpen(true)).resolves.toBe(true);
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "set_local_protected_sheet_open", { open: true });
    expect(parseLocalLoopbackContext({ ...lease, chatLabel: "Rose" })).toBeNull();

    mocks.invoke.mockClear();
    await expect(activateLocalLoopbackContext("discord", "../account", lease.conversationId)).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("rejects a native loopback lease that changes the requested context", async () => {
    mocks.invoke.mockResolvedValueOnce({
      contextToken: "ctx-1-abcdef",
      serviceId: "discord",
      accountId: "account-1",
      conversationId: "local-ffffffffffffffffffffffffffffffff",
    });
    await expect(activateLocalLoopbackContext(
      "discord",
      "account-1",
      "local-00112233445566778899aabbccddeeff",
    )).resolves.toBeNull();
  });

  it("binds manual peer protection only to the exact requested friend and explicit approval", async () => {
    const context = {
      contextToken: "ctx.peer-1",
      serviceId: "discord",
      accountId: "account-1",
      personId: "person-1",
      peerOslUserId: "osl-user-2",
      scopeApproved: false,
    };
    mocks.invoke.mockResolvedValueOnce(context).mockResolvedValueOnce(undefined);

    await expect(activateManualPeerContext("discord", "account-1", "person-1")).resolves.toEqual(context);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "activate_manual_peer_context", {
      serviceId: "discord",
      accountId: "account-1",
      personId: "person-1",
    });
    expect(mocks.invoke).toHaveBeenCalledTimes(1);

    await expect(setActiveHubFriendPermission(context.contextToken, context.personId, true, false)).resolves.toBe(true);
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "set_active_hub_friend_permission", {
      contextToken: context.contextToken,
      personId: context.personId,
      enabled: true,
      broadened: false,
    });
    expect(parseManualPeerContext({ ...context, exactConversationVerified: true })).toBeNull();
    expect(parseManualPeerContext({ ...context, scopeApproved: "yes" })).toBeNull();
  });

  it("binds native protection to the exact friend and opens only the OSL overlay", async () => {
    const context = {
      contextToken: "ctx.native-peer-1",
      serviceId: "discord",
      accountId: "native-account-1",
      personId: "person-1",
      peerOslUserId: "osl-user-2",
      scopeApproved: true,
    };
    mocks.invoke.mockResolvedValueOnce(context).mockResolvedValueOnce(true).mockResolvedValueOnce(true);
    await expect(activateNativeManualPeerContext("person-1")).resolves.toEqual(context);
    await expect(setNativeDiscordProtectedOverlayOpen(context.contextToken, true)).resolves.toBe(true);
    await expect(setNativeDiscordProtectedOverlayOpen(context.contextToken, false)).resolves.toBe(true);
    expect(mocks.invoke.mock.calls).toEqual([
      ["activate_native_manual_peer_context", { personId: "person-1" }],
      ["set_native_discord_protected_overlay_open", { contextToken: context.contextToken, open: true }],
      ["set_native_discord_protected_overlay_open", { contextToken: context.contextToken, open: false }],
    ]);

    mocks.invoke.mockClear();
    await expect(activateNativeManualPeerContext("person\0bad")).resolves.toBeNull();
    await expect(setNativeDiscordProtectedOverlayOpen("../context", true)).resolves.toBe(false);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("preserves the exact bounded native overlay rejection for the QA shell", async () => {
    mocks.invoke.mockRejectedValueOnce("The native Discord window changed before protection opened");
    await expect(setNativeDiscordProtectedOverlayOpenForQa("ctx.native-peer-1")).resolves.toEqual({
      opened: false,
      error: "The native Discord window changed before protection opened",
    });
    expect(mocks.invoke).toHaveBeenCalledWith("set_native_discord_protected_overlay_open", {
      contextToken: "ctx.native-peer-1",
      open: true,
    });
  });

  it("accepts only explicit person-to-person prepare and open claims", async () => {
    const prepared = {
      coverText: "OSL1.PEER.protected",
      expiresAt: 1_787_000_000,
      personToPersonE2ee: true,
      viewOnce: true,
    } as const;
    const opened = {
      plaintext: "hello",
      contextVerified: true,
      personToPersonE2ee: true,
      viewOnceConsumed: true,
      requireCaptureProtection: true,
    } as const;
    mocks.invoke.mockResolvedValueOnce(prepared).mockResolvedValueOnce(opened);

    await expect(preparePeerProseText("ctx.peer-1", "hello", true)).resolves.toEqual(prepared);
    await expect(openPeerProseText("ctx.peer-1", "person-1", prepared.coverText)).resolves.toEqual(opened);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "prepare_peer_prose_text", {
      contextToken: "ctx.peer-1",
      plaintext: "hello",
      viewOnce: true,
    });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "open_peer_prose_text", {
      contextToken: "ctx.peer-1",
      senderPersonId: "person-1",
      coverText: prepared.coverText,
    });
    expect(parsePreparedPeerProseText({ ...prepared, personToPersonE2ee: false })).toBeNull();
    expect(parsePreparedPeerProseText({ ...prepared, expiresAt: -1 })).toBeNull();
    expect(parsePreparedPeerProseText({ ...prepared, viewOnce: "yes" })).toBeNull();
    expect(parseOpenedPeerProseText({ ...opened, contextVerified: false })).toBeNull();
    expect(parseOpenedPeerProseText({ ...opened, personToPersonE2ee: false })).toBeNull();
    expect(parseOpenedPeerProseText({ ...opened, viewOnceConsumed: "yes" })).toBeNull();
    expect(parseOpenedPeerProseText({ ...opened, providerMessageDeleted: true })).toBeNull();
  });

  it("rejects malformed manual peer inputs before IPC", async () => {
    await expect(activateManualPeerContext("discord", "../account", "person-1")).resolves.toBeNull();
    await expect(activateManualPeerContext("Discord!", "account-1", "person-1")).resolves.toBeNull();
    await expect(preparePeerProseText("../context", "hello", false)).resolves.toBeNull();
    await expect(preparePeerProseText("ctx.peer-1", `${"🙂".repeat(251)}x`, false)).resolves.toBeNull();
    await expect(openPeerProseText("ctx.peer-1", "<person>", "OSL1.PEER.protected")).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("prepares and opens bounded attachments without inventing upload authority", async () => {
    const sealedB64 = "QUJDRA==";
    const prepared = {
      sealedB64,
      transportFilename: `osl-${"a".repeat(32)}.mp4`,
      transportMimeType: "video/mp4",
      originalMimeType: "image/png",
      ciphertextPrepared: true,
      automaticServiceUpload: false,
    } as const;
    const opened = {
      plaintextB64: "UE5HAA==",
      originalFilename: "photo.png",
      mimeType: "image/png",
      contextVerified: true,
    } as const;
    mocks.invoke.mockResolvedValueOnce(prepared).mockResolvedValueOnce(opened);

    await expect(prepareHubAttachment("context-token", "UE5HAA==", "photo.png")).resolves.toEqual(prepared);
    await expect(openHubAttachment("context-token", "osl-user", "message-1", sealedB64)).resolves.toEqual(opened);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "prepare_hub_attachment", {
      contextToken: "context-token", originalBytesB64: "UE5HAA==", originalFilename: "photo.png",
    });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "open_hub_attachment", {
      contextToken: "context-token", senderOslId: "osl-user", serviceMessageId: "message-1", sealedB64,
    });
    expect(parsePreparedHubAttachment({ ...prepared, automaticServiceUpload: true })).toBeNull();
    expect(parseOpenedHubAttachment({ ...opened, remoteUrl: "https://example.test" })).toBeNull();
  });

  it("rejects malformed attachment and view-once input before IPC", async () => {
    await expect(prepareHubAttachment("context-token", "not base64", "photo.png")).resolves.toBeNull();
    await expect(prepareHubAttachment("context-token", "UE5HAA==", `x${String.fromCharCode(0)}.png`)).resolves.toBeNull();
    await expect(openHubAttachment("context-token", "../sender", null, "QUJDRA==")).resolves.toBeNull();
    await expect(prepareLocalProtectedText("../context", "secret", true)).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("matches the native broker's 1000-byte UTF-8 plaintext limit", () => {
    expect(isLocalProtectedPlaintext("a".repeat(1_000))).toBe(true);
    expect(isLocalProtectedPlaintext("a".repeat(1_001))).toBe(false);
    expect(isLocalProtectedPlaintext("🔐".repeat(250))).toBe(true);
    expect(isLocalProtectedPlaintext("🔐".repeat(251))).toBe(false);
    expect(isHubPlaintext("🔐".repeat(250))).toBe(true);
    expect(isHubPlaintext("🔐".repeat(251))).toBe(false);
  });

  it("validates prepared E2EE responses exactly and within bounded wire limits", () => {
    const prepared = {
      messages: ["DPC0::content"],
      controlMessages: ["DPC0::control"],
      sessionId: null,
    };
    expect(parsePreparedEncryptedText(prepared)).toEqual(prepared);
    expect(parsePreparedEncryptedText({ ...prepared, authority: "remote" })).toBeNull();
    expect(parsePreparedEncryptedText({ ...prepared, messages: [] })).toBeNull();
    expect(parsePreparedEncryptedText({ ...prepared, sessionId: 0x1_0000_0000 })).toBeNull();
    expect(parsePreparedEncryptedText({ ...prepared, messages: ["x".repeat(256 * 1024 + 1)] })).toBeNull();
  });

  it("validates decrypted plaintext as a bounded scalar, never an expanded object", () => {
    expect(parseDecryptedHubPlaintext("hello\nthere")).toBe("hello\nthere");
    expect(parseDecryptedHubPlaintext("x".repeat(1_001))).toBeNull();
    expect(parseDecryptedHubPlaintext({ plaintext: "hello", remoteAuthority: true })).toBeNull();
  });

  it("invokes only the two context-bound trusted broker commands", async () => {
    mocks.invoke
      .mockResolvedValueOnce({ messages: ["DPC0::content"], controlMessages: [], sessionId: null })
      .mockResolvedValueOnce("secret text");

    await expect(prepareEncryptedText("context-token", "secret text")).resolves.toEqual({
      messages: ["DPC0::content"],
      controlMessages: [],
      sessionId: null,
    });
    await expect(decryptHubCapsule("context-token", "osl-user", "service-message", "DPC0::content")).resolves.toBe("secret text");
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "prepare_encrypted_text", {
      contextToken: "context-token",
      plaintext: "secret text",
    });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "decrypt_hub_capsule", {
      contextToken: "context-token",
      senderOslId: "osl-user",
      serviceMessageId: "service-message",
      capsule: "DPC0::content",
    });
  });

  it("fails closed before IPC for malformed broker inputs", async () => {
    await expect(prepareEncryptedText("context-token", "x".repeat(1_001))).resolves.toBeNull();
    await expect(decryptHubCapsule("context-token", "osl-user", null, "x".repeat(256 * 1024 + 1))).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("preserves partial cleanup and remote-unregister truth", () => {
    const result = parseFullCleanup({
      localCleanupComplete: false,
      removedTargets: ["hub_core"],
      failedTargets: ["service_profiles"],
      remoteUnregister: { identitiesFound: 2, succeeded: 1, failed: 1, unavailable: 0 },
      restartRequired: true,
      originalDiscordDataUntouched: true,
    });
    expect(result?.failedTargets).toEqual(["service_profiles"]);
    expect(result?.remoteUnregister.failed).toBe(1);
    expect(parseFullCleanup({ ...result, remoteUnregister: { failed: -1 } })).toBeNull();
  });

  it("accepts service burn evidence only with complete, bounded native truth fields", () => {
    const burnId = "a".repeat(64);
    const readiness = { burnId, manifestDigest: "b".repeat(64), indexedScopes: 3, coverageComplete: true, loginProfileUntouched: true, nativeHistoryUntouched: true };
    expect(parseHubServiceBurnReadiness(readiness)?.coverageComplete).toBe(true);
    expect(parseHubServiceBurnReadiness({ ...readiness, coverageComplete: "yes" })).toBeNull();
    const result = { burnId, scopesBurned: 3, rowsDestroyed: 4, whitelistEntriesRemoved: 2, remoteBlobsDeleted: 1, remoteBlobDeletionsFailed: 0, localCleanupComplete: true, remoteCleanupComplete: true, loginProfileUntouched: true, nativeHistoryUntouched: true };
    expect(parseHubServiceBurnResult(result)?.loginProfileUntouched).toBe(true);
    expect(parseHubServiceBurnResult({ ...result, loginProfileUntouched: false })).toBeNull();
  });

  it("accepts only bounded, local-only privacy findings", () => {
    const result = parseLocalPrivacyScan({
      findings: [{
        serviceId: "instagram",
        accountId: "qa-account",
        conversationId: "conversation-1",
        messageLocator: "message-1",
        authoredBySelf: true,
        createdAtUnixMs: 1_700_000_000_000,
        category: "credential",
        confidence: 94,
        reason: "This looks like a credential.",
        localPreview: "password: example",
        canRequestDelete: true,
        attachmentPath: null,
      }],
      messagesScanned: 1,
      messagesRejected: 0,
      truncated: false,
      analysisLocation: "this_device_only",
      persisted: false,
      attachmentsScanned: 0,
      imagesChecked: false,
      videosChecked: false,
      attachmentTypesScanned: [],
      uninspectedAttachments: [],
    });
    expect(result?.analysisLocation).toBe("this_device_only");
    expect(parseLocalPrivacyScan({ ...result, analysisLocation: "cloud" })).toBeNull();
    expect(parseLocalPrivacyScan({ ...result, findings: [{ ...result?.findings[0], authoredBySelf: false, canRequestDelete: true }] })).toBeNull();
  });

  it("accepts only the bounded context-review category allowlist", () => {
    for (const category of ["profanity", "sexual_content", "sensitive_health", "controlled_substances", "potentially_unlawful_conduct", "work_sensitive_information"]) {
      const result = parseLocalPrivacyScan({
        findings: [{
          serviceId: "instagram", accountId: "qa", conversationId: "chat", messageLocator: "message",
          authoredBySelf: true, createdAtUnixMs: null, category, confidence: 70,
          reason: "Review this message in context.", localPreview: "local", canRequestDelete: true, attachmentPath: null,
        }],
        messagesScanned: 1, messagesRejected: 0, truncated: false,
        analysisLocation: "this_device_only", persisted: false,
        attachmentsScanned: 0, imagesChecked: false, videosChecked: false,
        attachmentTypesScanned: [], uninspectedAttachments: [],
      });
      expect(result?.findings[0]?.category).toBe(category);
    }
    const invalid = parseLocalPrivacyScan({
      findings: [{
        serviceId: "instagram", accountId: "qa", conversationId: "chat", messageLocator: "message",
        authoredBySelf: true, createdAtUnixMs: null, category: "criminal_verdict", confidence: 100,
        reason: "Bad category.", localPreview: "local", canRequestDelete: true, attachmentPath: null,
      }],
      messagesScanned: 1, messagesRejected: 0, truncated: false,
      analysisLocation: "this_device_only", persisted: false,
      attachmentsScanned: 0, imagesChecked: false, videosChecked: false,
      attachmentTypesScanned: [], uninspectedAttachments: [],
    });
    expect(invalid).toBeNull();
  });

  it("preserves honest uninspected attachment reasons and rejects invented capability output", () => {
    const scan = {
      findings: [], messagesScanned: 1, messagesRejected: 0, truncated: false,
      analysisLocation: "this_device_only", persisted: false,
      attachmentsScanned: 1, imagesChecked: false, videosChecked: false,
      attachmentTypesScanned: ["png"],
      uninspectedAttachments: [{
        attachmentId: "photo", path: "photo.png", detectedType: "png",
        reason: "model_not_installed", detail: "Install the verified local image model pack.",
      }],
    } as const;
    expect(parseLocalPrivacyScan(scan)?.uninspectedAttachments[0]?.reason).toBe("model_not_installed");
    expect(parseLocalPrivacyScan({ ...scan, uninspectedAttachments: [{ ...scan.uninspectedAttachments[0], reason: "clean" }] })).toBeNull();
  });
});
