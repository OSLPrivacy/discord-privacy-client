/**
 * The small, explicit creation boundary behind the Chats pencil.  Keeping the
 * membership arithmetic here means the screen cannot quietly create a group
 * without its creator, or turn an Enclave's selected joining rule into a
 * different saved value.
 */

export const START_SOMETHING_CHOICES = ["direct", "group", "enclave"] as const;
export type StartSomethingChoice = typeof START_SOMETHING_CHOICES[number];

export const ENCLAVE_JOINING_RULES = ["invite_only", "approval_required", "anyone_with_link"] as const;
export type EnclaveJoiningRule = typeof ENCLAVE_JOINING_RULES[number];

export interface StartSomethingPerson {
  readonly personId: string;
  readonly oslUserId: string;
  readonly name: string;
}

export interface DirectConversation {
  readonly conversationId: string;
  readonly memberIds: readonly string[];
}

export interface GroupConversation {
  readonly groupId: string;
  readonly memberIds: readonly string[];
}

export interface EnclaveCreation {
  readonly enclaveId: string;
  readonly memberIds: readonly string[];
  readonly joiningRule: EnclaveJoiningRule;
}

export interface StartSomethingDependencies {
  readonly acceptDirectTarget: (target: string) => Promise<StartSomethingPerson | null>;
  readonly createDirectConversation: (creatorOslUserId: string, memberIds: readonly string[]) => Promise<DirectConversation | null>;
  readonly createGroupConversation: (name: string, memberIds: readonly string[]) => Promise<GroupConversation | null>;
  readonly createEnclave: (name: string, memberIds: readonly string[], joiningRule: EnclaveJoiningRule) => Promise<EnclaveCreation | null>;
}

function requiredText(value: string, label: string): string {
  const trimmed = value.trim();
  if (!trimmed) throw new Error(`${label} is required`);
  return trimmed;
}

function selectedPeople(people: readonly StartSomethingPerson[], selectedPersonIds: readonly string[]): StartSomethingPerson[] {
  const selected = new Set(selectedPersonIds);
  const resolved = people.filter((person) => selected.has(person.personId));
  if (resolved.length !== selected.size) throw new Error("A selected person is unavailable");
  return resolved;
}

/** Accept an invite/username and create exactly one two-member direct record. */
export async function startDirectConversation(
  target: string,
  creatorOslUserId: string,
  dependencies: StartSomethingDependencies,
): Promise<DirectConversation> {
  const creator = requiredText(creatorOslUserId, "Your OSL identity");
  const person = await dependencies.acceptDirectTarget(requiredText(target, "Their invite or username"));
  if (!person) throw new Error("Their invite or username was not accepted");
  const conversation = await dependencies.createDirectConversation(creator, [creator, person.oslUserId]);
  if (!conversation || conversation.memberIds.length !== 2) throw new Error("Direct conversation was not created");
  return conversation;
}

/** Three ticks plus the creator make the four-member group the UI promises. */
export async function startGroupConversation(
  name: string,
  creatorOslUserId: string,
  people: readonly StartSomethingPerson[],
  selectedPersonIds: readonly string[],
  dependencies: StartSomethingDependencies,
): Promise<GroupConversation> {
  const creator = requiredText(creatorOslUserId, "Your OSL identity");
  const selected = selectedPeople(people, selectedPersonIds);
  if (selected.length !== 3) throw new Error("Choose exactly three people for this group");
  const memberIds = [creator, ...selected.map((person) => person.oslUserId)];
  if (new Set(memberIds).size !== memberIds.length) throw new Error("Group members must be distinct");
  const group = await dependencies.createGroupConversation(requiredText(name, "Group name"), memberIds);
  if (!group || group.memberIds.length !== 4) throw new Error("Four-member group was not created");
  return group;
}

/** The chosen enum is passed through unchanged and checked again on return. */
export async function startEnclave(
  name: string,
  creatorOslUserId: string,
  people: readonly StartSomethingPerson[],
  selectedPersonIds: readonly string[],
  joiningRule: EnclaveJoiningRule,
  dependencies: StartSomethingDependencies,
): Promise<EnclaveCreation> {
  const creator = requiredText(creatorOslUserId, "Your OSL identity");
  const memberIds = [creator, ...selectedPeople(people, selectedPersonIds).map((person) => person.oslUserId)];
  if (new Set(memberIds).size !== memberIds.length) throw new Error("Enclave members must be distinct");
  const enclave = await dependencies.createEnclave(requiredText(name, "Enclave name"), memberIds, joiningRule);
  if (!enclave || enclave.joiningRule !== joiningRule) throw new Error("Enclave joining rule was not saved");
  return enclave;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] ?? character);
}

function peopleChecklist(people: readonly StartSomethingPerson[]): string {
  return `<fieldset class="start-something-people"><legend>People</legend>${people.map((person) => `<label><input type="checkbox" data-start-something-person="${escapeHtml(person.personId)}"/><span>${escapeHtml(person.name)}</span></label>`).join("")}</fieldset>`;
}

/** Sheet markup intentionally has one and only one choice control per path. */
export function startSomethingSheetMarkup(
  choice: StartSomethingChoice | null,
  ownInvite: string,
  people: readonly StartSomethingPerson[],
  joiningRule: EnclaveJoiningRule = "invite_only",
): string {
  const chooser = `<section class="start-something-choices" aria-label="Start something choices">${START_SOMETHING_CHOICES.map((item) => `<button type="button" data-start-something-choice="${item}" aria-pressed="${choice === item}">${item === "direct" ? "Direct message" : item === "group" ? "Group" : "Enclave"}</button>`).join("")}</section>`;
  const direct = choice === "direct" ? `<form data-start-something-direct><label>Their invite or username<input name="target" autocomplete="off" autocapitalize="none" required/></label><label>Your invite link<code data-start-something-own-invite tabindex="0">${escapeHtml(ownInvite)}</code></label><button type="button" data-start-something-copy-invite>Copy your invite link</button><button class="button primary" type="submit">Start direct message</button></form>` : "";
  const group = choice === "group" ? `<form data-start-something-group><label>Group name<input name="name" maxlength="80" required/></label>${peopleChecklist(people)}<button class="button primary" type="submit">Create group</button></form>` : "";
  const enclave = choice === "enclave" ? `<form data-start-something-enclave><label>Enclave name<input name="name" maxlength="80" required/></label>${peopleChecklist(people)}<label>How people join<select name="joiningRule"><option value="invite_only" ${joiningRule === "invite_only" ? "selected" : ""}>Invite only</option><option value="approval_required" ${joiningRule === "approval_required" ? "selected" : ""}>Request approval</option><option value="anyone_with_link" ${joiningRule === "anyone_with_link" ? "selected" : ""}>Anyone with the link</option></select></label><button class="button primary" type="submit">Create enclave</button></form>` : "";
  return `<dialog class="start-something-sheet" open aria-labelledby="start-something-title"><header><h2 id="start-something-title">START SOMETHING</h2><button type="button" data-start-something-close aria-label="Close">×</button></header>${chooser}${direct}${group}${enclave}</dialog>`;
}
