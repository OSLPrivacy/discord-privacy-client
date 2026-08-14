//! TASK 6804 — the recovery-kit *file*: pick it, freeze it, read it, refuse it.
//!
//! The two recovery pages ("Forgot Password" and "Restore Account") each show
//! twelve numbered word boxes and an "Upload recovery kit" chip. The chip is
//! the only reason this module exists: it must reach the *installed Windows
//! file picker*, take the bytes that picker selected, prove they are an OSL
//! recovery kit, and only then hand twelve words to the page. Everything else
//! -- cancelling, a flipped byte, a kit written by another version, a kit
//! belonging to somebody else, a file that was never a kit -- has to leave the
//! boxes and the account exactly as they were, and say so in one plain
//! sentence that contains no recovery word.
//!
//! Three defects this module is shaped to prevent:
//!
//! 1. **A label instead of a load.** The reviewed mock flips the chip to
//!    "Recovery kit loaded" on click. A label is not an import: the only way to
//!    obtain a [`LoadedRecoveryKit`] here is to hand [`load_frozen_recovery_kit`]
//!    a [`FrozenKitSelection`], and a `FrozenKitSelection` can only be built by
//!    reading a real path off the disk. There is no constructor that takes
//!    words, and no field is publicly writable.
//!
//! 2. **A second read.** Selecting a path and then opening it again at
//!    validation time is a time-of-check/time-of-use hole -- the file can be
//!    swapped between the two reads, so the bytes that were checked are not the
//!    bytes that were used. The selection is therefore *frozen*: one
//!    `fs::read`, one SHA-256 over that buffer, and validation is only ever
//!    given the frozen slice. [`LoadedRecoveryKit::validated_sha256`] is
//!    recomputed over the exact slice the parser consumed, so a caller can
//!    prove the checked bytes and the selected bytes are the same bytes.
//!
//! 3. **Words in the error path.** A refusal is a fixed sentence chosen from
//!    [`RecoveryKitRefusal`]; it never interpolates file content. `Debug` is
//!    written by hand for every type that can hold a phrase, because a derived
//!    `Debug` is how a recovery word reaches a log line.
//!
//! Nothing here writes anything. The importer this feeds is the existing
//! authenticated one -- `password_lifecycle::import_native_identity_phrase`
//! for Restore Account and the password-recovery verifier for Forgot Password
//! -- reached through each page's ordinary form submit. Uploading a kit fills
//! the boxes; it does not skip a journey.

use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

use crate::password_lifecycle::{native_user_id, parse_identity_phrase};

/// First line of every kit file. A file that does not start with this is not a
/// recovery kit, whatever its extension says.
pub const RECOVERY_KIT_MAGIC: &str = "OSL-RECOVERY-KIT";

/// The only kit version this build can read. TASK 6806 writes the same number.
pub const RECOVERY_KIT_VERSION: u32 = 1;

/// Extension offered by the Windows picker filter.
pub const RECOVERY_KIT_EXTENSION: &str = "oslkit";

/// A kit is six short lines. Anything larger was never one, and refusing by
/// size before decoding keeps a hostile file from being buffered as UTF-8.
pub const RECOVERY_KIT_MAX_BYTES: usize = 4096;

/// Words in each phrase. The page draws exactly this many numbered boxes.
pub const RECOVERY_KIT_WORDS: usize = 12;

const FIELD_VERSION: &str = "version: ";
const FIELD_USER_ID: &str = "user-id: ";
const FIELD_IDENTITY: &str = "identity-phrase: ";
const FIELD_PASSWORD: &str = "password-phrase: ";
const FIELD_DIGEST: &str = "digest: ";

/// Which recovery page asked, and therefore which of the two phrases in the
/// kit belongs in its boxes.
///
/// This is not cosmetic. Forgot Password resets a password with the *password
/// recovery phrase*; Restore Account rebuilds an identity with the *identity
/// phrase*. Putting one in the other's boxes sends the owner through a journey
/// that cannot succeed, with a secret they did not mean to use there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryKitPage {
    /// `onboardingRoute === "account-recovery"` — the password reset journey.
    ForgotPassword,
    /// `onboardingRoute === "import"` — the identity restore journey.
    RestoreAccount,
}

