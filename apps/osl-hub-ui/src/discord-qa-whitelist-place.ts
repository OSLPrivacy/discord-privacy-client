import { invoke } from "@tauri-apps/api/core";

// The open place is the exact conversation shown behind the protected surface,
// expressed as the record the hub's allowed-place commands understand
// (`apps/osl-hub/src/allowed_place_commands.rs`). The stable id follows the
// canonical `app:account:kind:place` shape that
// `AllowedPlaceRecord::discord_direct_message` builds on the hub side, so the
// same conversation always resolves to the same allowed-place row.
export interface DiscordQaOpenPlace {
  app: string;
  account: string;
  kind: string;
  stableId: string;
}

export interface DiscordQaOpenPlaceSource {
  serviceId: string;
  accountId: string;
  personId: string;
}

export function discordQaOpenPlace(source: DiscordQaOpenPlaceSource): DiscordQaOpenPlace {
  return {
    app: source.serviceId,
    account: source.accountId,
    kind: "direct_message",
    stableId: `${source.serviceId}:${source.accountId}:direct_message:${source.personId}`,
  };
}

export type AllowedPlaceCommand = "add" | "remove" | "allowed";

// Runs one allowed-place command against the hub. The app runner goes over
// Tauri IPC; tests may substitute a runner that drives the same commands
// through the headless CLI instead. Whatever the transport, `add` and `remove`
// must land in the same store the `allowed` query reads.
export type AllowedPlaceCommandRunner = (
  command: AllowedPlaceCommand,
  place: DiscordQaOpenPlace,
) => Promise<unknown>;

export const invokeAllowedPlaceCommand: AllowedPlaceCommandRunner = (command, place) => {
  if (command === "add") {
    return invoke<unknown>("add_allowed_place_record", {
      record: { app: place.app, account: place.account, kind: place.kind, stable_id: place.stableId },
    });
  }
  if (command === "remove") {
    return invoke<unknown>("remove_allowed_place_record", { stableId: place.stableId });
  }
  return invoke<unknown>("query_allowed_place_allowed", {
    query: { app: place.app, account: place.account, kind: place.kind, stable_id: place.stableId },
  });
};

// The `allowed` command answers either as a bare boolean (the security-state
// command) or as `{ allowed: boolean }` (the headless JSON command). Anything
// else fails closed.
export function parseAllowedPlaceAllowed(raw: unknown): boolean {
  if (typeof raw === "boolean") return raw;
  if (raw && typeof raw === "object" && typeof (raw as { allowed?: unknown }).allowed === "boolean") {
    return (raw as { allowed: boolean }).allowed;
  }
  return false;
}

export async function queryOpenPlaceAllowed(
  place: DiscordQaOpenPlace,
  run: AllowedPlaceCommandRunner = invokeAllowedPlaceCommand,
): Promise<boolean> {
  try {
    return parseAllowedPlaceAllowed(await run("allowed", place));
  } catch {
    return false;
  }
}

interface WhitelistButtonLike {
  dataset: { whitelistNext?: string };
  addEventListener(type: "click", listener: (event: { currentTarget: unknown }) => void): void;
}

export interface ConnectDiscordQaWhitelistButtonOptions {
  run?: AllowedPlaceCommandRunner;
  // Called after the add/remove command lands; `allowed` is the state the open
  // place was just moved to.
  onCommand?: (command: "add" | "remove", allowed: boolean) => void;
  onError?: () => void;
}

// Connects the single-place whitelist button (TASK 0109) to the allowed-place
// commands: an off-list click adds the open place, an on-list click removes
// it. The button's own `data-whitelist-next` attribute names the action, so
// the wiring cannot drift from what the button shows the user.
export function connectDiscordQaWhitelistButton(
  button: WhitelistButtonLike,
  openPlace: () => DiscordQaOpenPlace | null,
  options: ConnectDiscordQaWhitelistButtonOptions = {},
): void {
  const run = options.run ?? invokeAllowedPlaceCommand;
  button.addEventListener("click", (event) => {
    const target = event.currentTarget as WhitelistButtonLike;
    const command = target.dataset.whitelistNext === "allow" ? "add" : "remove";
    const place = openPlace();
    if (!place) {
      options.onError?.();
      return;
    }
    void run(command, place)
      .then(() => options.onCommand?.(command, command === "add"))
      .catch(() => options.onError?.());
  });
}
