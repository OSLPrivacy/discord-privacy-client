import { describe, expect, it } from "vitest";
import {
  messageTimerControlMarkup,
  setMessageTimer,
  UNREMOVABLE_OTHER_COPY_REASON,
  type MessageTimerContext,
  type MessageTimerWaitingRecord,
} from "./message-timer-control";

const discordFirst: MessageTimerContext = {
  appId: "discord",
  conversationKind: "ordinary_message",
  conversationId: "discord-dm-3314",
  messageLocator: "discord-message-3314-first",
};

const xDirectMessage: MessageTimerContext = {
  appId: "x",
  conversationKind: "direct_message",
  conversationId: "x-dm-3314",
  messageLocator: "x-message-3314",
};

const ordinaryEmail: MessageTimerContext = {
  appId: "email",
  conversationKind: "ordinary_email",
  conversationId: "email-thread-3314",
  messageLocator: "email-message-3314",
};

const discordSecond: MessageTimerContext = {
  ...discordFirst,
  messageLocator: "discord-message-3314-second",
};

describe("TASK 3314 timer refusal", () => {
  it("records Discord timers while leaving X direct messages and email off with a reason", async () => {
    const waitingRecords: MessageTimerWaitingRecord[] = [];
    let xTimersCreated = 0;
    const writer = (record: MessageTimerWaitingRecord): void => {
      waitingRecords.push(record);
      if (record.appId === "x") xTimersCreated += 1;
    };

    console.log(`TASK3314_DISCORD_WAITING_RECORDS_BEFORE=${waitingRecords.length}`);
    expect(waitingRecords).toHaveLength(0);

    const firstDiscordResult = await setMessageTimer(discordFirst, 120, writer);
    expect(firstDiscordResult).toMatchObject({ accepted: true });
    expect(waitingRecords).toHaveLength(1);
    expect(waitingRecords[0]).toMatchObject({
      appId: "discord",
      conversationKind: "ordinary_message",
      durationSeconds: 120,
    });
    console.log(`TASK3314_DISCORD_FIRST_TIMER_SECONDS=${waitingRecords[0]?.durationSeconds}`);
    console.log(`TASK3314_DISCORD_WAITING_RECORDS_AFTER_FIRST=${waitingRecords.length}`);

    const xMarkup = messageTimerControlMarkup(xDirectMessage, "x-timer-off-reason");
    expect(xMarkup).toContain('data-message-timer-state="off"');
    expect(xMarkup).toMatch(/<button[^>]*data-timer-state="off"[^>]*aria-describedby="x-timer-off-reason"[^>]*disabled>Timer off<\/button>/u);
    expect(xMarkup).toContain(`<p id="x-timer-off-reason" data-message-timer-reason>${UNREMOVABLE_OTHER_COPY_REASON}</p>`);
    const xResult = await setMessageTimer(xDirectMessage, 120, writer);
    expect(xResult).toEqual({ accepted: false, reason: UNREMOVABLE_OTHER_COPY_REASON });
    expect(xTimersCreated).toBe(0);
    expect(waitingRecords).toHaveLength(1);
    console.log("TASK3314_X_TIMER_STATE=off");
    console.log(`TASK3314_X_REFUSAL=${UNREMOVABLE_OTHER_COPY_REASON}`);
    console.log(`TASK3314_X_TIMERS_CREATED=${xTimersCreated}`);

    const emailMarkup = messageTimerControlMarkup(ordinaryEmail, "email-timer-off-reason");
    expect(emailMarkup).toContain('data-message-timer-state="off"');
    expect(emailMarkup).toContain('aria-describedby="email-timer-off-reason"');
    expect(emailMarkup).toContain(UNREMOVABLE_OTHER_COPY_REASON);
    const emailResult = await setMessageTimer(ordinaryEmail, 120, writer);
    expect(emailResult).toEqual({ accepted: false, reason: UNREMOVABLE_OTHER_COPY_REASON });
    expect(waitingRecords).toHaveLength(1);
    console.log("TASK3314_EMAIL_TIMER_STATE=off");
    console.log(`TASK3314_EMAIL_REFUSAL=${UNREMOVABLE_OTHER_COPY_REASON}`);

    const secondDiscordResult = await setMessageTimer(discordSecond, 120, writer);
    expect(secondDiscordResult).toMatchObject({ accepted: true });
    expect(waitingRecords).toHaveLength(2);
    expect(waitingRecords[1]).toMatchObject({
      appId: "discord",
      conversationKind: "ordinary_message",
      durationSeconds: 120,
    });
    console.log(`TASK3314_DISCORD_SECOND_TIMER_SECONDS=${waitingRecords[1]?.durationSeconds}`);
    console.log(`TASK3314_DISCORD_WAITING_RECORDS_AFTER_RETURN=${waitingRecords.length}`);
  });
});