impl RecoveryKitPage {
    pub fn id(self) -> &'static str {
        match self {
            Self::ForgotPassword => "forgot-password",
            Self::RestoreAccount => "restore-account",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "forgot-password" => Some(Self::ForgotPassword),
            "restore-account" => Some(Self::RestoreAccount),
            _ => None,
        }
    }

    /// The label the page's own form uses for the phrase it needs.
    pub fn phrase_label(self) -> &'static str {
        match self {
            Self::ForgotPassword => "password recovery phrase",
            Self::RestoreAccount => "identity phrase",
        }
    }

    pub const ALL: [Self; 2] = [Self::ForgotPassword, Self::RestoreAccount];
}

/// Every way a selection can be refused, and the exact sentence shown.
///
/// The sentences are constants on purpose. An error that formats any part of
/// the file is how a recovery word ends up in a toast, a `tracing` span or a
/// crash report, and the boxes are the *only* place a word is allowed to go.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryKitRefusal {
    /// The picker returned a path that could not be read at all.
    Unreadable,
    /// Not an OSL recovery kit: wrong first line, wrong shape, not text.
    NotAKitFile,
    /// Structurally a kit, written by a version this build cannot read.
    UnsupportedVersion,
    /// Structurally a kit, but the digest does not match the contents.
    Damaged,
    /// A kit whose phrases are not twelve lowercase words, or whose account id
    /// is not an OSL account id.
    Malformed,
    /// A kit for a different OSL account than the one being recovered.
    WrongIdentity,
}

impl RecoveryKitRefusal {
    /// One plain sentence. No file content, no path, no word.
    pub fn message(self) -> &'static str {
        match self {
            Self::Unreadable => "OSL could not read that file. Nothing was changed.",
            Self::NotAKitFile => "That file is not an OSL recovery kit. Nothing was changed.",
            Self::UnsupportedVersion => {
                "That recovery kit was written by a different version of OSL. Nothing was changed."
            }
            Self::Damaged => "That recovery kit file is damaged. Nothing was changed.",
            Self::Malformed => "That recovery kit is not complete. Nothing was changed.",
            Self::WrongIdentity => {
                "That recovery kit belongs to a different OSL account. Nothing was changed."
            }
        }
    }

    /// A stable machine tag for checks and receipts. Also word-free.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::NotAKitFile => "not-a-kit-file",
            Self::UnsupportedVersion => "unsupported-version",
            Self::Damaged => "damaged",
            Self::Malformed => "malformed",
            Self::WrongIdentity => "wrong-identity",
        }
    }
}

impl fmt::Display for RecoveryKitRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for RecoveryKitRefusal {}

/// The decoded kit. Both phrases, held in zeroizing buffers.
///
/// `Debug` is hand-written: deriving it would print two recovery phrases the
/// first time anything in the hub `{:?}`-formatted a kit.
pub struct RecoveryKitContents {
    user_id: String,
    identity_phrase: Zeroizing<String>,
    password_phrase: Zeroizing<String>,
}

impl fmt::Debug for RecoveryKitContents {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveryKitContents")
            .field("user_id", &"[REDACTED]")
            .field("identity_phrase", &"[REDACTED]")
            .field("password_phrase", &"[REDACTED]")
            .finish()
    }
}

