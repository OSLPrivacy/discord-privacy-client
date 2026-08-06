//! Pure production helpers behind the desktop binary's Tauri command surface.
//!
//! `src/main.rs` is a `[[bin]]` with `required-features = ["desktop"]`, whose
//! dependency tree cannot be built on a Linux CI host at all. Everything inside
//! it -- including every `#[cfg(test)]` module -- is therefore compiled by
//! nothing and run by nothing: `cargo test --features core --lib` is what CI
//! and every accept command actually execute. Same reason `qa_selftest_request`
//! already lives here.
//!
//! So the parts of that command surface that carry the real gates -- send
//! authority, checked-host ordering, reviewed-run identity binding, the
//! restart-proof drain order, and the registered-command/ACL surface itself --
//! live in the library, where they are compiled and proven. `main.rs` keeps
//! only the `#[tauri::command]` wrappers that call them.

use crate::autoscrub_run::{self, AutoScrubFleetStatus, AutoScrubReviewedRunRequest};
use crate::broker;
use crate::browser_footprint::{self, FootprintObservation, NativeBrowserImportBinding};
use crate::core_bridge::HubCoreState;
use crate::discord_carrier_geometry::CarrierDecision;
use crate::identity_binding_verifier::{
    AccountRef, BindingScope, IdentityBindingVerifier, PinnedOwner,
};
use crate::models::ServiceKind;
use crate::native_apps::BrowserImportId;
use crate::native_discord_adapter::{
    guided_deletion, DiscordCarrierLayout, NativeDiscordComposerState,
};
use crate::scrub_erasure::{self, ComposedErasureRequest, ErasureRequestInput};
use crate::service_host::ActiveServiceHost;
use crate::website_driver::{
    WebsiteDriver, WebsiteLiveRunProgress, WebsiteNamedControl, WebsitePageRequest,
    WebsiteTextPlacement,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Mutex};

pub fn build_review_ui_identity_binding_verifier(
    core: &HubCoreState,
) -> Result<IdentityBindingVerifier, String> {
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .as_ref()
        .cloned()
        .ok_or_else(|| "Unlock an OSL identity before starting AutoScrub".to_owned())?;
    Ok(IdentityBindingVerifier::new(PinnedOwner::from_identity(
        &identity,
    )))
}

