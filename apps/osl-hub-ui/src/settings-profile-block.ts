/**
 * The editable "Your profile" portion of Appearance.  This deliberately owns
 * no storage: the profile pane owns the global profile record and passes it in
 * here.  Updating the passed record (and calling save) keeps the source of
 * truth shared by the profile pane, rather than creating a second preference.
 */

export const VIBE_LIMIT = 40;
export const avatarColours = ["#2ac0f0", "#35c46a", "#b48cf2", "#f2a35c"] as const;
export const profileBackgrounds = [
  "#20272b",
  "linear-gradient(135deg, #2ac0f0, #15556b)",
  "linear-gradient(135deg, #35c46a, #174c31)",
  "linear-gradient(135deg, #b48cf2, #4c306b)",
  "linear-gradient(135deg, #f2a35c, #713a24)",
] as const;

/** The global record shape supplied by osl-profile-pane.ts. */
export interface AppearanceProfileRecord {
  displayName: string;
  /** The profile pane calls this aboutLine; Appearance presents it as Vibe. */
  aboutLine: string;
  avatar: string | null;
  colour: string;
  cardBackground: string;
}

export interface SettingsProfileBlockOptions {
  readonly profile: AppearanceProfileRecord;
  /** Persist the same global record after every user-visible change. */
  readonly save: (profile: AppearanceProfileRecord) => void | Promise<void>;
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;").replace(/'/gu, "&#39;");
}

function selected(value: string, expected: string): string {
  return value === expected ? " is-selected" : "";
}

function avatarMarkup(profile: AppearanceProfileRecord): string {
  if (profile.avatar) return `<img class="settings-profile-block__avatar-image" src="${escapeHtml(profile.avatar)}" alt="Profile avatar"/>`;
  const initial = Array.from(profile.displayName.trim())[0]?.toLocaleUpperCase() ?? "?";
  return `<span class="settings-profile-block__avatar-colour" style="background:${escapeHtml(profile.colour)}" aria-label="Colour avatar">${escapeHtml(initial)}</span>`;
}

/** Markup is exported separately so Settings can render it alongside its theme controls. */
export function settingsProfileBlockMarkup(profile: AppearanceProfileRecord): string {
  const vibe = Array.from(profile.aboutLine).slice(0, VIBE_LIMIT).join("");
  const backgrounds = profileBackgrounds.map((background, index) => `<button class="settings-profile-block__background${selected(profile.cardBackground, background)}" type="button" data-profile-background="${escapeHtml(background)}" aria-label="Profile background ${index + 1}" style="background:${background}"></button>`).join("");
  const colours = avatarColours.map((colour) => `<button class="settings-profile-block__colour${selected(profile.colour, colour)}" type="button" data-profile-avatar-colour="${colour}" aria-label="Avatar colour ${colour}" style="background:${colour}"></button>`).join("");
  return `<section class="settings-profile-block" aria-labelledby="settings-profile-title">
    <header><span>YOUR PROFILE</span><p>How you appear on a friend's card.</p></header>
    <div class="settings-profile-block__body">
      <div class="settings-profile-block__preview" aria-live="polite">${avatarMarkup(profile)}</div>
      <div class="settings-profile-block__fields">
        <label>Display name<input type="text" data-profile-display-name value="${escapeHtml(profile.displayName)}" autocomplete="nickname"/></label>
        <label class="settings-profile-block__vibe"><span>Vibe <output data-profile-vibe-count>${VIBE_LIMIT - Array.from(vibe).length} left</output></span><input type="text" data-profile-vibe maxlength="${VIBE_LIMIT}" value="${escapeHtml(vibe)}" placeholder="what's your vibe right now"/></label>
        <fieldset><legend>Avatar colour</legend><div class="settings-profile-block__choices">${colours}</div></fieldset>
        <div class="settings-profile-block__upload"><span>Avatar image</span><label class="button compact">Upload<input type="file" data-profile-avatar-upload accept="image/png,image/jpeg,image/gif"/></label><button class="button compact" type="button" data-profile-avatar-remove ${profile.avatar ? "" : "disabled"}>Remove</button><small>PNG, JPG, or GIF</small></div>
        <fieldset><legend>Profile background</legend><div class="settings-profile-block__choices">${backgrounds}<label class="settings-profile-block__hex">Custom hex<input type="text" data-profile-background-hex value="${escapeHtml(profile.cardBackground.startsWith("#") ? profile.cardBackground : "")}" pattern="#[0-9a-fA-F]{6}" placeholder="#20272b" maxlength="7"/></label></div></fieldset>
      </div>
    </div>
  </section>`;
}