impl RecoveryKitContents {
    pub fn new(user_id: String, identity_phrase: String, password_phrase: String) -> Self {
        Self {
            user_id,
            identity_phrase: Zeroizing::new(identity_phrase),
            password_phrase: Zeroizing::new(password_phrase),
        }
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// The phrase this page's journey needs, and only that one.
    pub fn phrase_for(&self, page: RecoveryKitPage) -> &str {
        match page {
            RecoveryKitPage::ForgotPassword => self.password_phrase.as_str(),
            RecoveryKitPage::RestoreAccount => self.identity_phrase.as_str(),
        }
    }
}

impl Drop for RecoveryKitContents {
    fn drop(&mut self) {
        self.user_id.zeroize();
    }
}

/// The canonical bytes a kit file holds, minus the digest line.
fn canonical_body(contents: &RecoveryKitContents) -> Zeroizing<String> {
    Zeroizing::new(format!(
        "{RECOVERY_KIT_MAGIC}\n{FIELD_VERSION}{RECOVERY_KIT_VERSION}\n{FIELD_USER_ID}{}\n{FIELD_IDENTITY}{}\n{FIELD_PASSWORD}{}\n",
        contents.user_id,
        contents.identity_phrase.as_str(),
        contents.password_phrase.as_str(),
    ))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest.iter() {
        use fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

/// Write the kit form TASK 6806's save picker emits and this reader accepts.
///
/// Kept next to the parser deliberately: a writer that lives somewhere else is
/// how the two halves drift until the app can no longer read its own file. The
/// digest is a corruption check, not a signature -- a kit is plaintext by
/// necessity (it *is* the secret), so there is no key it could be authenticated
/// with that would not also be in the file. It catches a flipped byte, a
/// truncated copy and a hand-edited line; it does not claim to catch a forger,
/// and the identity binding below is what makes a forged kit useless.
pub fn encode_recovery_kit(contents: &RecoveryKitContents) -> Vec<u8> {
    let body = canonical_body(contents);
    let digest = sha256_hex(body.as_bytes());
    format!("{}{FIELD_DIGEST}{digest}\n", body.as_str()).into_bytes()
}

/// Decode and refuse. Order is deliberate and each step is separately named in
/// the refusal, because "that file is wrong" is not something an owner locked
/// out of their account can act on.
pub fn parse_recovery_kit(bytes: &[u8]) -> Result<RecoveryKitContents, RecoveryKitRefusal> {
    if bytes.is_empty() || bytes.len() > RECOVERY_KIT_MAX_BYTES {
        return Err(RecoveryKitRefusal::NotAKitFile);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| RecoveryKitRefusal::NotAKitFile)?;
    let text = Zeroizing::new(text.replace("\r\n", "\n"));

    let mut lines = text.split('\n');
    let magic = lines.next().ok_or(RecoveryKitRefusal::NotAKitFile)?;
    if magic != RECOVERY_KIT_MAGIC {
        return Err(RecoveryKitRefusal::NotAKitFile);
    }
    let mut field = |prefix: &str| -> Result<Zeroizing<String>, RecoveryKitRefusal> {
        let line = lines.next().ok_or(RecoveryKitRefusal::NotAKitFile)?;
        let value = line
            .strip_prefix(prefix)
            .ok_or(RecoveryKitRefusal::NotAKitFile)?;
        Ok(Zeroizing::new(value.to_owned()))
    };
    let version_text = field(FIELD_VERSION)?;
    let user_id = field(FIELD_USER_ID)?;
    let identity_phrase = field(FIELD_IDENTITY)?;
    let password_phrase = field(FIELD_PASSWORD)?;
    let declared_digest = field(FIELD_DIGEST)?;
    // Trailing content past the digest line means this is not the six-line
    // form; only the final newline may remain.
    if lines.next().map(str::trim).unwrap_or("") != "" || lines.next().is_some() {
        return Err(RecoveryKitRefusal::NotAKitFile);
    }

    // Integrity before meaning. A corrupt byte anywhere in the body has to read
    // as "damaged", not as "wrong version" or "wrong account" -- telling an
    // owner their kit belongs to somebody else when a disk flipped a bit sends
    // them to delete the only copy they have.
    let body = Zeroizing::new(format!(
        "{RECOVERY_KIT_MAGIC}\n{FIELD_VERSION}{}\n{FIELD_USER_ID}{}\n{FIELD_IDENTITY}{}\n{FIELD_PASSWORD}{}\n",
        version_text.as_str(),
        user_id.as_str(),
        identity_phrase.as_str(),
        password_phrase.as_str(),
    ));
    if !constant_time_eq(sha256_hex(body.as_bytes()).as_bytes(), declared_digest.as_bytes()) {
        return Err(RecoveryKitRefusal::Damaged);
    }

    let version = version_text
        .parse::<u32>()
        .map_err(|_| RecoveryKitRefusal::NotAKitFile)?;
    if version != RECOVERY_KIT_VERSION {
        return Err(RecoveryKitRefusal::UnsupportedVersion);
    }

    if !is_osl_account_id(user_id.as_str())
        || !is_twelve_lowercase_words(identity_phrase.as_str())
        || !is_twelve_lowercase_words(password_phrase.as_str())
    {
        return Err(RecoveryKitRefusal::Malformed);
    }

    Ok(RecoveryKitContents::new(
        user_id.as_str().to_owned(),
        identity_phrase.as_str().to_owned(),
        password_phrase.as_str().to_owned(),
    ))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

fn is_osl_account_id(value: &str) -> bool {
    value.len() == 44
        && value.starts_with("osl_")
        && value[4..].bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_twelve_lowercase_words(phrase: &str) -> bool {
    let words: Vec<&str> = phrase.split(' ').collect();
    words.len() == RECOVERY_KIT_WORDS
        && words
            .iter()
            .all(|word| !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_lowercase()))
}

/// The account id the kit's identity phrase actually derives to.
///
/// This is what makes "wrong identity" mean something on a machine that has no
/// account yet. A kit spliced together from two owners' files -- one owner's
/// identity phrase under another's account id -- fails here on the Restore
/// Account page, where there is nothing local to compare against.
pub fn derived_account_id(identity_phrase: &str) -> Result<String, RecoveryKitRefusal> {
    let entropy = parse_identity_phrase(identity_phrase).map_err(|_| RecoveryKitRefusal::Malformed)?;
    let identity = keystore::identity_from_entropy(entropy, "osl-recovery-kit".to_owned());
    Ok(native_user_id(&identity))
}

/// A path the picker returned, read exactly once.
///
/// `bytes` is the single copy every later stage sees. There is no method that
/// re-opens `path`.
pub struct FrozenKitSelection {
    path: String,
    sha256: String,
    bytes: Vec<u8>,
}

impl fmt::Debug for FrozenKitSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrozenKitSelection")
            .field("path", &self.path)
            .field("sha256", &self.sha256)
            .field("bytes", &format_args!("[{} bytes withheld]", self.bytes.len()))
            .finish()
    }
}

impl FrozenKitSelection {
    /// One read, one hash. Everything downstream is given the buffer.
    pub fn freeze(path: &Path) -> Result<Self, RecoveryKitRefusal> {
        let metadata = std::fs::metadata(path).map_err(|_| RecoveryKitRefusal::Unreadable)?;
        if !metadata.is_file() {
            return Err(RecoveryKitRefusal::Unreadable);
        }
        let bytes = std::fs::read(path).map_err(|_| RecoveryKitRefusal::Unreadable)?;
        let sha256 = sha256_hex(&bytes);
        Ok(Self {
            path: path.to_string_lossy().into_owned(),
            sha256,
            bytes,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl Drop for FrozenKitSelection {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// What a page is allowed to know after a successful load.
///
/// Constructed nowhere but [`load_frozen_recovery_kit`]. The words are the one
/// secret that must cross into the renderer -- they are going into visible
/// boxes the owner is about to read -- so they are here, and nothing else is.
pub struct LoadedRecoveryKit {
    page: RecoveryKitPage,
    path: String,
    selected_sha256: String,
    validated_sha256: String,
    account_id: String,
    words: Vec<Zeroizing<String>>,
}

impl fmt::Debug for LoadedRecoveryKit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoadedRecoveryKit")
            .field("page", &self.page)
            .field("path", &self.path)
            .field("selected_sha256", &self.selected_sha256)
            .field("validated_sha256", &self.validated_sha256)
            .field("account_id", &self.account_id)
            .field("words", &format_args!("[{} words withheld]", self.words.len()))
            .finish()
    }
}

impl LoadedRecoveryKit {
    pub fn page(&self) -> RecoveryKitPage {
        self.page
    }

    /// The path frozen at selection time, never re-derived.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// SHA-256 of the bytes the picker's selection was read into.
    pub fn selected_sha256(&self) -> &str {
        &self.selected_sha256
    }

    /// SHA-256 recomputed over the exact slice the parser consumed. Equal to
    /// [`Self::selected_sha256`] by construction; exposed so a check can prove
    /// the selected bytes are the validated bytes rather than trust the claim.
    pub fn validated_sha256(&self) -> &str {
        &self.validated_sha256
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    /// Twelve words, in order, for the twelve numbered boxes.
    pub fn words(&self) -> Vec<String> {
        self.words.iter().map(|word| word.as_str().to_owned()).collect()
    }

    pub fn word_count(&self) -> usize {
        self.words.len()
    }
}

/// Validate a frozen selection for one page.
///
/// `expected_account_id` is `Some` on a device that already holds an account
/// (the Forgot Password case): a kit for a different account is refused before
/// a single box changes. It is `None` on a fresh device, where the internal
/// binding check below is the whole identity comparison.
pub fn load_frozen_recovery_kit(
    page: RecoveryKitPage,
    selection: &FrozenKitSelection,
    expected_account_id: Option<&str>,
) -> Result<LoadedRecoveryKit, RecoveryKitRefusal> {
    let bytes = selection.bytes();
    let validated_sha256 = sha256_hex(bytes);
    let contents = parse_recovery_kit(bytes)?;

    // Identity comparison, both directions it can be wrong.
    if derived_account_id(contents.phrase_for(RecoveryKitPage::RestoreAccount))? != contents.user_id()
    {
        return Err(RecoveryKitRefusal::WrongIdentity);
    }
    if let Some(expected) = expected_account_id {
        if expected != contents.user_id() {
            return Err(RecoveryKitRefusal::WrongIdentity);
        }
    }

    let words = contents
        .phrase_for(page)
        .split(' ')
        .map(|word| Zeroizing::new(word.to_owned()))
        .collect::<Vec<_>>();
    if words.len() != RECOVERY_KIT_WORDS {
        return Err(RecoveryKitRefusal::Malformed);
    }

    Ok(LoadedRecoveryKit {
        page,
        path: selection.path().to_owned(),
        selected_sha256: selection.sha256().to_owned(),
        validated_sha256,
        account_id: contents.user_id().to_owned(),
        words,
    })
}

/// The installed file picker, as a port.
///
/// The desktop binary's implementation is the Windows dialog
/// (`tauri_plugin_dialog`, `blocking_pick_file`) in `recovery_kit_picker.rs`.
/// `Ok(None)` is cancellation and must stay distinguishable from every refusal:
/// cancelling is not an error and must not put a sentence on the screen.
pub trait RecoveryKitPicker {
    fn pick(&self) -> Result<Option<PathBuf>, RecoveryKitRefusal>;
}

/// The whole chip journey: pick, freeze, read, validate.
///
/// Note the shape of the signature -- there is no path or byte parameter. A
/// caller cannot name a file. That is what stops the renderer (or anything that
/// reaches `invoke`) from using the recovery pages as an arbitrary-file reader,
/// and it is why the Tauri command takes only a page id.
pub fn load_recovery_kit_through_picker(
    page: RecoveryKitPage,
    picker: &dyn RecoveryKitPicker,
    expected_account_id: Option<&str>,
) -> Result<Option<LoadedRecoveryKit>, RecoveryKitRefusal> {
    let Some(path) = picker.pick()? else {
        // Cancelled. Nothing read, nothing changed, nothing said.
        return Ok(None);
    };
    let selection = FrozenKitSelection::freeze(&path)?;
    load_frozen_recovery_kit(page, &selection, expected_account_id).map(Some)
}

// The behaviour tests for this module live in
// `tests/task_6804_recovery_kit_upload.rs`, not in a `#[cfg(test)] mod tests`
// here. `cargo test --lib` does not build in this tree at all -- 71 pre-existing
// errors in other modules' test code stop it before anything of ours runs -- so
// an in-module test would be a test nothing executes. The integration target
// compiles the library without `cfg(test)`, which is why it runs, and it works
// on real files written to a real temporary directory rather than on a
// hand-built `FrozenKitSelection`, so the byte reader is exercised too.
