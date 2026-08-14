// TASK 1603 - the unsigned / unknown-publisher install note.
//
// OSL ships without a Windows code-signing certificate. That is a recorded
// decision, not an oversight: `docs/release/code-signing-decision.md` keeps it
// deferred, and `docs/download.html` already says the beta is not code signed.
// This module is the note a person actually reads at install time, and it has
// exactly three jobs:
//
//   1. Name both warnings Windows shows, in Windows' own words, so neither one
//      arrives as a surprise -- the SmartScreen "Windows protected your PC"
//      block (the unsigned-app warning) and the "Unknown publisher" line in the
//      User Account Control box.
//   2. Say once, in one sentence, that neither warning is proof the installer
//      is unsafe -- because both fire on a missing signature, not on anything
//      Windows found in the file.
//   3. Hand over the one check a person can actually run: the exact
//      `Get-FileHash` command and the exact value it has to print.
//
// What it must never do is promise that Windows will come to trust OSL.
// Microsoft is explicit that a certificate does not clear SmartScreen, that
// reputation "can take several weeks and hundreds of clean installs", and that
// Smart App Control blocks unsigned files outright
// (docs/release/code-signing-decision.md §3). So the promise is unwritable
// here: `unsignedPublisherNotePromises` scans the rendered words for that class
// of claim, and the note's own tests require it to come back empty.
//
// The expected checksum is never typed into this file. It arrives in a release
// record that `unsignedPublisherNoteRelease` refuses unless it is a real
// 64-character SHA-256 for a real `OSL-<version>.exe`, which is the same rule
// `scripts/build-download-meta.mjs` applies to the download page: release
// evidence, never hand-entered.

import "./unsigned-publisher-note.css";

export const UNSIGNED_PUBLISHER_NOTE_TITLE = "OSL is not code signed";

export const UNSIGNED_PUBLISHER_NOTE_LEAD =
  "OSL Privacy is built without a Windows code-signing certificate, so the installer you download is"
  + " unsigned. Windows has no publisher name to read out of it, and it tells you so twice.";

export interface WindowsInstallWarning {
  /** Stable id, so a test can assert both warnings are present by name. */
  id: "unsigned" | "unknown-publisher";
  /** The words Windows puts on screen. */
  name: string;
  /** Which dialog shows them. The two are different dialogs at different moments. */
  where: string;
  detail: string;
}

/** Both warnings, in the order a person meets them. */
export const WINDOWS_INSTALL_WARNINGS: readonly WindowsInstallWarning[] = [
  {
    id: "unsigned",
    name: "Windows protected your PC",
    where: "Microsoft Defender SmartScreen, the moment you open the installer",
    detail:
      "This is the unsigned-app warning. SmartScreen shows it for a file that carries no code"
      + " signature and that it holds no record of. Getting to the installer from here means pressing"
      + " More info and then Run anyway.",
  },
  {
    id: "unknown-publisher",
    name: "Unknown publisher",
    where: "the User Account Control box that asks for permission to install",
    detail:
      "This is the unknown-publisher warning. Where a signed installer names the company that signed"
      + " it, OSL's installer leaves that line reading Unknown, because there is no signature to read"
      + " a name from.",
  },
];

/**
 * The one sentence that says what the two warnings are not. Exactly one
 * sentence, deliberately: it is a statement of fact about what Windows checked,
 * not reassurance, and repeating it would turn it into reassurance.
 */
export const NEITHER_IS_PROOF_SENTENCE =
  "Neither warning is proof that this installer is unsafe; both of them report a signature that is"
  + " missing, not something Windows found inside the file.";

/** What the check does buy, and the three things it does not. */
export const CHECKSUM_LIMITS: readonly string[] = [
  "A matching hash tells you the bytes on your disk are the bytes this release published, and nothing more.",
  "It does not remove either warning. For as long as OSL is unsigned, both of them come up on every"
    + " release, on every machine, however many people have already installed it.",
  "On Windows 11 with Smart App Control switched on, an unsigned installer is blocked outright rather"
    + " than warned about, and no checksum changes that.",
];

export const LONGER_CHECK_REFERENCE = "docs/release/verify-your-download.md";

export interface UnsignedPublisherNoteRelease {
  /** The published release asset, e.g. `OSL-2.0.0.exe`. */
  installerName: string;
  /** Lower-case hex SHA-256, as `SHA256SUMS.txt` lists it. */
  sha256: string;
}

