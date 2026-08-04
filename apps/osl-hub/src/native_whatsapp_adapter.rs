//! Pure structural contracts for WhatsApp's native WebView2 surface.
//!
//! The live Windows accessibility side is allowed to collect only bounded
//! metadata. This module keeps the trust, discovery, and placement decisions
//! deterministic and testable without touching the remote process.

use sha2::{Digest, Sha256};

pub use crate::native_a11y::{
    acquire_uia2_window, clear_uia2_composer, place_uia2_carrier, resolve_uia2_composer,
    uia2_carrier_carries_submit, Uia2AcquireError, Uia2CallTimeout, Uia2ComposerError,
    Uia2ComposerMatcher, Uia2Deadline, Uia2Editable, Uia2OwnedWindow, Uia2PlacementRefusal,
    Uia2ResolvedWindow, Uia2Syscalls, Uia2TreeRoute, Uia2WakePolicy, Uia2WindowPlan,
    Uia2WindowResolveError, Uia2WindowShape, WEBVIEW2_PROCESS_NAME,
};

pub const WHATSAPP_ROOT_WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";
pub const WHATSAPP_CARRIER_PREFIX: &str = "OSL1.WA.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhatsAppProcessKind {
    StoreAppRoot,
    WebView2Child,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhatsAppControlType {
    Window,
    Pane,
    Document,
    Edit,
    List,
    ListItem,
    Text,
    Button,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WhatsAppBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WhatsAppBounds {
    fn is_positive(self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatsAppStructuralNode {
    pub structural_id: String,
    pub parent_structural_id: Option<String>,
    pub process_kind: WhatsAppProcessKind,
    pub store_package_family_name: Option<String>,
    pub window_class: Option<String>,
    pub control_type: WhatsAppControlType,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub framework_id: Option<String>,
    pub runtime_hash: Option<String>,
    pub bounds: Option<WhatsAppBounds>,
    pub enabled: bool,
    pub offscreen: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedWhatsAppContentRoot {
    pub app_root_id: String,
    pub webview_ancestor_id: String,
    pub content_root_id: String,
    pub content_runtime_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatsAppComposerBinding {
    pub node_id: String,
    pub runtime_hash: String,
    pub bounds: WhatsAppBounds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhatsAppTranscriptBinding {
    pub node_id: String,
    pub runtime_hash: String,
    pub bounds: WhatsAppBounds,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredWhatsAppPair {
    pub content_root: TrustedWhatsAppContentRoot,
    pub composer: WhatsAppComposerBinding,
    pub transcript: WhatsAppTranscriptBinding,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WhatsAppTextFragmentRole {
    ExactBody,
    SenderLabel,
    Timestamp,
    Reaction,
    LinkPreviewTitle,
    LinkPreviewDescription,
    Summary,
    Unknown,
}

#[derive(Clone, PartialEq, Eq)]
pub struct WhatsAppTextFragment {
    pub role: WhatsAppTextFragmentRole,
    pub text: String,
}

#[derive(Clone, PartialEq, Eq)]
pub struct WhatsAppTranscriptRow {
    pub row_id: String,
    pub transcript_node_id: String,
    pub runtime_hash: String,
    pub bounds: Option<WhatsAppBounds>,
    pub offscreen: bool,
    pub fragments: Vec<WhatsAppTextFragment>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct WhatsAppBodyCandidate {
    pub row_id: String,
    pub row_runtime_hash: String,
    pub body: String,
    pub bounds: WhatsAppBounds,
}

#[derive(Clone, PartialEq, Eq)]
pub struct WhatsAppCarrierRowProof {
    pub row_id: String,
    pub row_runtime_hash: String,
    pub carrier_sha256_label: String,
    pub bounds: WhatsAppBounds,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PlacedWhatsAppCarrier {
    pub composer_node_id: String,
    pub composer_runtime_hash: String,
    pub carrier: String,
    pub row_proof: WhatsAppCarrierRowProof,
}

#[derive(Clone, PartialEq, Eq)]
pub enum WhatsAppLinkPreviewBlockReason {
    InvalidCarrier,
    ExactBodyUnavailable,
    PreviewFragmentsAreNotExactBody,
    MissingCarrierRowProof,
    AmbiguousCarrierRowProof,
}

#[derive(Clone, PartialEq, Eq)]
pub enum WhatsAppLinkPreviewCapability {
    SupportedWithoutPreview {
        row_id: String,
        carrier_sha256_label: String,
    },
    Blocked {
        reason: WhatsAppLinkPreviewBlockReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhatsAppAdapterRefusal {
    MissingExactAppRoot,
    AmbiguousAppRoot,
    MissingWebView2ContentRoot,
    AmbiguousWebView2ContentRoot,
    MissingComposer,
    AmbiguousComposer,
    MissingTranscript,
    AmbiguousTranscript,
    MissingExactBodyCandidate,
    BodyCandidateSupportBlocked,
    InvalidCarrier,
    MissingCarrierRowProof,
    AmbiguousCarrierRowProof,
}

pub fn trusted_whatsapp_content_root(
    nodes: &[WhatsAppStructuralNode],
) -> Result<TrustedWhatsAppContentRoot, WhatsAppAdapterRefusal> {
    let roots: Vec<&WhatsAppStructuralNode> = nodes
        .iter()
        .filter(|node| {
            node.process_kind == WhatsAppProcessKind::StoreAppRoot
                && node.store_package_family_name.as_deref()
                    == Some(crate::native_apps::whatsapp_store_package_family_name())
                && node.parent_structural_id.is_none()
                && node.window_class.as_deref() == Some(WHATSAPP_ROOT_WINDOW_CLASS)
                && node.control_type == WhatsAppControlType::Window
                && node.enabled
                && !node.offscreen
                && node.bounds.map_or(false, WhatsAppBounds::is_positive)
        })
        .collect();
    let app_root = match roots.as_slice() {
        [root] => *root,
        [] => return Err(WhatsAppAdapterRefusal::MissingExactAppRoot),
        _ => return Err(WhatsAppAdapterRefusal::AmbiguousAppRoot),
    };

    let content_roots: Vec<TrustedWhatsAppContentRoot> = nodes
        .iter()
        .filter_map(|node| {
            let webview_ancestor_id = trusted_webview_ancestor_id(nodes, app_root, node)?;
            if !is_content_root_candidate(node) {
                return None;
            }
            Some(TrustedWhatsAppContentRoot {
                app_root_id: app_root.structural_id.clone(),
                webview_ancestor_id,
                content_root_id: node.structural_id.clone(),
                content_runtime_hash: node.runtime_hash.clone()?,
            })
        })
        .collect();

    match content_roots.as_slice() {
        [content_root] => Ok(content_root.clone()),
        [] => Err(WhatsAppAdapterRefusal::MissingWebView2ContentRoot),
        _ => Err(WhatsAppAdapterRefusal::AmbiguousWebView2ContentRoot),
    }
}

pub fn discover_whatsapp_pair(
    nodes: &[WhatsAppStructuralNode],
) -> Result<DiscoveredWhatsAppPair, WhatsAppAdapterRefusal> {
    let content_root = trusted_whatsapp_content_root(nodes)?;
    let composer_candidates: Vec<WhatsAppComposerBinding> = nodes
        .iter()
        .filter(|node| descendant_of(nodes, node, &content_root.content_root_id))
        .filter_map(composer_binding)
        .collect();
    let transcript_candidates: Vec<WhatsAppTranscriptBinding> = nodes
        .iter()
        .filter(|node| descendant_of(nodes, node, &content_root.content_root_id))
        .filter(|node| node.structural_id != content_root.content_root_id)
        .filter_map(transcript_binding)
        .collect();

    let composer = match composer_candidates.as_slice() {
        [composer] => composer.clone(),
        [] => return Err(WhatsAppAdapterRefusal::MissingComposer),
        _ => return Err(WhatsAppAdapterRefusal::AmbiguousComposer),
    };
    let transcript = match transcript_candidates.as_slice() {
        [transcript] => transcript.clone(),
        [] => return Err(WhatsAppAdapterRefusal::MissingTranscript),
        _ => return Err(WhatsAppAdapterRefusal::AmbiguousTranscript),
    };

    Ok(DiscoveredWhatsAppPair {
        content_root,
        composer,
        transcript,
    })
}

pub fn extract_whatsapp_body_candidates(
    pair: &DiscoveredWhatsAppPair,
    rows: &[WhatsAppTranscriptRow],
) -> Result<Vec<WhatsAppBodyCandidate>, WhatsAppAdapterRefusal> {
    let mut candidates = Vec::new();
    for row in rows.iter().filter(|row| {
        row.transcript_node_id == pair.transcript.node_id
            && !row.offscreen
            && row.bounds.map_or(false, WhatsAppBounds::is_positive)
    }) {
        let mut row_has_blocking_text = false;
        for fragment in &row.fragments {
            let text = canonical_whatsapp_body(&fragment.text);
            if text.is_empty() {
                continue;
            }
            match fragment.role {
                WhatsAppTextFragmentRole::ExactBody => {
                    candidates.push(WhatsAppBodyCandidate {
                        row_id: row.row_id.clone(),
                        row_runtime_hash: row.runtime_hash.clone(),
                        body: text,
                        bounds: row.bounds.expect("row bounds were checked above"),
                    });
                }
                WhatsAppTextFragmentRole::SenderLabel
                | WhatsAppTextFragmentRole::Timestamp
                | WhatsAppTextFragmentRole::Reaction => {}
                WhatsAppTextFragmentRole::LinkPreviewTitle
                | WhatsAppTextFragmentRole::LinkPreviewDescription
                | WhatsAppTextFragmentRole::Summary
                | WhatsAppTextFragmentRole::Unknown => {
                    row_has_blocking_text = true;
                }
            }
        }
        if row_has_blocking_text {
            return Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked);
        }
    }
    if candidates.is_empty() {
        return Err(WhatsAppAdapterRefusal::MissingExactBodyCandidate);
    }
    Ok(candidates)
}

pub fn place_whatsapp_carrier(
    pair: &DiscoveredWhatsAppPair,
    carrier_payload: &str,
    rows_after_write: &[WhatsAppTranscriptRow],
) -> Result<PlacedWhatsAppCarrier, WhatsAppAdapterRefusal> {
    let carrier = prefixed_whatsapp_carrier(carrier_payload)?;
    let matching_candidates: Vec<WhatsAppBodyCandidate> =
        extract_whatsapp_body_candidates(pair, rows_after_write)?
            .into_iter()
            .filter(|candidate| candidate.body == carrier)
            .collect();
    let proof_candidate = match matching_candidates.as_slice() {
        [candidate] => candidate,
        [] => return Err(WhatsAppAdapterRefusal::MissingCarrierRowProof),
        _ => return Err(WhatsAppAdapterRefusal::AmbiguousCarrierRowProof),
    };

    Ok(PlacedWhatsAppCarrier {
        composer_node_id: pair.composer.node_id.clone(),
        composer_runtime_hash: pair.composer.runtime_hash.clone(),
        carrier: carrier.clone(),
        row_proof: WhatsAppCarrierRowProof {
            row_id: proof_candidate.row_id.clone(),
            row_runtime_hash: proof_candidate.row_runtime_hash.clone(),
            carrier_sha256_label: carrier_sha256_label(&carrier),
            bounds: proof_candidate.bounds,
        },
    })
}

pub fn whatsapp_link_preview_capability(
    pair: &DiscoveredWhatsAppPair,
    carrier_payload: &str,
    rows_after_write: &[WhatsAppTranscriptRow],
) -> WhatsAppLinkPreviewCapability {
    if rows_after_write
        .iter()
        .filter(|row| {
            row.transcript_node_id == pair.transcript.node_id
                && !row.offscreen
                && row.bounds.map_or(false, WhatsAppBounds::is_positive)
        })
        .flat_map(|row| &row.fragments)
        .any(|fragment| {
            !canonical_whatsapp_body(&fragment.text).is_empty()
                && matches!(
                    fragment.role,
                    WhatsAppTextFragmentRole::LinkPreviewTitle
                        | WhatsAppTextFragmentRole::LinkPreviewDescription
                )
        })
    {
        return WhatsAppLinkPreviewCapability::Blocked {
            reason: WhatsAppLinkPreviewBlockReason::PreviewFragmentsAreNotExactBody,
        };
    }

    match place_whatsapp_carrier(pair, carrier_payload, rows_after_write) {
        Ok(placed) => WhatsAppLinkPreviewCapability::SupportedWithoutPreview {
            row_id: placed.row_proof.row_id,
            carrier_sha256_label: placed.row_proof.carrier_sha256_label,
        },
        Err(WhatsAppAdapterRefusal::InvalidCarrier) => WhatsAppLinkPreviewCapability::Blocked {
            reason: WhatsAppLinkPreviewBlockReason::InvalidCarrier,
        },
        Err(WhatsAppAdapterRefusal::MissingExactBodyCandidate)
        | Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked) => {
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::ExactBodyUnavailable,
            }
        }
        Err(WhatsAppAdapterRefusal::MissingCarrierRowProof) => {
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::MissingCarrierRowProof,
            }
        }
        Err(WhatsAppAdapterRefusal::AmbiguousCarrierRowProof) => {
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::AmbiguousCarrierRowProof,
            }
        }
        Err(_) => WhatsAppLinkPreviewCapability::Blocked {
            reason: WhatsAppLinkPreviewBlockReason::ExactBodyUnavailable,
        },
    }
}

fn trusted_webview_ancestor_id(
    nodes: &[WhatsAppStructuralNode],
    app_root: &WhatsAppStructuralNode,
    node: &WhatsAppStructuralNode,
) -> Option<String> {
    let mut cursor = Some(node);
    let mut webview_ancestor_id = None;
    let mut depth = 0usize;
    while let Some(current) = cursor {
        if depth > 32 {
            return None;
        }
        if current.structural_id == app_root.structural_id {
            return webview_ancestor_id;
        }
        if is_webview2_lineage_node(current) {
            webview_ancestor_id = Some(current.structural_id.clone());
        }
        cursor = current
            .parent_structural_id
            .as_deref()
            .and_then(|parent_id| {
                nodes
                    .iter()
                    .find(|candidate| candidate.structural_id == parent_id)
            });
        depth += 1;
    }
    None
}

fn descendant_of(
    nodes: &[WhatsAppStructuralNode],
    node: &WhatsAppStructuralNode,
    ancestor_id: &str,
) -> bool {
    let mut cursor = Some(node);
    let mut depth = 0usize;
    while let Some(current) = cursor {
        if depth > 32 {
            return false;
        }
        if current.structural_id == ancestor_id {
            return true;
        }
        cursor = current
            .parent_structural_id
            .as_deref()
            .and_then(|parent_id| {
                nodes
                    .iter()
                    .find(|candidate| candidate.structural_id == parent_id)
            });
        depth += 1;
    }
    false
}

fn composer_binding(node: &WhatsAppStructuralNode) -> Option<WhatsAppComposerBinding> {
    if node.control_type != WhatsAppControlType::Edit || !visible_structural_node(node) {
        return None;
    }
    let has_composer_hint = token_contains(&node.automation_id, &["composer", "input", "message"])
        || token_contains(
            &node.class_name,
            &["composer", "input", "editable", "textbox"],
        );
    if !has_composer_hint {
        return None;
    }
    Some(WhatsAppComposerBinding {
        node_id: node.structural_id.clone(),
        runtime_hash: node.runtime_hash.clone()?,
        bounds: node.bounds?,
    })
}

fn transcript_binding(node: &WhatsAppStructuralNode) -> Option<WhatsAppTranscriptBinding> {
    if !matches!(
        node.control_type,
        WhatsAppControlType::List | WhatsAppControlType::Document
    ) || !visible_structural_node(node)
    {
        return None;
    }
    let has_transcript_hint = token_contains(
        &node.automation_id,
        &["message-list", "messages", "conversation", "transcript"],
    ) || token_contains(
        &node.class_name,
        &["message-list", "conversation", "transcript"],
    );
    if !has_transcript_hint {
        return None;
    }
    Some(WhatsAppTranscriptBinding {
        node_id: node.structural_id.clone(),
        runtime_hash: node.runtime_hash.clone()?,
        bounds: node.bounds?,
    })
}

fn is_content_root_candidate(node: &WhatsAppStructuralNode) -> bool {
    visible_structural_node(node)
        && matches!(
            node.control_type,
            WhatsAppControlType::Document | WhatsAppControlType::List
        )
        && is_webview2_lineage_node(node)
}

fn visible_structural_node(node: &WhatsAppStructuralNode) -> bool {
    node.enabled
        && !node.offscreen
        && node.bounds.map_or(false, WhatsAppBounds::is_positive)
        && node.runtime_hash.is_some()
}

fn is_webview2_lineage_node(node: &WhatsAppStructuralNode) -> bool {
    if node.process_kind == WhatsAppProcessKind::WebView2Child {
        return true;
    }
    node.class_name.as_deref().map_or(false, |class_name| {
        class_name.contains("Chrome_WidgetWin") || class_name.contains("WebView2")
    }) || node.framework_id.as_deref() == Some("Chrome")
}

fn token_contains(token: &Option<String>, needles: &[&str]) -> bool {
    token.as_deref().map_or(false, |value| {
        let value = value.to_ascii_lowercase();
        needles.iter().any(|needle| value.contains(needle))
    })
}

fn canonical_whatsapp_body(text: &str) -> String {
    // CRLF and a lone CR both normalize to LF, exactly as before. Spelled with
    // char literals rather than string ones so the module-wide submit-shaped
    // scan can cover this whole file without an exception: the scan's needle is
    // a line break inside a *string*, which is how a commit would be smuggled
    // into a placed value, and this read-path normalizer must not look like one.
    let mut normalized = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\r' {
            if characters.peek() == Some(&'\n') {
                characters.next();
            }
            normalized.push('\n');
        } else {
            normalized.push(character);
        }
    }
    normalized
        .trim_matches(|value: char| value.is_whitespace() && value != '\n')
        .to_owned()
}

fn prefixed_whatsapp_carrier(payload: &str) -> Result<String, WhatsAppAdapterRefusal> {
    let payload = payload.trim();
    if payload.is_empty()
        || payload.len() > 4096
        || payload.chars().any(|value| value.is_control())
        || payload.contains(char::is_whitespace)
    {
        return Err(WhatsAppAdapterRefusal::InvalidCarrier);
    }
    Ok(format!("{WHATSAPP_CARRIER_PREFIX}{payload}"))
}

fn carrier_sha256_label(carrier: &str) -> String {
    let digest = Sha256::digest(carrier.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    format!("sha256:{encoded}")
}

// ---------------------------------------------------------------------------
// Live placement, driven through the shared UIA2 substrate in `native_a11y`.
// ---------------------------------------------------------------------------

/// WhatsApp Desktop's shell process. The Appx package is
/// `5319275A.WhatsAppDesktop`; the running image is `WhatsApp.exe`.
pub const WHATSAPP_DESKTOP_PROCESS_NAME: &str = "WhatsApp";

/// Chromium builds its accessibility tree lazily and A-00 measured ~90 s to a
/// fully populated Discord tree. WhatsApp's content is a Chromium tree in
/// another process, so it gets the same budget.
pub const WHATSAPP_UIA2_DEFAULT_WAIT_MS: u64 = 90_000;

/// Deliberately larger than Signal's 750 ms.
///
/// A-00b's first residual risk is a whole-subtree scan on a *live* Chromium
/// tree: it is one cross-process call per node, and it has never been timed.
/// WhatsApp's content is a full Chromium document hosted in a second process,
/// so the per-call budget is set above the Electron adapters' and stays a hard
/// bound rather than becoming an excuse to wait forever.
pub const WHATSAPP_UIA2_DEFAULT_CALL_TIMEOUT_MS: u64 = 2_000;

/// WhatsApp's measured window shape.
///
/// Shape 3 of A-00's three: the WinUI 3 shell proves ownership and a **sibling
/// `msedgewebview2` process' window** carries the content. Binding WhatsApp's
/// own `WinUIDesktopWin32WindowClass` root yields 8 elements and never
/// populates, which a single-window probe reads as "WhatsApp cannot be driven".
/// Once bound in the sibling process the class is `Chrome_WidgetWin_1` like any
/// other Electron app, so the renderer child and Chromium's wake handshake
/// apply unchanged.
///
/// # Why `tree_route` is `UiaNative` and not Discord's `MsaaBridge`
///
/// The two axes are independent and Discord's answer is not transferable.
/// Discord's shipping route reads Chromium's custom MSAA client object on the
/// **outer** window because OSL has already borrowed and reparented that exact
/// window, and `wake_electron_accessibility` hands back an `IAccessible` there.
/// WhatsApp is not a borrowed window: nothing in OSL adopts it, the content
/// root is in a process OSL never claimed, and the handle this plan binds is
/// the renderer child -- which is precisely the window A-00 measured with UI
/// Automation directly (11 elements, one writable `Phone number` edit on the
/// login screen). Taking `MsaaBridge` here would claim a bridged read that has
/// never been measured on this provider, on a window that is not the one
/// Chromium hands its client object for.
pub const WHATSAPP_UIA2_WINDOW_PLAN: Uia2WindowPlan = Uia2WindowPlan::sibling_chromium_renderer(
    "WhatsApp",
    WHATSAPP_DESKTOP_PROCESS_NAME,
    WHATSAPP_ROOT_WINDOW_CLASS,
    WEBVIEW2_PROCESS_NAME,
    WHATSAPP_UIA2_DEFAULT_WAIT_MS,
    WHATSAPP_UIA2_DEFAULT_CALL_TIMEOUT_MS,
);

/// How WhatsApp's conversation composer is told apart from every other writable
/// element WhatsApp exposes.
///
/// The refusals are the load-bearing half. A-00 measured WhatsApp signed out,
/// and the *only* writable element on that screen was `Phone number` -- a
/// perfectly good `ValuePattern` target that is not a composer, and placing a
/// carrier there would type OSL's payload into a login form. `non_composer_stems`
/// is checked first, so `Search messages` is refused even though it carries the
/// composer stem.
pub const WHATSAPP_COMPOSER_MATCHER: Uia2ComposerMatcher = Uia2ComposerMatcher {
    composer_stems: &[
        "type a message",
        "write a message",
        "message",
        "nachricht",
        "mensaje",
        "mensagem",
        "messaggio",
    ],
    non_composer_stems: &[
        "search",
        "buscar",
        "suchen",
        "durchsuchen",
        "pesquisar",
        "cerca",
        "filter",
        "filtro",
        "request",
        "phone number",
        "telefonnummer",
        "teléfono",
        "country",
        "país",
        "verification",
        "verificación",
        "code",
        "new chat",
        "caption",
    ],
};

/// Why a live WhatsApp placement did not happen, or that it did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhatsAppPlacementStatus {
    Placed,
    /// The carrier could never be placed: empty, oversized, or carrying a
    /// character a composer treats as a commit.
    InvalidCarrier,
    /// No WhatsApp shell window at all.
    AppNotRunning,
    /// The shell is running but no sibling WebView2 content window was found.
    /// This is the state a single-window probe misreports as "WhatsApp cannot
    /// be driven", so it is a distinct status rather than "no composer".
    WebView2ContentUnavailable,
    /// Chromium was asked for its accessibility object and refused.
    WakeRefused,
    /// The tree never reached the populated threshold. Skipping Chromium's wake
    /// produces exactly this, which is why it carries its counts.
    AccessibilityUnavailable {
        seen: usize,
        needed: usize,
    },
    /// Resolution came back with WhatsApp's own WinUI shell as the bound window.
    /// That window never populates; it is refused rather than driven.
    BoundTheAppShell,
    /// The substrate's syscall seam could not be reached with a deadline the
    /// acquisition derived from the plan.
    DeadlineUnavailable,
    /// No editable element is WhatsApp's composer -- not signed in, no
    /// conversation open, or only a search or login field is exposed.
    ComposerUnavailable,
    ComposerAmbiguous,
    ComposerNotWritable,
    ComposerNotEmpty,
    ReadbackMismatch,
    ProbeClearFailed,
    CallTimedOut,
    /// The backend reported a submit-shaped interaction. Placement is
    /// abandoned: OSL never authorizes a send.
    SubmitShapedCallObserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WhatsAppLivePlacementReceipt {
    pub placed: bool,
    /// Never a literal: the only thing that can set this true is the backend's
    /// own submit-shaped counter, read either side of the write.
    pub enter_sent: bool,
    pub status: WhatsAppPlacementStatus,
    pub bound_process_id: u32,
    /// True would mean the resolver handed back WhatsApp's own WinUI shell.
    pub bound_is_app_shell: bool,
    pub tree_route: Uia2TreeRoute,
    pub woke: bool,
    pub elements: usize,
    pub readback_contains_carrier: bool,
    pub cleared: bool,
}

impl WhatsAppLivePlacementReceipt {
    fn unbound(status: WhatsAppPlacementStatus) -> Self {
        Self {
            placed: false,
            enter_sent: false,
            status,
            bound_process_id: 0,
            bound_is_app_shell: false,
            tree_route: WHATSAPP_UIA2_WINDOW_PLAN.tree_route,
            woke: false,
            elements: 0,
            readback_contains_carrier: false,
            cleared: false,
        }
    }
}

/// A borrowed `Uia2Syscalls` that keeps the deadline the substrate handed it.
///
/// [`Uia2Deadline`]'s only constructor is private to `native_a11y`, on purpose:
/// a syscall cannot be reached except with a deadline the acquisition derived
/// from the plan. The consequence for a *consumer* is that the editable scan --
/// which sits between acquisition and placement and has no wrapper of its own
/// in the substrate -- cannot be called from outside that module at all, because
/// no deadline can be minted here.
///
/// This wrapper resolves that without weakening the token: it never constructs
/// a deadline, it only remembers the one `acquire_uia2_window` already passed
/// through it, and [`whatsapp_editables`] refuses unless that remembered
/// deadline is still the plan's. `submit_shaped_calls` is forwarded, not
/// answered -- a wrapper that answered `0` would blind the placement guard.
pub struct WhatsAppUia2Session<'host> {
    host: &'host dyn Uia2Syscalls,
    deadline: std::cell::Cell<Option<Uia2Deadline>>,
}

impl<'host> WhatsAppUia2Session<'host> {
    pub fn new(host: &'host dyn Uia2Syscalls) -> Self {
        Self {
            host,
            deadline: std::cell::Cell::new(None),
        }
    }

    /// The deadline the substrate derived from the plan, or `None` if the
    /// substrate has not issued a single bounded call through this session yet.
    pub fn recorded_deadline(&self) -> Option<Uia2Deadline> {
        self.deadline.get()
    }

    fn record(&self, deadline: Uia2Deadline) -> Uia2Deadline {
        self.deadline.set(Some(deadline));
        deadline
    }
}

impl Uia2Syscalls for WhatsAppUia2Session<'_> {
    fn enumerate_windows(
        &self,
        deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
        self.host.enumerate_windows(self.record(deadline))
    }

    fn wake_chromium(&self, hwnd: isize, deadline: Uia2Deadline) -> Result<bool, Uia2CallTimeout> {
        self.host.wake_chromium(hwnd, self.record(deadline))
    }

    fn element_count(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        deadline: Uia2Deadline,
    ) -> Result<usize, Uia2CallTimeout> {
        self.host.element_count(hwnd, route, self.record(deadline))
    }

    fn editable_elements(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
        self.host
            .editable_elements(hwnd, route, self.record(deadline))
    }

    fn set_value(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        element: &Uia2Editable,
        value: &str,
        deadline: Uia2Deadline,
    ) -> Result<bool, Uia2CallTimeout> {
        self.host
            .set_value(hwnd, route, element, value, self.record(deadline))
    }

    fn value_of(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        element: &Uia2Editable,
        deadline: Uia2Deadline,
    ) -> Result<Option<String>, Uia2CallTimeout> {
        self.host
            .value_of(hwnd, route, element, self.record(deadline))
    }

    fn submit_shaped_calls(&self) -> usize {
        self.host.submit_shaped_calls()
    }

    fn settle(&self, millis: u64) {
        self.host.settle(millis);
    }
}

/// Scan the bound window's editable elements under the plan's own deadline.
pub fn whatsapp_editables(
    session: &WhatsAppUia2Session<'_>,
    window: Uia2ResolvedWindow,
) -> Result<Vec<Uia2Editable>, WhatsAppPlacementStatus> {
    let deadline = session
        .recorded_deadline()
        .ok_or(WhatsAppPlacementStatus::DeadlineUnavailable)?;
    if deadline.millis() != window.call_timeout_ms {
        return Err(WhatsAppPlacementStatus::DeadlineUnavailable);
    }
    session
        .editable_elements(window.bound_hwnd, window.tree_route, deadline)
        .map_err(|_| WhatsAppPlacementStatus::CallTimedOut)
}

fn whatsapp_acquire_failure(error: Uia2AcquireError) -> WhatsAppPlacementStatus {
    match error {
        Uia2AcquireError::Resolve(Uia2WindowResolveError::MissingAppOuter) => {
            WhatsAppPlacementStatus::AppNotRunning
        }
        Uia2AcquireError::Resolve(Uia2WindowResolveError::MissingSiblingContentOuter)
        | Uia2AcquireError::Resolve(Uia2WindowResolveError::MissingRendererChild) => {
            WhatsAppPlacementStatus::WebView2ContentUnavailable
        }
        Uia2AcquireError::WakeRefused => WhatsAppPlacementStatus::WakeRefused,
        Uia2AcquireError::TreeNeverPopulated { seen, needed } => {
            WhatsAppPlacementStatus::AccessibilityUnavailable { seen, needed }
        }
        Uia2AcquireError::CallTimedOut(_) => WhatsAppPlacementStatus::CallTimedOut,
    }
}

fn whatsapp_composer_failure(error: Uia2ComposerError) -> WhatsAppPlacementStatus {
    match error {
        Uia2ComposerError::NoEditable | Uia2ComposerError::NoComposerName => {
            WhatsAppPlacementStatus::ComposerUnavailable
        }
        Uia2ComposerError::NoWritableEditable => WhatsAppPlacementStatus::ComposerNotWritable,
        Uia2ComposerError::Ambiguous(_) => WhatsAppPlacementStatus::ComposerAmbiguous,
    }
}

/// Resolve WhatsApp's composer through the shared substrate and place a
/// carrier into it. Placement only.
///
/// Nothing here can commit a message. The prohibition rests on four separate
/// mechanisms, because a field that says so proves nothing:
/// 1. the carrier is built by [`prefixed_whatsapp_carrier`], which refuses any
///    whitespace or control character, so a line break cannot ride inside it,
///    and the substrate refuses one again at the seam;
/// 2. [`Uia2Syscalls`] has no verb that could commit -- no key, no posted
///    message, no pattern that activates a control;
/// 3. the backend's own submit-shaped counter is read either side of the write
///    and any delta abandons placement with `enter_sent = true`; and
/// 4. `whatsapp_placement_module_holds_no_submit_shaped_mechanism` scans this
///    module's production source for a mechanism that would bypass the backend.
pub fn drive_whatsapp_composer_placement(
    host: &dyn Uia2Syscalls,
    carrier_payload: &str,
    allow_replace_existing: bool,
) -> WhatsAppLivePlacementReceipt {
    whatsapp_placement(host, carrier_payload, allow_replace_existing, false)
}

/// The same path, then always clear the composer. Never leave a carrier sitting
/// in a real person's chat after a probe.
pub fn probe_whatsapp_composer_write_then_clear(
    host: &dyn Uia2Syscalls,
    carrier_payload: &str,
    allow_replace_existing: bool,
) -> WhatsAppLivePlacementReceipt {
    whatsapp_placement(host, carrier_payload, allow_replace_existing, true)
}

fn whatsapp_placement(
    host: &dyn Uia2Syscalls,
    carrier_payload: &str,
    allow_replace_existing: bool,
    clear_after: bool,
) -> WhatsAppLivePlacementReceipt {
    let carrier = match prefixed_whatsapp_carrier(carrier_payload) {
        Ok(carrier) if !uia2_carrier_carries_submit(&carrier) => carrier,
        _ => return WhatsAppLivePlacementReceipt::unbound(WhatsAppPlacementStatus::InvalidCarrier),
    };

    let session = WhatsAppUia2Session::new(host);
    let acquired = match acquire_uia2_window(WHATSAPP_UIA2_WINDOW_PLAN, &session) {
        Ok(acquired) => acquired,
        Err(error) => {
            return WhatsAppLivePlacementReceipt::unbound(whatsapp_acquire_failure(error))
        }
    };

    let window = acquired.window;
    let bound_is_app_shell = window.bound_hwnd == window.app_outer_hwnd;
    let bound = |status| WhatsAppLivePlacementReceipt {
        placed: false,
        enter_sent: false,
        status,
        bound_process_id: window.bound_process_id,
        bound_is_app_shell,
        tree_route: window.tree_route,
        woke: acquired.woke,
        elements: acquired.elements,
        readback_contains_carrier: false,
        cleared: false,
    };

    if bound_is_app_shell {
        return bound(WhatsAppPlacementStatus::BoundTheAppShell);
    }

    let editables = match whatsapp_editables(&session, window) {
        Ok(editables) => editables,
        Err(status) => return bound(status),
    };
    let composer = match resolve_uia2_composer(WHATSAPP_COMPOSER_MATCHER, &editables) {
        Ok(composer) => composer,
        Err(error) => return bound(whatsapp_composer_failure(error)),
    };

    let receipt = match place_uia2_carrier(
        &session,
        acquired,
        &composer,
        &carrier,
        allow_replace_existing,
    ) {
        Ok(receipt) => receipt,
        Err(Uia2PlacementRefusal::SubmitShaped) => {
            let mut refusal = bound(WhatsAppPlacementStatus::SubmitShapedCallObserved);
            refusal.enter_sent = true;
            return refusal;
        }
        Err(Uia2PlacementRefusal::ExistingDraft) => {
            return bound(WhatsAppPlacementStatus::ComposerNotEmpty)
        }
        Err(Uia2PlacementRefusal::SetValueRefused) => {
            return bound(WhatsAppPlacementStatus::ComposerNotWritable)
        }
        Err(Uia2PlacementRefusal::ReadbackMissingCarrier) => {
            return bound(WhatsAppPlacementStatus::ReadbackMismatch)
        }
        Err(Uia2PlacementRefusal::EmptyCarrier)
        | Err(Uia2PlacementRefusal::CarrierCarriesSubmit) => {
            return bound(WhatsAppPlacementStatus::InvalidCarrier)
        }
        Err(Uia2PlacementRefusal::CallTimedOut(_)) => {
            return bound(WhatsAppPlacementStatus::CallTimedOut)
        }
    };

    let mut placed = bound(WhatsAppPlacementStatus::Placed);
    placed.placed = receipt.placed;
    placed.readback_contains_carrier = receipt.readback_holds_carrier;
    placed.enter_sent = receipt.submit_shaped_observed;

    if clear_after {
        match clear_uia2_composer(&session, acquired, &composer) {
            Ok(()) => placed.cleared = true,
            Err(_) => placed.status = WhatsAppPlacementStatus::ProbeClearFailed,
        }
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(
        id: &str,
        parent: Option<&str>,
        control_type: WhatsAppControlType,
    ) -> WhatsAppStructuralNode {
        WhatsAppStructuralNode {
            structural_id: id.to_owned(),
            parent_structural_id: parent.map(str::to_owned),
            process_kind: WhatsAppProcessKind::Other,
            store_package_family_name: None,
            window_class: None,
            control_type,
            automation_id: None,
            class_name: None,
            framework_id: None,
            runtime_hash: Some(format!("runtime-{id}")),
            bounds: Some(WhatsAppBounds {
                x: 1,
                y: 1,
                width: 10,
                height: 10,
            }),
            enabled: true,
            offscreen: false,
        }
    }

    fn app_root() -> WhatsAppStructuralNode {
        WhatsAppStructuralNode {
            structural_id: "app-root".to_owned(),
            parent_structural_id: None,
            process_kind: WhatsAppProcessKind::StoreAppRoot,
            store_package_family_name: Some(
                crate::native_apps::whatsapp_store_package_family_name().to_owned(),
            ),
            window_class: Some(WHATSAPP_ROOT_WINDOW_CLASS.to_owned()),
            control_type: WhatsAppControlType::Window,
            automation_id: None,
            class_name: None,
            framework_id: None,
            runtime_hash: Some("runtime-app-root".to_owned()),
            bounds: Some(WhatsAppBounds {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            }),
            enabled: true,
            offscreen: false,
        }
    }

    fn webview(id: &str, parent: &str) -> WhatsAppStructuralNode {
        let mut node = node(id, Some(parent), WhatsAppControlType::Pane);
        node.process_kind = WhatsAppProcessKind::WebView2Child;
        node.class_name = Some("Chrome_WidgetWin_0".to_owned());
        node.framework_id = Some("Chrome".to_owned());
        node
    }

    fn trusted_nodes_with_pair() -> Vec<WhatsAppStructuralNode> {
        let root = app_root();
        let bridge = webview("webview", "app-root");
        let mut content = webview("content", "webview");
        content.control_type = WhatsAppControlType::Document;
        content.runtime_hash = Some("runtime-content".to_owned());
        let mut transcript = node("transcript", Some("content"), WhatsAppControlType::List);
        transcript.automation_id = Some("message-list".to_owned());
        transcript.runtime_hash = Some("runtime-transcript".to_owned());
        let mut composer = node("composer", Some("content"), WhatsAppControlType::Edit);
        composer.automation_id = Some("main-composer-input".to_owned());
        composer.runtime_hash = Some("runtime-composer".to_owned());
        vec![root, bridge, content, transcript, composer]
    }

    fn discovered_pair() -> DiscoveredWhatsAppPair {
        discover_whatsapp_pair(&trusted_nodes_with_pair()).expect("pair fixture should be exact")
    }

    fn row(row_id: &str, role: WhatsAppTextFragmentRole, text: &str) -> WhatsAppTranscriptRow {
        WhatsAppTranscriptRow {
            row_id: row_id.to_owned(),
            transcript_node_id: "transcript".to_owned(),
            runtime_hash: format!("runtime-{row_id}"),
            bounds: Some(WhatsAppBounds {
                x: 5,
                y: 5,
                width: 50,
                height: 20,
            }),
            offscreen: false,
            fragments: vec![WhatsAppTextFragment {
                role,
                text: text.to_owned(),
            }],
        }
    }

    fn exact_body_row(row_id: &str, text: &str) -> WhatsAppTranscriptRow {
        row(row_id, WhatsAppTextFragmentRole::ExactBody, text)
    }

    fn exact_body_row_with_preview(
        row_id: &str,
        body: &str,
        preview: &str,
    ) -> WhatsAppTranscriptRow {
        let mut row = exact_body_row(row_id, body);
        row.fragments.push(WhatsAppTextFragment {
            role: WhatsAppTextFragmentRole::LinkPreviewTitle,
            text: preview.to_owned(),
        });
        row
    }

    #[test]
    fn whatsapp_content_root() {
        let root = app_root();
        let bridge = webview("webview", "app-root");
        let mut content = webview("content", "webview");
        content.control_type = WhatsAppControlType::Document;
        content.runtime_hash = Some("runtime-content".to_owned());

        let trusted =
            trusted_whatsapp_content_root(&[root.clone(), bridge.clone(), content.clone()])
                .expect("one trusted WhatsApp WebView2 content root should be discovered");
        assert_eq!(trusted.app_root_id, "app-root");
        assert_eq!(trusted.webview_ancestor_id, "webview");
        assert_eq!(trusted.content_root_id, "content");
        assert_eq!(trusted.content_runtime_hash, "runtime-content");

        let mut plain_document = node(
            "plain-document",
            Some("app-root"),
            WhatsAppControlType::Document,
        );
        plain_document.runtime_hash = Some("runtime-plain-document".to_owned());
        assert_eq!(
            trusted_whatsapp_content_root(&[root.clone(), plain_document]),
            Err(WhatsAppAdapterRefusal::MissingWebView2ContentRoot),
            "a document under the WhatsApp root is not trusted unless WebView2 is in its lineage"
        );

        let mut spoofed_root = root;
        spoofed_root.store_package_family_name = Some("not.whatsapp".to_owned());
        assert_eq!(
            trusted_whatsapp_content_root(&[spoofed_root, bridge, content]),
            Err(WhatsAppAdapterRefusal::MissingExactAppRoot),
            "a WebView2 child under a non-WhatsApp root must not be trusted"
        );
    }

    #[test]
    fn whatsapp_pair() {
        let nodes = trusted_nodes_with_pair();
        let pair = discover_whatsapp_pair(&nodes).expect("one composer/transcript pair exists");
        assert_eq!(pair.content_root.content_root_id, "content");
        assert_eq!(pair.composer.node_id, "composer");
        assert_eq!(pair.composer.runtime_hash, "runtime-composer");
        assert_eq!(pair.transcript.node_id, "transcript");
        assert_eq!(pair.transcript.runtime_hash, "runtime-transcript");

        let mut missing_composer = nodes.clone();
        missing_composer.retain(|node| node.structural_id != "composer");
        assert_eq!(
            discover_whatsapp_pair(&missing_composer),
            Err(WhatsAppAdapterRefusal::MissingComposer),
            "transcript-only discovery must refuse instead of returning a partial pair"
        );

        let mut ambiguous_transcript = nodes;
        let mut second = node(
            "second-transcript",
            Some("content"),
            WhatsAppControlType::Document,
        );
        second.automation_id = Some("conversation-messages".to_owned());
        ambiguous_transcript.push(second);
        assert_eq!(
            discover_whatsapp_pair(&ambiguous_transcript),
            Err(WhatsAppAdapterRefusal::AmbiguousTranscript),
            "more than one matching transcript must refuse the pair"
        );
    }

    #[test]
    fn whatsapp_body_candidates() {
        let pair = discovered_pair();
        let rows = vec![
            row("timestamp", WhatsAppTextFragmentRole::Timestamp, "10:41"),
            exact_body_row("body", "  exact\r\nbody  "),
        ];

        let candidates = extract_whatsapp_body_candidates(&pair, &rows)
            .expect("one exact body row should be extracted");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].row_id, "body");
        assert_eq!(candidates[0].row_runtime_hash, "runtime-body");
        assert_eq!(candidates[0].body, "exact\nbody");

        assert!(
            matches!(
                extract_whatsapp_body_candidates(
                    &pair,
                    &[row(
                        "preview",
                        WhatsAppTextFragmentRole::LinkPreviewDescription,
                        "preview text"
                    )],
                ),
                Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked)
            ),
            "preview text cannot be promoted into an exact body candidate"
        );

        assert!(
            matches!(
                extract_whatsapp_body_candidates(
                    &pair,
                    &[row(
                        "metadata",
                        WhatsAppTextFragmentRole::Reaction,
                        "thumbs up"
                    )],
                ),
                Err(WhatsAppAdapterRefusal::MissingExactBodyCandidate)
            ),
            "metadata-only rows are not body candidates"
        );
    }

    #[test]
    fn whatsapp_carrier() {
        let pair = discovered_pair();
        let placed = place_whatsapp_carrier(
            &pair,
            "publiccover",
            &[exact_body_row("carrier-row", "OSL1.WA.publiccover")],
        )
        .expect("prefixed exact body row should prove carrier placement");
        assert_eq!(placed.composer_node_id, "composer");
        assert_eq!(placed.composer_runtime_hash, "runtime-composer");
        assert_eq!(placed.carrier, "OSL1.WA.publiccover");
        assert_eq!(placed.row_proof.row_id, "carrier-row");
        assert_eq!(placed.row_proof.row_runtime_hash, "runtime-carrier-row");
        assert!(placed.row_proof.carrier_sha256_label.starts_with("sha256:"));

        assert!(
            matches!(
                place_whatsapp_carrier(
                    &pair,
                    "publiccover",
                    &[exact_body_row("unprefixed", "publiccover")]
                ),
                Err(WhatsAppAdapterRefusal::MissingCarrierRowProof)
            ),
            "the carrier proof must include the OSL WhatsApp prefix"
        );

        assert!(
            matches!(
                place_whatsapp_carrier(
                    &pair,
                    "publiccover",
                    &[
                        exact_body_row("carrier-a", "OSL1.WA.publiccover"),
                        exact_body_row("carrier-b", "OSL1.WA.publiccover"),
                    ],
                ),
                Err(WhatsAppAdapterRefusal::AmbiguousCarrierRowProof)
            ),
            "duplicate matching rows cannot prove one write"
        );
        assert!(matches!(
            place_whatsapp_carrier(&pair, "has spaces", &[]),
            Err(WhatsAppAdapterRefusal::InvalidCarrier)
        ));
    }

    #[test]
    fn whatsapp_content_root_trusts_only_webview2_descendant_of_exact_whatsapp_root() {
        let root = app_root();
        let bridge = webview("webview", "app-root");
        let mut content = webview("content", "webview");
        content.control_type = WhatsAppControlType::Document;
        content.runtime_hash = Some("runtime-content".to_owned());
        let trusted =
            trusted_whatsapp_content_root(&[root.clone(), bridge.clone(), content.clone()])
                .expect("one WebView2 descendant content root should be trusted");
        assert_eq!(trusted.app_root_id, "app-root");
        assert_eq!(trusted.webview_ancestor_id, "webview");
        assert_eq!(trusted.content_root_id, "content");
        assert_eq!(trusted.content_runtime_hash, "runtime-content");

        let mut unbound_root = root.clone();
        unbound_root.store_package_family_name = None;
        assert_eq!(
            trusted_whatsapp_content_root(&[unbound_root, bridge.clone(), content.clone()]),
            Err(WhatsAppAdapterRefusal::MissingExactAppRoot)
        );

        let mut spoofed_root = root.clone();
        spoofed_root.store_package_family_name =
            Some("5319275A.WhatsAppDesktop_attacker".to_owned());
        assert_eq!(
            trusted_whatsapp_content_root(&[spoofed_root, bridge.clone(), content.clone()]),
            Err(WhatsAppAdapterRefusal::MissingExactAppRoot)
        );

        let mut not_webview = node(
            "plain-document",
            Some("app-root"),
            WhatsAppControlType::Document,
        );
        not_webview.runtime_hash = Some("runtime-plain".to_owned());
        assert_eq!(
            trusted_whatsapp_content_root(&[root.clone(), not_webview]),
            Err(WhatsAppAdapterRefusal::MissingWebView2ContentRoot)
        );

        let mut sibling = webview("sibling-content", "foreign-root");
        sibling.control_type = WhatsAppControlType::Document;
        assert_eq!(
            trusted_whatsapp_content_root(&[
                root.clone(),
                bridge.clone(),
                content.clone(),
                sibling
            ]),
            Ok(trusted)
        );

        let mut second_content = webview("second-content", "webview");
        second_content.control_type = WhatsAppControlType::Document;
        assert_eq!(
            trusted_whatsapp_content_root(&[root, bridge, content, second_content]),
            Err(WhatsAppAdapterRefusal::AmbiguousWebView2ContentRoot)
        );
    }

    #[test]
    fn whatsapp_pair_discovers_composer_and_transcript_only_as_one_pair() {
        let nodes = trusted_nodes_with_pair();
        let pair = discover_whatsapp_pair(&nodes).expect("exact pair should be discovered");
        assert_eq!(pair.content_root.content_root_id, "content");
        assert_eq!(pair.composer.node_id, "composer");
        assert_eq!(pair.composer.runtime_hash, "runtime-composer");
        assert_eq!(pair.transcript.node_id, "transcript");
        assert_eq!(pair.transcript.runtime_hash, "runtime-transcript");

        let mut missing_transcript = nodes.clone();
        missing_transcript.retain(|node| node.structural_id != "transcript");
        assert_eq!(
            discover_whatsapp_pair(&missing_transcript),
            Err(WhatsAppAdapterRefusal::MissingTranscript)
        );

        let mut missing_composer = nodes.clone();
        missing_composer.retain(|node| node.structural_id != "composer");
        assert_eq!(
            discover_whatsapp_pair(&missing_composer),
            Err(WhatsAppAdapterRefusal::MissingComposer)
        );

        let mut ambiguous_composer = nodes.clone();
        let mut second = node(
            "second-composer",
            Some("content"),
            WhatsAppControlType::Edit,
        );
        second.automation_id = Some("composer-input".to_owned());
        ambiguous_composer.push(second);
        assert_eq!(
            discover_whatsapp_pair(&ambiguous_composer),
            Err(WhatsAppAdapterRefusal::AmbiguousComposer)
        );
    }

    #[test]
    fn whatsapp_body_candidates_extract_only_exact_row_bodies_or_block() {
        let pair = discovered_pair();
        let rows = vec![
            row("timestamp", WhatsAppTextFragmentRole::Timestamp, "10:41"),
            row(
                "body",
                WhatsAppTextFragmentRole::ExactBody,
                "  hello\r\nworld  ",
            ),
        ];
        let candidates = extract_whatsapp_body_candidates(&pair, &rows)
            .expect("one exact body row should produce a body candidate");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].row_id, "body");
        assert_eq!(candidates[0].row_runtime_hash, "runtime-body");
        assert_eq!(candidates[0].body, "hello\nworld");

        let preview = vec![row(
            "preview",
            WhatsAppTextFragmentRole::LinkPreviewTitle,
            "Example Domain",
        )];
        assert!(matches!(
            extract_whatsapp_body_candidates(&pair, &preview),
            Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked)
        ));

        let unknown = vec![row(
            "unknown",
            WhatsAppTextFragmentRole::Unknown,
            "maybe body",
        )];
        assert!(matches!(
            extract_whatsapp_body_candidates(&pair, &unknown),
            Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked)
        ));

        let metadata_only = vec![row(
            "timestamp",
            WhatsAppTextFragmentRole::Timestamp,
            "10:42",
        )];
        assert!(matches!(
            extract_whatsapp_body_candidates(&pair, &metadata_only),
            Err(WhatsAppAdapterRefusal::MissingExactBodyCandidate)
        ));
    }

    #[test]
    fn whatsapp_carrier_writes_prefixed_carrier_only_with_row_proof() {
        let pair = discovered_pair();
        let proof_rows = vec![exact_body_row("carrier-row", "OSL1.WA.publiccover")];
        let placed = place_whatsapp_carrier(&pair, "publiccover", &proof_rows)
            .expect("matching exact body row should prove placement");
        assert_eq!(placed.composer_node_id, "composer");
        assert_eq!(placed.composer_runtime_hash, "runtime-composer");
        assert_eq!(placed.carrier, "OSL1.WA.publiccover");
        assert_eq!(placed.row_proof.row_id, "carrier-row");
        assert_eq!(placed.row_proof.row_runtime_hash, "runtime-carrier-row");
        assert!(placed.row_proof.carrier_sha256_label.starts_with("sha256:"));

        let unprefixed_row = vec![exact_body_row("unprefixed", "publiccover")];
        assert!(matches!(
            place_whatsapp_carrier(&pair, "publiccover", &unprefixed_row),
            Err(WhatsAppAdapterRefusal::MissingCarrierRowProof)
        ));

        let preview_row = vec![row(
            "preview",
            WhatsAppTextFragmentRole::LinkPreviewTitle,
            "OSL1.WA.publiccover",
        )];
        assert!(matches!(
            place_whatsapp_carrier(&pair, "publiccover", &preview_row),
            Err(WhatsAppAdapterRefusal::BodyCandidateSupportBlocked)
        ));

        let duplicate_rows = vec![
            exact_body_row("carrier-row-a", "OSL1.WA.publiccover"),
            exact_body_row("carrier-row-b", "OSL1.WA.publiccover"),
        ];
        assert!(matches!(
            place_whatsapp_carrier(&pair, "publiccover", &duplicate_rows),
            Err(WhatsAppAdapterRefusal::AmbiguousCarrierRowProof)
        ));

        assert!(matches!(
            place_whatsapp_carrier(&pair, "has spaces", &[]),
            Err(WhatsAppAdapterRefusal::InvalidCarrier)
        ));
    }

    #[test]
    fn whatsapp_link_preview_reports_blocked_for_preview_fragments_without_guessing() {
        let pair = discovered_pair();
        let plain_rows = vec![exact_body_row("carrier-row", "OSL1.WA.publiccover")];
        match whatsapp_link_preview_capability(&pair, "publiccover", &plain_rows) {
            WhatsAppLinkPreviewCapability::SupportedWithoutPreview {
                row_id,
                carrier_sha256_label,
            } => {
                assert_eq!(row_id, "carrier-row");
                assert!(carrier_sha256_label.starts_with("sha256:"));
            }
            WhatsAppLinkPreviewCapability::Blocked { .. } => {
                panic!("plain exact carrier row should remain supported")
            }
        }

        let preview_rows = vec![exact_body_row_with_preview(
            "carrier-row",
            "OSL1.WA.publiccover",
            "Example Domain",
        )];
        assert!(matches!(
            whatsapp_link_preview_capability(&pair, "publiccover", &preview_rows),
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::PreviewFragmentsAreNotExactBody
            }
        ));

        let missing_proof = vec![exact_body_row("other-row", "OSL1.WA.different")];
        assert!(matches!(
            whatsapp_link_preview_capability(&pair, "publiccover", &missing_proof),
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::MissingCarrierRowProof
            }
        ));

        assert!(matches!(
            whatsapp_link_preview_capability(&pair, "has spaces", &plain_rows),
            WhatsAppLinkPreviewCapability::Blocked {
                reason: WhatsAppLinkPreviewBlockReason::InvalidCarrier
            }
        ));
    }

    // -----------------------------------------------------------------
    // Live placement through the shared substrate, driven off Windows
    // against A-00's recorded WhatsApp window graph.
    //
    // The host is `native_a11y::tests::RecordedHost` on purpose: A-00b made it
    // `pub(crate)` so every adapter drives the same fake through the same seam.
    // A second fake here would be the fork the substrate exists to prevent.
    // -----------------------------------------------------------------

    use crate::native_a11y::tests::{composer, whatsapp_graph, RecordedHost};

    const WHATSAPP_MEASURED_ELEMENTS: usize = 11;
    const WEBVIEW2_PROCESS_ID: u32 = 7500;
    const WHATSAPP_SHELL_PROCESS_ID: u32 = 7400;
    const PAYLOAD: &str = "alpha7731osl";
    const CARRIER: &str = "OSL1.WA.alpha7731osl";

    /// WhatsApp as A-00 measured it: the WinUI shell, and the content in a
    /// sibling WebView2 process, woken and polled before it populates.
    fn whatsapp_host(editables: Vec<Uia2Editable>) -> RecordedHost {
        RecordedHost::new(whatsapp_graph(), WHATSAPP_MEASURED_ELEMENTS)
            .chromium(2)
            .with_editables(editables)
    }

    /// The shell with no sibling WebView2 window: what a single-window probe
    /// sees, and the state that reads as "WhatsApp cannot be driven".
    fn whatsapp_shell_only_host() -> RecordedHost {
        let shell_only = whatsapp_graph()
            .into_iter()
            .filter(|window| window.process_name == "WhatsApp.exe")
            .collect::<Vec<_>>();
        assert_eq!(
            shell_only.len(),
            2,
            "the shell half of the graph is two windows"
        );
        RecordedHost::new(shell_only, 8).chromium(2)
    }

    #[test]
    fn whatsapp_plan_is_the_sibling_webview2_shape_on_the_uia_native_route() {
        let plan = WHATSAPP_UIA2_WINDOW_PLAN;

        assert_eq!(plan.shape, Uia2WindowShape::SiblingChromiumRenderer);
        assert_eq!(plan.sibling_process_name, Some(WEBVIEW2_PROCESS_NAME));
        assert_eq!(plan.app_outer_class, WHATSAPP_ROOT_WINDOW_CLASS);
        assert_eq!(
            WHATSAPP_ROOT_WINDOW_CLASS,
            crate::native_a11y::WHATSAPP_OUTER_WINDOW_CLASS,
            "this module and the substrate must name the same shell class"
        );
        assert_eq!(
            plan.wake_policy,
            Uia2WakePolicy::WmGetObjectChromium,
            "the content is Chromium, so it needs Chromium's handshake"
        );
        assert!(plan.poll_until_populated);

        // The axis A-00b added. Discord's shipping route is Chromium's MSAA
        // client object on an OUTER window OSL has already borrowed. WhatsApp
        // binds a renderer child in a process OSL never claimed, which is the
        // window A-00 measured with UI Automation directly, so the route is
        // UiaNative and copying Discord's answer would claim an unmeasured read.
        assert_eq!(plan.tree_route, Uia2TreeRoute::UiaNative);
        assert_ne!(
            plan.tree_route,
            Uia2WindowPlan::chromium_outer_msaa_root(
                "WhatsApp",
                WHATSAPP_DESKTOP_PROCESS_NAME,
                10,
                WHATSAPP_UIA2_DEFAULT_WAIT_MS,
                WHATSAPP_UIA2_DEFAULT_CALL_TIMEOUT_MS,
            )
            .tree_route,
            "WhatsApp must not inherit Discord's bridged route by assumption"
        );
    }

    #[test]
    fn whatsapp_places_a_carrier_in_the_webview2_sibling_and_never_in_the_shell() {
        let host = whatsapp_host(vec![composer("Type a message")]);

        let receipt = drive_whatsapp_composer_placement(&host, PAYLOAD, false);

        assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);
        assert!(receipt.placed);
        assert!(receipt.readback_contains_carrier);
        assert!(!receipt.enter_sent);
        assert!(
            !receipt.bound_is_app_shell,
            "binding WhatsApp's WinUI root is the dead end that reads as undrivable"
        );
        assert_eq!(
            receipt.bound_process_id, WEBVIEW2_PROCESS_ID,
            "the content lives in the sibling msedgewebview2 process, not in {WHATSAPP_SHELL_PROCESS_ID}"
        );
        assert!(
            receipt.woke,
            "Chromium's tree does not exist until it is woken"
        );
        assert_eq!(receipt.elements, WHATSAPP_MEASURED_ELEMENTS);
        assert_eq!(host.set_values.borrow().as_slice(), [CARRIER]);
    }

    #[test]
    fn whatsapp_reads_back_with_contains_because_a_live_ui_augments_its_own_fields() {
        let mut host = whatsapp_host(vec![composer("Type a message")]);
        host.readback_suffix = " and a suggestion WhatsApp appended";

        let receipt = drive_whatsapp_composer_placement(&host, PAYLOAD, false);

        assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);
        assert!(receipt.readback_contains_carrier);
    }

    #[test]
    fn whatsapp_probe_always_clears_after_it_writes() {
        let host = whatsapp_host(vec![composer("Type a message")]);

        let receipt = probe_whatsapp_composer_write_then_clear(&host, PAYLOAD, false);

        assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);
        assert!(receipt.cleared);
        assert!(!receipt.enter_sent);
        assert_eq!(
            host.set_values.borrow().as_slice(),
            [CARRIER, ""],
            "a probe must never leave a carrier in a real person's chat"
        );
    }

    #[test]
    fn whatsapp_signed_out_refuses_instead_of_typing_into_the_phone_number_box() {
        // A-00 measured WhatsApp signed out: eleven elements, and the ONE
        // writable one was `Phone number`. It is a perfectly good ValuePattern
        // target, which is exactly why it has to be refused by name.
        for name in [
            "Phone number",
            "Search or start new chat",
            "Search messages",
            "Message requests",
            "Country code",
            "Verification code",
        ] {
            let host = whatsapp_host(vec![composer(name)]);
            let receipt = drive_whatsapp_composer_placement(&host, PAYLOAD, false);
            assert_eq!(
                receipt.status,
                WhatsAppPlacementStatus::ComposerUnavailable,
                "{name:?} is not a composer and must not be written into"
            );
            assert!(!receipt.placed);
            assert!(
                host.set_values.borrow().is_empty(),
                "{name:?}: nothing may be written when no composer exists"
            );
        }

        // The refusals are the name, not an inert path: put a real composer
        // beside the login field and the placement goes through.
        let host = whatsapp_host(vec![composer("Phone number"), composer("Type a message")]);
        let receipt = drive_whatsapp_composer_placement(&host, PAYLOAD, false);
        assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);
        assert_eq!(host.set_values.borrow().as_slice(), [CARRIER]);
    }

    #[test]
    fn whatsapp_refuses_ambiguity_and_unwritable_elements_rather_than_guessing() {
        let two = whatsapp_host(vec![composer("Message Liam"), composer("Message Ana")]);
        assert_eq!(
            drive_whatsapp_composer_placement(&two, PAYLOAD, false).status,
            WhatsAppPlacementStatus::ComposerAmbiguous
        );
        assert!(two.set_values.borrow().is_empty());

        let mut read_only = composer("Type a message");
        read_only.read_only = true;
        let locked = whatsapp_host(vec![read_only]);
        assert_eq!(
            drive_whatsapp_composer_placement(&locked, PAYLOAD, false).status,
            WhatsAppPlacementStatus::ComposerNotWritable
        );
        assert!(locked.set_values.borrow().is_empty());

        let none = whatsapp_host(Vec::new());
        assert_eq!(
            drive_whatsapp_composer_placement(&none, PAYLOAD, false).status,
            WhatsAppPlacementStatus::ComposerUnavailable
        );
    }

    #[test]
    fn whatsapp_shell_without_its_webview2_sibling_is_a_distinct_refusal() {
        // The trap, stated as a status: the app IS running and IS drivable in
        // principle; what is missing is the sibling content window. Reporting
        // this as "no composer" is how WhatsApp gets written off.
        let shell_only = whatsapp_shell_only_host();
        let receipt = drive_whatsapp_composer_placement(&shell_only, PAYLOAD, false);
        assert_eq!(
            receipt.status,
            WhatsAppPlacementStatus::WebView2ContentUnavailable
        );
        assert!(!receipt.placed);
        assert!(shell_only.set_values.borrow().is_empty());

        let absent = RecordedHost::new(Vec::new(), 0).chromium(2);
        assert_eq!(
            drive_whatsapp_composer_placement(&absent, PAYLOAD, false).status,
            WhatsAppPlacementStatus::AppNotRunning
        );
    }

    #[test]
    fn whatsapp_without_the_chromium_wake_never_populates() {
        // The RecordedHost answers one element until it is woken, which is what
        // A-00 measured: skipping Chromium's handshake is not slower, it is
        // blind. A wake that is refused is a different failure from a tree that
        // never fills, and both are distinct from "no composer".
        let mut refused = whatsapp_host(vec![composer("Type a message")]);
        refused.wake_answers = false;
        assert_eq!(
            drive_whatsapp_composer_placement(&refused, PAYLOAD, false).status,
            WhatsAppPlacementStatus::WakeRefused
        );

        let mut never = whatsapp_host(vec![composer("Type a message")]);
        never.settles_before_populated = usize::MAX;
        assert_eq!(
            drive_whatsapp_composer_placement(&never, PAYLOAD, false).status,
            WhatsAppPlacementStatus::AccessibilityUnavailable {
                seen: 1,
                needed: crate::native_a11y::ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            }
        );
    }

    #[test]
    fn whatsapp_placement_refuses_a_backend_that_reports_a_submit_shaped_call() {
        // The session wrapper forwards `submit_shaped_calls` instead of
        // answering it. A wrapper that answered zero would blind the guard, and
        // `enter_sent` would become a field that cannot be true -- D-139's
        // finding 2, one layer further out.
        let mut host = whatsapp_host(vec![composer("Type a message")]);
        host.submit_shaped_on_set = true;

        let receipt = drive_whatsapp_composer_placement(&host, PAYLOAD, false);

        assert_eq!(
            receipt.status,
            WhatsAppPlacementStatus::SubmitShapedCallObserved
        );
        assert!(
            receipt.enter_sent,
            "the verdict comes from the backend's counter"
        );
        assert!(!receipt.placed);
        assert_eq!(host.submit_shaped_calls(), 1);
    }

    #[test]
    fn whatsapp_refuses_to_replace_an_operator_draft_by_default() {
        let host = whatsapp_host(vec![composer("Type a message")]);
        *host.value.borrow_mut() = Some("half a sentence the owner typed".to_owned());

        assert_eq!(
            drive_whatsapp_composer_placement(&host, PAYLOAD, false).status,
            WhatsAppPlacementStatus::ComposerNotEmpty
        );
        assert!(host.set_values.borrow().is_empty());

        assert_eq!(
            drive_whatsapp_composer_placement(&host, PAYLOAD, true).status,
            WhatsAppPlacementStatus::Placed
        );
    }

    #[test]
    fn whatsapp_refuses_a_carrier_that_could_commit_before_anything_is_written() {
        for payload in ["", "   ", "alpha\nsend", "alpha\r\nsend", "has spaces"] {
            let host = whatsapp_host(vec![composer("Type a message")]);
            let receipt = drive_whatsapp_composer_placement(&host, payload, false);
            assert_eq!(
                receipt.status,
                WhatsAppPlacementStatus::InvalidCarrier,
                "{payload:?} must never reach a live composer"
            );
            assert!(host.set_values.borrow().is_empty());
        }

        // And the substrate's own seam agrees, so the refusal does not depend
        // on this module's prefix rule alone.
        assert!(uia2_carrier_carries_submit(&format!(
            "{WHATSAPP_CARRIER_PREFIX}alpha\nsend"
        )));
    }

    #[test]
    fn whatsapp_every_cross_process_call_carries_the_plans_deadline() {
        let host = whatsapp_host(vec![composer("Type a message")]);
        let receipt = probe_whatsapp_composer_write_then_clear(&host, PAYLOAD, false);
        assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);

        let deadlines = host.deadlines.borrow();
        assert!(!deadlines.is_empty());
        assert!(
            deadlines
                .iter()
                .all(|deadline| *deadline == WHATSAPP_UIA2_DEFAULT_CALL_TIMEOUT_MS),
            "every call must carry the plan's deadline, saw {deadlines:?}"
        );
    }

    #[test]
    fn whatsapp_editable_scan_refuses_without_a_deadline_the_acquisition_derived() {
        // The session never mints a deadline; it only keeps the one the
        // substrate handed it. Before the substrate has made any call there is
        // nothing to keep, and the scan refuses rather than inventing a bound.
        let host = whatsapp_host(vec![composer("Type a message")]);
        let session = WhatsAppUia2Session::new(&host);
        assert!(session.recorded_deadline().is_none());

        let window = Uia2ResolvedWindow {
            app_outer_hwnd: 0x4001,
            bound_hwnd: 0x5002,
            bound_process_id: WEBVIEW2_PROCESS_ID,
            wake_policy: Uia2WakePolicy::WmGetObjectChromium,
            tree_route: Uia2TreeRoute::UiaNative,
            poll_until_populated: true,
            populated_min_elements: crate::native_a11y::ELECTRON_UIA2_POPULATED_MIN_ELEMENTS,
            call_timeout_ms: WHATSAPP_UIA2_DEFAULT_CALL_TIMEOUT_MS,
        };
        assert_eq!(
            whatsapp_editables(&session, window),
            Err(WhatsAppPlacementStatus::DeadlineUnavailable)
        );

        let acquired = acquire_uia2_window(WHATSAPP_UIA2_WINDOW_PLAN, &session)
            .expect("WhatsApp acquires through the substrate");
        assert_eq!(
            session.recorded_deadline().map(Uia2Deadline::millis),
            Some(WHATSAPP_UIA2_DEFAULT_CALL_TIMEOUT_MS)
        );
        assert_eq!(
            whatsapp_editables(&session, acquired.window).map(|editables| editables.len()),
            Ok(1)
        );
    }

    #[test]
    fn whatsapp_reports_a_call_that_overran_its_deadline_instead_of_driving_on() {
        let mut host = whatsapp_host(vec![composer("Type a message")]);
        host.never_answers = true;
        assert_eq!(
            drive_whatsapp_composer_placement(&host, PAYLOAD, false).status,
            WhatsAppPlacementStatus::CallTimedOut
        );
    }

    /// Every mechanism that could commit a WhatsApp message without going
    /// through the substrate's syscall seam, spelled as it would appear in Rust
    /// source. Case-insensitive, over code with comments removed.
    const SUBMIT_SHAPED_MECHANISMS: &[&str] = &[
        "sendinput",
        "keybd_event",
        "keyeventf",
        "input_keyboard",
        "vk_return",
        "vk_enter",
        "postmessage",
        "sendmessage",
        "sendnotifymessage",
        "wm_keydown",
        "wm_keyup",
        "wm_char",
        "wm_ime_char",
        "invoke",
        "\\n\"",
        "\\r\"",
        "\\u{000a}",
        "\\u{000d}",
    ];

    /// Built rather than written, so this file does not contain the marker it
    /// splits on -- a literal here would create phantom test-module boundaries
    /// and the region count below would be scanning the wrong thing.
    fn test_module_marker() -> String {
        format!("#[cfg({})]", "test")
    }

    fn production_code(source: &str) -> String {
        source
            .lines()
            .map(|line| line.split("//").next().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n")
            .to_ascii_lowercase()
    }

    /// This file's production halves. It has TWO test modules, not one, and the
    /// second sits after `pub mod scan_selectors`, so splitting once and taking
    /// the head -- which is what the Signal guard does -- would leave the whole
    /// selector module unscanned. The count is asserted so a third test module
    /// fails this guard loudly instead of quietly escaping it.
    fn whatsapp_production_regions() -> Vec<(&'static str, String)> {
        let source = include_str!("native_whatsapp_adapter.rs");
        let segments = source
            .split(test_module_marker().as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            segments.len(),
            3,
            "native_whatsapp_adapter.rs has exactly two test modules; a new one \
             must be accounted for here rather than silently escaping the scan"
        );
        let selectors = segments[1]
            .split_once(format!("pub mod {} {{", "scan_selectors").as_str())
            .expect("the scan-only selector module must stay inside the scanned region")
            .1;
        vec![
            (
                "native_whatsapp_adapter.rs (live placement)",
                production_code(segments[0]),
            ),
            (
                "native_whatsapp_adapter.rs (scan_selectors)",
                production_code(selectors),
            ),
        ]
    }

    #[test]
    fn whatsapp_placement_module_holds_no_submit_shaped_mechanism() {
        // The scanner must be able to fire, or this guard is decoration.
        assert!(production_code("let _ = element.Invoke(0);").contains("invoke"));
        assert!(production_code("set_value(&format!(\"{carrier}\\n\"))").contains("\\n\""));
        assert!(
            production_code("// element.Invoke(0) described in a comment")
                .trim()
                .is_empty(),
            "comments must be stripped before scanning"
        );

        let mut regions = whatsapp_production_regions();
        regions.push((
            "native_a11y.rs",
            production_code(
                include_str!("native_a11y.rs")
                    .split(test_module_marker().as_str())
                    .next()
                    .unwrap_or_default(),
            ),
        ));

        for (module, anchor) in [
            (
                "native_whatsapp_adapter.rs (live placement)",
                "fn drive_whatsapp_composer_placement",
            ),
            (
                "native_whatsapp_adapter.rs (scan_selectors)",
                "fn whatsapp_composer_candidate",
            ),
            ("native_a11y.rs", "fn resolve_uia2_window"),
        ] {
            let code = &regions
                .iter()
                .find(|(name, _)| *name == module)
                .expect("every named region must be scanned")
                .1;
            assert!(
                code.contains(anchor),
                "{module}: scanned region lost its production code, so the scan is vacuous"
            );
        }

        for (module, code) in &regions {
            for mechanism in SUBMIT_SHAPED_MECHANISMS {
                assert!(
                    !code.contains(mechanism),
                    "{module} contains the submit-shaped mechanism {mechanism:?}. \
                     No OSL mode may silently send: the placement path may only write \
                     a value and read it back."
                );
            }
        }
    }

    #[test]
    fn whatsapp_body_normalizer_still_folds_crlf_after_losing_its_string_literals() {
        // `canonical_whatsapp_body` was respelled with char literals so the scan
        // above can cover this whole file without an exception. Same behaviour.
        assert_eq!(canonical_whatsapp_body("  exact\r\nbody  "), "exact\nbody");
        assert_eq!(canonical_whatsapp_body("lone\rcarriage"), "lone\ncarriage");
        assert_eq!(canonical_whatsapp_body("  plain  "), "plain");
        assert_eq!(canonical_whatsapp_body("\r"), "\n");
    }
}

