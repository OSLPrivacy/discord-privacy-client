/**
 * Connects the per-friend "Auto-whitelist new accounts" switch (TASK 0267) to
 * the Hub's future-account commands (TASK 0263):
 * `get_hub_friend_future_account_auto_whitelist` reads the saved state and
 * `set_hub_friend_future_account_auto_whitelist` saves a flip. Dependencies
 * are injected, like `removeHubFriend`, so a fixture can drive the same
 * connect code against a fake Hub.
 */

export const GET_FUTURE_ACCOUNT_COMMAND = "get_hub_friend_future_account_auto_whitelist";
export const SET_FUTURE_ACCOUNT_COMMAND = "set_hub_friend_future_account_auto_whitelist";

export type FutureAccountCommand =
  | typeof GET_FUTURE_ACCOUNT_COMMAND
  | typeof SET_FUTURE_ACCOUNT_COMMAND;

export interface FutureAccountSwitchSetting {
  personId: string;
  enabled: boolean;
}

export interface FutureAccountCommandDependencies {
  isTauriRuntime(): boolean;
  invoke(command: FutureAccountCommand, args: { personId: string; enabled?: boolean }): Promise<unknown>;
  recordBackendFailure(command: FutureAccountCommand, error: unknown): void;
}

function safeFutureAccountPersonId(personId: string): boolean {
  return personId.length > 0
    && personId.length <= 180
    && !/[<>\u0000-\u001f\u007f]/.test(personId);
}

function parseFutureAccountSetting(raw: unknown): FutureAccountSwitchSetting | null {
  if (typeof raw !== "object" || raw === null) return null;
  const dto = raw as { personId?: unknown; enabled?: unknown };
  if (typeof dto.personId !== "string" || !safeFutureAccountPersonId(dto.personId)) return null;
  if (typeof dto.enabled !== "boolean") return null;
  return { personId: dto.personId, enabled: dto.enabled };
}

export async function loadFutureAccountSwitchStates(
  personIds: readonly string[],
  dependencies: FutureAccountCommandDependencies,
): Promise<Map<string, boolean>> {
  const states = new Map<string, boolean>();
  if (!dependencies.isTauriRuntime()) return states;
  for (const personId of personIds) {
    if (!safeFutureAccountPersonId(personId)) continue;
    try {
      const setting = parseFutureAccountSetting(
        await dependencies.invoke(GET_FUTURE_ACCOUNT_COMMAND, { personId }),
      );
      // A friend whose state cannot be read keeps no entry: the switch draws
      // off, and the next open retries instead of trusting a guess.
      if (setting && setting.personId === personId) states.set(personId, setting.enabled);
    } catch (error) {
      dependencies.recordBackendFailure(GET_FUTURE_ACCOUNT_COMMAND, error);
    }
  }
  return states;
}

export async function saveFutureAccountSwitch(
  personId: string,
  enabled: boolean,
  dependencies: FutureAccountCommandDependencies,
): Promise<FutureAccountSwitchSetting | null> {
  if (!dependencies.isTauriRuntime() || !safeFutureAccountPersonId(personId) || typeof enabled !== "boolean") {
    return null;
  }
  try {
    const setting = parseFutureAccountSetting(
      await dependencies.invoke(SET_FUTURE_ACCOUNT_COMMAND, { personId, enabled }),
    );
    // The Hub echoes what it stored. Anything else is a failed save.
    return setting && setting.personId === personId && setting.enabled === enabled ? setting : null;
  } catch (error) {
    dependencies.recordBackendFailure(SET_FUTURE_ACCOUNT_COMMAND, error);
    return null;
  }
}