const INSTALLER_NAME_PATTERN = /^OSL-[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?\.exe$/u;
const SHA256_PATTERN = /^[0-9a-f]{64}$/u;

/**
 * The only way to get a release into the note. A checksum nobody measured
 * cannot be rendered: anything that is not 64 hex characters for a real
 * `OSL-<version>.exe` is refused here rather than printed as if it were true.
 */
export function unsignedPublisherNoteRelease(
  installerName: string,
  sha256: string,
): UnsignedPublisherNoteRelease {
  if (!INSTALLER_NAME_PATTERN.test(installerName)) {
    throw new Error(`unsigned-publisher note: not a release installer name: ${installerName}`);
  }
  const normalised = sha256.trim().toLowerCase();
  if (!SHA256_PATTERN.test(normalised)) {
    throw new Error("unsigned-publisher note: expected a 64-character hex SHA-256 from the release checksum list");
  }
  return { installerName, sha256: normalised };
}

/** The exact command, character for character, that the note tells people to run. */
export function checksumCommand(installerName: string): string {
  return `Get-FileHash -Algorithm SHA256 -LiteralPath .\\${installerName}`;
}

/** The value `Get-FileHash` prints, which is upper case. */
export function expectedHashValue(release: UnsignedPublisherNoteRelease): string {
  return release.sha256.toUpperCase();
}

/** The same value as the release's signed `SHA256SUMS.txt` lists it: lower case, two spaces. */
export function checksumListLine(release: UnsignedPublisherNoteRelease): string {
  return `${release.sha256}  ${release.installerName}`;
}

/**
 * Claims this note is not allowed to make. Every one of them is a promise that
 * Windows will come round -- the exact thing nobody can promise while OSL is
 * unsigned, and the exact thing an install note is tempted to say to keep a
 * reader moving.
 */
export const PROMISE_PATTERNS: readonly { id: string; pattern: RegExp }[] = [
  { id: "windows-will-trust", pattern: /\bwindows\b[^.;]{0,60}\bwill\b[^.;]{0,40}\b(trust|accept|allow|approve|recogni[sz]e|verify)\b/iu },
  { id: "warning-will-clear", pattern: /\b(smartscreen|warning|warnings|prompt|block|flag)\b[^.;]{0,60}\b(will|shall|should)\b[^.;]{0,40}\b(go away|disappear|stop|vanish|clear|lift|pass)\b/iu },
  { id: "no-warning-promised", pattern: /\b(no|zero)\s+(more\s+)?warnings?\b/iu },
  { id: "will-not-warn", pattern: /\b(will not|won'?t|never|no longer)\b[^.;]{0,30}\b(warn|warns|warning|appear|show up|block|blocked)\b/iu },
  { id: "becomes-trusted", pattern: /\b(is|are|be|becomes?|gets?|will\s+be)\s+(fully\s+)?trusted\b/iu },
  { id: "verified-publisher", pattern: /\b(verified|trusted)\s+publisher\b/iu },
  { id: "safe-to-bypass", pattern: /\b(safe|fine|okay|ok)\b[^.;]{0,30}\b(ignore|bypass|skip|click through|dismiss|override)\b/iu },
  { id: "guaranteed-safe", pattern: /\b(guarantee[sd]?|promise[sd]?)\b[^.;]{0,60}\b(safe|trust\w*|clean)\b/iu },
  { id: "reputation-will-fix-it", pattern: /\breputation\b[^.;]{0,60}\b(will|once|after)\b/iu },
];

/**
 * Returns the id of every forbidden promise found in `text`. The note passes
 * only on an empty list, so "zero promises" is a measured number rather than a
 * reading of the prose.
 */
export function unsignedPublisherNotePromises(text: string): string[] {
  return PROMISE_PATTERNS.filter(({ pattern }) => pattern.test(text)).map(({ id }) => id);
}

/** Sentences, counted the way a reader counts them. */
export function sentences(text: string): string[] {
  return text
    .split(/(?<=[.!?])\s+/u)
    .map((sentence) => sentence.trim())
    .filter((sentence) => sentence.length > 0);
}

/** Every word of the note, in reading order, with no markup in the way. */
export function unsignedPublisherNoteText(release: UnsignedPublisherNoteRelease): string {
  const lines = [
    UNSIGNED_PUBLISHER_NOTE_TITLE,
    UNSIGNED_PUBLISHER_NOTE_LEAD,
    ...WINDOWS_INSTALL_WARNINGS.flatMap((warning) => [
      `"${warning.name}" - ${warning.where}.`,
      warning.detail,
    ]),
    NEITHER_IS_PROOF_SENTENCE,
    "Check the download yourself",
    `What you can check is that the file on your disk is the file this release published. Open`
      + ` PowerShell in the folder you downloaded ${release.installerName} into and run this exact command:`,
    checksumCommand(release.installerName),
    `Expected Hash value for ${release.installerName}:`,
    expectedHashValue(release),
    "The release's signed SHA256SUMS.txt lists the same value in lower case:",
    checksumListLine(release),
    "Compare it character for character. If it does not match, delete the file and download it again"
      + " from the release page, and do not run the copy you have. The longer check, which also"
      + ` verifies the Minisign signature over that list, is written out in ${LONGER_CHECK_REFERENCE}.`,
    "What checking the hash does not do",
    ...CHECKSUM_LIMITS,
  ];
  return lines.join("\n");
}

function escapeHtml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;");
}

function warningMarkup(warning: WindowsInstallWarning): string {
  return `<li class="upn-warning" id="unsigned-publisher-note-warning-${warning.id}" data-warning="${warning.id}">
      <p class="upn-warning-name" data-warning-name="${warning.id}"><strong>&ldquo;${escapeHtml(warning.name)}&rdquo;</strong> &mdash; ${escapeHtml(warning.where)}.</p>
      <p class="upn-warning-detail">${escapeHtml(warning.detail)}</p>
    </li>`;
}

/**
 * The note as it is shown. `data-promises` carries the scan result so the count
 * can be read straight off the rendered page in a browser, not only in a unit
 * test.
 */
export function unsignedPublisherNoteMarkup(release: UnsignedPublisherNoteRelease): string {
  const promises = unsignedPublisherNotePromises(unsignedPublisherNoteText(release));
  const command = checksumCommand(release.installerName);
  const installer = escapeHtml(release.installerName);
  return `<section class="unsigned-publisher-note" id="unsigned-publisher-note" aria-labelledby="unsigned-publisher-note-title" data-installer="${installer}" data-expected-sha256="${release.sha256}" data-promises="${escapeHtml(promises.join(","))}" data-promise-count="${promises.length}">
  <header class="upn-header">
    <h1 id="unsigned-publisher-note-title">${escapeHtml(UNSIGNED_PUBLISHER_NOTE_TITLE)}</h1>
    <p class="upn-lead">${escapeHtml(UNSIGNED_PUBLISHER_NOTE_LEAD)}</p>
  </header>
  <h2 class="upn-subhead" id="unsigned-publisher-note-warnings-heading">The two warnings Windows shows</h2>
  <ul class="upn-warnings" aria-labelledby="unsigned-publisher-note-warnings-heading">${WINDOWS_INSTALL_WARNINGS.map(warningMarkup).join("")}</ul>
  <p class="upn-not-proof" id="unsigned-publisher-note-not-proof">${escapeHtml(NEITHER_IS_PROOF_SENTENCE)}</p>
  <h2 class="upn-subhead" id="unsigned-publisher-note-check-heading">Check the download yourself</h2>
  <p class="upn-check-lead">What you can check is that the file on your disk is the file this release published. Open PowerShell in the folder you downloaded ${installer} into and run this exact command:</p>
  <pre class="upn-command" id="unsigned-publisher-note-command"><code>${escapeHtml(command)}</code></pre>
  <p class="upn-expected-label">Expected <code>Hash</code> value for ${installer}:</p>
  <pre class="upn-expected" id="unsigned-publisher-note-expected"><code>${escapeHtml(expectedHashValue(release))}</code></pre>
  <p class="upn-expected-label">The release's signed <code>SHA256SUMS.txt</code> lists the same value in lower case:</p>
  <pre class="upn-checksum-line" id="unsigned-publisher-note-checksum-line"><code>${escapeHtml(checksumListLine(release))}</code></pre>
  <p class="upn-compare">Compare it character for character. If it does not match, delete the file and download it again from the release page, and do not run the copy you have. The longer check, which also verifies the Minisign signature over that list, is written out in <code>${escapeHtml(LONGER_CHECK_REFERENCE)}</code>.</p>
  <h2 class="upn-subhead" id="unsigned-publisher-note-limits-heading">What checking the hash does not do</h2>
  <ul class="upn-limits" aria-labelledby="unsigned-publisher-note-limits-heading">${CHECKSUM_LIMITS.map((limit) => `<li>${escapeHtml(limit)}</li>`).join("")}</ul>
</section>`;
}

/** Mounts the note. It is text: there is nothing on it to press. */
export function renderUnsignedPublisherNote(
  host: HTMLElement,
  release: UnsignedPublisherNoteRelease,
): HTMLElement {
  host.innerHTML = unsignedPublisherNoteMarkup(release);
  const note = host.querySelector<HTMLElement>("#unsigned-publisher-note");
  if (!note) throw new Error("unsigned-publisher note failed to mount");
  return note;
}
