/** The three person-specific commands exposed by the accepted-friend page. */
export const FRIEND_PAGE_ACTION_COMMANDS = {
  message: "activate_osl_chat_context",
  remove: "remove_hub_friend",
  block: "osl_block_friend_request",
} as const;

export type FriendPageAction = keyof typeof FRIEND_PAGE_ACTION_COMMANDS;

export interface FriendPageActionControl {
  readonly dataset: { readonly oslChatOpen?: string; readonly removePerson?: string; readonly blockPerson?: string };
  addEventListener(type: "click", listener: () => void): void;
}

export interface FriendPageActionRoot {
  querySelectorAll(selector: string): Iterable<FriendPageActionControl>;
}

export interface FriendPageActionCommands<Result> {
  message(personId: string): Promise<Result>;
  remove(personId: string): Promise<Result>;
  block(personId: string): Promise<Result>;
}

function personId(action: FriendPageAction, control: FriendPageActionControl): string {
  if (action === "message") return control.dataset.oslChatOpen ?? "";
  if (action === "remove") return control.dataset.removePerson ?? "";
  return control.dataset.blockPerson ?? "";
}

/**
 * Bind each visible action once and forward the exact direct command result to
 * the caller. Keeping this boundary free of DOM rendering lets the fixture
 * prove the user gesture reaches the command rather than merely finding text.
 */
export function connectFriendPageActions<Result>(
  root: FriendPageActionRoot,
  commands: FriendPageActionCommands<Result>,
  onResult: (action: FriendPageAction, result: Result) => void,
): number {
  const bindings: readonly [FriendPageAction, string][] = [
    ["message", "[data-friend-page-element=\"Message\"]"],
    ["remove", "[data-friend-page-element=\"remove\"]"],
    ["block", "[data-friend-page-element=\"block\"]"],
  ];
  let connected = 0;
  for (const [action, selector] of bindings) {
    for (const control of root.querySelectorAll(selector)) {
      const id = personId(action, control);
      if (!id) continue;
      connected += 1;
      control.addEventListener("click", () => {
        void commands[action](id).then((result) => onResult(action, result));
      });
    }
  }
  return connected;
}