/// Compose an erasure request for the desktop command surface.
///
/// This is intentionally a local transform: the user receives text to review
/// and send from their own mailbox; OSL never transports the request.
pub fn compose_erasure_request_for_user(
    input: ErasureRequestInput,
) -> Result<ComposedErasureRequest, String> {
    scrub_erasure::compose_erasure_request(&input).map_err(|_| {
        "Complete provider, account identifier, and data categories are required".to_owned()
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedEmailOpenMessageReadRequest {
    pub page_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedEmailOpenMessageRead {
    pub cover_message: String,
    pub conversation_identity: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedEmailLiveRunProgressRequest {
    pub page_url: String,
}

const ORDINARY_SEND_PROGRESS_LABEL: &str = "ordinary send progress";
const ORDINARY_SEND_PROGRESS_MAX_BYTES: u64 = 16 * 1024;
const ORDINARY_SEND_STEPS: [&str; 5] = [
    "private_save",
    "service_acceptance",
    "local_save",
    "receiver_publish",
    "final_confirmation",
];
const ORDINARY_SEND_LOCAL_SAVE_CONTROL: &str = "Save local";
const ORDINARY_SEND_RECEIVER_PUBLISH_CONTROL: &str = "Publish to receiver";
const ORDINARY_SEND_FINAL_CONFIRMATION_CONTROL: &str = "Confirm final";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OrdinarySendProgressRequest {
    pub page_url: String,
    pub draft_text: String,
    pub progress_path: PathBuf,
    pub max_steps: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdinarySendProgressStep {
    pub name: String,
    pub completed: bool,
    #[serde(default)]
    pub run_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrdinarySendProgress {
    pub send_id: String,
    pub steps: Vec<OrdinarySendProgressStep>,
    pub final_confirmation: bool,
}

pub struct OrdinarySendProgressStore {
    path: PathBuf,
}

impl OrdinarySendProgressStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load_or_new(&self, send_id: &str) -> Result<OrdinarySendProgress, String> {
        let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
            &self.path,
            ORDINARY_SEND_PROGRESS_MAX_BYTES,
            ORDINARY_SEND_PROGRESS_LABEL,
        )?
        else {
            return Ok(new_ordinary_send_progress(send_id));
        };
        let progress: OrdinarySendProgress =
            serde_json::from_slice(&bytes).map_err(|_| "ordinary send progress is invalid")?;
        if progress.send_id == send_id && progress.step_names() == ORDINARY_SEND_STEPS {
            Ok(progress.with_derived_confirmation())
        } else {
            Ok(new_ordinary_send_progress(send_id))
        }
    }

    fn save(&self, progress: &OrdinarySendProgress) -> Result<(), String> {
        let bytes = serde_json::to_vec(&progress.clone().with_derived_confirmation())
            .map_err(|_| "ordinary send progress could not be encoded")?;
        crate::atomic_file::write_recoverable(&self.path, &bytes, ORDINARY_SEND_PROGRESS_LABEL)
    }
}

impl OrdinarySendProgress {
    pub fn completed_step_names(&self) -> Vec<&str> {
        self.steps
            .iter()
            .filter(|step| step.completed)
            .map(|step| step.name.as_str())
            .collect()
    }

    fn step_names(&self) -> Vec<&str> {
        self.steps.iter().map(|step| step.name.as_str()).collect()
    }

    fn with_derived_confirmation(mut self) -> Self {
        for step in &mut self.steps {
            if step.completed && step.run_count == 0 {
                step.run_count = 1;
            }
        }
        self.final_confirmation = self.steps.len() == ORDINARY_SEND_STEPS.len()
            && self.steps.iter().all(|step| step.completed);
        self
    }

    fn mark_completed(&mut self, name: &str) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.name == name) {
            if !step.completed {
                step.completed = true;
                step.run_count = step.run_count.saturating_add(1);
            }
        }
        self.final_confirmation = self.steps.iter().all(|step| step.completed);
    }
}

pub fn ordinary_send_stable_id(page_url: &str, draft_text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ordinary-send-v1\0");
    hasher.update(page_url.trim().as_bytes());
    hasher.update(b"\0");
    hasher.update(draft_text.as_bytes());
    let digest = hasher.finalize();
    format!("ordinary-send-v1-{}", hex_prefix(&digest, 12))
}

pub fn read_ordinary_send_progress(
    request: &OrdinarySendProgressRequest,
) -> Result<OrdinarySendProgress, String> {
    let send_id = ordinary_send_stable_id(&request.page_url, &request.draft_text);
    OrdinarySendProgressStore::new(request.progress_path.clone()).load_or_new(&send_id)
}

pub fn send_ordinary_message_with_progress<D>(
    driver: &mut D,
    request: OrdinarySendProgressRequest,
) -> Result<OrdinarySendProgress, String>
where
    D: WebsiteDriver,
{
    if request.page_url.trim().is_empty() {
        return Err("ordinary send requires an open service page".to_owned());
    }
    if request.draft_text.is_empty() {
        return Err("ordinary send requires draft text".to_owned());
    }

    let send_id = ordinary_send_stable_id(&request.page_url, &request.draft_text);
    let store = OrdinarySendProgressStore::new(request.progress_path.clone());
    let mut progress = store.load_or_new(&send_id)?;
    let page = driver
        .find_page(WebsitePageRequest {
            url: request.page_url,
        })
        .map_err(|error| error.to_string())?;
    driver.read_page(&page).map_err(|error| error.to_string())?;
    let max_steps = request.max_steps.unwrap_or(ORDINARY_SEND_STEPS.len());

    for (index, step) in ORDINARY_SEND_STEPS.iter().enumerate() {
        if index >= max_steps {
            break;
        }
        if progress
            .steps
            .iter()
            .any(|existing| existing.name == *step && existing.completed)
        {
            continue;
        }
        match *step {
            "private_save" => {}
            "service_acceptance" => driver
                .place_text(WebsiteTextPlacement {
                    page: page.clone(),
                    text: request.draft_text.clone(),
                })
                .map_err(|error| error.to_string())?,
            "local_save" => {
                press_ordinary_send_control(driver, &page, ORDINARY_SEND_LOCAL_SAVE_CONTROL)?
            }
            "receiver_publish" => {
                press_ordinary_send_control(driver, &page, ORDINARY_SEND_RECEIVER_PUBLISH_CONTROL)?
            }
            "final_confirmation" => press_ordinary_send_control(
                driver,
                &page,
                ORDINARY_SEND_FINAL_CONFIRMATION_CONTROL,
            )?,
            _ => unreachable!("ordinary send step list is fixed"),
        }
        progress.mark_completed(step);
        store.save(&progress)?;
    }

    Ok(progress.with_derived_confirmation())
}

fn new_ordinary_send_progress(send_id: &str) -> OrdinarySendProgress {
    OrdinarySendProgress {
        send_id: send_id.to_owned(),
        steps: ORDINARY_SEND_STEPS
            .iter()
            .map(|name| OrdinarySendProgressStep {
                name: (*name).to_owned(),
                completed: false,
                run_count: 0,
            })
            .collect(),
        final_confirmation: false,
    }
}

fn press_ordinary_send_control<D>(
    driver: &mut D,
    page: &crate::website_driver::WebsitePage,
    name: &str,
) -> Result<(), String>
where
    D: WebsiteDriver,
{
    driver
        .press_named_control(WebsiteNamedControl {
            page: page.clone(),
            name: name.to_owned(),
        })
        .map_err(|error| error.to_string())
}

pub fn read_protected_email_open_message_with_driver<D>(
    driver: &mut D,
    request: ProtectedEmailOpenMessageReadRequest,
) -> Result<ProtectedEmailOpenMessageRead, String>
where
    D: WebsiteDriver,
{
    if request.page_url.trim().is_empty() {
        return Err("Protected email reader requires an open service page".to_owned());
    }

    let page = driver
        .find_page(WebsitePageRequest {
            url: request.page_url,
        })
        .map_err(|error| error.to_string())?;
    let selected = driver
        .read_selected_email(&page)
        .map_err(|error| error.to_string())?;

    Ok(ProtectedEmailOpenMessageRead {
        cover_message: selected.body,
        conversation_identity: selected.conversation_identity,
    })
}

pub fn read_protected_email_live_run_progress_with_driver<D>(
    driver: &mut D,
    request: ProtectedEmailLiveRunProgressRequest,
) -> Result<WebsiteLiveRunProgress, String>
where
    D: WebsiteDriver,
{
    if request.page_url.trim().is_empty() {
        return Err("Protected email progress requires an open service page".to_owned());
    }

    let page = driver
        .find_page(WebsitePageRequest {
            url: request.page_url,
        })
        .map_err(|error| error.to_string())?;
    driver
        .read_live_run_progress(&page)
        .map_err(|error| error.to_string())
}

pub fn require_review_ui_identity_binding_from_verifier(
    verifier: &IdentityBindingVerifier,
    request: &AutoScrubReviewedRunRequest,
) -> Result<(), String> {
    let account = AccountRef {
        service_id: service_kind_id(request.service_id).to_owned(),
        account_id: request.account_id.clone(),
    };
    verifier
        .verify(&account, BindingScope::ScrubDeletion)
        .map_err(|_| "A reviewed identity binding is required before starting AutoScrub".to_owned())
}

pub fn start_autoscrub_reviewed_run_after_review_ui_binding<T, Start>(
    verifier: &IdentityBindingVerifier,
    request: AutoScrubReviewedRunRequest,
    start: Start,
) -> Result<T, String>
where
    Start: FnOnce(AutoScrubReviewedRunRequest) -> Result<T, String>,
{
    require_review_ui_identity_binding_from_verifier(verifier, &request)?;
    start(request)
}

pub fn start_autoscrub_reviewed_run_inner(
    core: &HubCoreState,
    request: AutoScrubReviewedRunRequest,
) -> Result<AutoScrubFleetStatus, String> {
    start_autoscrub_reviewed_run_checked(
        core,
        request,
        build_review_ui_identity_binding_verifier,
        |core, request| autoscrub_run::start_reviewed_run(&core.osl, request),
    )
}

pub fn start_autoscrub_reviewed_run_checked<BuildVerifier, Start>(
    core: &HubCoreState,
    request: AutoScrubReviewedRunRequest,
    build_verifier: BuildVerifier,
    start: Start,
) -> Result<AutoScrubFleetStatus, String>
where
    BuildVerifier: FnOnce(&HubCoreState) -> Result<IdentityBindingVerifier, String>,
    Start:
        FnOnce(&HubCoreState, AutoScrubReviewedRunRequest) -> Result<AutoScrubFleetStatus, String>,
{
    let verifier = build_verifier(core)?;
    require_review_ui_identity_binding_from_verifier(&verifier, &request)?;
    start(core, request)
}

pub struct CheckedHost {
    pub context_epoch: u64,
    pub active: ActiveServiceHost,
    pub owner_osl_user_id: String,
    pub scope_binding: String,
}

impl CheckedHost {
    pub fn attended_operator_names(&self) -> Result<Vec<String>, String> {
        let _ = self;
        Err("Hosted session scan requires a reviewed attended operator-name binding".to_owned())
    }
}

pub fn checked_hosted_session_scan_flow<BuildChecked, BindOperators, Scan, Recheck>(
    build_checked: BuildChecked,
    bind_operators: BindOperators,
    scan: Scan,
    recheck: Recheck,
) -> Result<crate::native_discord_adapter::guided_deletion::DeletionScan, String>
where
    BuildChecked: FnOnce() -> Result<CheckedHost, String>,
    BindOperators: FnOnce(&CheckedHost) -> Result<Vec<String>, String>,
    Scan: FnOnce(
        &CheckedHost,
        &[String],
    )
        -> Result<crate::native_discord_adapter::guided_deletion::DeletionScan, String>,
    Recheck: FnOnce(&CheckedHost) -> Result<(), String>,
{
    let checked = build_checked()?;
    let operator_names = bind_operators(&checked)?;
    if operator_names.is_empty() {
        return Err(
            "Hosted session scan requires a reviewed attended operator-name binding".to_owned(),
        );
    }
    let scan = scan(&checked, &operator_names)?;
    if scan.generation != checked.active.generation {
        return Err("Hosted session scan context changed during native scan".to_owned());
    }
    recheck(&checked)?;
    Ok(scan)
}

#[derive(Default)]
struct DiscordGuidedDeletionPlanProducer {
    scan: Option<guided_deletion::DeletionScan>,
    preview: Option<guided_deletion::DeletionPreview>,
}

/// Native-held step-4 state for one Discord guided-deletion route.
///
/// The renderer may choose rows and echo a digest, but it never mints a
/// confirmed plan. This producer stores the last reviewed native scan, builds a
/// preview from that exact scan, and confirms only the last preview against the
/// current native-held scope and generation.
#[derive(Default)]
pub struct DiscordGuidedDeletionPlanState {
    inner: Mutex<DiscordGuidedDeletionPlanProducer>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuidedDeletionRunAuthorityInput {
    pub run_id: String,
    pub attended_action_id: String,
    pub scope_binding_hash: String,
    pub generation: u64,
    pub plan_digest: String,
    pub mode: String,
}

impl DiscordGuidedDeletionPlanState {
    pub fn record_scan(
        &self,
        scan: guided_deletion::DeletionScan,
    ) -> Result<guided_deletion::DeletionScan, String> {
        let mut producer = self
            .inner
            .lock()
            .map_err(|_| "Discord guided-deletion plan state is unavailable".to_owned())?;
        producer.scan = Some(scan.clone());
        producer.preview = None;
        Ok(scan)
    }

    pub fn build_preview(
        &self,
        scan_ordinals: &[usize],
        pro: bool,
    ) -> Result<guided_deletion::DeletionPreview, String> {
        let mut producer = self
            .inner
            .lock()
            .map_err(|_| "Discord guided-deletion plan state is unavailable".to_owned())?;
        let scan = producer
            .scan
            .as_ref()
            .ok_or_else(|| "Run and review a Discord scan before previewing deletion".to_owned())?;
        let preview = guided_deletion::build_preview(scan, scan_ordinals, pro)
            .map_err(|refusal| refusal.reason().to_owned())?;
        producer.preview = Some(preview.clone());
        Ok(preview)
    }

    pub fn confirm_preview(
        &self,
        plan_digest: &str,
        current_scope_binding_hash: &str,
        current_generation: u64,
        authority: GuidedDeletionRunAuthorityInput,
    ) -> Result<guided_deletion::ConfirmedPlan, String> {
        let mut producer = self
            .inner
            .lock()
            .map_err(|_| "Discord guided-deletion plan state is unavailable".to_owned())?;
        let preview = producer.preview.as_ref().ok_or_else(|| {
            "Preview the exact Discord deletion plan before confirming it".to_owned()
        })?;
        let authority = guided_deletion_run_authority(authority, plan_digest)
            .map_err(|refusal| refusal.reason().to_owned())?;
        let confirmed = guided_deletion::confirm_preview(
            preview,
            plan_digest,
            current_scope_binding_hash,
            current_generation,
            Some(authority),
        )
        .map_err(|refusal| refusal.reason().to_owned())?;
        producer.preview = None;
        Ok(confirmed)
    }
}

fn guided_deletion_run_authority(
    input: GuidedDeletionRunAuthorityInput,
    expected_plan_digest: &str,
) -> Result<guided_deletion::DeleteRunAuthority, guided_deletion::PlanRefusal> {
    if input.mode != "attended_delete_run_v1" || input.plan_digest != expected_plan_digest {
        return Err(guided_deletion::PlanRefusal::RunAuthorityStale);
    }
    guided_deletion::DeleteRunAuthority::attended(
        input.run_id,
        input.attended_action_id,
        input.scope_binding_hash,
        input.generation,
        input.plan_digest,
    )
}

pub struct NativeDiscordProductSendAuthority {
    pub carrier: String,
}

pub fn require_native_discord_product_send_authority(
    composer: &NativeDiscordComposerState,
    scope_binding: &str,
    layout: Option<DiscordCarrierLayout>,
) -> Result<NativeDiscordProductSendAuthority, String> {
    let plan = composer.take_prepared_carrier_plan(scope_binding, layout);
    if plan.decision != CarrierDecision::RowOverlay {
        return Err("The protected message is not ready to send; nothing was placed".to_owned());
    }
    let carrier = plan.cover_text().ok_or_else(|| {
        "The protected message is not ready to send; nothing was placed".to_owned()
    })?;
    Ok(NativeDiscordProductSendAuthority { carrier })
}

pub fn with_native_discord_product_send_authority<T, Place>(
    composer: &NativeDiscordComposerState,
    scope_binding: &str,
    layout: Option<DiscordCarrierLayout>,
    place: Place,
) -> Result<T, String>
where
    Place: FnOnce(NativeDiscordProductSendAuthority) -> Result<T, String>,
{
    let product_send_authority =
        require_native_discord_product_send_authority(composer, scope_binding, layout)?;
    place(product_send_authority)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoPostImageInput {
    pub image_id: String,
    pub png_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageCopyForCommand {
    pub image_copy_id: String,
    pub png_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageCopyCommandResult {
    pub image_copy_ids: Vec<String>,
    pub image_copies: Vec<ImageCopyForCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageHiddenProviderPostResult {
    pub image_copy_ids: Vec<String>,
    pub provider_post_count: usize,
}

pub fn direct_photo_post_command_image_copies(
    images: Vec<PhotoPostImageInput>,
    pointer: [u8; stego::IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; stego::IMAGE_HIDDEN_CHECK_MARK_BYTES],
) -> Result<ImageCopyCommandResult, String> {
    image_hidden_photo_command_copies("direct-post", images, pointer, check_mark)
}

pub fn story_photo_command_image_copies(
    images: Vec<PhotoPostImageInput>,
    pointer: [u8; stego::IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; stego::IMAGE_HIDDEN_CHECK_MARK_BYTES],
) -> Result<ImageCopyCommandResult, String> {
    image_hidden_photo_command_copies("story", images, pointer, check_mark)
}

pub fn ordinary_text_send_command_image_copies() -> ImageCopyCommandResult {
    ImageCopyCommandResult {
        image_copy_ids: Vec::new(),
        image_copies: Vec::new(),
    }
}

pub fn direct_photo_post_after_image_quality_check<CheckQuality, ProviderPost>(
    images: Vec<PhotoPostImageInput>,
    pointer: [u8; stego::IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; stego::IMAGE_HIDDEN_CHECK_MARK_BYTES],
    check_quality: CheckQuality,
    provider_post: ProviderPost,
) -> Result<ImageHiddenProviderPostResult, String>
where
    CheckQuality: FnOnce(&ImageCopyCommandResult) -> Result<(), String>,
    ProviderPost: FnOnce(&ImageCopyCommandResult) -> Result<usize, String>,
{
    post_image_hidden_command_after_quality_check(
        "direct-post",
        images,
        pointer,
        check_mark,
        check_quality,
        provider_post,
    )
}

pub fn story_photo_post_after_image_quality_check<CheckQuality, ProviderPost>(
    images: Vec<PhotoPostImageInput>,
    pointer: [u8; stego::IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; stego::IMAGE_HIDDEN_CHECK_MARK_BYTES],
    check_quality: CheckQuality,
    provider_post: ProviderPost,
) -> Result<ImageHiddenProviderPostResult, String>
where
    CheckQuality: FnOnce(&ImageCopyCommandResult) -> Result<(), String>,
    ProviderPost: FnOnce(&ImageCopyCommandResult) -> Result<usize, String>,
{
    post_image_hidden_command_after_quality_check(
        "story",
        images,
        pointer,
        check_mark,
        check_quality,
        provider_post,
    )
}

fn post_image_hidden_command_after_quality_check<CheckQuality, ProviderPost>(
    command: &'static str,
    images: Vec<PhotoPostImageInput>,
    pointer: [u8; stego::IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; stego::IMAGE_HIDDEN_CHECK_MARK_BYTES],
    check_quality: CheckQuality,
    provider_post: ProviderPost,
) -> Result<ImageHiddenProviderPostResult, String>
where
    CheckQuality: FnOnce(&ImageCopyCommandResult) -> Result<(), String>,
    ProviderPost: FnOnce(&ImageCopyCommandResult) -> Result<usize, String>,
{
    let copies = image_hidden_photo_command_copies(command, images, pointer, check_mark)?;
    check_quality(&copies)?;
    let provider_post_count = provider_post(&copies)?;
    Ok(ImageHiddenProviderPostResult {
        image_copy_ids: copies.image_copy_ids,
        provider_post_count,
    })
}

fn image_hidden_photo_command_copies(
    command: &'static str,
    images: Vec<PhotoPostImageInput>,
    pointer: [u8; stego::IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; stego::IMAGE_HIDDEN_CHECK_MARK_BYTES],
) -> Result<ImageCopyCommandResult, String> {
    let image_copies = images
        .into_iter()
        .map(|image| {
            let png_bytes =
                stego::encode_png_hidden_pointer_bytes(&image.png_bytes, pointer, check_mark)
                    .map_err(|error| format!("OSL image hiding failed: {error}"))?;
            let image_copy_id = image_copy_id(command, &image.image_id, &png_bytes);
            Ok(ImageCopyForCommand {
                image_copy_id,
                png_bytes,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let image_copy_ids = image_copies
        .iter()
        .map(|copy| copy.image_copy_id.clone())
        .collect();
    Ok(ImageCopyCommandResult {
        image_copy_ids,
        image_copies,
    })
}

fn image_copy_id(command: &str, image_id: &str, png_bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL image-copy command v1");
    digest.update(command.as_bytes());
    digest.update([0]);
    digest.update(image_id.as_bytes());
    digest.update([0]);
    digest.update(png_bytes);
    let digest = digest.finalize();
    format!("image-copy-{}", hex_prefix(&digest, 12))
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
    let mut out = String::with_capacity(count * 2);
    for byte in bytes.iter().take(count) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(any(test, feature = "discord-qa-shell"))]
pub fn canonical_native_visible_row_qa_build_hash(value: Option<&str>) -> Result<String, String> {
    let value = value.ok_or_else(|| "The QA build hash is unavailable".to_owned())?;
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("The QA build hash is unavailable".to_owned());
    }
    Ok(value.to_ascii_lowercase())
}

#[cfg(any(test, feature = "discord-qa-shell"))]
pub fn recorded_executable_hash_matches_rebuild(
    recorded: &str,
    rebuilt: &str,
) -> Result<String, String> {
    let recorded = canonical_native_visible_row_qa_build_hash(Some(recorded))?;
    let rebuilt = canonical_native_visible_row_qa_build_hash(Some(rebuilt))?;
    if recorded == rebuilt {
        Ok(rebuilt)
    } else {
        Err(
            "The rebuilt QA executable hash does not match the recorded first-session hash"
                .to_owned(),
        )
    }
}

#[cfg(any(test, feature = "discord-qa-shell"))]
pub struct NativeVisibleRowQaRequestContext {
    pub owner: String,
    pub context_epoch: u64,
    pub context_host: ActiveServiceHost,
    pub scope_binding: String,
    pub build_hash: String,
    pub osl_target_identity_sha256: String,
}

#[cfg(any(test, feature = "discord-qa-shell"))]
pub fn prepare_native_visible_row_qa_request<
    VerifyCaller,
    RequireLock,
    LoadOwner,
    SnapshotContext,
    BindScope,
    RecheckContext,
    BuildHash,
    TargetIdentity,
>(
    verify_caller: VerifyCaller,
    require_lock: RequireLock,
    load_owner: LoadOwner,
    snapshot_context: SnapshotContext,
    bind_scope: BindScope,
    mut recheck_context: RecheckContext,
    build_hash: BuildHash,
    target_identity: TargetIdentity,
) -> Result<NativeVisibleRowQaRequestContext, String>
where
    VerifyCaller: FnOnce() -> Result<(), String>,
    RequireLock: FnOnce() -> Result<(), String>,
    LoadOwner: FnOnce() -> Result<String, String>,
    SnapshotContext: FnOnce() -> Result<(u64, ActiveServiceHost), String>,
    BindScope: FnOnce() -> Result<String, String>,
    RecheckContext: FnMut(u64, &ActiveServiceHost) -> Result<(), String>,
    BuildHash: FnOnce() -> Result<String, String>,
    TargetIdentity: FnOnce() -> Result<String, String>,
{
    verify_caller()?;
    require_lock()?;
    let owner = load_owner()?;
    let (context_epoch, context_host) = snapshot_context()?;
    let scope_binding = bind_scope()?;
    recheck_context(context_epoch, &context_host)?;
    let build_hash = build_hash()?;
    let osl_target_identity_sha256 = target_identity()?;
    Ok(NativeVisibleRowQaRequestContext {
        owner,
        context_epoch,
        context_host,
        scope_binding,
        build_hash,
        osl_target_identity_sha256,
    })
}

#[cfg(any(test, feature = "discord-qa-shell"))]
pub fn finish_native_visible_row_qa_request<RequireLock, RecheckContext, Persist>(
    context: &NativeVisibleRowQaRequestContext,
    receipt: broker::NativeVisibleRowRuntimeReceipt,
    require_lock: RequireLock,
    mut recheck_context: RecheckContext,
    persist: Persist,
) -> Result<broker::NativeVisibleRowRuntimeReceipt, String>
where
    RequireLock: FnOnce() -> Result<(), String>,
    RecheckContext: FnMut(u64, &ActiveServiceHost) -> Result<(), String>,
    Persist: FnOnce(&broker::NativeVisibleRowRuntimeReceipt) -> Result<(), String>,
{
    recheck_context(context.context_epoch, &context.context_host)?;
    require_lock()?;
    persist(&receipt)?;
    Ok(receipt)
}

#[cfg(feature = "discord-qa-shell")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordHeadlessQaPoll {
    pub opened_count: usize,
    pub pending_view_once_count: usize,
    pub acknowledgment_count: usize,
    pub fetched: u32,
}

#[cfg(feature = "discord-qa-shell")]
#[allow(clippy::too_many_arguments)]
pub fn poll_native_discord_headless_qa_restart_proof_flow<
    Opened,
    LoadOwner,
    LoadContextToken,
    LoadHost,
    ValidateHostBefore,
    Drain,
    RecordPoll,
    ReloadHost,
    ValidateHostAfter,
    RecordOpened,
    Summarize,
>(
    caller_label: &str,
    load_owner: LoadOwner,
    load_context_token: LoadContextToken,
    load_host: LoadHost,
    validate_host_before: ValidateHostBefore,
    drain: Drain,
    record_poll: RecordPoll,
    reload_host: ReloadHost,
    validate_host_after: ValidateHostAfter,
    record_opened: RecordOpened,
    summarize: Summarize,
) -> Result<DiscordHeadlessQaPoll, String>
where
    LoadOwner: FnOnce() -> Result<String, String>,
    LoadContextToken: FnOnce() -> Result<String, String>,
    LoadHost: FnOnce(&str) -> Result<ActiveServiceHost, String>,
    ValidateHostBefore: FnOnce(&str, &ActiveServiceHost) -> Result<(), String>,
    Drain: FnOnce() -> Result<Opened, String>,
    RecordPoll: FnOnce(Result<&Opened, &str>) -> Result<(), String>,
    ReloadHost: FnOnce(&str) -> Result<ActiveServiceHost, String>,
    ValidateHostAfter: FnOnce(&str, &ActiveServiceHost) -> Result<(), String>,
    RecordOpened: FnOnce(&Opened) -> Result<(), String>,
    Summarize: FnOnce(&Opened) -> DiscordHeadlessQaPoll,
{
    if caller_label != "main" {
        return Err("Only the trusted Discord QA shell may poll headless QA".to_owned());
    }
    let owner = load_owner()?;
    let context_token = load_context_token()?;
    let host = load_host(&owner)?;
    validate_host_before(&context_token, &host)?;
    let opened = drain();
    record_poll(opened.as_ref().map_err(String::as_str))?;
    let opened = opened?;
    let current = reload_host(&owner)?;
    if current != host {
        return Err("The native Discord QA host changed during receive".to_owned());
    }
    validate_host_after(&context_token, &current)?;
    record_opened(&opened)?;
    Ok(summarize(&opened))
}

pub fn service_kind_id(kind: ServiceKind) -> &'static str {
    match kind {
        ServiceKind::Discord => "discord",
        ServiceKind::Telegram => "telegram",
        ServiceKind::WhatsApp => "whatsapp",
        ServiceKind::Email => "email",
        ServiceKind::Signal => "signal",
    }
}

/*
tauri::generate_handler![
*/
#[macro_export]
macro_rules! hub_tauri_commands {
    ($callback:ident) => {
        $callback! {
            get_onboarding_preferences,
            ai_carrier_status,
            set_ai_carrier_preview_enabled,
            build_integrity_status,
            installed_build_record,
            list_hub_app_notifications,
            set_hub_notifications_enabled,
            set_hub_screenshot_protection,
            save_onboarding_preferences,
            save_burn_review_state,
            get_burn_review_state,
            back_burn_review,
            set_tor_preference,
            scan_local_privacy,
            open_hosted_session_scan,
            request_hosted_session_scan,
            initialize_scrub_index,
            set_scrub_index_manifest,
            get_scrub_index_manifest,
            get_scrub_index_scan,
            append_scrub_index_chunk,
            get_scrub_index_status,
            cancel_scrub_index,
            list_linked_services,
            get_core_readiness,
            list_core_features,
            get_hub_license_state,
            osl_mail_get_status,
            osl_mail_provision,
            osl_mail_send,
            osl_mail_burn,
            get_mass_cleanup_capabilities,
            discover_mass_cleanup_targets,
            execute_mass_cleanup_batch,
            get_autoscrub_run_fl,
            start_autoscrub_reviewed_run,
            request_autoscrub_global_stop,
            compose_scrub_erasure_request,
            validate_hub_activation_code,
            clear_hub_activation_code,
            unlock_hub_password_gate,
            create_hub_osl_identity,
            import_hub_osl_identity_phrase,
            setup_hub_main_password,
            view_hub_recovery_phrase,
            get_hub_recovery_kit_unsaved,
            set_hub_recovery_kit_unsaved,
            lock_hub_session,
            emit_active_session_reset,
            get_hub_password_role_status,
            set_hub_stealth_password,
            remove_hub_stealth_password,
            set_hub_burn_password,
            remove_hub_burn_password,
            check_hub_for_updates,
            install_hub_update,
            open_hub_releases_page,
            open_hub_source_repository,
            list_native_apps,
            install_native_app,
            get_mullvad_status,
            list_components,
            install_component,
            remove_component,
            install_mullvad,
            open_mullvad,
            list_browser_imports,
            open_browser_import,
            list_browser_profiles_for_consent,
            grant_browser_profile_consent,
            scan_consented_browser_profile,
            load_detected_browser_footprint,
            revoke_detected_browser_footprint,
            get_firefox_status,
            install_firefox,
            begin_browser_account_import,
            begin_protected_browser_import,
            finish_protected_browser_import,
            launch_firefox_service,
            get_default_browser_companion_status,
            host_default_browser_companion,
            resize_default_browser_companion,
            focus_default_browser_companion,
            detach_default_browser_companion,
            host_native_app_window,
            native_app_takeover_requires_consent,
            discord_marker_available,
            resize_native_app_window,
            focus_native_app_window,
            detach_native_app_window,
            get_signal_protected_send_readiness,
            claim_whatsapp_qa_window,
            resize_whatsapp_qa_window,
            get_whatsapp_qa_protection_status,
            begin_whatsapp_visual_binding,
            confirm_whatsapp_visual_binding,
            prepare_whatsapp_qa_protected_text,
            open_whatsapp_qa_protected_text,
            set_native_discord_protected_overlay_open,
            get_native_discord_overlay_state,
            prepare_native_discord_overlay_text,
            #[cfg(feature = "discord-qa-shell")]
            send_native_discord_qa_atomic_text,
            #[cfg(feature = "discord-qa-shell")]
            record_native_discord_qa_send_stage,
            #[cfg(feature = "discord-qa-shell")]
            send_native_discord_qa_probe,
            #[cfg(feature = "discord-qa-shell")]
            request_native_discord_visible_row_qa_receipt,
            #[cfg(feature = "discord-qa-shell")]
            run_native_discord_headless_qa,
            #[cfg(feature = "discord-qa-shell")]
            poll_native_discord_headless_qa,
            prepare_osl_chat_text,
            send_native_discord_overlay_carrier,
            open_native_discord_overlay_text,
            rehydrate_native_discord_overlay_history,
            reveal_native_discord_overlay_view_once,
            open_osl_chat_text,
            list_osl_chat_history,
            select_osl_chat_attachment,
            drop_osl_chat_attachments,
            list_osl_chat_attachments,
            open_osl_chat_attachment,
            select_native_discord_overlay_attachment,
            list_native_discord_overlay_attachments,
            open_native_discord_overlay_attachment,
            burn_native_discord_overlay_chat,
            set_native_discord_overlay_security,
            set_native_discord_covertext_enabled,
            host_mullvad_window,
            resize_mullvad_window,
            focus_mullvad_window,
            restore_mullvad_window,
            create_service_account,
            open_service_host,
            request_hosted_session_scan_command,
            scan_discord_own_messages_for_deletion,
            preview_discord_guided_deletion,
            execute_discord_guided_deletion,
            close_service_host,
            set_local_protected_sheet_open,
            remove_service_account,
            activate_local_loopback_context,
            activate_manual_peer_context,
            activate_native_manual_peer_context,
            activate_osl_chat_context,
            set_osl_chat_capture_preference,
            close_osl_chat_context,
            prepare_encrypted_text,
            decrypt_hub_capsule,
            prepare_peer_prose_text,
            open_peer_prose_text,
            prepare_local_protected_text_with_policy,
            decrypt_local_protected_capsule,
            prepare_hub_attachment,
            open_hub_attachment,
            export_hub_friend_code,
            copy_hub_friend_invite,
            add_hub_friend,
            claim_hub_username,
            get_hub_username_status,
            add_hub_friend_by_username,
            get_osl_profile,
            set_owner_profile_picture,
            read_owner_profile_picture,
            read_owner_profile_picture_for_friend,
            clear_owner_profile_picture,
            get_osl_chat_local_state_key,
            save_osl_profile,
            verify_hub_friend_safety_number,
            remove_hub_friend,
            list_hub_people,
            compare_allowed_place_direction_state,
            list_group_verification_build_entries,
            set_hub_friend_nickname,
            set_active_hub_friend_permission,
            set_active_hub_friend_reach,
            revoke_active_hub_friend_scope,
            get_active_hub_context_security,
            set_active_hub_context_security,
            list_hub_identities,
            create_hub_identity_slot,
            recover_hub_identity_slot,
            switch_hub_identity,
            burn_active_hub_identity,
            execute_hub_full_cleanup,
            get_hub_service_burn_readiness,
            burn_hub_service_account,
            burn_active_hub_context,
            get_hub_revocation_status
        }
    };
}

#[cfg(test)]
mod erasure_command_wiring_tests {
    use super::compose_erasure_request_for_user;
    use crate::scrub_erasure::{ErasureDataCategory, ErasureRequestInput};

    #[test]
    fn scr_g1_desktop_command_path_composes_locally_before_any_user_send() {
        let request = compose_erasure_request_for_user(ErasureRequestInput {
            provider_name: "Example Social".to_owned(),
            provider_account_identifier: "@river".to_owned(),
            data_categories: vec![ErasureDataCategory::PostsAndMessages],
        })
        .expect("the desktop command path must reach local erasure composition");

        assert!(request.body.contains("Account identifier: @river"));
        assert!(!request.body.contains("OSL"));
    }
}

#[cfg(test)]
mod discord_guided_deletion_plan_producer_tests {
    use super::{DiscordGuidedDeletionPlanState, GuidedDeletionRunAuthorityInput};
    use crate::native_discord_adapter::guided_deletion::{
        DeletionPreview, DeletionScan, RowShape, ScannedRow, WalkCompleteness,
    };

    fn row(scan_ordinal: usize) -> ScannedRow {
        ScannedRow {
            scan_ordinal,
            shape_ordinal: 0,
            shape: RowShape {
                height_px: 44,
                children: 0,
            },
            text_len: 12 + scan_ordinal,
            authored_by_operator: true,
        }
    }

    fn scan(rows: Vec<ScannedRow>) -> DeletionScan {
        DeletionScan {
            scope_binding_hash: "a".repeat(64),
            generation: 7,
            rows_seen: 8,
            rows_unreadable: 0,
            walk: WalkCompleteness::Complete,
            candidates: rows,
        }
    }

    fn authority(preview: &DeletionPreview) -> GuidedDeletionRunAuthorityInput {
        GuidedDeletionRunAuthorityInput {
            run_id: "run-7".to_owned(),
            attended_action_id: "attended-action-7".to_owned(),
            scope_binding_hash: preview.scope_binding_hash.clone(),
            generation: preview.generation,
            plan_digest: preview.plan_digest.clone(),
            mode: "attended_delete_run_v1".to_owned(),
        }
    }

    #[test]
    fn mutant_scope_drift_after_digest_echo_requires_reconfirmation() {
        let state = DiscordGuidedDeletionPlanState::default();
        state
            .record_scan(scan(vec![row(1), row(2)]))
            .expect("scan is stored");
        let confirmed_by_user = state
            .build_preview(&[1], true)
            .expect("first preview can be shown");
        let changed_review_scope = state
            .build_preview(&[2], true)
            .expect("changing the reviewed row produces a new preview");
        assert_ne!(
            confirmed_by_user.plan_digest, changed_review_scope.plan_digest,
            "the test fixture must exercise a real exact-plan change"
        );

        let refused = state.confirm_preview(
            &confirmed_by_user.plan_digest,
            &changed_review_scope.scope_binding_hash,
            changed_review_scope.generation,
            authority(&confirmed_by_user),
        );
        assert_eq!(
            refused,
            Err("confirmation_no_longer_matches_the_plan".to_owned()),
            "a digest echoed for the old review scope must not confirm the new plan"
        );
    }

    #[test]
    fn mutant_unachievable_target_plan_is_refused() {
        let state = DiscordGuidedDeletionPlanState::default();
        state
            .record_scan(scan(vec![row(1)]))
            .expect("scan is stored");
        assert_eq!(
            state.build_preview(&[99], true),
            Err("row_is_not_in_this_scan".to_owned()),
            "a plan naming a row outside the reviewed native scan must refuse"
        );
    }

    #[test]
    fn producer_confirms_only_once_for_the_exact_native_scope() {
        let state = DiscordGuidedDeletionPlanState::default();
        state
            .record_scan(scan(vec![row(1)]))
            .expect("scan is stored");
        let preview = state.build_preview(&[1], true).expect("preview");
        let plan = state
            .confirm_preview(
                &preview.plan_digest,
                &preview.scope_binding_hash,
                preview.generation,
                authority(&preview),
            )
            .expect("exact preview confirms");
        assert_eq!(plan.preview().plan_digest, preview.plan_digest);
        assert_eq!(
            state.confirm_preview(
                &preview.plan_digest,
                &preview.scope_binding_hash,
                preview.generation,
                authority(&preview),
            ),
            Err("Preview the exact Discord deletion plan before confirming it".to_owned()),
            "confirmation consumes the preview; a retry must preview and confirm again"
        );
    }
}

/// Test-only counterpart of `main.rs`'s handler-name callback: turns the one
/// authoritative command list above into the strings the registration proofs
/// compare against the ACL files.
#[cfg(test)]
macro_rules! hub_tauri_command_names {
    ($($(#[$meta:meta])* $command:ident),* $(,)?) => {{
        let mut commands = ::std::vec::Vec::new();
        $(
            $(#[$meta])*
            commands.push(stringify!($command).to_owned());
        )*
        commands
    }};
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFootprintConsentRequest {
    pub browser_id: BrowserImportId,
    pub browser_profile_account: String,
    pub browser_profile_id: String,
    pub import_run_id: String,
    pub consent: bool,
}

fn valid_browser_footprint_binding_field(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(|ch| ch.is_control())
}

pub fn checked_browser_footprint_binding(
    owner: &str,
    request: &BrowserFootprintConsentRequest,
    explicit_consent: bool,
) -> Result<NativeBrowserImportBinding, String> {
    if !valid_browser_footprint_binding_field(&request.browser_profile_account)
        || !valid_browser_footprint_binding_field(&request.browser_profile_id)
        || !valid_browser_footprint_binding_field(&request.import_run_id)
    {
        return Err("The browser footprint binding is invalid".to_owned());
    }
    Ok(NativeBrowserImportBinding {
        owner_osl_user_id: owner.to_owned(),
        browser_id: request.browser_id,
        browser_profile_account: request.browser_profile_account.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        import_run_id: request.import_run_id.clone(),
        explicit_consent,
    })
}

#[cfg(test)]
mod native_visible_row_qa_command_tests {
    use super::{
        canonical_native_visible_row_qa_build_hash, direct_photo_post_after_image_quality_check,
        direct_photo_post_command_image_copies, finish_native_visible_row_qa_request,
        ordinary_text_send_command_image_copies, prepare_native_visible_row_qa_request,
        recorded_executable_hash_matches_rebuild, require_native_discord_product_send_authority,
        story_photo_command_image_copies, story_photo_post_after_image_quality_check,
        with_native_discord_product_send_authority, ActiveServiceHost, PhotoPostImageInput,
    };
    use crate::native_discord_adapter::{
        deidentify_prepared_visual_structure, DiscordCarrierLayout, DiscordCarrierPadding,
        DiscordCarrierRowKind, NativeDiscordComposerState, NativeVisibleRowQaTriState,
    };
    use base64::Engine;
    use std::cell::{Cell, RefCell};

    const TEST_FLAGTEXT: &str = "ok i will weekend again with you get what i was thinking usual";

    fn measured_layout() -> DiscordCarrierLayout {
        DiscordCarrierLayout {
            content_width_px: 240.0,
            average_grapheme_width_px: 8.0,
            line_height_px: 18.0,
            zoom: 1.0,
            density: 1.0,
            padding: DiscordCarrierPadding::ShapeMatched,
            row_kind: DiscordCarrierRowKind::PlainText,
        }
    }

    #[test]
    fn native_visible_row_send_authority_closure_refuses_before_placement() {
        let composer = NativeDiscordComposerState::default();
        let placed = Cell::new(false);
        let missing = with_native_discord_product_send_authority(
            &composer,
            "scope-a",
            Some(measured_layout()),
            |_| {
                placed.set(true);
                Ok(())
            },
        );
        assert!(missing.is_err());
        assert!(!placed.get());

        composer.remember_prepared_visual_structure(
            "scope-a",
            deidentify_prepared_visual_structure("private"),
            TEST_FLAGTEXT.to_owned(),
        );
        let wrong_scope = with_native_discord_product_send_authority(
            &composer,
            "scope-b",
            Some(measured_layout()),
            |_| {
                placed.set(true);
                Ok(())
            },
        );
        assert!(wrong_scope.is_err());
        assert!(!placed.get());

        composer.remember_prepared_visual_structure(
            "scope-a",
            deidentify_prepared_visual_structure("private"),
            TEST_FLAGTEXT.to_owned(),
        );
        let events = RefCell::new(Vec::<&'static str>::new());
        let carrier = with_native_discord_product_send_authority(
            &composer,
            "scope-a",
            Some(measured_layout()),
            |authority| {
                events.borrow_mut().push("authority");
                placed.set(true);
                Ok(authority.carrier)
            },
        )
        .expect("same-scope prepared carrier authorizes native placement");
        assert!(placed.get());
        assert_eq!(events.into_inner(), ["authority"]);
        assert!(carrier
            .split_whitespace()
            .eq(TEST_FLAGTEXT.split_whitespace()));
    }

    #[test]
    fn task_0661_photo_posts_and_stories_return_image_copy_ids_text_send_returns_none() {
        let pointer = [
            0x06, 0x61, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0,
            0xd0, 0xe0, 0xf0, 0x0f, 0x1e, 0x2d,
        ];
        let check_mark = [0xca, 0xfe, 0x66, 0x10];
        let source_png = fixture_png();

        let direct = direct_photo_post_command_image_copies(
            vec![PhotoPostImageInput {
                image_id: "direct-photo-fixture".to_owned(),
                png_bytes: source_png.clone(),
            }],
            pointer,
            check_mark,
        )
        .expect("direct photo post command uses image hiding");
        let story = story_photo_command_image_copies(
            vec![PhotoPostImageInput {
                image_id: "story-photo-fixture".to_owned(),
                png_bytes: source_png,
            }],
            pointer,
            check_mark,
        )
        .expect("story command uses image hiding");
        let text = ordinary_text_send_command_image_copies();

        let direct_decoded =
            stego::decode_png_hidden_pointer_bytes(&direct.image_copies[0].png_bytes)
                .unwrap()
                .expect("direct post image copy carries the hidden pointer");
        let story_decoded =
            stego::decode_png_hidden_pointer_bytes(&story.image_copies[0].png_bytes)
                .unwrap()
                .expect("story image copy carries the hidden pointer");

        println!(
            "TASK0661 direct_post_image_copy_ids={}",
            direct.image_copy_ids.join(",")
        );
        println!(
            "TASK0661 direct_post_image_copy_count={}",
            direct.image_copy_ids.len()
        );
        println!(
            "TASK0661 direct_post_decoded_pointer_hex={}",
            hex(&direct_decoded.pointer)
        );
        println!(
            "TASK0661 story_image_copy_ids={}",
            story.image_copy_ids.join(",")
        );
        println!(
            "TASK0661 story_image_copy_count={}",
            story.image_copy_ids.len()
        );
        println!(
            "TASK0661 story_decoded_pointer_hex={}",
            hex(&story_decoded.pointer)
        );
        println!(
            "TASK0661 text_send_image_copy_ids={}",
            if text.image_copy_ids.is_empty() {
                "none".to_owned()
            } else {
                text.image_copy_ids.join(",")
            }
        );
        println!(
            "TASK0661 text_send_image_copy_count={}",
            text.image_copy_ids.len()
        );

        assert_eq!(direct.image_copy_ids.len(), 1);
        assert!(direct.image_copy_ids[0].starts_with("image-copy-"));
        assert_eq!(story.image_copy_ids.len(), 1);
        assert!(story.image_copy_ids[0].starts_with("image-copy-"));
        assert_eq!(direct_decoded.pointer, pointer);
        assert_eq!(direct_decoded.check_mark, check_mark);
        assert_eq!(story_decoded.pointer, pointer);
        assert_eq!(story_decoded.check_mark, check_mark);
        assert!(text.image_copy_ids.is_empty());
        assert!(text.image_copies.is_empty());
    }

    #[test]
    fn task_0665_failed_image_quality_check_produces_provider_post_count_0() {
        let pointer = [
            0x06, 0x65, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0,
            0xd0, 0xe0, 0xf0, 0x0f, 0x1e, 0x2d,
        ];
        let check_mark = [0xca, 0xfe, 0x66, 0x50];
        let refusal = "TASK0665_IMAGE_QUALITY_FAILED";
        let direct_provider_posts = Cell::new(0usize);
        let direct = direct_photo_post_after_image_quality_check(
            vec![PhotoPostImageInput {
                image_id: "direct-quality-failure".to_owned(),
                png_bytes: fixture_png(),
            }],
            pointer,
            check_mark,
            |copies| {
                assert_eq!(copies.image_copy_ids.len(), 1);
                Err(refusal.to_owned())
            },
            |_| {
                direct_provider_posts.set(direct_provider_posts.get() + 1);
                Ok(direct_provider_posts.get())
            },
        );
        let direct_error = direct.expect_err("failed direct-post quality check refuses posting");

        let story_provider_posts = Cell::new(0usize);
        let story = story_photo_post_after_image_quality_check(
            vec![PhotoPostImageInput {
                image_id: "story-quality-failure".to_owned(),
                png_bytes: fixture_png(),
            }],
            pointer,
            check_mark,
            |copies| {
                assert_eq!(copies.image_copy_ids.len(), 1);
                Err(refusal.to_owned())
            },
            |_| {
                story_provider_posts.set(story_provider_posts.get() + 1);
                Ok(story_provider_posts.get())
            },
        );
        let story_error = story.expect_err("failed story quality check refuses posting");

        let successful_provider_posts = Cell::new(0usize);
        let successful = direct_photo_post_after_image_quality_check(
            vec![PhotoPostImageInput {
                image_id: "direct-quality-success".to_owned(),
                png_bytes: fixture_png(),
            }],
            pointer,
            check_mark,
            |copies| {
                let decoded =
                    stego::decode_png_hidden_pointer_bytes(&copies.image_copies[0].png_bytes)
                        .unwrap()
                        .expect("quality check reads hidden pointer from the prepared image copy");
                assert_eq!(decoded.pointer, pointer);
                assert_eq!(decoded.check_mark, check_mark);
                Ok(())
            },
            |copies| {
                assert_eq!(copies.image_copy_ids.len(), 1);
                successful_provider_posts.set(successful_provider_posts.get() + 1);
                Ok(successful_provider_posts.get())
            },
        )
        .expect("successful quality check reaches provider post");

        println!("TASK0665_FAILED_QUALITY_CHECK_REFUSAL={direct_error}");
        println!(
            "TASK0665_DIRECT_PROVIDER_POST_COUNT={}",
            direct_provider_posts.get()
        );
        println!("TASK0665_STORY_QUALITY_CHECK_REFUSAL={story_error}");
        println!(
            "TASK0665_STORY_PROVIDER_POST_COUNT={}",
            story_provider_posts.get()
        );
        println!(
            "TASK0665_SUCCESS_PROVIDER_POST_COUNT={}",
            successful.provider_post_count
        );

        assert_eq!(direct_error, refusal);
        assert_eq!(story_error, refusal);
        assert_eq!(direct_provider_posts.get(), 0);
        assert_eq!(story_provider_posts.get(), 0);
        assert_eq!(successful.provider_post_count, 1);
    }

    fn fixture_png() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(
                "iVBORw0KGgoAAAANSUhEUgAAAAwAAAAICAIAAABChommAAABM0lEQVR42gEoAdf+ABFbyxxgxCdlvTJqtj1vr0h0qFN5oV5+mmmDk3SIjH+NhYqSfgAUaMIfbbsqcrQ1d61AfKZLgZ9Whphhi5FskIp3lYOCmnyNn3UAF3W5InqyLX+rOISkQ4mdTo6WWZOPZJiIb52BeqJ6hadzkKxsABqCsCWHqTCMojuRm0aWlFGbjVyghmelf3KqeH2vcYi0apO5YwAdj6colKAzmZk+npJJo4tUqIRfrX1qsnZ1t2+AvGiLwWGWxloAIJyeK6GXNqaQQauJTLCCV7V7Yrp0bb9teMRmg8lfjs5YmdNRACOplS6ujjmzh0S4gE+9eVrCcmXHa3DMZHvRXYbWVpHbT5zgSAAmtowxu4U8wH5HxXdSynBdz2lo1GJz2Vt+3lSJ402U6Eaf7T/16JBhoW65AQAAAABJRU5ErkJggg==",
            )
            .expect("fixture PNG base64 decodes")
    }

    fn hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    #[cfg(feature = "discord-qa-shell")]
    fn registered_commands() -> Vec<String> {
        hub_tauri_commands!(hub_tauri_command_names)
    }

    #[cfg(feature = "discord-qa-shell")]
    fn command_is_registered(command: &str) -> bool {
        registered_commands()
            .iter()
            .any(|registered| registered == command)
    }

    fn test_active_host() -> ActiveServiceHost {
        ActiveServiceHost {
            service_id: "discord".to_owned(),
            account_id: "acct-1".to_owned(),
            generation: 7,
            owner_namespace: "owner-ns".to_owned(),
        }
    }

    fn runtime_receipt_fixture() -> crate::broker::NativeVisibleRowRuntimeReceipt {
        use crate::broker::NativeVisibleRowRuntimeOutcomes;
        use NativeVisibleRowQaTriState::{Accepted, Refused};

        crate::broker::NativeVisibleRowRuntimeReceipt {
            schema_version: 2,
            observed_at_unix_ms: 1,
            build_hash: "a".repeat(40),
            osl_target_identity_sha256: "b".repeat(64),
            discord_target_identity_sha256: "c".repeat(64),
            scope_binding_sha256: "d".repeat(64),
            window_generation: 7,
            rows_observed: 2,
            native_proof_some: 2,
            native_proof_none: 0,
            authenticated_own_outgoing: 1,
            authenticated_peer_incoming: 1,
            broker_plaintext_rows: 2,
            broker_refused_rows: 0,
            outcomes: NativeVisibleRowRuntimeOutcomes {
                own_outgoing: Accepted,
                peer_incoming: Accepted,
                peer_anchor: Accepted,
                zero_rows: Refused,
                missing_proof: Refused,
                mixed_scope: Refused,
                different_non_self: Refused,
                replay: Refused,
                reorder: Refused,
                persistence: Accepted,
            },
            accepted: true,
        }
    }

    #[test]
    fn native_visible_row_qa_command_is_reachable_only_through_trusted_state() {
        #[cfg(feature = "discord-qa-shell")]
        assert!(command_is_registered(
            "request_native_discord_visible_row_qa_receipt"
        ));

        let events = RefCell::new(Vec::<&'static str>::new());
        let context = prepare_native_visible_row_qa_request(
            || {
                events.borrow_mut().push("caller");
                Ok(())
            },
            || {
                events.borrow_mut().push("lock-before");
                Ok(())
            },
            || {
                events.borrow_mut().push("owner");
                Ok("owner-1".to_owned())
            },
            || {
                events.borrow_mut().push("snapshot");
                Ok((42, test_active_host()))
            },
            || {
                events.borrow_mut().push("scope");
                Ok("scope-binding".to_owned())
            },
            |epoch, host| {
                events.borrow_mut().push("context-before");
                assert_eq!(epoch, 42);
                assert_eq!(host.service_id, "discord");
                Ok(())
            },
            || {
                events.borrow_mut().push("build-hash");
                canonical_native_visible_row_qa_build_hash(Some(
                    "ABCDEF0123456789ABCDEF0123456789ABCDEF01",
                ))
            },
            || {
                events.borrow_mut().push("target");
                Ok("b".repeat(64))
            },
        )
        .expect("trusted state prepares the native receipt request");
        assert_eq!(context.owner, "owner-1");
        assert_eq!(context.scope_binding, "scope-binding");
        assert_eq!(
            context.build_hash,
            "abcdef0123456789abcdef0123456789abcdef01"
        );
        assert_eq!(context.osl_target_identity_sha256, "b".repeat(64));

        let persisted = Cell::new(false);
        let receipt = finish_native_visible_row_qa_request(
            &context,
            runtime_receipt_fixture(),
            || {
                events.borrow_mut().push("lock-after");
                Ok(())
            },
            |epoch, host| {
                events.borrow_mut().push("context-after");
                assert_eq!(epoch, 42);
                assert_eq!(host.generation, 7);
                Ok(())
            },
            |receipt| {
                events.borrow_mut().push("persist");
                persisted.set(true);
                assert!(receipt.accepted);
                Ok(())
            },
        )
        .expect("receipt persists only after post-read gates");
        assert!(receipt.accepted);
        assert!(persisted.get());
        assert_eq!(
            events.into_inner(),
            [
                "caller",
                "lock-before",
                "owner",
                "snapshot",
                "scope",
                "context-before",
                "build-hash",
                "target",
                "context-after",
                "lock-after",
                "persist",
            ]
        );

        let stale_after_events = RefCell::new(Vec::<&'static str>::new());
        let stale_after = finish_native_visible_row_qa_request(
            &context,
            runtime_receipt_fixture(),
            || {
                stale_after_events.borrow_mut().push("lock-after");
                Ok(())
            },
            |_epoch, _host| {
                stale_after_events.borrow_mut().push("context-after");
                Err("stale native context".to_owned())
            },
            |_receipt| {
                stale_after_events.borrow_mut().push("persist");
                Ok(())
            },
        );
        match stale_after {
            Err(error) => assert_eq!(error, "stale native context"),
            Ok(_) => panic!("stale native context must refuse the QA receipt"),
        }
        assert_eq!(
            stale_after_events.into_inner(),
            ["context-after"],
            "a stale post-read context must refuse before the lock-after gate or persistence"
        );

        let finish_refused_events = RefCell::new(Vec::<&'static str>::new());
        let finish_persisted = Cell::new(false);
        let stale_finish = finish_native_visible_row_qa_request(
            &context,
            runtime_receipt_fixture(),
            || {
                finish_refused_events.borrow_mut().push("lock-after");
                Ok(())
            },
            |_epoch, _host| {
                finish_refused_events.borrow_mut().push("context-after");
                Err("stale native context".to_owned())
            },
            |_| {
                finish_refused_events.borrow_mut().push("persist");
                finish_persisted.set(true);
                Ok(())
            },
        );
        match stale_finish {
            Err(error) => assert_eq!(error, "stale native context"),
            Ok(_) => panic!("stale post-read context must refuse the QA receipt"),
        }
        assert_eq!(
            finish_refused_events.into_inner(),
            ["context-after"],
            "post-read context drift must refuse before lock recheck or persistence"
        );
        assert!(
            !finish_persisted.get(),
            "a stale post-read context must not persist a native visible-row receipt"
        );

        let refused_events = RefCell::new(Vec::<&'static str>::new());
        let refused = prepare_native_visible_row_qa_request(
            || {
                refused_events.borrow_mut().push("caller");
                Err("untrusted caller".to_owned())
            },
            || {
                refused_events.borrow_mut().push("lock-before");
                Ok(())
            },
            || Ok("owner-1".to_owned()),
            || Ok((42, test_active_host())),
            || Ok("scope-binding".to_owned()),
            |_epoch, _host| Ok(()),
            || {
                canonical_native_visible_row_qa_build_hash(Some(
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ))
            },
            || Ok("b".repeat(64)),
        );
        match refused {
            Err(error) => assert_eq!(error, "untrusted caller"),
            Ok(_) => panic!("untrusted caller must refuse"),
        }
        assert_eq!(
            refused_events.into_inner(),
            ["caller"],
            "untrusted callers must refuse before lock, context, native read or persist"
        );

        let composer = NativeDiscordComposerState::default();
        let placed = Cell::new(false);
        let missing_authority = with_native_discord_product_send_authority(
            &composer,
            "scope-a",
            Some(measured_layout()),
            |_| {
                placed.set(true);
                Ok(())
            },
        );
        assert!(missing_authority.is_err());
        assert!(
            !placed.get(),
            "without product send authority the native placement/send closure must not run"
        );

        composer.remember_prepared_visual_structure(
            "scope-a",
            deidentify_prepared_visual_structure("private"),
            TEST_FLAGTEXT.to_owned(),
        );
        let wrong_scope = with_native_discord_product_send_authority(
            &composer,
            "scope-b",
            Some(measured_layout()),
            |_| {
                placed.set(true);
                Ok(())
            },
        );
        assert!(wrong_scope.is_err());
        assert!(!placed.get());

        composer.remember_prepared_visual_structure(
            "scope-a",
            deidentify_prepared_visual_structure("private"),
            TEST_FLAGTEXT.to_owned(),
        );
        let carrier = with_native_discord_product_send_authority(
            &composer,
            "scope-a",
            Some(measured_layout()),
            |authority| {
                placed.set(true);
                Ok(authority.carrier)
            },
        )
        .expect("same-scope prepared product send authority permits placement");
        assert!(placed.get());
        assert!(carrier
            .split_whitespace()
            .eq(TEST_FLAGTEXT.split_whitespace()));
        assert!(
            require_native_discord_product_send_authority(
                &composer,
                "scope-a",
                Some(measured_layout())
            )
            .is_err(),
            "product send authority must be single-use"
        );
    }

    #[test]
    fn native_visible_row_product_send_authority_is_single_use() {
        let state = NativeDiscordComposerState::default();
        assert!(
            require_native_discord_product_send_authority(
                &state,
                "scope-a",
                Some(measured_layout())
            )
            .is_err(),
            "absent product send authority must refuse before carrier placement"
        );

        state.remember_prepared_visual_structure(
            "scope-a",
            deidentify_prepared_visual_structure("private"),
            TEST_FLAGTEXT.to_owned(),
        );
        assert!(
            require_native_discord_product_send_authority(
                &state,
                "scope-b",
                Some(measured_layout())
            )
            .is_err(),
            "a prepared carrier for another scope must not authorize this send"
        );

        state.remember_prepared_visual_structure(
            "scope-a",
            deidentify_prepared_visual_structure("private"),
            TEST_FLAGTEXT.to_owned(),
        );
        let authority = require_native_discord_product_send_authority(
            &state,
            "scope-a",
            Some(measured_layout()),
        )
        .expect("same-scope prepared carrier authorizes one send");
        assert!(authority
            .carrier
            .split_whitespace()
            .eq(TEST_FLAGTEXT.split_whitespace()));
        assert!(
            require_native_discord_product_send_authority(
                &state,
                "scope-a",
                Some(measured_layout())
            )
            .is_err(),
            "product send authority must be single-use"
        );
    }

    #[test]
    fn native_visible_row_qa_build_hash_is_present_canonical_and_bounded() {
        assert_eq!(
            canonical_native_visible_row_qa_build_hash(Some(
                "ABCDEF0123456789ABCDEF0123456789ABCDEF01"
            ))
            .unwrap(),
            "abcdef0123456789abcdef0123456789abcdef01"
        );
        assert!(canonical_native_visible_row_qa_build_hash(None).is_err());
        assert!(canonical_native_visible_row_qa_build_hash(Some("abc")).is_err());
        assert!(canonical_native_visible_row_qa_build_hash(Some(
            "gggggggggggggggggggggggggggggggggggggggg"
        ))
        .is_err());
    }

    #[test]
    fn second_session_rebuild_reproduces_recorded_executable_hash() {
        let first_session = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        let second_session = "abcdef0123456789abcdef0123456789abcdef01";
        assert_eq!(
            recorded_executable_hash_matches_rebuild(first_session, second_session).unwrap(),
            second_session
        );

        let mismatch = recorded_executable_hash_matches_rebuild(
            first_session,
            "1111111111111111111111111111111111111111",
        );
        match mismatch {
            Err(error) => assert_eq!(
                error,
                "The rebuilt QA executable hash does not match the recorded first-session hash"
            ),
            Ok(_) => panic!("a second-session rebuild with a different hash must refuse"),
        }

        assert!(recorded_executable_hash_matches_rebuild(first_session, "not-a-hash").is_err());
    }

    #[test]
    fn native_carrier_send_authority_requires_a_same_scope_prepared_message() {
        let state = NativeDiscordComposerState::default();
        assert!(require_native_discord_product_send_authority(
            &state,
            "scope-a",
            Some(measured_layout())
        )
        .is_err());

        state.remember_prepared_visual_structure(
            "scope-a",
            deidentify_prepared_visual_structure("private"),
            TEST_FLAGTEXT.to_owned(),
        );
        assert!(require_native_discord_product_send_authority(
            &state,
            "scope-b",
            Some(measured_layout())
        )
        .is_err());

        state.remember_prepared_visual_structure(
            "scope-a",
            deidentify_prepared_visual_structure("private"),
            TEST_FLAGTEXT.to_owned(),
        );
        let authority = require_native_discord_product_send_authority(
            &state,
            "scope-a",
            Some(measured_layout()),
        )
        .expect("same-scope prepared carrier authorizes one send");
        assert!(authority
            .carrier
            .split_whitespace()
            .eq(TEST_FLAGTEXT.split_whitespace()));
        assert!(require_native_discord_product_send_authority(
            &state,
            "scope-a",
            Some(measured_layout())
        )
        .is_err());
    }
}

#[cfg(test)]
mod tauri_registration_surface_tests {
    use super::*;
    use crate::identity_binding_verifier::BindingEvidence;
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, BTreeSet};

    fn handler_commands() -> BTreeSet<String> {
        hub_tauri_commands!(hub_tauri_command_names)
            .into_iter()
            .collect()
    }

    fn toml_string_field(line: &str, field: &str) -> Option<String> {
        let value = line.strip_prefix(field)?.trim_start();
        let value = value.strip_prefix('=')?.trim_start();
        let value = value.strip_prefix('"')?.strip_suffix('"')?;
        Some(value.to_owned())
    }

    fn toml_string_array_field(line: &str, field: &str) -> Option<Vec<String>> {
        let value = line.strip_prefix(field)?.trim_start();
        let value = value.strip_prefix('=')?.trim_start();
        let value = value.strip_prefix('[')?.strip_suffix(']')?;
        Some(
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(|part| {
                    part.strip_prefix('"')
                        .and_then(|part| part.strip_suffix('"'))
                        .expect("commands.allow entries must be TOML strings")
                        .to_owned()
                })
                .collect(),
        )
    }

    fn permission_commands(source: &str) -> BTreeMap<String, String> {
        let mut permissions = BTreeMap::new();
        for block in source.split("[[permission]]").skip(1) {
            let mut identifier = None::<String>;
            let mut allowed_commands = None::<Vec<String>>;
            for line in block.lines().map(str::trim).filter(|line| !line.is_empty()) {
                if identifier.is_none() {
                    identifier = toml_string_field(line, "identifier");
                }
                if allowed_commands.is_none() {
                    allowed_commands = toml_string_array_field(line, "commands.allow");
                }
            }
            let Some(identifier) = identifier else {
                continue;
            };
            let commands = allowed_commands
                .unwrap_or_else(|| panic!("permission {identifier} must declare commands.allow"));
            assert_eq!(
                commands.len(),
                1,
                "permission {identifier} must grant exactly one command"
            );
            let previous = permissions.insert(identifier.clone(), commands[0].clone());
            assert!(
                previous.is_none(),
                "permission identifier {identifier} must be unique"
            );
        }
        permissions
    }

    fn capability_permissions(source: &str) -> BTreeSet<String> {
        let value: serde_json::Value = serde_json::from_str(source).expect("capability JSON");
        value["permissions"]
            .as_array()
            .expect("permissions array")
            .iter()
            .map(|permission| {
                permission
                    .as_str()
                    .expect("permission is a string")
                    .to_owned()
            })
            .collect()
    }

    fn command_permission(command: &str) -> String {
        format!("allow-{}", command.replace('_', "-"))
    }

    fn is_registered_and_granted(
        handlers: &BTreeSet<String>,
        permissions: &BTreeMap<String, String>,
        capability: &BTreeSet<String>,
        command: &str,
    ) -> bool {
        let permission = command_permission(command);
        handlers.contains(command)
            && permissions
                .get(&permission)
                .is_some_and(|allowed| allowed == command)
            && capability.contains(&permission)
    }

    fn assert_registered_and_granted(
        handlers: &BTreeSet<String>,
        permissions: &BTreeMap<String, String>,
        capability: &BTreeSet<String>,
        command: &str,
    ) {
        assert!(
            is_registered_and_granted(handlers, permissions, capability, command),
            "{command} must be registered in generate_handler, declared in hub.toml, and granted by hub.json"
        );

        let mut missing_handler = handlers.clone();
        missing_handler.remove(command);
        assert!(
            !is_registered_and_granted(&missing_handler, permissions, capability, command),
            "removing {command} from generate_handler must make the registration proof fail"
        );

        let permission = command_permission(command);
        let mut missing_permission = permissions.clone();
        missing_permission.remove(&permission);
        assert!(
            !is_registered_and_granted(handlers, &missing_permission, capability, command),
            "removing {permission} from hub.toml must make the registration proof fail"
        );

        let mut missing_capability = capability.clone();
        missing_capability.remove(&permission);
        assert!(
            !is_registered_and_granted(handlers, permissions, &missing_capability, command),
            "removing {permission} from hub.json must make the registration proof fail"
        );
    }

    fn assert_each_registration_surface_is_required(
        handlers: &BTreeSet<String>,
        permissions: &BTreeMap<String, String>,
        capability: &BTreeSet<String>,
        commands: &[&str],
    ) {
        for command in commands {
            let command = *command;
            let permission = command_permission(command);

            let mut missing_handler = handlers.clone();
            missing_handler.remove(command);
            assert!(
                !is_registered_and_granted(&missing_handler, permissions, capability, command),
                "removing {command} from generate_handler must fail the registration proof"
            );

            let mut missing_permission = permissions.clone();
            missing_permission.remove(&permission);
            assert!(
                !is_registered_and_granted(handlers, &missing_permission, capability, command),
                "removing {permission} from hub.toml must fail the registration proof"
            );

            let mut missing_capability = capability.clone();
            missing_capability.remove(&permission);
            assert!(
                !is_registered_and_granted(handlers, permissions, &missing_capability, command),
                "removing {permission} from hub.json must fail the registration proof"
            );
        }
    }

    fn registration_inputs() -> (BTreeSet<String>, BTreeMap<String, String>, BTreeSet<String>) {
        (
            handler_commands(),
            permission_commands(include_str!("../permissions/hub.toml")),
            capability_permissions(include_str!("../capabilities/hub.json")),
        )
    }

    fn network_registration_inputs(
    ) -> (BTreeSet<String>, BTreeMap<String, String>, BTreeSet<String>) {
        (
            handler_commands(),
            permission_commands(include_str!("../permissions/hub.toml")),
            capability_permissions(include_str!("../capabilities/osl-network.json")),
        )
    }

    /// T15-A3/A4 — the "see my recovery phrase again" surface.
    ///
    /// `cmd_osl_view_recovery_phrase` has existed in `crates/ipc` all along,
    /// but the only `invoke_handler` that ever registered it was the excluded
    /// legacy `src-tauri` shell. In the shipping app the phrase was shown once
    /// during onboarding and then unreachable forever. Registration alone is
    /// not enough either: without the `permissions/hub.toml` declaration and
    /// the `capabilities/hub.json` grant the webview's `invoke` is rejected by
    /// the ACL before it reaches the handler, so all three are asserted, and
    /// each one is proven load-bearing by removing it.
    #[test]
    fn view_hub_recovery_phrase_is_registered_and_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_registered_and_granted(
            &handlers,
            &permissions,
            &capability,
            "view_hub_recovery_phrase",
        );
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &["view_hub_recovery_phrase"],
        );
    }

    #[test]
    fn recovery_word_retype_check_is_registered_and_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &["check_hub_recovery_word_retype"],
        );
    }

    #[test]
    fn live_server_revision_report_is_registered_and_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &["get_live_server_revision_report"],
        );
    }

    #[test]
    fn burn_review_state_commands_are_registered_and_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &[
                "save_burn_review_state",
                "get_burn_review_state",
                "back_burn_review",
            ],
        );
    }

    /// D-108 — the missing construction site for the UI's `SecureLocalStore`.
    ///
    /// The store is implemented and unit-tested in `secure-local-store.ts` and
    /// production never built it, because there was no key to build it with.
    /// The key command is that missing line, so it has to clear the same three
    /// surfaces as every other reachable command: without the `hub.toml`
    /// declaration and the `hub.json` grant the webview's `invoke` is rejected
    /// by the ACL before the handler runs, and the UI would silently fall back
    /// to "no store" — which is exactly the state the defect describes.
    #[test]
    fn osl_chat_local_state_key_is_registered_and_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_registered_and_granted(
            &handlers,
            &permissions,
            &capability,
            "get_osl_chat_local_state_key",
        );
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &["get_osl_chat_local_state_key"],
        );
    }

    #[test]
    fn recovery_kit_status_commands_are_registered_and_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &[
                "get_hub_recovery_kit_unsaved",
                "set_hub_recovery_kit_unsaved",
            ],
        );
    }

    #[test]
    fn component_lifecycle_commands_are_registered_and_acl_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &["list_components", "install_component", "remove_component"],
        );
    }

    #[test]
    fn group_verification_build_queries_are_registered_and_acl_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &[
                "compare_allowed_place_direction_state",
                "list_group_verification_build_entries",
            ],
        );
    }

    // D-252: this attribute was NOT here. `component_lifecycle_commands_...`
    // above carried two `#[test]`s and this function carried none, so the
    // `emit_active_session_reset` registration and ACL grant were asserted by a
    // function the harness never called. `duplicate_macro_attributes` is the
    // only gate that ever noticed, and it is in no CI job because the hub is
    // excluded from the linted workspace.
    #[test]
    fn session_reset_emitter_is_registered_and_granted() {
        let (handlers, permissions, capability) = network_registration_inputs();
        assert_registered_and_granted(
            &handlers,
            &permissions,
            &capability,
            "emit_active_session_reset",
        );
    }

    const BROWSER_CONSENT_COMMANDS: [&str; 5] = [
        "list_browser_profiles_for_consent",
        "grant_browser_profile_consent",
        "scan_consented_browser_profile",
        "load_detected_browser_footprint",
        "revoke_detected_browser_footprint",
    ];

    const BROWSER_CONSENT_TAURI_COMMANDS: [&str; 15] = [
        "list_browser_imports",
        "open_browser_import",
        "get_firefox_status",
        "install_firefox",
        "begin_browser_account_import",
        "begin_protected_browser_import",
        "finish_protected_browser_import",
        "launch_firefox_service",
        "get_default_browser_companion_status",
        "host_default_browser_companion",
        "resize_default_browser_companion",
        "focus_default_browser_companion",
        "detach_default_browser_companion",
        "native_app_takeover_requires_consent",
        "host_native_app_window",
    ];

    const F1_FOOTPRINT_NATIVE_COMMANDS: [&str; 2] = [
        "load_detected_browser_footprint",
        "revoke_detected_browser_footprint",
    ];

    const BROWSER_NATIVE_CONSENT_COMMANDS: [&str; 15] = [
        "list_browser_imports",
        "open_browser_import",
        "get_firefox_status",
        "install_firefox",
        "begin_browser_account_import",
        "begin_protected_browser_import",
        "finish_protected_browser_import",
        "launch_firefox_service",
        "get_default_browser_companion_status",
        "host_default_browser_companion",
        "resize_default_browser_companion",
        "focus_default_browser_companion",
        "detach_default_browser_companion",
        "native_app_takeover_requires_consent",
        "host_native_app_window",
    ];

    #[test]
    fn load_detected_browser_footprint_and_revoke_detected_browser_footprint_are_registered_and_durable(
    ) {
        let (handlers, permissions, capability) = registration_inputs();
        let expected_permissions = F1_FOOTPRINT_NATIVE_COMMANDS
            .iter()
            .map(|command| command_permission(command))
            .collect::<BTreeSet<_>>();

        for command in F1_FOOTPRINT_NATIVE_COMMANDS {
            assert_registered_and_granted(&handlers, &permissions, &capability, command);

            let mut without_handler = handlers.clone();
            without_handler.remove(command);
            assert!(
                !is_registered_and_granted(&without_handler, &permissions, &capability, command),
                "removing {command} from generate_handler must fail the footprint command proof"
            );

            let permission = command_permission(command);
            let mut without_permission = permissions.clone();
            without_permission.remove(&permission);
            assert!(
                !is_registered_and_granted(&handlers, &without_permission, &capability, command),
                "removing {permission} from hub.toml must fail the footprint command proof"
            );

            let mut without_capability = capability.clone();
            without_capability.remove(&permission);
            assert!(
                !is_registered_and_granted(&handlers, &permissions, &without_capability, command),
                "removing {permission} from hub.json must fail the footprint command proof"
            );
        }

        let declared_footprint_permissions = permissions
            .iter()
            .filter_map(|(permission, command)| {
                F1_FOOTPRINT_NATIVE_COMMANDS
                    .contains(&command.as_str())
                    .then_some(permission.clone())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            declared_footprint_permissions, expected_permissions,
            "the F1 footprint commands must keep exactly their two fixed permission identifiers"
        );

        let granted_footprint_permissions = capability
            .intersection(&expected_permissions)
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            granted_footprint_permissions, expected_permissions,
            "the main-window capability must keep granting both F1 footprint commands"
        );
    }

    /// The burn-acknowledgement read-back has to be reachable from a shipping
    /// build, which is exactly what it was not: `security::revocation_status`
    /// had no `#[tauri::command]`, no permission, and no capability grant, so
    /// the "must be shown `Not acknowledged` — never a success" contract on
    /// `queue_scope_revocations_locked` had no live surface at all.
    #[test]
    fn get_hub_revocation_status_is_registered_and_acl_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_registered_and_granted(
            &handlers,
            &permissions,
            &capability,
            "get_hub_revocation_status",
        );
    }

    #[test]
    fn browser_consent_commands_are_registered_and_acl_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        let expected_permissions = BROWSER_CONSENT_COMMANDS
            .iter()
            .map(|command| command_permission(command))
            .collect::<BTreeSet<_>>();

        for command in BROWSER_CONSENT_COMMANDS {
            assert_registered_and_granted(&handlers, &permissions, &capability, command);
        }
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &BROWSER_CONSENT_COMMANDS,
        );

        let declared_browser_consent_permissions = permissions
            .iter()
            .filter_map(|(permission, command)| {
                BROWSER_CONSENT_COMMANDS
                    .contains(&command.as_str())
                    .then_some(permission.clone())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            declared_browser_consent_permissions, expected_permissions,
            "the browser-consent command group must use exactly the five fixed permission identifiers"
        );

        let granted_browser_consent_permissions = capability
            .intersection(&expected_permissions)
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            granted_browser_consent_permissions, expected_permissions,
            "the main-window capability must grant every fixed browser-consent permission"
        );

        let mut missing_handler = handlers.clone();
        missing_handler.remove("scan_consented_browser_profile");
        assert!(
            !is_registered_and_granted(
                &missing_handler,
                &permissions,
                &capability,
                "scan_consented_browser_profile",
            ),
            "removing a browser-consent handler entry must make the proof fail"
        );
        let mut missing_permission = permissions.clone();
        missing_permission.remove("allow-load-detected-browser-footprint");
        assert!(
            !is_registered_and_granted(
                &handlers,
                &missing_permission,
                &capability,
                "load_detected_browser_footprint",
            ),
            "removing a browser-consent permission declaration must make the proof fail"
        );
        let mut missing_capability = capability.clone();
        missing_capability.remove("allow-revoke-detected-browser-footprint");
        assert!(
            !is_registered_and_granted(
                &handlers,
                &permissions,
                &missing_capability,
                "revoke_detected_browser_footprint",
            ),
            "removing a browser-consent capability grant must make the proof fail"
        );

        let request = BrowserFootprintConsentRequest {
            browser_id: BrowserImportId::Chrome,
            browser_profile_account: "browser-account".to_owned(),
            browser_profile_id: "profile-id".to_owned(),
            import_run_id: "import-run".to_owned(),
            consent: true,
        };
        let observation = FootprintObservation {
            owner_osl_user_id: "owner-1".to_owned(),
            browser_id: request.browser_id,
            browser_profile_account: request.browser_profile_account.clone(),
            browser_profile_id: request.browser_profile_id.clone(),
            import_run_id: request.import_run_id.clone(),
            observed_at_unix_ms: 12,
        };
        let mut state = browser_footprint::BrowserFootprintState {
            observations: vec![observation],
            bindings: vec![
                checked_browser_footprint_binding("owner-1", &request, false)
                    .expect("well-formed browser footprint binding"),
            ],
        };
        assert!(
            browser_footprint::hydrate_consented_for_owner(
                &state,
                "owner-1",
                request.browser_id,
                &request.browser_profile_account,
                &request.browser_profile_id,
                &request.import_run_id,
            )
            .is_none(),
            "a browser footprint binding without explicit consent must refuse hydration"
        );
        state.bindings = vec![checked_browser_footprint_binding("owner-1", &request, true)
            .expect("explicit browser footprint consent binding")];
        assert_eq!(
            browser_footprint::hydrate_consented_for_owner(
                &state,
                "owner-1",
                request.browser_id,
                &request.browser_profile_account,
                &request.browser_profile_id,
                &request.import_run_id,
            )
            .expect("explicit consent hydrates the exact footprint")
            .len(),
            1,
            "explicit consent must hydrate only the exact owner/browser/profile/import scope"
        );
    }

    fn pw3_test_checked_host() -> CheckedHost {
        CheckedHost {
            context_epoch: 42,
            active: ActiveServiceHost {
                service_id: "discord".to_owned(),
                account_id: "acct-1".to_owned(),
                generation: 9,
                owner_namespace: "owner-ns".to_owned(),
            },
            owner_osl_user_id: "owner-1".to_owned(),
            scope_binding: "scope-binding".to_owned(),
        }
    }

    fn pw3_test_deletion_scan() -> crate::native_discord_adapter::guided_deletion::DeletionScan {
        crate::native_discord_adapter::guided_deletion::DeletionScan {
            scope_binding_hash: "scan-hash".to_owned(),
            generation: 9,
            rows_seen: 1,
            rows_unreadable: 0,
            walk: crate::native_discord_adapter::guided_deletion::WalkCompleteness::Complete,
            candidates: Vec::new(),
        }
    }

    #[test]
    fn browser_consent_tauri_commands_and_acl_are_registered() {
        let (handlers, permissions, capability) = registration_inputs();
        let browser_consent_surface = BROWSER_CONSENT_TAURI_COMMANDS;
        assert_eq!(
            BROWSER_NATIVE_CONSENT_COMMANDS, browser_consent_surface,
            "the legacy browser/native consent alias must match the reconciled Tauri command surface"
        );
        let expected_permissions = browser_consent_surface
            .iter()
            .map(|command| command_permission(command))
            .collect::<BTreeSet<_>>();
        for command in browser_consent_surface {
            assert_registered_and_granted(&handlers, &permissions, &capability, command);
        }
        let declared_permissions = permissions
            .iter()
            .filter_map(|(permission, command)| {
                browser_consent_surface
                    .contains(&command.as_str())
                    .then_some(permission.clone())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            declared_permissions, expected_permissions,
            "the reconciled browser-consent Tauri surface must keep exactly its fixed permission identifiers"
        );

        let granted_permissions = capability
            .intersection(&expected_permissions)
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            granted_permissions, expected_permissions,
            "the main-window capability must grant the full reconciled browser-consent Tauri surface"
        );
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &browser_consent_surface,
        );

        let mut missing_consent_probe = handlers.clone();
        missing_consent_probe.remove("native_app_takeover_requires_consent");
        assert!(
            !is_registered_and_granted(
                &missing_consent_probe,
                &permissions,
                &capability,
                "native_app_takeover_requires_consent",
            ),
            "removing the consent probe must make the proof fail"
        );
        let mut missing_host_handler = handlers.clone();
        missing_host_handler.remove("host_native_app_window");
        assert!(
            !is_registered_and_granted(
                &missing_host_handler,
                &permissions,
                &capability,
                "host_native_app_window",
            ),
            "removing the native host command must make the proof fail"
        );
        let mut missing_permission = permissions.clone();
        missing_permission.remove("allow-native-app-takeover-requires-consent");
        assert!(
            !is_registered_and_granted(
                &handlers,
                &missing_permission,
                &capability,
                "native_app_takeover_requires_consent",
            ),
            "removing the consent probe permission declaration must make the proof fail"
        );
        let mut missing_capability = capability.clone();
        missing_capability.remove("allow-native-app-takeover-requires-consent");
        assert!(
            !is_registered_and_granted(
                &handlers,
                &permissions,
                &missing_capability,
                "native_app_takeover_requires_consent",
            ),
            "removing the consent probe ACL grant must make the proof fail"
        );
    }

    #[test]
    fn hosted_session_scan_commands_are_registered() {
        let (handlers, permissions, capability) = registration_inputs();
        const HOSTED_SESSION_SCAN_COMMANDS: [&str; 6] = [
            "open_hosted_session_scan",
            "request_hosted_session_scan",
            "request_hosted_session_scan_command",
            "scan_discord_own_messages_for_deletion",
            "preview_discord_guided_deletion",
            "execute_discord_guided_deletion",
        ];
        let expected_permissions = HOSTED_SESSION_SCAN_COMMANDS
            .iter()
            .map(|command| command_permission(command))
            .collect::<BTreeSet<_>>();
        for command in HOSTED_SESSION_SCAN_COMMANDS {
            assert_registered_and_granted(&handlers, &permissions, &capability, command);
        }
        let declared_permissions = permissions
            .iter()
            .filter_map(|(permission, command)| {
                HOSTED_SESSION_SCAN_COMMANDS
                    .contains(&command.as_str())
                    .then_some(permission.clone())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            declared_permissions, expected_permissions,
            "hosted session scan commands must keep exactly their fixed permission identifiers"
        );
        let granted_permissions = capability
            .intersection(&expected_permissions)
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            granted_permissions, expected_permissions,
            "hosted session scan commands must all be granted by the main-window capability"
        );
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &HOSTED_SESSION_SCAN_COMMANDS,
        );
        for forbidden in ["request_hosted_session_scan_comman", "delete_own_item"] {
            let forbidden_permission = command_permission(forbidden);
            assert!(
                !handlers.contains(forbidden),
                "{forbidden} must not be registered"
            );
            assert!(
                !permissions.values().any(|command| command == forbidden),
                "{forbidden} must not be ACL-granted"
            );
            assert!(
                !capability.contains(&forbidden_permission),
                "{forbidden} must not have a capability grant"
            );
        }
        for forbidden_permission in [
            "allow-request-hosted-session-scan-comman",
            "allow-delete-own-item",
        ] {
            assert!(
                !permissions.contains_key(forbidden_permission),
                "{forbidden_permission} must not be declared as a Tauri permission"
            );
            assert!(
                !capability.contains(forbidden_permission),
                "{forbidden_permission} must not be granted by the main-window capability"
            );
        }
    }

    #[test]
    fn request_hosted_session_scan_command_routes_through_checked_host() {
        let (handlers, permissions, capability) = registration_inputs();
        assert_registered_and_granted(
            &handlers,
            &permissions,
            &capability,
            "request_hosted_session_scan_command",
        );

        let missing_checked_host_events = RefCell::new(Vec::<&'static str>::new());
        let missing_checked_host = checked_hosted_session_scan_flow(
            || {
                missing_checked_host_events
                    .borrow_mut()
                    .push("checked-host");
                Err("missing checked host".to_owned())
            },
            |_checked| {
                missing_checked_host_events
                    .borrow_mut()
                    .push("attended-binding");
                Ok(vec!["operator".to_owned()])
            },
            |_checked, _operator_names| {
                missing_checked_host_events.borrow_mut().push("native-scan");
                Ok(pw3_test_deletion_scan())
            },
            |_checked| {
                missing_checked_host_events
                    .borrow_mut()
                    .push("context-recheck");
                Ok(())
            },
        );
        match missing_checked_host {
            Err(error) => assert_eq!(error, "missing checked host"),
            Ok(_) => panic!("missing checked host must refuse the hosted scan"),
        }
        assert_eq!(
            missing_checked_host_events.into_inner(),
            ["checked-host"],
            "the hosted scan command must build CheckedHost before binding operators or scanning"
        );

        let events = RefCell::new(Vec::<&'static str>::new());
        let scan = checked_hosted_session_scan_flow(
            || {
                events.borrow_mut().push("checked-host");
                Ok(pw3_test_checked_host())
            },
            |checked| {
                events.borrow_mut().push("attended-binding");
                assert_eq!(checked.active.service_id, "discord");
                Ok(vec!["operator".to_owned()])
            },
            |checked, operator_names| {
                events.borrow_mut().push("native-scan");
                assert_eq!(checked.scope_binding, "scope-binding");
                assert_eq!(operator_names, ["operator".to_owned()]);
                Ok(pw3_test_deletion_scan())
            },
            |checked| {
                events.borrow_mut().push("context-recheck");
                assert_eq!(checked.context_epoch, 42);
                Ok(())
            },
        )
        .expect("checked scan succeeds only after every gate");
        assert_eq!(scan.generation, 9);
        assert_eq!(scan.scope_binding_hash, "scan-hash");
        assert_eq!(scan.rows_seen, 1);
        assert_eq!(
            events.into_inner(),
            [
                "checked-host",
                "attended-binding",
                "native-scan",
                "context-recheck"
            ],
            "scan must be checked host -> attended binding -> native scan -> context recheck"
        );

        let refusal_events = RefCell::new(Vec::<&'static str>::new());
        let refused = checked_hosted_session_scan_flow(
            || {
                refusal_events.borrow_mut().push("checked-host");
                Ok(pw3_test_checked_host())
            },
            |_checked| {
                refusal_events.borrow_mut().push("attended-binding");
                Err("missing attended binding".to_owned())
            },
            |_checked, _operator_names| {
                refusal_events.borrow_mut().push("native-scan");
                Ok(pw3_test_deletion_scan())
            },
            |_checked| {
                refusal_events.borrow_mut().push("context-recheck");
                Ok(())
            },
        );
        match refused {
            Err(error) => assert_eq!(error, "missing attended binding"),
            Ok(_) => panic!("missing attended binding must refuse"),
        }
        assert_eq!(
            refusal_events.into_inner(),
            ["checked-host", "attended-binding"],
            "absence of attended binding must refuse before scan or post-scan success"
        );

        let empty_binding_events = RefCell::new(Vec::<&'static str>::new());
        let empty_binding = checked_hosted_session_scan_flow(
            || {
                empty_binding_events.borrow_mut().push("checked-host");
                Ok(pw3_test_checked_host())
            },
            |_checked| {
                empty_binding_events.borrow_mut().push("attended-binding");
                Ok(Vec::new())
            },
            |_checked, _operator_names| {
                empty_binding_events.borrow_mut().push("native-scan");
                Ok(pw3_test_deletion_scan())
            },
            |_checked| {
                empty_binding_events.borrow_mut().push("context-recheck");
                Ok(())
            },
        );
        match empty_binding {
            Err(error) => assert_eq!(
                error,
                "Hosted session scan requires a reviewed attended operator-name binding"
            ),
            Ok(_) => panic!("an empty attended binding must refuse before native scan"),
        }
        assert_eq!(
            empty_binding_events.into_inner(),
            ["checked-host", "attended-binding"],
            "an empty attended binding must not be interpreted as permission to scan"
        );

        let generation_drift_events = RefCell::new(Vec::<&'static str>::new());
        let generation_drift = checked_hosted_session_scan_flow(
            || {
                generation_drift_events.borrow_mut().push("checked-host");
                Ok(pw3_test_checked_host())
            },
            |_checked| {
                generation_drift_events
                    .borrow_mut()
                    .push("attended-binding");
                Ok(vec!["operator".to_owned()])
            },
            |_checked, _operator_names| {
                generation_drift_events.borrow_mut().push("native-scan");
                let mut scan = pw3_test_deletion_scan();
                scan.generation += 1;
                Ok(scan)
            },
            |_checked| {
                generation_drift_events.borrow_mut().push("context-recheck");
                Ok(())
            },
        );
        match generation_drift {
            Err(error) => assert_eq!(
                error,
                "Hosted session scan context changed during native scan"
            ),
            Ok(_) => panic!("a scan from a different native generation must refuse"),
        }
        assert_eq!(
            generation_drift_events.into_inner(),
            ["checked-host", "attended-binding", "native-scan"],
            "a generation-mismatched scan must refuse before context recheck or result return"
        );

        let stale_context_events = RefCell::new(Vec::<&'static str>::new());
        let stale_context = checked_hosted_session_scan_flow(
            || {
                stale_context_events.borrow_mut().push("checked-host");
                Ok(pw3_test_checked_host())
            },
            |_checked| {
                stale_context_events.borrow_mut().push("attended-binding");
                Ok(vec!["operator".to_owned()])
            },
            |_checked, _operator_names| {
                stale_context_events.borrow_mut().push("native-scan");
                Ok(pw3_test_deletion_scan())
            },
            |_checked| {
                stale_context_events.borrow_mut().push("context-recheck");
                Err("stale hosted context".to_owned())
            },
        );
        match stale_context {
            Err(error) => assert_eq!(error, "stale hosted context"),
            Ok(_) => panic!("stale hosted context must refuse after native scan"),
        }
        assert_eq!(
            stale_context_events.into_inner(),
            [
                "checked-host",
                "attended-binding",
                "native-scan",
                "context-recheck"
            ],
            "a stale hosted context must refuse after native scan and before returning data"
        );

        let native_scan_refusal_events = RefCell::new(Vec::<&'static str>::new());
        let native_scan_refused = checked_hosted_session_scan_flow(
            || {
                native_scan_refusal_events.borrow_mut().push("checked-host");
                Ok(pw3_test_checked_host())
            },
            |_checked| {
                native_scan_refusal_events
                    .borrow_mut()
                    .push("attended-binding");
                Ok(vec!["operator".to_owned()])
            },
            |_checked, _operator_names| {
                native_scan_refusal_events.borrow_mut().push("native-scan");
                Err("native scan refused".to_owned())
            },
            |_checked| {
                native_scan_refusal_events
                    .borrow_mut()
                    .push("context-recheck");
                Ok(())
            },
        );
        match native_scan_refused {
            Err(error) => assert_eq!(error, "native scan refused"),
            Ok(_) => panic!("native scan refusal must not be converted into success"),
        }
        assert_eq!(
            native_scan_refusal_events.into_inner(),
            ["checked-host", "attended-binding", "native-scan"],
            "native scan refusal must stop before context recheck or result return"
        );

        let missing_checked_host_events = RefCell::new(Vec::<&'static str>::new());
        let missing_checked_host = checked_hosted_session_scan_flow(
            || {
                missing_checked_host_events
                    .borrow_mut()
                    .push("checked-host");
                Err("missing checked host".to_owned())
            },
            |_checked| {
                missing_checked_host_events
                    .borrow_mut()
                    .push("attended-binding");
                Ok(vec!["operator".to_owned()])
            },
            |_checked, _operator_names| {
                missing_checked_host_events.borrow_mut().push("native-scan");
                Ok(pw3_test_deletion_scan())
            },
            |_checked| {
                missing_checked_host_events
                    .borrow_mut()
                    .push("context-recheck");
                Ok(())
            },
        );
        match missing_checked_host {
            Err(error) => assert_eq!(error, "missing checked host"),
            Ok(_) => panic!("missing checked host must refuse the hosted scan request"),
        }
        assert_eq!(
            missing_checked_host_events.into_inner(),
            ["checked-host"],
            "the hosted scan route must build CheckedHost before binding, scanning, or returning success"
        );
    }

    #[test]
    fn identity_binding_verifier_is_wired_as_review_ui_production_caller() {
        let state = HubCoreState::default();
        *state.osl.license_state.lock().expect("license state lock") = keystore::LicenseStateDto {
            state: keystore::LicenseState::Paid,
            raw_status: "ACTIVE".to_owned(),
            current_period_end: None,
            last_validated_at: None,
        };
        state.osl.install_identity(keystore::identity_from_entropy(
            [77; 16],
            "owner".to_owned(),
        ));
        let before = autoscrub_run::fleet_status(&state.osl)
            .expect("paid test state can read AutoScrub fleet");
        let request = AutoScrubReviewedRunRequest {
            service_id: ServiceKind::Discord,
            account_id: "acct-1".to_owned(),
            review_token: "review-token".to_owned(),
            plan_digest: "a".repeat(64),
            reviewed_item_count: 1,
            consent: autoscrub_run::AutoScrubRunConsent::ReviewedBatchOnly,
        };
        let owner_identity = state
            .osl
            .identity
            .lock()
            .expect("identity lock")
            .as_ref()
            .cloned()
            .expect("test identity installed");
        let owner = PinnedOwner::from_identity(&owner_identity);
        let mut verifier = IdentityBindingVerifier::new(owner);
        let refused_before_start = Cell::new(false);
        let refused_checked = start_autoscrub_reviewed_run_checked(
            &state,
            request.clone(),
            |_| Ok(IdentityBindingVerifier::new(owner)),
            |_, _| {
                refused_before_start.set(true);
                Ok(before.clone())
            },
        );
        match refused_checked {
            Err(error) => assert_eq!(
                error, "A reviewed identity binding is required before starting AutoScrub",
                "the production review gate must refuse before the reviewed-run start callback"
            ),
            Ok(_) => panic!("missing review binding must refuse before starting AutoScrub"),
        }
        assert!(
            !refused_before_start.get(),
            "missing review binding must stop before any AutoScrub run can open"
        );
        assert_eq!(
            require_review_ui_identity_binding_from_verifier(&verifier, &request),
            Err("A reviewed identity binding is required before starting AutoScrub".to_owned()),
            "absence of an exact review binding must refuse before the reviewed run opens"
        );
        verifier
            .bind(
                AccountRef {
                    service_id: "discord".to_owned(),
                    account_id: "acct-1".to_owned(),
                },
                BindingScope::ScrubIndex,
                BindingEvidence::CallerAttested,
            )
            .expect("index binding can be recorded");
        assert_eq!(
            require_review_ui_identity_binding_from_verifier(&verifier, &request),
            Err("A reviewed identity binding is required before starting AutoScrub".to_owned()),
            "a non-destructive review binding must not authorize a destructive AutoScrub run"
        );
        verifier
            .bind(
                AccountRef {
                    service_id: "discord".to_owned(),
                    account_id: "acct-2".to_owned(),
                },
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .expect("other-account deletion binding can be recorded");
        assert_eq!(
            require_review_ui_identity_binding_from_verifier(&verifier, &request),
            Err("A reviewed identity binding is required before starting AutoScrub".to_owned()),
            "a deletion binding for another account must not authorize this reviewed run"
        );
        verifier
            .bind(
                AccountRef {
                    service_id: "discord".to_owned(),
                    account_id: "acct-1".to_owned(),
                },
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .expect("exact deletion binding can be recorded");
        assert_eq!(
            require_review_ui_identity_binding_from_verifier(&verifier, &request),
            Ok(()),
            "the review UI production helper must accept only the exact ScrubDeletion binding"
        );

        let started_without_binding = Cell::new(false);
        let unbound_verifier =
            IdentityBindingVerifier::new(PinnedOwner::from_identity(&owner_identity));
        match start_autoscrub_reviewed_run_after_review_ui_binding(
            &unbound_verifier,
            request.clone(),
            |_| {
                started_without_binding.set(true);
                Ok(())
            },
        ) {
            Err(error) => assert_eq!(
                error,
                "A reviewed identity binding is required before starting AutoScrub"
            ),
            Ok(_) => panic!("missing review binding must refuse before start"),
        }
        assert!(
            !started_without_binding.get(),
            "the reviewed run start closure must be unreachable without the binding"
        );

        let start_events = RefCell::new(Vec::<&'static str>::new());
        let started = start_autoscrub_reviewed_run_after_review_ui_binding(
            &verifier,
            request.clone(),
            |started_request| {
                start_events.borrow_mut().push("start");
                assert_eq!(started_request.account_id, "acct-1");
                Ok(started_request.reviewed_item_count)
            },
        )
        .expect("exact review binding allows the production start step to run");
        assert_eq!(started, 1);
        assert_eq!(start_events.into_inner(), ["start"]);

        let mut accepted_verifier = IdentityBindingVerifier::new(owner);
        accepted_verifier
            .bind(
                AccountRef {
                    service_id: "discord".to_owned(),
                    account_id: "acct-1".to_owned(),
                },
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .expect("exact deletion binding can be recorded for checked path");
        let accepted_start = Cell::new(false);
        let accepted = start_autoscrub_reviewed_run_checked(
            &state,
            request.clone(),
            |_| Ok(accepted_verifier),
            |_, accepted_request| {
                accepted_start.set(true);
                assert!(
                    accepted_request.account_id == "acct-1",
                    "accepted request must target the reviewed account"
                );
                Ok(before.clone())
            },
        )
        .expect("an exact reviewed binding reaches the reviewed-run start callback");
        assert_eq!(accepted.open_run_count, before.open_run_count);
        assert!(
            accepted_start.get(),
            "exact ScrubDeletion binding must be enough to reach the reviewed-run start callback"
        );
        match start_autoscrub_reviewed_run_inner(&state, request) {
            Err(error) => assert_eq!(
                error, "A reviewed identity binding is required before starting AutoScrub",
                "the reviewed-run production path must refuse before starting without an exact binding"
            ),
            Ok(_) => panic!("reviewed AutoScrub must not start without an exact identity binding"),
        }
        let after = autoscrub_run::fleet_status(&state.osl)
            .expect("paid test state can reread AutoScrub fleet");
        assert_eq!(
            after.open_run_count, before.open_run_count,
            "missing reviewed identity binding must refuse before opening a reviewed run"
        );
    }

    #[test]
    fn autoscrub_run_lifecycle_commands_are_registered_and_acl_granted() {
        let (handlers, permissions, capability) = registration_inputs();
        const AUTOSCRUB_RUN_LIFECYCLE_COMMANDS: [&str; 3] = [
            "get_autoscrub_run_fl",
            "start_autoscrub_reviewed_run",
            "request_autoscrub_global_stop",
        ];
        let expected_permissions = AUTOSCRUB_RUN_LIFECYCLE_COMMANDS
            .iter()
            .map(|command| command_permission(command))
            .collect::<BTreeSet<_>>();
        for command in AUTOSCRUB_RUN_LIFECYCLE_COMMANDS {
            assert_registered_and_granted(&handlers, &permissions, &capability, command);
        }
        let declared_permissions = permissions
            .iter()
            .filter_map(|(permission, command)| {
                AUTOSCRUB_RUN_LIFECYCLE_COMMANDS
                    .contains(&command.as_str())
                    .then_some(permission.clone())
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            declared_permissions, expected_permissions,
            "AutoScrub lifecycle commands must keep exactly their fixed permission identifiers"
        );
        let granted_permissions = capability
            .intersection(&expected_permissions)
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            granted_permissions, expected_permissions,
            "AutoScrub lifecycle commands must all be granted by the main-window capability"
        );
        assert_each_registration_surface_is_required(
            &handlers,
            &permissions,
            &capability,
            &AUTOSCRUB_RUN_LIFECYCLE_COMMANDS,
        );
        for forbidden in [
            "open_autoscrub_reviewed_run",
            "step_autoscrub_reviewed_run",
            "halt_autoscrub_reviewed_run",
            "start_autoscrub_reviewed_run_command",
        ] {
            let forbidden_permission = command_permission(forbidden);
            assert!(
                !handlers.contains(forbidden),
                "{forbidden} must not be registered as an AutoScrub lifecycle command"
            );
            assert!(
                !permissions.values().any(|command| command == forbidden),
                "{forbidden} must not be declared in the AutoScrub lifecycle ACL"
            );
            assert!(
                !capability.contains(&forbidden_permission),
                "{forbidden_permission} must not be granted by the main-window capability"
            );
        }
    }
}

#[cfg(all(test, feature = "discord-qa-shell"))]
mod b6_startup_gate_tests {
    use super::{
        poll_native_discord_headless_qa_restart_proof_flow, ActiveServiceHost,
        DiscordHeadlessQaPoll,
    };
    use std::cell::RefCell;

    #[test]
    fn p5_offline_queue_restart_proof() {
        struct FakeOpened {
            opened_count: usize,
            pending_view_once_count: usize,
            acknowledgment_count: usize,
            fetched: u32,
        }

        fn relaunched_host() -> ActiveServiceHost {
            ActiveServiceHost {
                service_id: "discord".to_owned(),
                account_id: "native-discord-b".to_owned(),
                generation: 2,
                owner_namespace: "owner-b".to_owned(),
            }
        }

        fn summarize(opened: &FakeOpened) -> DiscordHeadlessQaPoll {
            DiscordHeadlessQaPoll {
                opened_count: opened.opened_count,
                pending_view_once_count: opened.pending_view_once_count,
                acknowledgment_count: opened.acknowledgment_count,
                fetched: opened.fetched,
            }
        }

        let stale_pre_kill_context_token = "stale-token-before-b-restart";
        let events = RefCell::new(Vec::<&'static str>::new());
        let poll = poll_native_discord_headless_qa_restart_proof_flow(
            "main",
            || {
                events.borrow_mut().push("owner");
                Ok("osl-b".to_owned())
            },
            || {
                events.borrow_mut().push("reload-context-token");
                Ok("token-after-b-relaunch".to_owned())
            },
            |owner| {
                events.borrow_mut().push("host-before");
                assert_eq!(owner, "osl-b");
                Ok(relaunched_host())
            },
            |context_token, host| {
                events.borrow_mut().push("validate-before");
                assert_eq!(context_token, "token-after-b-relaunch");
                assert_ne!(context_token, stale_pre_kill_context_token);
                assert_eq!(host.generation, 2);
                Ok(())
            },
            || {
                events.borrow_mut().push("drain-production-inbox");
                Ok(FakeOpened {
                    opened_count: 1,
                    pending_view_once_count: 2,
                    acknowledgment_count: 3,
                    fetched: 4,
                })
            },
            |opened| {
                events.borrow_mut().push("poll-receipt");
                assert_eq!(
                    opened.expect("poll records successful drain").opened_count,
                    1
                );
                Ok(())
            },
            |owner| {
                events.borrow_mut().push("host-after");
                assert_eq!(owner, "osl-b");
                Ok(relaunched_host())
            },
            |context_token, host| {
                events.borrow_mut().push("validate-after");
                assert_eq!(context_token, "token-after-b-relaunch");
                assert_eq!(host.generation, 2);
                Ok(())
            },
            |opened| {
                events.borrow_mut().push("opened-receipt");
                assert_eq!(opened.fetched, 4);
                Ok(())
            },
            summarize,
        )
        .expect("B can relaunch and verify the queued drain through reloaded state");
        assert_eq!(poll.opened_count, 1);
        assert_eq!(poll.pending_view_once_count, 2);
        assert_eq!(poll.acknowledgment_count, 3);
        assert_eq!(poll.fetched, 4);
        assert_eq!(
            events.into_inner(),
            [
                "owner",
                "reload-context-token",
                "host-before",
                "validate-before",
                "drain-production-inbox",
                "poll-receipt",
                "host-after",
                "validate-after",
                "opened-receipt",
            ],
            "restart proof must reload and validate B's current host before and after draining"
        );

        let untrusted_events = RefCell::new(Vec::<&'static str>::new());
        let untrusted = poll_native_discord_headless_qa_restart_proof_flow(
            "native-discord-overlay",
            || {
                untrusted_events.borrow_mut().push("owner");
                Ok("osl-b".to_owned())
            },
            || Ok("token-after-b-relaunch".to_owned()),
            |_| Ok(relaunched_host()),
            |_, _| Ok(()),
            || {
                untrusted_events.borrow_mut().push("drain-production-inbox");
                Ok(FakeOpened {
                    opened_count: 1,
                    pending_view_once_count: 0,
                    acknowledgment_count: 0,
                    fetched: 1,
                })
            },
            |_| Ok(()),
            |_| Ok(relaunched_host()),
            |_, _| Ok(()),
            |_| Ok(()),
            summarize,
        );
        match untrusted {
            Err(error) => assert_eq!(
                error,
                "Only the trusted Discord QA shell may poll headless QA"
            ),
            Ok(_) => panic!("an untrusted caller must not drive B's restart drain"),
        }
        assert!(
            untrusted_events.into_inner().is_empty(),
            "untrusted callers must refuse before owner, context, host, or drain state is read"
        );

        let stale_validation_events = RefCell::new(Vec::<&'static str>::new());
        let stale_validation = poll_native_discord_headless_qa_restart_proof_flow(
            "main",
            || {
                stale_validation_events.borrow_mut().push("owner");
                Ok("osl-b".to_owned())
            },
            || {
                stale_validation_events
                    .borrow_mut()
                    .push("reload-context-token");
                Ok(stale_pre_kill_context_token.to_owned())
            },
            |_| {
                stale_validation_events.borrow_mut().push("host-before");
                Ok(relaunched_host())
            },
            |context_token, _host| {
                stale_validation_events.borrow_mut().push("validate-before");
                assert_eq!(context_token, stale_pre_kill_context_token);
                Err("stale native context".to_owned())
            },
            || {
                stale_validation_events
                    .borrow_mut()
                    .push("drain-production-inbox");
                Ok(FakeOpened {
                    opened_count: 1,
                    pending_view_once_count: 0,
                    acknowledgment_count: 0,
                    fetched: 1,
                })
            },
            |_| Ok(()),
            |_| Ok(relaunched_host()),
            |_, _| Ok(()),
            |_| Ok(()),
            summarize,
        );
        match stale_validation {
            Err(error) => assert_eq!(error, "stale native context"),
            Ok(_) => panic!("a stale pre-restart context token must refuse before drain"),
        }
        assert_eq!(
            stale_validation_events.into_inner(),
            [
                "owner",
                "reload-context-token",
                "host-before",
                "validate-before",
            ],
            "stale context validation must stop before the inbox drain"
        );

        let failed_drain_events = RefCell::new(Vec::<&'static str>::new());
        let failed_drain = poll_native_discord_headless_qa_restart_proof_flow(
            "main",
            || {
                failed_drain_events.borrow_mut().push("owner");
                Ok("osl-b".to_owned())
            },
            || {
                failed_drain_events
                    .borrow_mut()
                    .push("reload-context-token");
                Ok("token-after-b-relaunch".to_owned())
            },
            |_| {
                failed_drain_events.borrow_mut().push("host-before");
                Ok(relaunched_host())
            },
            |_, _| {
                failed_drain_events.borrow_mut().push("validate-before");
                Ok(())
            },
            || {
                failed_drain_events
                    .borrow_mut()
                    .push("drain-production-inbox");
                Err("offline queue unavailable".to_owned())
            },
            |opened| {
                failed_drain_events.borrow_mut().push("poll-receipt");
                match opened {
                    Err(error) => assert_eq!(error, "offline queue unavailable"),
                    Ok(_) => panic!("poll receipt must receive the drain refusal"),
                }
                Ok(())
            },
            |_| {
                failed_drain_events.borrow_mut().push("host-after");
                Ok(relaunched_host())
            },
            |_, _| Ok(()),
            |_| Ok(()),
            summarize,
        );
        match failed_drain {
            Err(error) => assert_eq!(error, "offline queue unavailable"),
            Ok(_) => panic!("a failed drain must not return a green restart proof"),
        }
        assert_eq!(
            failed_drain_events.into_inner(),
            [
                "owner",
                "reload-context-token",
                "host-before",
                "validate-before",
                "drain-production-inbox",
                "poll-receipt",
            ],
            "a failed drain is recorded but cannot advance to host recheck or opened receipt"
        );

        let drift_events = RefCell::new(Vec::<&'static str>::new());
        let drift = poll_native_discord_headless_qa_restart_proof_flow(
            "main",
            || {
                drift_events.borrow_mut().push("owner");
                Ok("osl-b".to_owned())
            },
            || {
                drift_events.borrow_mut().push("reload-context-token");
                Ok("token-after-b-relaunch".to_owned())
            },
            |_| {
                drift_events.borrow_mut().push("host-before");
                Ok(relaunched_host())
            },
            |_, _| {
                drift_events.borrow_mut().push("validate-before");
                Ok(())
            },
            || {
                drift_events.borrow_mut().push("drain-production-inbox");
                Ok(FakeOpened {
                    opened_count: 1,
                    pending_view_once_count: 0,
                    acknowledgment_count: 0,
                    fetched: 1,
                })
            },
            |opened| {
                drift_events.borrow_mut().push("poll-receipt");
                assert!(opened.is_ok());
                Ok(())
            },
            |_| {
                drift_events.borrow_mut().push("host-after");
                let mut host = relaunched_host();
                host.generation += 1;
                Ok(host)
            },
            |_, _| {
                drift_events.borrow_mut().push("validate-after");
                Ok(())
            },
            |_| {
                drift_events.borrow_mut().push("opened-receipt");
                Ok(())
            },
            summarize,
        );
        match drift {
            Err(error) => assert_eq!(error, "The native Discord QA host changed during receive"),
            Ok(_) => panic!("host drift after drain must refuse the restart proof"),
        }
        assert_eq!(
            drift_events.into_inner(),
            [
                "owner",
                "reload-context-token",
                "host-before",
                "validate-before",
                "drain-production-inbox",
                "poll-receipt",
                "host-after",
            ],
            "host drift must refuse before the opened receipt is recorded"
        );

        let stale_after_events = RefCell::new(Vec::<&'static str>::new());
        let stale_after = poll_native_discord_headless_qa_restart_proof_flow(
            "main",
            || {
                stale_after_events.borrow_mut().push("owner");
                Ok("osl-b".to_owned())
            },
            || {
                stale_after_events.borrow_mut().push("reload-context-token");
                Ok("token-after-b-relaunch".to_owned())
            },
            |_| {
                stale_after_events.borrow_mut().push("host-before");
                Ok(relaunched_host())
            },
            |_, _| {
                stale_after_events.borrow_mut().push("validate-before");
                Ok(())
            },
            || {
                stale_after_events
                    .borrow_mut()
                    .push("drain-production-inbox");
                Ok(FakeOpened {
                    opened_count: 1,
                    pending_view_once_count: 0,
                    acknowledgment_count: 0,
                    fetched: 1,
                })
            },
            |opened| {
                stale_after_events.borrow_mut().push("poll-receipt");
                assert!(opened.is_ok());
                Ok(())
            },
            |_| {
                stale_after_events.borrow_mut().push("host-after");
                Ok(relaunched_host())
            },
            |_, _| {
                stale_after_events.borrow_mut().push("validate-after");
                Err("stale native context after drain".to_owned())
            },
            |_| {
                stale_after_events.borrow_mut().push("opened-receipt");
                Ok(())
            },
            summarize,
        );
        match stale_after {
            Err(error) => assert_eq!(error, "stale native context after drain"),
            Ok(_) => panic!("post-drain host validation failure must refuse the restart proof"),
        }
        assert_eq!(
            stale_after_events.into_inner(),
            [
                "owner",
                "reload-context-token",
                "host-before",
                "validate-before",
                "drain-production-inbox",
                "poll-receipt",
                "host-after",
                "validate-after",
            ],
            "post-drain validation must refuse before recording opened receipt success"
        );
    }
}
