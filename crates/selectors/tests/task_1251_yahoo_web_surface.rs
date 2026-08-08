#![allow(dead_code)]

extern crate self as sha2;

pub struct Sha256;

pub struct TestDigest([u8; 32]);

pub trait Digest {
    fn digest(data: Vec<u8>) -> TestDigest;
}

impl Digest for Sha256 {
    fn digest(data: Vec<u8>) -> TestDigest {
        let mut output = [0u8; 32];
        for (index, byte) in data.into_iter().enumerate() {
            output[index % output.len()] ^= byte;
        }
        TestDigest(output)
    }
}

impl std::fmt::LowerHex for TestDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

mod adapters {
    use std::collections::BTreeSet;

    pub const ADAPTER_ABI_VERSION: u32 = 1;

    pub type AdapterAppId = adapter_profile::AdapterService;
    pub type SurfaceKind = adapter_profile::AdapterSurface;
    pub type CapabilitySet = BTreeSet<adapter_profile::Capability>;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Bounds {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum A11yTree {
        Both,
        WebAx,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum BindingEvidence {
        Accessibility { tree: A11yTree },
        Pixel,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SurfaceTarget {
        pub app: AdapterAppId,
        pub surface: SurfaceKind,
        pub generation: u64,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct NodeRef(pub(crate) u64);

    impl NodeRef {
        pub fn for_claimed_node(handle: u64) -> Self {
            Self(handle)
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SurfaceBinding {
        pub app: AdapterAppId,
        pub generation: u64,
        pub evidence: BindingEvidence,
        pub composer: NodeRef,
        pub transcript: Option<NodeRef>,
        pub bounds: Bounds,
        pub bound_at_ms: u64,
        pub(crate) scope_binding_hash: String,
    }

    impl SurfaceBinding {
        pub fn for_claimed_surface(
            app: AdapterAppId,
            generation: u64,
            evidence: BindingEvidence,
            composer: NodeRef,
            transcript: Option<NodeRef>,
            bounds: Bounds,
            bound_at_ms: u64,
            scope_binding_hash: impl Into<String>,
        ) -> Self {
            Self {
                app,
                generation,
                evidence,
                composer,
                transcript,
                bounds,
                bound_at_ms,
                scope_binding_hash: scope_binding_hash.into(),
            }
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SurfaceState {
        pub composer_text_sha256: String,
        pub composer_is_empty: bool,
        pub composer_is_password_field: bool,
        pub focused: bool,
        pub occluded: bool,
        pub read_was_complete: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum DestinationStatus {
        Attested,
        Changed,
        Unknown,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DestinationIdentity {
        pub status: DestinationStatus,
        pub account_digest: String,
        pub conversation_digest: String,
        pub recipients_digest: String,
        pub scope_binding_hash: String,
        pub evidence: BindingEvidence,
        pub attested_at_ms: u64,
        pub ttl_ms: u64,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Carrier(pub String);

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PlacementAuthorization {
        scope_binding_hash: String,
    }

    impl PlacementAuthorization {
        pub fn for_scope(scope_binding_hash: impl Into<String>) -> Self {
            Self {
                scope_binding_hash: scope_binding_hash.into(),
            }
        }

        pub(crate) fn scope_binding_hash(&self) -> &str {
            &self.scope_binding_hash
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SendAuthorization {
        scope_binding_hash: String,
    }

    impl SendAuthorization {
        pub fn for_scope(scope_binding_hash: impl Into<String>) -> Self {
            Self {
                scope_binding_hash: scope_binding_hash.into(),
            }
        }

        pub(crate) fn scope_binding_hash(&self) -> &str {
            &self.scope_binding_hash
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PlacementStatus {
        Placed,
        NotPlaced,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PlacementReceipt {
        pub status: PlacementStatus,
        pub placed_sha256: Option<String>,
        pub elapsed_ms: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum SendOutcome {
        Sent,
        NotSent,
        Unknown,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SendReceipt {
        pub outcome: SendOutcome,
        pub elapsed_ms: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PaintConfidence {
        Exact,
        Approximate,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PaintTarget {
        pub carrier_sha256: String,
        pub rect: Bounds,
        pub clipped_by: Option<Bounds>,
        pub confidence: PaintConfidence,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum AdapterRefusal {
        DestinationUnattested,
        GenerationStale,
        PlatformUnsupported,
        ReadIncomplete,
        WindowGone,
        AccessibilityUnavailable,
    }

    pub trait SurfaceAdapter: Send + Sync {
        fn abi_version(&self) -> u32;
        fn app(&self) -> AdapterAppId;
        fn surface(&self) -> SurfaceKind;
        fn capabilities(&self, now_unix_seconds: u64) -> CapabilitySet;
        fn locate(&self, target: &SurfaceTarget) -> Result<SurfaceBinding, AdapterRefusal>;
        fn read_state(&self, binding: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal>;
        fn destination(
            &self,
            binding: &SurfaceBinding,
        ) -> Result<DestinationIdentity, AdapterRefusal>;
        fn place(
            &self,
            binding: &SurfaceBinding,
            authorization: &PlacementAuthorization,
            carrier: &Carrier,
        ) -> PlacementReceipt;
        fn commit(
            &self,
            binding: &SurfaceBinding,
            authorization: &SendAuthorization,
            placed: &PlacementReceipt,
        ) -> SendReceipt;
        fn paint_targets(
            &self,
            binding: &SurfaceBinding,
        ) -> Result<Vec<PaintTarget>, AdapterRefusal>;
    }

    pub(crate) fn same_scope(actual: &str, expected: &str) -> bool {
        !actual.is_empty() && actual == expected
    }

    pub fn is_send_evidence_admissible(evidence: &BindingEvidence) -> bool {
        !matches!(evidence, BindingEvidence::Pixel)
    }
}

#[path = "../../../apps/osl-hub/src/web_surface_adapter/mod.rs"]
mod web_surface_adapter;

use adapter_profile::{AdapterService, AdapterSurface, Capability, SelectorKind};
use adapters::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use web_surface_adapter::{
    WebPageControlRefusal, WebPageControls, WebSurfaceAdapter, WebSurfaceBackend,
};

struct YahooPage {
    body_present: bool,
    send_present: bool,
    draft_body: String,
    sent_messages: Vec<String>,
}

struct YahooFixture {
    page: Mutex<YahooPage>,
    placements: AtomicUsize,
    commits: AtomicUsize,
    last_control_refusal: Mutex<Option<WebPageControlRefusal>>,
}

impl YahooFixture {
    fn new() -> Self {
        let targets = adapter_profile::yahoo_web_mail_targets();
        let body_present = targets
            .iter()
            .any(|target| target.name == "body" && target.selector.kind == SelectorKind::BodyInput);
        let send_present = targets.iter().any(|target| {
            target.name == "Send" && target.selector.kind == SelectorKind::SendButton
        });

        Self {
            page: Mutex::new(YahooPage {
                body_present,
                send_present,
                draft_body: String::new(),
                sent_messages: Vec::new(),
            }),
            placements: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            last_control_refusal: Mutex::new(None),
        }
    }

    fn placement_count(&self) -> usize {
        self.placements.load(Ordering::SeqCst)
    }

    fn commit_count(&self) -> usize {
        self.commits.load(Ordering::SeqCst)
    }

    fn sent_message_count(&self) -> usize {
        self.page.lock().unwrap().sent_messages.len()
    }

    fn first_sent_message(&self) -> Option<String> {
        self.page.lock().unwrap().sent_messages.first().cloned()
    }

    fn set_send_present(&self, present: bool) {
        self.page.lock().unwrap().send_present = present;
    }

    fn last_control_refusal(&self) -> Option<WebPageControlRefusal> {
        *self.last_control_refusal.lock().unwrap()
    }
}

fn yahoo_profile() -> adapter_profile::ProfilePayload {
    adapter_profile::ProfilePayload {
        domain: "osl/adapter-profile/v1".into(),
        schema_version: 1,
        adapter_id: "yahoo.web".into(),
        app: adapter_profile::AppDescriptor {
            stable_id: "yahoo".into(),
            display_name: "Yahoo Mail".into(),
            service_family: "email".into(),
            min_app_version: None,
        },
        revision: adapter_profile::ProfileRevision {
            number: 1,
            label: "task-1251".into(),
        },
        issued_at_unix_seconds: 1,
        expires_at_unix_seconds: u64::MAX,
        support: adapter_profile::SupportLevel::Supported,
        authority: adapter_profile::AuthorityRequirements {
            user_consent_required: true,
            account_binding_required: true,
            release_authority_required: true,
            harmless_canary_required: true,
        },
        selectors: adapter_profile::yahoo_web_mail_targets()
            .into_iter()
            .map(|target| target.selector)
            .collect(),
        fallbacks: vec![],
        canary: adapter_profile::HarmlessCanary {
            selector: adapter_profile::SelectorKind::AppRoot,
            expected_text: "Yahoo Mail".into(),
            max_age_seconds: 1,
        },
    }
}

fn yahoo_binding(generation: u64) -> SurfaceBinding {
    SurfaceBinding::for_claimed_surface(
        AdapterService::Yahoo,
        generation,
        BindingEvidence::Accessibility {
            tree: A11yTree::WebAx,
        },
        NodeRef::for_claimed_node(1251),
        Some(NodeRef::for_claimed_node(1252)),
        Bounds {
            x: 0,
            y: 0,
            width: 100,
            height: 20,
        },
        1,
        "yahoo-scope",
    )
}

impl WebSurfaceBackend for YahooFixture {
    fn capabilities(&self, _: &adapter_profile::ProfilePayload, _: u64) -> CapabilitySet {
        [
            Capability::PlaceProtectedPayload,
            Capability::SendProtectedPayload,
        ]
        .into_iter()
        .collect()
    }

    fn is_current_generation(&self, generation: u64) -> bool {
        generation == 7
    }

    fn wake_accessibility(&self) -> Result<(), AdapterRefusal> {
        Ok(())
    }

    fn locate(
        &self,
        _: &adapter_profile::ProfilePayload,
        target: &SurfaceTarget,
    ) -> Result<SurfaceBinding, AdapterRefusal> {
        Ok(yahoo_binding(target.generation))
    }

    fn read_state(&self, _: &SurfaceBinding) -> Result<SurfaceState, AdapterRefusal> {
        Ok(SurfaceState {
            composer_text_sha256: "a".repeat(64),
            composer_is_empty: self.page.lock().unwrap().draft_body.is_empty(),
            composer_is_password_field: false,
            focused: true,
            occluded: false,
            read_was_complete: true,
        })
    }

    fn page_controls(
        &self,
        _: &adapter_profile::ProfilePayload,
        _: &SurfaceBinding,
    ) -> WebPageControls {
        let page = self.page.lock().unwrap();
        let controls = WebPageControls {
            body_present: page.body_present,
            send_present: page.send_present,
        };
        *self.last_control_refusal.lock().unwrap() = controls.validate().err();
        controls
    }

    fn destination(&self, b: &SurfaceBinding) -> Result<DestinationIdentity, AdapterRefusal> {
        Ok(DestinationIdentity {
            status: DestinationStatus::Attested,
            account_digest: "a".repeat(64),
            conversation_digest: "b".repeat(64),
            recipients_digest: "c".repeat(64),
            scope_binding_hash: "yahoo-scope".into(),
            evidence: b.evidence.clone(),
            attested_at_ms: 1,
            ttl_ms: 1,
        })
    }

    fn place(&self, _: &SurfaceBinding, carrier: &Carrier) -> PlacementReceipt {
        self.placements.fetch_add(1, Ordering::SeqCst);
        self.page.lock().unwrap().draft_body = carrier.0.clone();
        PlacementReceipt {
            status: PlacementStatus::Placed,
            placed_sha256: Some("d".repeat(64)),
            elapsed_ms: 1,
        }
    }

    fn commit(&self, _: &SurfaceBinding, _: &PlacementReceipt) -> SendReceipt {
        self.commits.fetch_add(1, Ordering::SeqCst);
        let mut page = self.page.lock().unwrap();
        let draft = page.draft_body.clone();
        page.sent_messages.push(draft);
        SendReceipt {
            outcome: SendOutcome::Sent,
            elapsed_ms: 1,
        }
    }

    fn paint_targets(&self, _: &SurfaceBinding) -> Result<Vec<PaintTarget>, AdapterRefusal> {
        Ok(vec![])
    }
}

#[test]
fn task_1251_yahoo_send_missing_refuses_second_draft_without_touching_first_sent_message() {
    let adapter =
        WebSurfaceAdapter::new(AdapterService::Yahoo, yahoo_profile(), YahooFixture::new());
    let target = SurfaceTarget {
        app: AdapterService::Yahoo,
        surface: AdapterSurface::FixedOfficialWebOrigin,
        generation: 7,
    };
    let binding = adapter.locate(&target).unwrap();

    let sent_before = adapter.backend().sent_message_count();
    println!("TASK1251_YAHOO_SENT_MESSAGE_COUNT_BEFORE={sent_before}");
    assert_eq!(sent_before, 0);

    let first_place = adapter.place(
        &binding,
        &PlacementAuthorization::for_scope("yahoo-scope"),
        &Carrier("MAPLE-4172".into()),
    );
    assert_eq!(first_place.status, PlacementStatus::Placed);

    let first_send = adapter.commit(
        &binding,
        &SendAuthorization::for_scope("yahoo-scope"),
        &first_place,
    );
    assert_eq!(first_send.outcome, SendOutcome::Sent);
    let sent_after_first = adapter.backend().sent_message_count();
    let first_sent_message = adapter.backend().first_sent_message().unwrap();
    println!("TASK1251_YAHOO_FIRST_RESULT=sent {first_sent_message}");
    println!("TASK1251_YAHOO_SENT_MESSAGE_COUNT_AFTER_FIRST={sent_after_first}");
    assert_eq!(sent_after_first, 1);
    assert_eq!(first_sent_message, "MAPLE-4172");

    adapter.backend().set_send_present(false);
    let second_place = adapter.place(
        &binding,
        &PlacementAuthorization::for_scope("yahoo-scope"),
        &Carrier("MAPLE-4172-BLOCKED".into()),
    );
    assert_eq!(second_place.status, PlacementStatus::NotPlaced);
    let refusal = adapter.backend().last_control_refusal().unwrap();
    assert_eq!(refusal, WebPageControlRefusal::MissingSend);
    println!(
        "TASK1251_YAHOO_MISSING_SEND_REFUSAL=Yahoo Send missing before placement refusal={refusal}"
    );

    let second_send = adapter.commit(
        &binding,
        &SendAuthorization::for_scope("yahoo-scope"),
        &second_place,
    );
    assert_eq!(second_send.outcome, SendOutcome::NotSent);

    let sent_after_missing_send = adapter.backend().sent_message_count();
    let first_after_missing_send = adapter.backend().first_sent_message().unwrap();
    println!("TASK1251_YAHOO_SECOND_DRAFT_TRIED=MAPLE-4172-BLOCKED");
    println!("TASK1251_YAHOO_FIRST_MESSAGE_AFTER_MISSING_SEND={first_after_missing_send}");
    println!("TASK1251_YAHOO_SENT_MESSAGE_COUNT_AFTER_MISSING_SEND={sent_after_missing_send}");
    println!(
        "TASK1251_YAHOO_PLACEMENT_COUNT_AFTER_MISSING_SEND={}",
        adapter.backend().placement_count()
    );
    println!(
        "TASK1251_YAHOO_COMMIT_COUNT_AFTER_MISSING_SEND={}",
        adapter.backend().commit_count()
    );

    assert_eq!(first_after_missing_send, "MAPLE-4172");
    assert_eq!(sent_after_missing_send, 1);
    assert_eq!(adapter.backend().placement_count(), 1);
    assert_eq!(adapter.backend().commit_count(), 1);
}