function validHex(value: string): boolean { return /^#[0-9a-f]{6}$/iu.test(value); }

/** Bind after insertion. Each mutation is saved immediately through the pane's global-record writer. */
export function bindSettingsProfileBlock(root: ParentNode, options: SettingsProfileBlockOptions): void {
  const { profile, save } = options;
  const persist = (): void => { void save(profile); };
  const refreshAvatar = (): void => {
    const preview = root.querySelector<HTMLElement>(".settings-profile-block__preview");
    if (preview) preview.innerHTML = avatarMarkup(profile);
    const remove = root.querySelector<HTMLButtonElement>("[data-profile-avatar-remove]");
    if (remove) remove.disabled = profile.avatar === null;
  };
  const displayName = root.querySelector<HTMLInputElement>("[data-profile-display-name]");
  displayName?.addEventListener("input", () => { profile.displayName = displayName.value; persist(); });
  const vibe = root.querySelector<HTMLInputElement>("[data-profile-vibe]");
  const counter = root.querySelector<HTMLOutputElement>("[data-profile-vibe-count]");
  vibe?.addEventListener("input", () => {
    profile.aboutLine = Array.from(vibe.value).slice(0, VIBE_LIMIT).join("");
    if (vibe.value !== profile.aboutLine) vibe.value = profile.aboutLine;
    if (counter) counter.value = `${VIBE_LIMIT - Array.from(profile.aboutLine).length} left`;
    persist();
  });
  root.querySelectorAll<HTMLButtonElement>("[data-profile-avatar-colour]").forEach((button) => button.addEventListener("click", () => {
    profile.colour = button.dataset.profileAvatarColour ?? profile.colour;
    root.querySelectorAll("[data-profile-avatar-colour]").forEach((choice) => choice.classList.toggle("is-selected", choice === button));
    if (profile.avatar === null) refreshAvatar();
    persist();
  }));
  root.querySelectorAll<HTMLButtonElement>("[data-profile-background]").forEach((button) => button.addEventListener("click", () => {
    profile.cardBackground = button.dataset.profileBackground ?? profile.cardBackground;
    root.querySelectorAll("[data-profile-background]").forEach((choice) => choice.classList.toggle("is-selected", choice === button));
    persist();
  }));
  root.querySelector<HTMLInputElement>("[data-profile-background-hex]")?.addEventListener("change", (event) => {
    const input = event.currentTarget;
    if (!validHex(input.value)) { input.value = profile.cardBackground.startsWith("#") ? profile.cardBackground : ""; return; }
    profile.cardBackground = input.value.toLowerCase();
    root.querySelectorAll("[data-profile-background]").forEach((choice) => choice.classList.remove("is-selected"));
    persist();
  });
  root.querySelector<HTMLButtonElement>("[data-profile-avatar-remove]")?.addEventListener("click", () => { profile.avatar = null; refreshAvatar(); persist(); });
  root.querySelector<HTMLInputElement>("[data-profile-avatar-upload]")?.addEventListener("change", (event) => {
    const file = event.currentTarget.files?.[0];
    if (!file || !/image\/(png|jpeg|gif)/u.test(file.type)) return;
    const reader = new FileReader();
    reader.addEventListener("load", () => { if (typeof reader.result === "string") { profile.avatar = reader.result; refreshAvatar(); persist(); } }, { once: true });
    reader.readAsDataURL(file);
  });
}
