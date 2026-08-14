/**
 * The final permission sheet before an AutoScrub schedule may delete.
 *
 * The sheet deliberately edits a copy of the already-saved schedule.  Cancel
 * discards that copy and restores the exact saved values; it must not turn a
 * previous Find-only schedule into a deletion schedule merely by opening this
 * screen.
 */

export type AutoScrubScheduleMode = "find_only" | "find_and_delete";

export interface AutoScrubScheduleValues {
  readonly selectedAccountIds: readonly string[];
  readonly selectedRuleNames: readonly string[];
  readonly mode: AutoScrubScheduleMode;
}

export interface AutoScrubDeletionPermissionState {
  readonly screen: "schedule" | "deletion-permission";
  readonly savedSchedule: AutoScrubScheduleValues;
  readonly agreementChecked: boolean;
}

export type AutoScrubDeletionPermissionAction = "find-only" | "find-and-delete" | "cancel";

function copySchedule(values: AutoScrubScheduleValues): AutoScrubScheduleValues {
  return {
    selectedAccountIds: [...values.selectedAccountIds],
    selectedRuleNames: [...values.selectedRuleNames],
    mode: values.mode,
  };
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Open a fresh permission sheet from the schedule values that were saved. */
export function openAutoScrubDeletionPermission(
  savedSchedule: AutoScrubScheduleValues,
): AutoScrubDeletionPermissionState {
  return {
    screen: "deletion-permission",
    savedSchedule: copySchedule(savedSchedule),
    agreementChecked: false,
  };
}

/** The one agreement tick is intentionally separate from the three risk statements it acknowledges. */
export function setAutoScrubDeletionAgreement(
  state: AutoScrubDeletionPermissionState,
  agreementChecked: boolean,
): AutoScrubDeletionPermissionState {
  return { ...state, agreementChecked };
}

/**
 * Resolve a page action.  Every outcome returns to the schedule and carries a
 * complete read-back value.  In particular Cancel returns an independent copy
 * of the saved schedule without modifying a field.
 */
export function resolveAutoScrubDeletionPermission(
  state: AutoScrubDeletionPermissionState,
  action: AutoScrubDeletionPermissionAction,
): AutoScrubDeletionPermissionState {
  if (state.screen !== "deletion-permission") return state;
  if (action === "find-and-delete" && !state.agreementChecked) return state;
  return {
    screen: "schedule",
    savedSchedule: {
      ...copySchedule(state.savedSchedule),
      mode: action === "find-only" ? "find_only"
        : action === "find-and-delete" ? "find_and_delete"
          : state.savedSchedule.mode,
    },
    agreementChecked: false,
  };
}

export function renderAutoScrubDeletionPermission(state: AutoScrubDeletionPermissionState): string {
  if (state.screen !== "deletion-permission") return "";
  const accounts = state.savedSchedule.selectedAccountIds.map((account) => `<li>${escapeHtml(account)}</li>`).join("");
  const rules = state.savedSchedule.selectedRuleNames.map((rule) => `<li>${escapeHtml(rule)}</li>`).join("");
  return `<section class="autoscrub-deletion-permission" aria-labelledby="autoscrub-deletion-permission-title">
    <p class="eyebrow">AutoScrub schedule</p>
    <h2 id="autoscrub-deletion-permission-title">Allow AutoScrub to delete matches?</h2>
    <p>Review the exact schedule scope before choosing how it runs.</p>
    <div class="autoscrub-deletion-scope">
      <section aria-labelledby="autoscrub-selected-accounts"><h3 id="autoscrub-selected-accounts">Selected accounts</h3><ul>${accounts || "<li>No accounts selected</li>"}</ul></section>
      <section aria-labelledby="autoscrub-selected-rules"><h3 id="autoscrub-selected-rules">Selected rules</h3><ul>${rules || "<li>No rules selected</li>"}</ul></section>
    </div>
    <aside class="warning autoscrub-deletion-risks" aria-labelledby="autoscrub-deletion-risks-title">
      <h3 id="autoscrub-deletion-risks-title">Deletion risks</h3>
      <ul><li>Deleted messages can be permanent.</li><li>Service rules may forbid automated reading or deletion.</li><li>Suspension or ban risk is real.</li></ul>
    </aside>
    <label class="autoscrub-deletion-agreement"><input id="autoscrub-deletion-agreement" type="checkbox" ${state.agreementChecked ? "checked" : ""}/><span>I understand these risks and allow AutoScrub to delete matching messages in this exact schedule.</span></label>
    <footer class="autoscrub-deletion-actions"><button class="button" type="button" data-autoscrub-deletion-action="cancel">Cancel</button><button class="button" type="button" data-autoscrub-deletion-action="find-only">Find only</button><button class="button primary" type="button" data-autoscrub-deletion-action="find-and-delete" ${state.agreementChecked ? "" : "disabled"}>Find and delete</button></footer>
  </section>`;
}

export interface AutoScrubDeletionPermissionCallbacks {
  /** Re-render the still-open sheet after its agreement tick changes. */
  readonly onStateChange: (state: AutoScrubDeletionPermissionState) => void;
  readonly onResolve: (state: AutoScrubDeletionPermissionState, action: AutoScrubDeletionPermissionAction) => void;
}

/** Connect the rendered sheet to its state transition without granting deletion authority itself. */
export function bindAutoScrubDeletionPermission(
  root: ParentNode,
  state: AutoScrubDeletionPermissionState,
  callbacks: AutoScrubDeletionPermissionCallbacks,
): void {
  root.querySelector<HTMLInputElement>("#autoscrub-deletion-agreement")?.addEventListener("change", (event) => {
    callbacks.onStateChange(setAutoScrubDeletionAgreement(state, (event.currentTarget as HTMLInputElement).checked));
  });
  root.querySelectorAll<HTMLButtonElement>("[data-autoscrub-deletion-action]").forEach((button) => button.addEventListener("click", () => {
    const action = button.dataset.autoscrubDeletionAction;
    if (action !== "find-only" && action !== "find-and-delete" && action !== "cancel") return;
    callbacks.onResolve(resolveAutoScrubDeletionPermission(state, action), action);
  }));
}
