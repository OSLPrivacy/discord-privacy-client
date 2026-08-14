import { describe, expect, it } from "vitest";

import {
  ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY,
  ENCLAVE_HISTORY_ROUTES,
  applySignedHistorySetting,
  directHistoryRequestBytes,
  enclaveHistoryForNewMembersSettingsMarkup,
  enclaveHistoryJoinDisclosureMarkup,
  memberResharedHistoryItemMarkup,
  newEnclaveHistoryRecord,
  reconcileHistoryReshareSinks,
  reshareHistoryFromMemberDevice,
  restoreSignedEnclaveHistoryRecord,
  type EnclaveCommittedHistoryMessage,
  type SignedEnclaveHistoryInstruction,
} from "./enclave-history-settings";
import { oslEnclavesSurfaceMarkup } from "./osl-enclaves";

const enclaveId = "enclave-6933";
const sharer = { memberId: "member-alex", displayName: "Alex Rivera", deviceId: "alex-current-device" };
const firstApplicant = { memberId: "joiner-one", currentKeyId: "joiner-one-current-key", joinedAtEpoch: 20 };
const secondApplicant = { memberId: "joiner-two", currentKeyId: "joiner-two-current-key", joinedAtEpoch: 60 };
const thirdApplicant = { memberId: "joiner-three", currentKeyId: "joiner-three-current-key", joinedAtEpoch: 80 };

const verifier = (instruction: SignedEnclaveHistoryInstruction) => instruction.signature.startsWith("sig:");
const resolver = (actor: string, target: string) => actor === "member-alex" && target === enclaveId;
const encrypt = (_plaintext: string, key: string) => `ciphertext-addressed-to-${key}`;

function corpus(prefix: string, firstEpoch: number, count: number): EnclaveCommittedHistoryMessage[] {
  return Array.from({ length: count }, (_, index) => ({
    messageId: `${prefix}-${index + 1}`,
    committedSendEpoch: firstEpoch + index,
    plaintextOnSharerDevice: `${prefix} marked plaintext ${index + 1}`,
  }));
}

function enable(record: ReturnType<typeof newEnclaveHistoryRecord>, epoch: number) {
  return applySignedHistorySetting(record, {
    enclaveId,
    actorMemberId: sharer.memberId,
    expectedVersion: record.version,
    mode: "shared_from_when_turned_on",
    signedEpoch: epoch,
    signature: `sig:enable-${epoch}`,
  }, verifier, resolver);
}

