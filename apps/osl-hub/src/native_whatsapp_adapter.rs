//! Pure structural contracts for WhatsApp's native WebView2 surface.
//!
//! The live Windows accessibility side is allowed to collect only bounded
//! metadata. This module keeps the trust, discovery, and placement decisions
//! deterministic and testable without touching the remote process.

use sha2::{Digest, Sha256};

// `Uia2Deadline` is deliberately NOT re-exported here. D-155 gave the substrate
// a public `acquire_uia2_editables`, so this module no longer handles a budget
// at any point, and a scan test below pins that it never starts again.
pub use crate::native_a11y::{
    acquire_uia2_editables, acquire_uia2_window, clear_uia2_composer, place_uia2_carrier,
    resolve_uia2_composer, uia2_carrier_carries_submit, Uia2AcquireError, Uia2Acquired,
    Uia2CallTimeout, Uia2ComposerError, Uia2ComposerMatcher, Uia2Editable, Uia2OwnedWindow,
    Uia2PlacementRefusal, Uia2ResolvedWindow, Uia2Syscalls, Uia2TreeRoute, Uia2WakePolicy,
    Uia2WindowPlan, Uia2WindowResolveError, Uia2WindowShape, WEBVIEW2_PROCESS_NAME,
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

/// WhatsApp Desktop's shell process, **measured on the owner's host**, not
/// assumed: `WhatsApp.Root.exe`.
///
/// ```text
/// ProcessId ParentProcessId Name               cmd
///     23884            9644 WhatsApp.Root.exe  "C:\Program Files\WindowsApps\
///                                               5319275A.WhatsAppDesktop_2.2629.100.0_x64
///                                               __cv1g1gvanyjgm\WhatsApp.Root.exe"
/// ```
///
/// `native_a11y`'s recorded fixture calls this process `WhatsApp.exe`, and the
/// substrate compares image names exactly once `.exe` is stripped. `WhatsApp`
/// therefore matches nothing on the real machine, and the adapter would have
/// found no window on a host where WhatsApp was running. The fixture string was
/// never checked against a live process from Rust; this one was, and the live
/// run below reports `image="WhatsApp.Root.exe"` -- which also refutes A-00b's
/// residual risk 4, since `OpenProcess` is clearly not refused for this Appx
/// package.
pub const WHATSAPP_DESKTOP_PROCESS_NAME: &str = "WhatsApp.Root";

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
/// Discord's shipping route reads Chromium's MSAA client object on the
/// **outer** window because OSL has already borrowed and reparented that exact
/// window.
///
/// **Corrected by D-176.** That sentence used to end "and
/// `wake_electron_accessibility` hands back an `IAccessible` there", which was
/// not true when it was written: the handshake asked for Chromium's honeypot
/// object id, which every provider answers with nothing, so it handed back
/// `None` at Discord's outer window as much as anywhere else. It is true now,
/// because the handshake takes the object at `OBJID_CLIENT` -- see
/// [`crate::native_a11y::wake_electron_accessibility_with`] for the measurement
/// and for what a returned object does and does not prove.
///
/// WhatsApp is not a borrowed window: nothing in OSL adopts it, the content
/// root is in a process OSL never claimed, and the handle this plan binds is
/// the renderer child -- which is precisely the window A-00 measured with UI
/// Automation directly (11 elements, one writable `Phone number` edit on the
/// login screen). Taking `MsaaBridge` here would claim a bridged read that has
/// never been measured on this provider, on a window that is not the one
/// Chromium hands its client object for.
///
/// # How this plan reaches the content window, measured live
///
/// Run from Rust on the owner's Windows host, WhatsApp shown, through this very
/// plan and the substrate's own enumerator:
///
/// ```text
/// candidate hwnd=198342 pid=23884 image="WhatsApp.Root.exe" class="WinUIDesktopWin32WindowClass"
///           visible=true area=1092960 parent=None      associated_app=None
/// candidate hwnd=67446  pid=24196 image="msedgewebview2.exe" class="Chrome_WidgetWin_1"
///           visible=true area=1068000 parent=None      associated_app=None
/// candidate hwnd=132018 pid=24196 image="msedgewebview2.exe" class="Chrome_RenderWidgetHostHWND"
///           visible=true area=1068000 parent=Some(67446) associated_app=None
/// ```
///
/// The shell resolves. The content window exists, is visible, and carries the
/// renderer child. What was missing was the link: `associated_app_hwnd` is
/// `None`, and the resolver used to require it to equal the shell's handle. The
/// WebView2 window is genuinely top-level -- parent, `GA_ROOT`, `GA_ROOTOWNER`
/// and `GWLP_HWNDPARENT` are all zero or itself, in both directions -- so no
/// ancestry-derived field can ever produce that link, and this plan resolved to
/// `Resolve(MissingSiblingContentOuter)`.
///
/// The relationship that does exist is **process parentage**: msedgewebview2
/// pid 24196 has `ParentProcessId` 23884, the shell, and its command line
/// carries `--webview-exe-name=WhatsApp.Root.exe`. D-156 taught the substrate
/// that association, in [`crate::native_a11y::classify_sibling_host`]:
/// parentage is the link and the switch is the corroboration, both are
/// required, and a host that is parented here while naming another application
/// is refused rather than guessed at.
///
/// **The anchor is not a unique key, and ambiguity fails closed (D-180).** If
/// more than one host satisfies both signals the substrate refuses; it does not
/// pick the larger, because size is not evidence of which host holds the
/// conversation and "biggest visible `msedgewebview2`" was already rejected as
/// this task's mutant [1]. Nothing downstream would catch a wrong choice --
/// `whatsapp_placement` guards only `bound_is_app_shell` -- and the text this
/// adapter places IS the carrier for the payload, so binding the wrong window
/// sends it somewhere the user did not choose. If a legitimate second window
/// ever appears (a popped-out chat, a media viewer), the resolution is a
/// positive discriminator built on [`WHATSAPP_COMPOSER_MATCHER`], never a
/// heuristic.
///
/// **And the corroboration is an accident control, not an anti-spoofing
/// defence.** A process can choose its apparent parent with
/// `PROC_THREAD_ATTRIBUTE_PARENT_PROCESS`, and a command line is chosen by
/// whoever launches the process, so both signals are attacker-controlled and
/// requiring both costs an adversary nothing. What it does buy is telling
/// Windows Search's WebView2 apart from WhatsApp's, which is the failure that
/// was actually measured on this machine.
///
/// It still must not fall back to "the biggest visible msedgewebview2 window":
/// this machine runs a second one for Windows Search, it is larger, and
/// `native_a11y`'s `largest_visible_webview2_is_the_decoy_not_whatsapp` pins
/// that it would be chosen. The refusal also stays reachable -- a shell with no
/// parented WebView2 still produces
/// [`WhatsAppPlacementStatus::WebView2ContentUnavailable`], which is what
/// `the_measured_host_has_no_window_link_from_the_shell_to_its_webview2` and
/// `native_a11y`'s `a_shell_with_no_parented_webview2_still_refuses` hold down.
///
/// **Note for whoever owns this file next.** This module's own recorded graphs
/// were captured before the process table was read, so `contract_whatsapp_graph`
/// still carries the window-tree link and `measured_whatsapp_graph` carries no
/// link at all. Both are still correct as recorded and both still pass, but
/// neither exercises parentage: adding `.hosted_by(23884, Some("WhatsApp.Root.exe"))`
/// to the two msedgewebview2 entries of `measured_whatsapp_graph` is what would
/// flip it from refusing to placing, and that edit belongs to this lane, not to
/// D-156's.
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
    /// No usable WhatsApp shell window. Measured on the owner's host: this is
    /// what a *running* WhatsApp closed to the tray produces, because every one
    /// of its windows reports `IsWindowVisible` false and the substrate binds
    /// only visible windows. "Not running" and "hidden" are indistinguishable
    /// from here, so the status does not claim to tell them apart.
    AppWindowUnavailable,
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
    ///
    /// **Unreachable since D-155.** It described the one way A-02c's deadline
    /// relay could fail: being asked for the editable scan before the
    /// acquisition had handed it a budget. The substrate now derives that budget
    /// itself inside `acquire_uia2_editables`, so the state cannot occur. Kept
    /// rather than deleted -- a receipt status is observable contract, and this
    /// is the owner's call to retire, not a lane's to drop in passing.
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

/// Scan the bound window's editable elements under the plan's own deadline.
///
/// The deadline is no longer this module's problem: [`acquire_uia2_editables`]
/// derives it inside the substrate from the acquisition's own
/// `call_timeout_ms`, which is why the relay that used to sit here is gone
/// (D-155). Nothing in this module can name a budget, let alone invent one.
fn whatsapp_editables(
    host: &dyn Uia2Syscalls,
    acquired: Uia2Acquired,
) -> Result<Vec<Uia2Editable>, WhatsAppPlacementStatus> {
    acquire_uia2_editables(host, acquired).map_err(|_| WhatsAppPlacementStatus::CallTimedOut)
}

fn whatsapp_acquire_failure(error: Uia2AcquireError) -> WhatsAppPlacementStatus {
    match error {
        Uia2AcquireError::Resolve(Uia2WindowResolveError::MissingAppOuter) => {
            WhatsAppPlacementStatus::AppWindowUnavailable
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

    let acquired = match acquire_uia2_window(WHATSAPP_UIA2_WINDOW_PLAN, host) {
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

    let editables = match whatsapp_editables(host, acquired) {
        Ok(editables) => editables,
        Err(status) => return bound(status),
    };
    let composer = match resolve_uia2_composer(WHATSAPP_COMPOSER_MATCHER, &editables) {
        Ok(composer) => composer,
        Err(error) => return bound(whatsapp_composer_failure(error)),
    };

    let receipt =
        match place_uia2_carrier(host, acquired, &composer, &carrier, allow_replace_existing) {
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
        match clear_uia2_composer(host, acquired, &composer) {
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

    use crate::native_a11y::tests::{composer, owned, RecordedHost};

    // Only the live probe's enumeration capture still names the syscall seam's
    // deadline token; the production half no longer handles a budget anywhere.
    #[cfg(target_os = "windows")]
    use crate::native_a11y::Uia2Deadline;

    // The window graph MEASURED on the owner's Windows host on 2026-08-04, by
    // enumerating every top-level window and its descendants and printing class,
    // image name, parent, GA_ROOT, GA_ROOTOWNER, GW_OWNER and GWLP_HWNDPARENT:
    //
    //   198342 pid=23884 WhatsApp.Root  WinUIDesktopWin32WindowClass          parent=0
    //   197328 pid=23884 WhatsApp.Root  Microsoft.UI.Content.DesktopChildSiteBridge parent=198342
    //    67446 pid=24196 msedgewebview2 Chrome_WidgetWin_1                    parent=0
    //   132018 pid=24196 msedgewebview2 Chrome_RenderWidgetHostHWND           parent=67446
    //
    // `native_a11y`'s recorded fixture has the WebView2 outer window parented to
    // the shell. On the real machine it is a TOP-LEVEL window: parent, GA_ROOT,
    // GA_ROOTOWNER and GWLP_HWNDPARENT are all zero or itself, in both
    // directions. That difference is the whole finding below.
    const SHELL_HWND: isize = 198342;
    const BRIDGE_HWND: isize = 197328;
    const WEBVIEW_OUTER_HWND: isize = 67446;
    const RENDERER_HWND: isize = 132018;
    const DECOY_WEBVIEW_HWND: isize = 900001;
    const SHELL_IMAGE: &str = "WhatsApp.Root.exe";

    /// The graph the substrate's `SiblingChromiumRenderer` contract requires:
    /// the WebView2 content window carrying `associated_app_hwnd` back to the
    /// shell OSL claimed. Everything else is as measured.
    fn contract_whatsapp_graph() -> Vec<Uia2OwnedWindow> {
        vec![
            owned(
                SHELL_HWND,
                None,
                None,
                23884,
                SHELL_IMAGE,
                WHATSAPP_ROOT_WINDOW_CLASS,
                1_092_960,
            ),
            owned(
                BRIDGE_HWND,
                Some(SHELL_HWND),
                None,
                23884,
                SHELL_IMAGE,
                "Microsoft.UI.Content.DesktopChildSiteBridge",
                1_068_000,
            ),
            owned(
                WEBVIEW_OUTER_HWND,
                None,
                Some(SHELL_HWND),
                24196,
                "msedgewebview2.exe",
                crate::native_a11y::ELECTRON_OUTER_WINDOW_CLASS,
                1_068_000,
            ),
            owned(
                RENDERER_HWND,
                Some(WEBVIEW_OUTER_HWND),
                Some(SHELL_HWND),
                24196,
                "msedgewebview2.exe",
                crate::native_a11y::ELECTRON_RENDERER_WINDOW_CLASS,
                1_068_000,
            ),
            // Windows Search hosts its own WebView2 on this machine
            // (`--webview-exe-name=SearchApp`, pid 22824). It is bigger, and it
            // is not WhatsApp's. If the association is ever dropped, this is
            // what OSL would place a carrier into.
            owned(
                DECOY_WEBVIEW_HWND,
                None,
                None,
                22824,
                "msedgewebview2.exe",
                crate::native_a11y::ELECTRON_OUTER_WINDOW_CLASS,
                1_920 * 1_080,
            ),
        ]
    }

    /// The same graph exactly as measured: the WebView2 window has no window
    /// relationship to the shell at all, so `associated_app_hwnd` is `None`.
    fn measured_whatsapp_graph() -> Vec<Uia2OwnedWindow> {
        contract_whatsapp_graph()
            .into_iter()
            .map(|mut window| {
                if window.process_name == "msedgewebview2.exe" {
                    window.associated_app_hwnd = None;
                }
                window
            })
            .collect()
    }

    fn whatsapp_graph() -> Vec<Uia2OwnedWindow> {
        contract_whatsapp_graph()
    }

    const WHATSAPP_MEASURED_ELEMENTS: usize = 11;
    const WEBVIEW2_PROCESS_ID: u32 = 24196;
    const WHATSAPP_SHELL_PROCESS_ID: u32 = 23884;
    const PAYLOAD: &str = "alpha7731osl";
    const CARRIER: &str = "OSL1.WA.alpha7731osl";

    /// Keep the window list the substrate enumerated, so a failed live acquire
    /// can report the evidence the acquisition itself saw.
    ///
    /// **This is not the relay coming back.** It never touches a deadline: it
    /// forwards every call unchanged, budget included, and records only the
    /// `Vec<Uia2OwnedWindow>` the substrate had already asked for and been
    /// given. `submit_shaped_calls` is forwarded, never answered. It exists for
    /// the one `#[ignore]`d live probe below, whose candidate dump is what
    /// produced A-02c's `associated_app=None` finding; the sibling-association
    /// lane still needs it.
    #[cfg(target_os = "windows")]
    struct CapturedEnumeration<'host> {
        host: &'host dyn Uia2Syscalls,
        windows: std::cell::RefCell<Vec<Uia2OwnedWindow>>,
    }

    #[cfg(target_os = "windows")]
    impl<'host> CapturedEnumeration<'host> {
        fn new(host: &'host dyn Uia2Syscalls) -> Self {
            Self {
                host,
                windows: std::cell::RefCell::new(Vec::new()),
            }
        }

        fn captured(&self) -> Vec<Uia2OwnedWindow> {
            self.windows.borrow().clone()
        }
    }

    #[cfg(target_os = "windows")]
    impl Uia2Syscalls for CapturedEnumeration<'_> {
        fn enumerate_windows(
            &self,
            deadline: Uia2Deadline,
        ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
            let windows = self.host.enumerate_windows(deadline)?;
            *self.windows.borrow_mut() = windows.clone();
            Ok(windows)
        }

        fn wake_chromium(
            &self,
            hwnd: isize,
            deadline: Uia2Deadline,
        ) -> Result<bool, Uia2CallTimeout> {
            self.host.wake_chromium(hwnd, deadline)
        }

        fn element_count(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            deadline: Uia2Deadline,
        ) -> Result<usize, Uia2CallTimeout> {
            self.host.element_count(hwnd, route, deadline)
        }

        fn editable_elements(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            deadline: Uia2Deadline,
        ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
            self.host.editable_elements(hwnd, route, deadline)
        }

        fn set_value(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            element: &Uia2Editable,
            value: &str,
            deadline: Uia2Deadline,
        ) -> Result<bool, Uia2CallTimeout> {
            self.host.set_value(hwnd, route, element, value, deadline)
        }

        fn value_of(
            &self,
            hwnd: isize,
            route: Uia2TreeRoute,
            element: &Uia2Editable,
            deadline: Uia2Deadline,
        ) -> Result<Option<String>, Uia2CallTimeout> {
            self.host.value_of(hwnd, route, element, deadline)
        }

        fn submit_shaped_calls(&self) -> usize {
            self.host.submit_shaped_calls()
        }

        fn settle(&self, millis: u64) {
            self.host.settle(millis);
        }
    }

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
            .filter(|window| window.process_name == SHELL_IMAGE)
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
    fn task_1062_records_every_placement_receipt_field_for_a_marked_byte_string() {
        // This is intentionally the same prepared, non-sending WhatsApp
        // companion fixture as the 1060 discovery command.  The marker is
        // ASCII so its byte sequence is unambiguous in both the carrier and
        // the recorded composer value.
        const MARKER: &[u8] = b"TASK1062-WA-BYTES-7F3A";
        let marker = std::str::from_utf8(MARKER).expect("ASCII marker");
        let host = whatsapp_host(vec![composer("Type a message")]);

        let receipt = drive_whatsapp_composer_placement(&host, marker, false);
        let composer_value = host.value.borrow().clone().unwrap_or_default();
        let marker_in_composer = composer_value.contains(marker);

        println!(
            "TASK1062 marker_utf8={marker} marker_hex={} marker_bytes_len={}",
            MARKER.iter().map(|byte| format!("{byte:02X}")).collect::<String>(),
            MARKER.len(),
        );
        println!(
            "TASK1062 receipt placed={} enter_sent={} status={:?} bound_process_id={} bound_is_app_shell={} tree_route={:?} woke={} elements={} readback_contains_carrier={} cleared={}",
            receipt.placed,
            receipt.enter_sent,
            receipt.status,
            receipt.bound_process_id,
            receipt.bound_is_app_shell,
            receipt.tree_route,
            receipt.woke,
            receipt.elements,
            receipt.readback_contains_carrier,
            receipt.cleared,
        );
        println!(
            "TASK1062 compose_box name=Type a message exact_marker_present={} value={composer_value}",
            marker_in_composer,
        );

        assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);
        assert!(receipt.placed);
        assert!(!receipt.enter_sent);
        assert!(receipt.readback_contains_carrier);
        assert!(marker_in_composer, "compose box must contain the exact marker");
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
    fn whatsapp_binds_only_the_webview2_that_belongs_to_it() {
        // This machine runs two independent WebView2 hosts: WhatsApp's
        // (`--webview-exe-name=WhatsApp.Root.exe`, pid 24196) and Windows
        // Search's (`--webview-exe-name=SearchApp`, pid 22824). The decoy is
        // the LARGER window, so "biggest visible msedgewebview2" would pick it
        // and OSL would place a carrier into the wrong application entirely.
        let host = whatsapp_host(vec![composer("Type a message")]);
        let receipt = drive_whatsapp_composer_placement(&host, PAYLOAD, false);

        assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);
        assert_eq!(
            receipt.bound_process_id, WEBVIEW2_PROCESS_ID,
            "the association is what keeps OSL out of another app's WebView2"
        );
        assert_ne!(receipt.bound_process_id, 22824);
    }

    #[test]
    fn the_measured_host_has_no_window_link_from_the_shell_to_its_webview2() {
        // MEASURED, not assumed. On the owner's host the WebView2 content
        // window is top-level: parent, GA_ROOT, GA_ROOTOWNER and
        // GWLP_HWNDPARENT are all zero or itself, so
        // `win32::enumerate`'s `associated_app_hwnd`, which is
        // `root_ancestor_of(hwnd).filter(|root| pid_of(root) != pid)`, is
        // `None` -- and `resolve_uia2_window` requires it to equal the shell.
        //
        // A-00b listed this as residual risk 3, "inferred from A-00's prose,
        // not measured from Rust". It is now measured and the inference is
        // wrong: A-00's "sibling process" is not a window-tree relationship.
        // The relationship that DOES exist is process parentage --
        // msedgewebview2 pid 24196 has ParentProcessId 23884, the shell -- with
        // the `--webview-exe-name=WhatsApp.Root.exe` switch corroborating it.
        //
        // That fix belongs to `native_a11y`, which this lane does not own. What
        // this lane owes is that the adapter REFUSES rather than falling back to
        // an unlinked WebView2: an unverified content window is exactly the
        // decoy above, and writing there is a disclosure. So the assertion is
        // the refusal, not the placement.
        let host = RecordedHost::new(measured_whatsapp_graph(), WHATSAPP_MEASURED_ELEMENTS)
            .chromium(2)
            .with_editables(vec![composer("Type a message")]);

        let receipt = drive_whatsapp_composer_placement(&host, PAYLOAD, false);

        assert_eq!(
            receipt.status,
            WhatsAppPlacementStatus::WebView2ContentUnavailable,
            "without a trustworthy link to the shell the content window must be refused"
        );
        assert!(!receipt.placed);
        assert!(
            host.set_values.borrow().is_empty(),
            "nothing may be written into a WebView2 that has not been tied to WhatsApp"
        );

        // And the ONLY difference between refusing and placing is that one
        // field, so this test is measuring the association and nothing else.
        let linked = RecordedHost::new(contract_whatsapp_graph(), WHATSAPP_MEASURED_ELEMENTS)
            .chromium(2)
            .with_editables(vec![composer("Type a message")]);
        assert_eq!(
            drive_whatsapp_composer_placement(&linked, PAYLOAD, false).status,
            WhatsAppPlacementStatus::Placed
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
            WhatsAppPlacementStatus::AppWindowUnavailable
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
        // The relay forwards `submit_shaped_calls` instead of
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

    /// Was `whatsapp_editable_scan_refuses_without_a_deadline_the_acquisition_derived`.
    ///
    /// A-02c's relay could be asked for the scan before the acquisition had
    /// handed it a budget, and refused with `DeadlineUnavailable`. D-155 made
    /// that state unrepresentable: the scan takes a `Uia2Acquired`, so there is
    /// no way to reach it without an acquisition, and the substrate derives the
    /// budget from that acquisition's own plan. The assertion is pointed at the
    /// property that replaced the refusal.
    #[test]
    fn whatsapp_editable_scan_can_only_run_under_the_deadline_the_acquisition_derived() {
        let host = whatsapp_host(vec![composer("Type a message")]);
        let acquired = acquire_uia2_window(WHATSAPP_UIA2_WINDOW_PLAN, &host)
            .expect("WhatsApp acquires through the substrate");
        assert_eq!(
            acquired.window.call_timeout_ms,
            WHATSAPP_UIA2_DEFAULT_CALL_TIMEOUT_MS
        );
        host.deadlines.borrow_mut().clear();

        assert_eq!(
            whatsapp_editables(&host, acquired).map(|editables| editables.len()),
            Ok(1)
        );
        assert_eq!(
            *host.deadlines.borrow(),
            vec![WHATSAPP_UIA2_DEFAULT_CALL_TIMEOUT_MS],
            "the scan must issue exactly one call, under the plan's own budget"
        );
    }

    /// Was `the_deadline_relay_cannot_blind_the_submit_shaped_guard_or_invent_a_budget`.
    /// The relay is gone; both things it was pinned for still have to hold of
    /// the direct path.
    #[test]
    fn nothing_stands_between_the_placement_guard_and_the_backends_own_counter() {
        // The submit-shaped counter is read straight off the backend. Anything
        // in the way that answered it itself would blind every placement guard.
        let host = whatsapp_host(vec![composer("Type a message")]);
        host.submit_shaped.set(3);
        let receipt = drive_whatsapp_composer_placement(&host, PAYLOAD, false);
        assert_eq!(
            receipt.status,
            WhatsAppPlacementStatus::SubmitShapedCallObserved,
            "a backend already admitting a submit-shaped call must refuse the path"
        );
        assert!(receipt.enter_sent);
        assert!(!receipt.placed);

        // And no budget is invented anywhere along the way: every call the whole
        // path issues carries the plan's own deadline.
        let host = whatsapp_host(vec![composer("Type a message")]);
        assert_eq!(
            drive_whatsapp_composer_placement(&host, PAYLOAD, false).status,
            WhatsAppPlacementStatus::Placed
        );
        let deadlines = host.deadlines.borrow();
        assert!(!deadlines.is_empty());
        assert!(
            deadlines
                .iter()
                .all(|deadline| *deadline == WHATSAPP_UIA2_WINDOW_PLAN.call_timeout_ms),
            "every call, including the editable scan, must carry the plan's own \
             deadline, saw {deadlines:?}"
        );
    }

    /// Neither adapter may grow a budget of its own again. Both reach
    /// `editable_elements` only through the substrate's door, which derives the
    /// deadline from the plan; a `Uia2Deadline` named in this module would be
    /// the first step back to a relay.
    #[test]
    fn the_adapter_never_builds_a_deadline_of_its_own() {
        for (module, code) in whatsapp_production_regions() {
            assert!(
                !code.contains("uia2deadline"),
                "{module} must not name the deadline token at all: the substrate \
                 derives every budget from the plan"
            );
        }
        let (_, live) = whatsapp_production_regions().remove(0);
        assert!(
            live.contains("acquire_uia2_editables"),
            "the editable scan must go through the substrate's public door"
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

    /// Drive the REAL WhatsApp composer through the shared substrate.
    ///
    /// This is the only thing in this lane that touches a live provider, and it
    /// cannot run here: `win32` is `cfg(target_os = "windows")` and this machine
    /// is Linux. It is `#[ignore]`d so it never runs unattended, and it is
    /// read-only unless `OSL_WA_PROBE_CARRIER` is set -- setting that variable
    /// is what opts into a write, and the composer is cleared immediately after.
    ///
    /// The host is `desktop()`, not `rooted_at`: OSL reparents *Discord's*
    /// window into its own hierarchy, which is why Discord's consumption had to
    /// be rooted. Nothing in OSL borrows WhatsApp, and WhatsApp's content window
    /// belongs to a process OSL never claimed, so the enumeration has to be
    /// desktop-wide or the sibling would be unreachable.
    ///
    /// ```text
    /// # from WSL, build the Windows test binary:
    /// flock /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml \
    ///   --lib --target x86_64-pc-windows-gnu -j 4 --no-run
    /// # then, on the Windows host, WhatsApp signed in with a conversation open:
    /// set OSL_WA_PROBE_CARRIER=alpha7731osl
    /// osl_privacy_hub-<hash>.exe --ignored --nocapture --test-threads=1 \
    ///   drive_the_real_whatsapp_composer_through_the_substrate
    /// ```
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "drives live WhatsApp Desktop on a Windows host; run explicitly"]
    fn drive_the_real_whatsapp_composer_through_the_substrate() {
        let win32 = crate::native_a11y::win32::Uia2Win32Host::desktop();
        let host = CapturedEnumeration::new(&win32);
        let acquired = match acquire_uia2_window(WHATSAPP_UIA2_WINDOW_PLAN, &host) {
            Ok(acquired) => acquired,
            Err(error) => {
                // Say WHY, from what the substrate itself enumerated, instead of
                // leaving the conductor to guess between "not running", "hidden",
                // "named something else" and "the association is missing". This
                // is the dump that produced A-02c's `associated_app=None`
                // finding, so it is kept verbatim -- it reports the exact list
                // the failed acquisition saw, not a second enumeration.
                for window in host.captured().iter().filter(|window| {
                    let name = window.process_name.to_ascii_lowercase();
                    name.contains("whatsapp") || name.contains("webview2")
                }) {
                    eprintln!(
                        "  candidate hwnd={} pid={} image={:?} class={:?} \
                         visible={} area={} parent={:?} associated_app={:?}",
                        window.hwnd,
                        window.process_id,
                        window.process_name,
                        window.class_name,
                        window.visible,
                        window.area,
                        window.parent_hwnd,
                        window.associated_app_hwnd,
                    );
                }
                panic!("whatsapp: acquire failed: {error:?}");
            }
        };
        eprintln!(
            "whatsapp: pid={} elements={} woke={} settled_ms={} bound_is_app_shell={}",
            acquired.window.bound_process_id,
            acquired.elements,
            acquired.woke,
            acquired.settled_ms,
            acquired.window.bound_hwnd == acquired.window.app_outer_hwnd,
        );

        let editables = whatsapp_editables(&host, acquired)
            .unwrap_or_else(|status| panic!("whatsapp: editable scan refused: {status:?}"));
        eprintln!(
            "whatsapp: editable={} writable={}",
            editables.len(),
            editables
                .iter()
                .filter(|element| element.writable())
                .count()
        );
        for element in &editables {
            eprintln!(
                "  edit name={:?} value_pattern={} enabled={} kbd={} read_only={}",
                element.name,
                element.value_pattern,
                element.enabled,
                element.keyboard_focusable,
                element.read_only
            );
        }

        let composer = resolve_uia2_composer(WHATSAPP_COMPOSER_MATCHER, &editables)
            .unwrap_or_else(|error| panic!("whatsapp: no composer resolved: {error:?}"));
        eprintln!("whatsapp: composer name={:?}", composer.name);

        let Ok(payload) = std::env::var("OSL_WA_PROBE_CARRIER") else {
            eprintln!("whatsapp: read-only probe, nothing written");
            return;
        };
        let receipt = probe_whatsapp_composer_write_then_clear(&host, &payload, false);
        eprintln!("whatsapp: {receipt:?}");
        assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);
        assert!(receipt.readback_contains_carrier);
        assert!(
            receipt.cleared,
            "the composer must never be left holding a carrier"
        );
        assert!(!receipt.enter_sent, "nothing here may commit a message");
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
    // Pure WhatsApp Desktop accessibility selectors.
    //
    // This module is scan-only. It models the structural facts OSL needs before it
    // can claim WhatsApp support: one editable composer paired with one transcript
    // surface, and exact message-body nodes only when the platform exposes them as
    // such. Localized labels and placeholder text are deliberately not selector
    // inputs.

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
