import { describe, expect, it } from "vitest";
import {
  bindMessengerAttachmentTray,
  createMessengerAttachmentTray,
  messengerAttachmentTrayMarkup,
  type MessengerAttachmentReceipt,
} from "./messenger-attachment-tray";

const fixtureBytes = new TextEncoder().encode("TASK1197 Messenger drag fixture: silver tern\n");

class PrivateBox extends EventTarget {
  trayMarkup = messengerAttachmentTrayMarkup(createMessengerAttachmentTray());
}

function fixtureFile(): File {
  return {
    name: "silver-tern.txt",
    size: fixtureBytes.byteLength,
    arrayBuffer: async () => new Uint8Array(fixtureBytes).buffer,
  } as File;
}

function fixtureFileList(): FileList {
  const file = fixtureFile();
  const files = [file] as unknown as FileList;
  Object.defineProperty(files, "item", {
    value: (index: number) => index === 0 ? file : null,
  });
  return files;
}

function dropEvent(files: FileList): Event {
  const event = new Event("drop", { cancelable: true });
  Object.defineProperty(event, "dataTransfer", { value: { files } });
  return event;
}

function trayCardCount(markup: string): number {
  return (markup.match(/data-messenger-attachment-card=/gu) ?? []).length;
}

describe("TASK 1197 - Messenger attachment drag safety", () => {
  it("leaves one tray card and posts no Messenger message without Send", async () => {
    const tray = createMessengerAttachmentTray();
    const picker = new EventTarget() as HTMLInputElement;
    const privateBox = new PrivateBox();
    const sendButton = new EventTarget();
    let messengerReceivingJobRuns = 0;
    let messengerMessagePostCount = 0;
    let sendPressCount = 0;
    let settleReceivingJob!: () => void;
    const receivingJobSettled = new Promise<void>((resolve) => { settleReceivingJob = resolve; });

    const postMessengerMessage = (): void => {
      messengerMessagePostCount += 1;
    };
    sendButton.addEventListener("click", () => {
      sendPressCount += 1;
      postMessengerMessage();
    });

    const messengerReceivingJob = (receipt: MessengerAttachmentReceipt): void => {
      messengerReceivingJobRuns += 1;
      if (process.env.TASK_1197_STUB_MESSENGER_RECEIVER === "1") return;
      expect(receipt.addedCardCount).toBe(1);
      privateBox.trayMarkup = messengerAttachmentTrayMarkup(tray);
    };

    bindMessengerAttachmentTray(
      picker,
      privateBox as unknown as HTMLElement,
      tray,
      (receipt) => {
        messengerReceivingJob(receipt);
        settleReceivingJob();
      },
    );

    const dragOver = new Event("dragover", { cancelable: true });
    privateBox.dispatchEvent(dragOver);
    const drop = dropEvent(fixtureFileList());
    privateBox.dispatchEvent(drop);
    privateBox.dispatchEvent(new Event("dragleave"));
    await receivingJobSettled;

    const cardCount = trayCardCount(privateBox.trayMarkup);
    console.log(`TASK1197 tray_card_count=${cardCount}`);
    console.log(`TASK1197 messenger_message_post_count=${messengerMessagePostCount}`);
    console.log(`TASK1197 messenger_receiving_job_runs=${messengerReceivingJobRuns}`);
    console.log(`TASK1197 send_press_count=${sendPressCount}`);
    console.log(`TASK1197 dragover_prevented=${dragOver.defaultPrevented} drop_prevented=${drop.defaultPrevented}`);

    expect(dragOver.defaultPrevented).toBe(true);
    expect(drop.defaultPrevented).toBe(true);
    expect(messengerReceivingJobRuns).toBe(1);
    expect(cardCount).toBe(1);
    expect(tray.cards).toHaveLength(1);
    expect(tray.sendCount).toBe(0);
    expect(sendPressCount).toBe(0);
    expect(messengerMessagePostCount).toBe(0);
  });
});