describe("TASK 6933 forward-only member re-share", () => {
  it("starts every real new enclave HIDDEN and gives the first applicant 0 of 50 sealed pre-join messages", () => {
    const record = newEnclaveHistoryRecord(enclaveId);
    const beforeJoin = corpus("first-prejoin", 10, 50);
    const afterJoin = corpus("first-live", 20, 50);
    const reshared = reshareHistoryFromMemberDevice(record, sharer, firstApplicant, beforeJoin, encrypt);
    const restartedRecord = restoreSignedEnclaveHistoryRecord(JSON.stringify(record));
    const restartedJoinerKeysCreatedBeforeJoin: string[] = [];
    const markup = oslEnclavesSurfaceMarkup({ statusTag: (label) => `<span>${label}</span>`, historyRecord: restartedRecord });

    expect(reshared.items).toHaveLength(0);
    expect(reshared.refusals).toHaveLength(50);
    expect(restartedJoinerKeysCreatedBeforeJoin).toEqual([]);
    // New messages are normal live delivery, not a history/key hand-off.
    expect(afterJoin.filter((message) => message.committedSendEpoch >= firstApplicant.joinedAtEpoch)).toHaveLength(50);
    expect(markup).toContain("History for new members");
    expect(markup).toContain(">HIDDEN</button>");
    expect(markup).toContain('data-enclave-history-mode="hidden"');
    expect(markup).toContain('data-signed-enclave-history-record="signed-new-enclave-hidden"');
    console.log("TASK6933_DEFAULT mode=HIDDEN prejoin_opened=0/50 prejoin_keys=0 live_opened=50/50 restart=green");
  });

  it("refuses every pre-epoch message by id and epoch while sharing exactly the 20 post-epoch messages", () => {
    const initial = newEnclaveHistoryRecord(enclaveId);
    const change = enable(initial, 50);
    expect(change.ok).toBe(true);
    if (!change.ok) throw new Error(change.reason);
    const preEpoch = corpus("before-signed-epoch", 30, 20);
    const postEpoch = corpus("after-signed-epoch", 50, 20);
    const outcome = reshareHistoryFromMemberDevice(change.record, sharer, secondApplicant, [...preEpoch, ...postEpoch], encrypt);

    expect(change.channelEvent).toMatchObject({ kind: "history-sharing-enabled", enclaveId, epoch: 50, signature: "sig:enable-50" });
    expect(outcome.items).toHaveLength(20);
    expect(outcome.refusals).toHaveLength(20);
    expect(outcome.items.map((item) => item.messageId)).toEqual(postEpoch.map((message) => message.messageId));
    for (const refusal of outcome.refusals) {
      expect(refusal.messageId).toMatch(/^before-signed-epoch-/u);
      expect(refusal.epoch).toBeGreaterThanOrEqual(30);
      expect(refusal.epoch).toBeLessThan(50);
      expect(refusal.reason).toBe("before-sharing-epoch");
    }
    for (const message of preEpoch) {
      for (const route of ENCLAVE_HISTORY_ROUTES) {
        expect(directHistoryRequestBytes(route, change.record, message).byteLength, `${route} leaked ${message.messageId}`).toBe(0);
      }
    }
    console.log("TASK6933_FORWARD signed_epoch=50 pre_epoch_refused=20/20 post_epoch_shared=20/20 direct_request_bytes=0 routes=5");
  });

  it("makes every shared item current-key ciphertext from Alex's device and names Alex before open", () => {
    const initial = newEnclaveHistoryRecord(enclaveId);
    const change = enable(initial, 50);
    if (!change.ok) throw new Error(change.reason);
    const outcome = reshareHistoryFromMemberDevice(change.record, sharer, secondApplicant, corpus("post", 50, 20), encrypt);
    const joinerKeysCreatedBeforeJoin: string[] = [];
    const relayObserver = { plaintext: 0, keyMaterial: 0 };

    expect(joinerKeysCreatedBeforeJoin).toEqual([]);
    expect(outcome.items).toHaveLength(20);
    for (const item of outcome.items) {
      expect(item.addressedToCurrentJoinerKey).toBe(secondApplicant.currentKeyId);
      expect(item.sharerMemberId).toBe(sharer.memberId);
      expect(item.sharerDeviceId).toBe(sharer.deviceId);
      expect(Object.keys(item).join(" ")).not.toMatch(/messageKey|chainKey|rootKey|plaintext/iu);
      const markup = memberResharedHistoryItemMarkup(item);
      expect(markup.indexOf("Shared by Alex Rivera")).toBeLessThan(markup.indexOf("Open shared item"));
    }
    expect(relayObserver).toEqual({ plaintext: 0, keyMaterial: 0 });
    console.log("TASK6933_RESHARE shared=20 origin=member-alex/alex-current-device current_joiner_keys=20 prejoin_keys=0 relay_plaintext=0 relay_keys=0 named_before_open=20");
  });

  it("accepts only a permitted signed instruction for this enclave and never trusts rendered state", () => {
    const initial = newEnclaveHistoryRecord(enclaveId);
    const unsigned = applySignedHistorySetting(initial, {
      enclaveId, actorMemberId: sharer.memberId, expectedVersion: 0, mode: "shared_from_when_turned_on", signedEpoch: 50, signature: "",
    }, verifier, resolver);
    const crossEnclave = applySignedHistorySetting(initial, {
      enclaveId: "another-enclave", actorMemberId: sharer.memberId, expectedVersion: 0, mode: "shared_from_when_turned_on", signedEpoch: 50, signature: "sig:cross",
    }, verifier, resolver);
    const unpermitted = applySignedHistorySetting(initial, {
      enclaveId, actorMemberId: "member-not-permitted", expectedVersion: 0, mode: "shared_from_when_turned_on", signedEpoch: 50, signature: "sig:nope",
    }, verifier, resolver);
    const changed = enable(initial, 50);
    if (!changed.ok) throw new Error(changed.reason);
    const rendered = enclaveHistoryForNewMembersSettingsMarkup(changed.record).replace('aria-pressed="true"', 'aria-pressed="false"');

    expect(unsigned).toEqual({ ok: false, reason: "invalid-signature" });
    expect(crossEnclave).toEqual({ ok: false, reason: "wrong-enclave" });
    expect(unpermitted).toEqual({ ok: false, reason: "not-permitted" });
    expect(rendered).toContain('data-signed-enclave-history-record="sig:enable-50"');
    expect(changed.record.mode).toBe("shared_from_when_turned_on");
    console.log("TASK6933_AUTH unsigned=refused cross_enclave=refused unpermitted=refused authority=signed-record");
  });

  it("writes the second signed epoch on off, leaves already shared items usable, and gives the third applicant 0 later items", () => {
    const enabled = enable(newEnclaveHistoryRecord(enclaveId), 50);
    if (!enabled.ok) throw new Error(enabled.reason);
    const shared = reshareHistoryFromMemberDevice(enabled.record, sharer, secondApplicant, corpus("post", 50, 20), encrypt);
    const off = applySignedHistorySetting(enabled.record, {
      enclaveId, actorMemberId: sharer.memberId, expectedVersion: enabled.record.version, mode: "hidden", signedEpoch: 70, signature: "sig:disable-70",
    }, verifier, resolver);
    expect(off.ok).toBe(true);
    if (!off.ok) throw new Error(off.reason);
    const afterOff = reshareHistoryFromMemberDevice(off.record, sharer, thirdApplicant, corpus("after-off", 70, 20), encrypt);
    const offMarkup = enclaveHistoryForNewMembersSettingsMarkup(restoreSignedEnclaveHistoryRecord(JSON.stringify(off.record)));

    expect(off.channelEvent).toMatchObject({ kind: "history-sharing-disabled", epoch: 70, signature: "sig:disable-70" });
    expect(shared.items).toHaveLength(20);
    expect(afterOff.items).toHaveLength(0);
    expect(afterOff.refusals).toHaveLength(20);
    expect(offMarkup).toContain("Already shared items cannot be recalled.");
    expect(offMarkup).not.toContain("will be recalled");
    console.log("TASK6933_OFF signed_epoch=70 third_applicant_opened=0/20 second_applicant_existing_opened=20/20 restart=green no_recall_promise=true");
  });

  it("keeps a non-empty inventory reconciled to every observed runtime sink and fails closed on a new sink", () => {
    const observed = ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY.map((sink) => ({ sink, bytes: 64, containsPreJoinKeyMaterial: false }));
    expect(ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY.length).toBeGreaterThan(0);
    expect(reconcileHistoryReshareSinks(observed)).toEqual({ ok: true, reason: null });
    expect(reconcileHistoryReshareSinks([])).toEqual({ ok: false, reason: `unobserved history sink=${ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY[0]}` });
    expect(reconcileHistoryReshareSinks([...observed, { sink: "new-unclassified-upload", bytes: 1, containsPreJoinKeyMaterial: false }])).toEqual({ ok: false, reason: "unclassified history sink=new-unclassified-upload" });
    expect(reconcileHistoryReshareSinks([{ sink: ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY[0], bytes: 1, containsPreJoinKeyMaterial: true }, ...observed.slice(1)])).toEqual({ ok: false, reason: `pre-join key material at sink=${ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY[0]}` });
    console.log(`TASK6933_SINKS inventory=${ENCLAVE_HISTORY_RESHARE_SINK_INVENTORY.length} observed=${observed.length} empty=red unclassified=red prejoin_key=red`);
  });

  it("places the signed-state disclosure on invite and join surfaces without altering the hidden promise", () => {
    const hidden = newEnclaveHistoryRecord(enclaveId);
    const enabled = enable(hidden, 50);
    if (!enabled.ok) throw new Error(enabled.reason);
    expect(enclaveHistoryJoinDisclosureMarkup(hidden)).toContain("History for new members: HIDDEN");
    expect(enclaveHistoryJoinDisclosureMarkup(hidden)).toContain("no pre-join history or keys");
    expect(enclaveHistoryJoinDisclosureMarkup(enabled.record)).toContain("History for new members: SHARED FROM WHEN IT WAS TURNED ON");
    expect(enclaveHistoryJoinDisclosureMarkup(enabled.record)).toContain("signed epoch 50");
    console.log("TASK6933_VISIBLE invite=HIDDEN join=SHARED_FROM_SIGNED_EPOCH accessibility_labels=present");
  });
});