/// Scan-only selector model kept separate from the WebView2 structural contract.
pub mod scan_selectors {
    //! Pure WhatsApp Desktop accessibility selectors.
    //!
    //! This module is scan-only. It models the structural facts OSL needs before it
    //! can claim WhatsApp support: one editable composer paired with one transcript
    //! surface, and exact message-body nodes only when the platform exposes them as
    //! such. Localized labels and placeholder text are deliberately not selector
    //! inputs.

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct WhatsAppRect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    impl WhatsAppRect {
        pub fn valid(self) -> bool {
            self.right > self.left && self.bottom > self.top
        }

        pub fn width(self) -> i32 {
            self.right.saturating_sub(self.left)
        }

        pub fn height(self) -> i32 {
            self.bottom.saturating_sub(self.top)
        }

        pub fn contained_by(self, parent: Self) -> bool {
            self.valid()
                && parent.valid()
                && self.left >= parent.left
                && self.top >= parent.top
                && self.right <= parent.right
                && self.bottom <= parent.bottom
        }

        pub fn horizontal_overlap(self, other: Self) -> i32 {
            self.right
                .min(other.right)
                .saturating_sub(self.left.max(other.left))
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WhatsAppRole {
        Window,
        Pane,
        List,
        Document,
        Row,
        Text,
        EditableText,
        Button,
        Unknown,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WhatsAppNodeEvidence {
        None,
        ExactBody,
        AmbiguousBody,
    }

    #[derive(Clone)]
    pub struct WhatsAppNode {
        pub role: WhatsAppRole,
        pub evidence: WhatsAppNodeEvidence,
        pub bounds: WhatsAppRect,
        pub visible: bool,
        pub enabled: bool,
        pub focusable: bool,
        pub editable: bool,
        pub read_only: bool,
        pub localized_name: Option<String>,
        pub text: Option<String>,
        pub children: Vec<usize>,
    }

    impl WhatsAppNode {
        pub fn structural(role: WhatsAppRole, bounds: WhatsAppRect) -> Self {
            Self {
                role,
                evidence: WhatsAppNodeEvidence::None,
                bounds,
                visible: true,
                enabled: true,
                focusable: false,
                editable: false,
                read_only: true,
                localized_name: None,
                text: None,
                children: Vec::new(),
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum WhatsAppSelectorError {
        Missing,
        Ambiguous,
        Invalid,
        LimitExceeded,
        Unsupported,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct WhatsAppPair {
        pub composer_index: usize,
        pub transcript_index: usize,
    }

    #[derive(Clone, Eq, PartialEq)]
    pub struct WhatsAppBodyCandidate {
        pub node_index: usize,
        pub text: String,
        pub body_bounds: WhatsAppRect,
    }

    pub fn discover_whatsapp_pair(
        nodes: &[WhatsAppNode],
        window_bounds: WhatsAppRect,
    ) -> Result<WhatsAppPair, WhatsAppSelectorError> {
        if !window_bounds.valid() {
            return Err(WhatsAppSelectorError::Invalid);
        }

        let mut pairs = Vec::new();
        for (composer_index, composer) in nodes.iter().enumerate() {
            if !whatsapp_composer_candidate(composer, window_bounds) {
                continue;
            }
            for (transcript_index, transcript) in nodes.iter().enumerate() {
                if whatsapp_transcript_candidate(transcript, composer, window_bounds) {
                    pairs.push(WhatsAppPair {
                        composer_index,
                        transcript_index,
                    });
                }
            }
        }

        match pairs.as_slice() {
            [pair] => Ok(*pair),
            [] => Err(WhatsAppSelectorError::Missing),
            _ => Err(WhatsAppSelectorError::Ambiguous),
        }
    }

    pub fn extract_whatsapp_body_candidates(
        nodes: &[WhatsAppNode],
        row_index: usize,
        max_candidates: usize,
        max_text_bytes: usize,
    ) -> Result<Vec<WhatsAppBodyCandidate>, WhatsAppSelectorError> {
        let Some(row) = nodes.get(row_index) else {
            return Err(WhatsAppSelectorError::Missing);
        };
        if row.role != WhatsAppRole::Row
            || !row.visible
            || !row.bounds.valid()
            || max_candidates == 0
            || max_text_bytes == 0
        {
            return Err(WhatsAppSelectorError::Invalid);
        }

        let mut candidates = Vec::new();
        for index in descendants(nodes, row_index)? {
            let node = &nodes[index];
            match node.evidence {
                WhatsAppNodeEvidence::None => continue,
                WhatsAppNodeEvidence::AmbiguousBody => {
                    return Err(WhatsAppSelectorError::Unsupported)
                }
                WhatsAppNodeEvidence::ExactBody => {
                    let Some(text) = node.text.as_ref() else {
                        return Err(WhatsAppSelectorError::Unsupported);
                    };
                    if node.role != WhatsAppRole::Text
                        || !node.visible
                        || !node.bounds.contained_by(row.bounds)
                        || !valid_candidate_text(text, max_text_bytes)
                    {
                        return Err(WhatsAppSelectorError::Invalid);
                    }
                    if candidates.len() >= max_candidates {
                        return Err(WhatsAppSelectorError::LimitExceeded);
                    }
                    candidates.push(WhatsAppBodyCandidate {
                        node_index: index,
                        text: text.clone(),
                        body_bounds: node.bounds,
                    });
                }
            }
        }

        if candidates.is_empty() {
            return Err(WhatsAppSelectorError::Unsupported);
        }
        Ok(candidates)
    }

    fn whatsapp_composer_candidate(node: &WhatsAppNode, window_bounds: WhatsAppRect) -> bool {
        let conversation_left = window_bounds.left.saturating_add(window_bounds.width() / 3);
        let composer_band_top = window_bounds
            .top
            .saturating_add(window_bounds.height() * 2 / 3);
        node.role == WhatsAppRole::EditableText
            && node.visible
            && node.enabled
            && node.focusable
            && node.editable
            && !node.read_only
            && node.bounds.contained_by(window_bounds)
            && node.bounds.left >= conversation_left
            && node.bounds.top >= composer_band_top
            && node.bounds.width() >= 240
            && (24..=180).contains(&node.bounds.height())
    }

    fn whatsapp_transcript_candidate(
        node: &WhatsAppNode,
        composer: &WhatsAppNode,
        window_bounds: WhatsAppRect,
    ) -> bool {
        let required_overlap = composer.bounds.width().saturating_mul(3) / 5;
        matches!(node.role, WhatsAppRole::List | WhatsAppRole::Document)
            && node.visible
            && node.bounds.contained_by(window_bounds)
            && node.bounds.bottom <= composer.bounds.top
            && node.bounds.height() >= window_bounds.height() / 3
            && node.bounds.horizontal_overlap(composer.bounds) >= required_overlap
    }

    fn descendants(
        nodes: &[WhatsAppNode],
        root: usize,
    ) -> Result<Vec<usize>, WhatsAppSelectorError> {
        let Some(root_node) = nodes.get(root) else {
            return Err(WhatsAppSelectorError::Missing);
        };
        let mut result = Vec::new();
        let mut queue = std::collections::VecDeque::from(root_node.children.clone());
        while let Some(index) = queue.pop_front() {
            let Some(node) = nodes.get(index) else {
                return Err(WhatsAppSelectorError::Invalid);
            };
            if result.len() >= nodes.len() {
                return Err(WhatsAppSelectorError::Invalid);
            }
            result.push(index);
            queue.extend(node.children.iter().copied());
        }
        Ok(result)
    }

    fn valid_candidate_text(value: &str, max_text_bytes: usize) -> bool {
        !value.is_empty()
            && value.len() <= max_text_bytes
            && !value
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    }

    #[cfg(test)]
    mod scan_selector_tests {
        use super::*;

        fn rect(left: i32, top: i32, right: i32, bottom: i32) -> WhatsAppRect {
            WhatsAppRect {
                left,
                top,
                right,
                bottom,
            }
        }

        fn editable(bounds: WhatsAppRect, localized_name: &str) -> WhatsAppNode {
            let mut node = WhatsAppNode::structural(WhatsAppRole::EditableText, bounds);
            node.focusable = true;
            node.editable = true;
            node.read_only = false;
            node.localized_name = Some(localized_name.to_owned());
            node
        }

        fn transcript(bounds: WhatsAppRect, localized_name: &str) -> WhatsAppNode {
            let mut node = WhatsAppNode::structural(WhatsAppRole::List, bounds);
            node.localized_name = Some(localized_name.to_owned());
            node
        }

        fn body_text(bounds: WhatsAppRect, text: &str) -> WhatsAppNode {
            let mut node = WhatsAppNode::structural(WhatsAppRole::Text, bounds);
            node.evidence = WhatsAppNodeEvidence::ExactBody;
            node.text = Some(text.to_owned());
            node
        }

        fn ambiguous_body(bounds: WhatsAppRect, text: &str) -> WhatsAppNode {
            let mut node = WhatsAppNode::structural(WhatsAppRole::Text, bounds);
            node.evidence = WhatsAppNodeEvidence::AmbiguousBody;
            node.text = Some(text.to_owned());
            node
        }

        #[test]
        fn whatsapp_selector_pair() {
            let window = rect(0, 0, 1280, 900);
            let nodes = vec![
                transcript(rect(0, 85, 360, 840), "Chats"),
                transcript(rect(430, 86, 1210, 720), "Messages"),
                editable(rect(455, 748, 1180, 812), "Type a message"),
                editable(rect(16, 130, 320, 174), "Search or start new chat"),
                WhatsAppNode::structural(WhatsAppRole::Button, rect(1188, 748, 1236, 812)),
            ];

            assert_eq!(
                discover_whatsapp_pair(&nodes, window),
                Ok(WhatsAppPair {
                    composer_index: 2,
                    transcript_index: 1,
                })
            );

            let mut localized = nodes.clone();
            localized[1].localized_name = Some("Historial de mensajes".to_owned());
            localized[2].localized_name = Some("Escribe un mensaje".to_owned());
            assert_eq!(
                discover_whatsapp_pair(&localized, window),
                Ok(WhatsAppPair {
                    composer_index: 2,
                    transcript_index: 1,
                })
            );

            let mut ambiguous = localized;
            ambiguous.push(transcript(
                rect(440, 96, 1202, 710),
                "second matching transcript",
            ));
            assert_eq!(
                discover_whatsapp_pair(&ambiguous, window),
                Err(WhatsAppSelectorError::Ambiguous)
            );

            let mut missing = nodes;
            missing[2].read_only = true;
            assert_eq!(
                discover_whatsapp_pair(&missing, window),
                Err(WhatsAppSelectorError::Missing)
            );
        }

        #[test]
        fn whatsapp_selector_body_candidates() {
            let mut row = WhatsAppNode::structural(WhatsAppRole::Row, rect(430, 210, 1210, 330));
            row.children = vec![1, 2, 3, 4];
            let mut nested =
                WhatsAppNode::structural(WhatsAppRole::Pane, rect(500, 276, 1000, 322));
            nested.children = vec![5];
            let nodes = vec![
                row,
                body_text(rect(510, 226, 970, 252), "first exact body"),
                {
                    let mut timestamp =
                        WhatsAppNode::structural(WhatsAppRole::Text, rect(1120, 254, 1180, 274));
                    timestamp.text = Some("10:42".to_owned());
                    timestamp
                },
                WhatsAppNode::structural(WhatsAppRole::Button, rect(470, 226, 498, 252)),
                nested,
                body_text(rect(510, 286, 990, 314), "second exact body"),
            ];

            let candidates = extract_whatsapp_body_candidates(&nodes, 0, 4, 128)
                .expect("exact WhatsApp body evidence should be extracted");
            assert_eq!(candidates.len(), 2);
            assert_eq!(candidates[0].node_index, 1);
            assert_eq!(candidates[0].text, "first exact body");
            assert_eq!(candidates[0].body_bounds, rect(510, 226, 970, 252));
            assert_eq!(candidates[1].node_index, 5);
            assert_eq!(candidates[1].text, "second exact body");

            let mut ambiguous = nodes.clone();
            ambiguous[1] = ambiguous_body(rect(510, 226, 970, 252), "looks like a body");
            assert_eq!(
                extract_whatsapp_body_candidates(&ambiguous, 0, 4, 128).map(|value| value.len()),
                Err(WhatsAppSelectorError::Unsupported)
            );

            let mut no_exact = nodes.clone();
            no_exact[1].evidence = WhatsAppNodeEvidence::None;
            no_exact[5].evidence = WhatsAppNodeEvidence::None;
            assert_eq!(
                extract_whatsapp_body_candidates(&no_exact, 0, 4, 128).map(|value| value.len()),
                Err(WhatsAppSelectorError::Unsupported)
            );

            let mut invalid = nodes.clone();
            invalid[5].text = Some("bad\u{0008}body".to_owned());
            assert_eq!(
                extract_whatsapp_body_candidates(&invalid, 0, 4, 128).map(|value| value.len()),
                Err(WhatsAppSelectorError::Invalid)
            );

            assert_eq!(
                extract_whatsapp_body_candidates(&nodes, 0, 1, 128).map(|value| value.len()),
                Err(WhatsAppSelectorError::LimitExceeded)
            );
        }
    }
}
