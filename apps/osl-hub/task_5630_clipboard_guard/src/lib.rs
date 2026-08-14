//! Fail-closed verifier for TASK 5630's independently observed Windows trace.
//!
//! The verifier has no numeric clipboard-format or release-route allow-list.
//! Those inventories come from API/event tracing of the installed package.

use std::collections::{BTreeMap, BTreeSet};

pub mod trace;

pub const POST_QUIESCENCE_MS: u64 = 10 * 60 * 1_000;
pub const RECOVERY_DEADLINE_MS: u64 = 5_000;
pub const MAX_OBSERVATION_GAP_MS: u64 = 5_000;

/// Process names whose death must be proved between the first marked write and
/// the restore. The clipboard broker is the foreground peer of the hub process.
pub const REQUIRED_KILLS: [&str; 2] = ["osl", "osl-clipboard-broker"];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FormatClass {
    Unicode,
    Html,
    Rtf,
    Dib,
    FileDrop,
    Locale,
    AppPrivate,
    DelayedRender,
    Other,
}

impl FormatClass {
    pub const REQUIRED: [Self; 8] = [
        Self::Unicode,
        Self::Html,
        Self::Rtf,
        Self::Dib,
        Self::FileDrop,
        Self::Locale,
        Self::AppPrivate,
        Self::DelayedRender,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Unicode => "Unicode text",
            Self::Html => "HTML",
            Self::Rtf => "RTF",
            Self::Dib => "DIB bitmap",
            Self::FileDrop => "file-drop",
            Self::Locale => "locale",
            Self::AppPrivate => "registered app-private",
            Self::DelayedRender => "delayed-render",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatState {
    pub id: u32,
    pub name: String,
    pub class: FormatClass,
    pub bytes: Vec<u8>,
    pub delayed_obligation: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerState {
    pub window: u64,
    pub process_id: u32,
    pub thread_id: u32,
    pub behavior: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardInventory {
    pub runtime_discovered: bool,
    pub formats: Vec<FormatState>,
    pub owner: OwnerState,
    pub sequence: u32,
    pub history_metadata: Vec<u8>,
    pub cloud_metadata: Vec<u8>,
    pub complete: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Sink {
    Clipboard,
    Model(String),
    Writer(String),
}

impl Sink {
    fn label(&self) -> String {
        match self {
            Self::Clipboard => "clipboard".to_owned(),
            Self::Model(value) => format!("model:{value}"),
            Self::Writer(value) => format!("writer:{value}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateRepresentation {
    Plaintext,
    Encoding(String),
    Hash(String),
    Tokenization,
    Embedding,
    Compression(String),
    LengthPayload,
    Other(String),
}

impl PrivateRepresentation {
    fn label(&self) -> String {
        match self {
            Self::Plaintext => "plaintext".to_owned(),
            Self::Encoding(value) => format!("encoding:{value}"),
            Self::Hash(value) => format!("hash:{value}"),
            Self::Tokenization => "tokenization".to_owned(),
            Self::Embedding => "embedding".to_owned(),
            Self::Compression(value) => format!("compression:{value}"),
            Self::LengthPayload => "length-derived-payload".to_owned(),
            Self::Other(value) => format!("other:{value}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueOrigin {
    IndependentCover,
    PrivateDerived(PrivateRepresentation),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Actor {
    IndependentObserver,
    PlacementRoute,
    OutsideAdversary,
    RecoveryService,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventKind {
    InventoryComplete,
    ClipboardRead,
    ClipboardWrite,
    ModelInput,
    WriterInput,
    PlacementTouch,
    CarrierConsumed { cover: String },
    ProcessKilled { process: String },
    WindowsRestarted,
    CleanupObservation,
    /// The route's first write of the marked cover into the live clipboard.
    MarkedWrite,
    /// The recovery journal reached durable storage outside the OSL process.
    JournalCommit,
    /// An outside process published rich state under the given label (`B`).
    OutsideRichWrite { state: String },
    /// The automatic recovery service began its restore attempt.
    RecoveryStart,
    /// The automatic recovery service finished restoring.
    RestoreComplete,
    /// A person relaunched something by hand. Its presence is disqualifying.
    UserRelaunch,
    /// The recovery service process itself was killed.
    ServiceKilled,
    /// Post-reboot recovery ran before any placement.
    StartupRecovery,
    /// The first local cleanup result was reported.
    FirstCleanupResult,
    /// A marked value became visible on the named endpoint.
    MarkedPublication { endpoint: String },
}

/// Which contender takes its lock first when recovery and an outside rich
/// writer race. Both orders have to be observed for the race to be proved.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LockOrder {
    RecoveryThenWriter,
    WriterThenRecovery,
}

impl LockOrder {
    pub const BOTH: [Self; 2] = [Self::RecoveryThenWriter, Self::WriterThenRecovery];

    pub fn label(self) -> &'static str {
        match self {
            Self::RecoveryThenWriter => "recovery-then-writer",
            Self::WriterThenRecovery => "writer-then-recovery",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    pub ordinal: u64,
    pub real_time_ms: u64,
    pub name: String,
    pub actor: Actor,
    pub kind: EventKind,
    pub sink: Option<Sink>,
    pub origin: Option<ValueOrigin>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointSample {
    pub real_time_ms: u64,
    pub queue_depth: usize,
    pub current_marked: usize,
    pub history_marked: usize,
    pub cloud_marked: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndpointObservation {
    pub endpoint: String,
    pub source: bool,
    pub already_synchronized: bool,
    pub samples: Vec<EndpointSample>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryJournal {
    pub transaction: String,
    pub durable_outside_osl: bool,
    pub fsynced_before_first_write: bool,
    pub authenticated: bool,
    pub os_protected: bool,
    pub complete_snapshot: bool,
    pub expected_owner_sequence: bool,
    pub phase_and_cleanup: bool,
    pub erased_after_two_endpoint_quiescence: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipboardRecoveryProof {
    pub journal: RecoveryJournal,
    pub service_registered_before_write: bool,
    pub osl_and_broker_killed_after_consumption: bool,
    pub automatic_without_person: bool,
    pub restored_within_ms: u64,
    pub service_killed_and_windows_restarted: bool,
    pub startup_recovery_before_placement: bool,
    pub locked_owner_sequence_revalidation: bool,
    pub concurrent_b_written_after_journal: bool,
    pub exact_b_preserved: bool,
    pub stale_a_restored_over_b: bool,
    /// Which side won the lock in this run. Both orders must appear across the
    /// campaign or the contenders were serialized.
    pub lock_order: LockOrder,
    /// Exact positive-control bytes handed to the recovery service.
    pub control_value: Vec<u8>,
    /// Exact bytes read back off the clipboard after the restore.
    pub restored_value: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteKind {
    NonClipboard,
    Clipboard,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteObservation {
    pub transaction: String,
    pub route: String,
    pub kind: RouteKind,
    pub cover: String,
    pub inventory_before: ClipboardInventory,
    pub inventory_after: ClipboardInventory,
    pub sink_inventory_runtime_discovered: bool,
    pub sinks: Vec<Sink>,
    pub events: Vec<Event>,
    pub endpoints: Vec<EndpointObservation>,
    pub recovery: Option<ClipboardRecoveryProof>,
    pub real_clock: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteReceipt {
    pub route: String,
    pub formats: usize,
    pub sinks: usize,
    pub clipboard_route_events: usize,
    pub private_derived_values: usize,
    pub endpoints: usize,
    pub observed_ms: u64,
}

pub fn verify_route(observation: &RouteObservation) -> Result<RouteReceipt, String> {
    let label = format!(
        "route={} transaction={}",
        observation.route, observation.transaction
    );
    require_inventory(&label, &observation.inventory_before)?;
    require_inventory(&label, &observation.inventory_after)?;
    reconcile_inventory(
        &label,
        &observation.inventory_before,
        &observation.inventory_after,
    )?;
    if !observation.sink_inventory_runtime_discovered || observation.sinks.is_empty() {
        return Err(format!("{label}: empty or fixture sink inventory"));
    }
    let sinks = observation.sinks.iter().cloned().collect::<BTreeSet<_>>();
    if sinks.len() != observation.sinks.len() {
        return Err(format!("{label}: duplicate runtime sink inventory"));
    }
    let inventory_ordinal = observation
        .events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::InventoryComplete))
        .map(|event| event.ordinal)
        .min()
        .ok_or_else(|| format!("{label}: missing independent inventory event"))?;
    if let Some(event) = observation.events.iter().find(|event| {
        event.actor == Actor::PlacementRoute
            && matches!(event.kind, EventKind::PlacementTouch)
            && event.ordinal <= inventory_ordinal
    }) {
        return Err(format!(
            "{label}: earlier placement touch event={}",
            event.name
        ));
    }
    let mut route_clipboard_events = 0;
    let mut private_values = 0;
    for event in &observation.events {
        if matches!(event.origin, Some(ValueOrigin::PrivateDerived(_))) {
            private_values += 1;
        }
        if let Some(sink) = &event.sink {
            if !sinks.contains(sink) {
                return Err(format!(
                    "{label}: event={} undiscovered sink={}",
                    event.name,
                    sink.label()
                ));
            }
        }
        if event.actor == Actor::PlacementRoute
            && matches!(
                event.kind,
                EventKind::ClipboardRead | EventKind::ClipboardWrite
            )
        {
            route_clipboard_events += 1;
        }
        if let (Some(sink), Some(ValueOrigin::PrivateDerived(value))) = (&event.sink, &event.origin)
        {
            if matches!(sink, Sink::Clipboard | Sink::Model(_) | Sink::Writer(_)) {
                return Err(format!(
                    "{label}: private-derived {} event={} sink={}",
                    value.label(),
                    event.name,
                    sink.label()
                ));
            }
        }
    }
    let consumptions = observation.events.iter().filter(|event| {
        event.actor == Actor::IndependentObserver
            && matches!(&event.kind, EventKind::CarrierConsumed { cover } if cover == &observation.cover)
    }).count();
    if consumptions != 1 {
        return Err(format!(
            "{label}: carrier consumed exact cover {} times",
            consumptions
        ));
    }
    match observation.kind {
        RouteKind::NonClipboard => {
            if route_clipboard_events != 0 {
                return Err(format!("{label}: non-clipboard route emitted {route_clipboard_events} clipboard events"));
            }
            if observation.recovery.is_some() {
                return Err(format!(
                    "{label}: non-clipboard route fabricated recovery interval"
                ));
            }
        }
        RouteKind::Clipboard => {
            verify_recovery(&label, observation.recovery.as_ref(), &observation.events)?
        }
    }
    verify_publication_order(&label, &observation.events)?;
    let observed_ms = verify_endpoints(&label, &observation.endpoints, observation.real_clock)?;
    Ok(RouteReceipt {
        route: observation.route.clone(),
        formats: observation.inventory_before.formats.len(),
        sinks: observation.sinks.len(),
        clipboard_route_events: route_clipboard_events,
        private_derived_values: private_values,
        endpoints: observation.endpoints.len(),
        observed_ms,
    })
}

fn require_inventory(label: &str, inventory: &ClipboardInventory) -> Result<(), String> {
    if !inventory.runtime_discovered || !inventory.complete || inventory.formats.is_empty() {
        return Err(format!(
            "{label}: empty, fixture, or incomplete clipboard inventory"
        ));
    }
    let mut ids = BTreeSet::new();
    let classes = inventory
        .formats
        .iter()
        .map(|format| format.class)
        .collect::<BTreeSet<_>>();
    for format in &inventory.formats {
        if !ids.insert(format.id) {
            return Err(format!(
                "{label}: duplicate format={} name={}",
                format.id, format.name
            ));
        }
    }
    for class in FormatClass::REQUIRED {
        if !classes.contains(&class) {
            return Err(format!("{label}: missing format class={}", class.label()));
        }
    }
    Ok(())
}

fn reconcile_inventory(
    label: &str,
    before: &ClipboardInventory,
    after: &ClipboardInventory,
) -> Result<(), String> {
    if before.owner != after.owner {
        return Err(format!("{label}: clipboard owner behavior changed"));
    }
    if before.sequence != after.sequence {
        return Err(format!(
            "{label}: sequence {} -> {}",
            before.sequence, after.sequence
        ));
    }
    if before.history_metadata != after.history_metadata {
        return Err(format!("{label}: history metadata changed"));
    }
    if before.cloud_metadata != after.cloud_metadata {
        return Err(format!("{label}: cloud metadata changed"));
    }
    let observed = after
        .formats
        .iter()
        .map(|format| (format.id, format))
        .collect::<BTreeMap<_, _>>();
    for format in &before.formats {
        match observed.get(&format.id) {
            None => {
                return Err(format!(
                    "{label}: omitted format={} name={}",
                    format.id, format.name
                ))
            }
            Some(value) if *value != format => {
                return Err(format!(
                    "{label}: changed format={} name={}",
                    format.id, format.name
                ))
            }
            Some(_) => {}
        }
    }
    if before.formats.len() != after.formats.len() {
        return Err(format!(
            "{label}: format count {} -> {}",
            before.formats.len(),
            after.formats.len()
        ));
    }
    Ok(())
}

fn verify_recovery(label: &str, proof: Option<&ClipboardRecoveryProof>) -> Result<(), String> {
    let proof = proof
        .ok_or_else(|| format!("{label}: clipboard route omitted kill/restart recovery proof"))?;
    let journal = &proof.journal;
    if !(journal.durable_outside_osl
        && journal.fsynced_before_first_write
        && journal.authenticated
        && journal.os_protected
        && journal.complete_snapshot
        && journal.expected_owner_sequence
        && journal.phase_and_cleanup)
    {
        return Err(format!("{label}: transaction={} journal not durable/fsynced/authenticated/complete before first write", journal.transaction));
    }
    if !proof.service_registered_before_write
        || !proof.osl_and_broker_killed_after_consumption
        || !proof.automatic_without_person
    {
        return Err(format!(
            "{label}: transaction={} automatic recovery service/kill proof missing",
            journal.transaction
        ));
    }
    if proof.restored_within_ms > RECOVERY_DEADLINE_MS {
        return Err(format!(
            "{label}: transaction={} recovery deadline={}ms observed={}ms",
            journal.transaction, RECOVERY_DEADLINE_MS, proof.restored_within_ms
        ));
    }
    if !proof.service_killed_and_windows_restarted || !proof.startup_recovery_before_placement {
        return Err(format!(
            "{label}: transaction={} service death/startup recovery missing",
            journal.transaction
        ));
    }
    if !(proof.locked_owner_sequence_revalidation
        && proof.concurrent_b_written_after_journal
        && proof.exact_b_preserved)
        || proof.stale_a_restored_over_b
    {
        return Err(format!(
            "{label}: transaction={} locked revalidation failed; stale A-over-B restore",
            journal.transaction
        ));
    }
    if !journal.erased_after_two_endpoint_quiescence {
        return Err(format!(
            "{label}: transaction={} journal erased early or retained after quiescence",
            journal.transaction
        ));
    }
    Ok(())
}

fn verify_endpoints(
    label: &str,
    endpoints: &[EndpointObservation],
    real_clock: bool,
) -> Result<u64, String> {
    if !real_clock {
        return Err(format!(
            "{label}: shortened or accelerated observation clock"
        ));
    }
    if endpoints.len() < 2 {
        return Err(format!("{label}: one endpoint only"));
    }
    if endpoints.iter().filter(|endpoint| endpoint.source).count() != 1 {
        return Err(format!("{label}: source endpoint inventory invalid"));
    }
    if !endpoints
        .iter()
        .any(|endpoint| !endpoint.source && endpoint.already_synchronized)
    {
        return Err(format!("{label}: synchronized second endpoint missing"));
    }
    let mut names = BTreeSet::new();
    let mut observed_ms = u64::MAX;
    for endpoint in endpoints {
        if !names.insert(&endpoint.endpoint) || endpoint.samples.is_empty() {
            return Err(format!(
                "{label}: endpoint={} empty or duplicate",
                endpoint.endpoint
            ));
        }
        let samples = &endpoint.samples;
        let quiescent_at = samples
            .iter()
            .find(|sample| sample.queue_depth == 0)
            .ok_or_else(|| {
                format!(
                    "{label}: endpoint={} propagation queue never empty",
                    endpoint.endpoint
                )
            })?
            .real_time_ms;
        for pair in samples.windows(2) {
            let gap = pair[1].real_time_ms.saturating_sub(pair[0].real_time_ms);
            if gap > MAX_OBSERVATION_GAP_MS {
                return Err(format!(
                    "{label}: endpoint={} observation gap={}ms",
                    endpoint.endpoint, gap
                ));
            }
        }
        for sample in samples
            .iter()
            .filter(|sample| sample.real_time_ms >= quiescent_at)
        {
            if sample.queue_depth != 0
                || sample.current_marked != 0
                || sample.history_marked != 0
                || sample.cloud_marked != 0
            {
                return Err(format!(
                    "{label}: transaction late marked publication endpoint={} at={} queue={} current={} history={} cloud={}",
                    endpoint.endpoint, sample.real_time_ms, sample.queue_depth, sample.current_marked,
                    sample.history_marked, sample.cloud_marked
                ));
            }
        }
        let duration = samples
            .last()
            .unwrap()
            .real_time_ms
            .saturating_sub(quiescent_at);
        if duration < POST_QUIESCENCE_MS {
            return Err(format!(
                "{label}: endpoint={} shortened observation={}ms required={}ms",
                endpoint.endpoint, duration, POST_QUIESCENCE_MS
            ));
        }
        observed_ms = observed_ms.min(duration);
    }
    Ok(observed_ms)
}

pub fn verify_release_campaign(
    runtime_discovered: bool,
    routes: &[RouteObservation],
) -> Result<Vec<RouteReceipt>, String> {
    if !runtime_discovered || routes.is_empty() {
        return Err("release route inventory empty or fixture".to_owned());
    }
    let mut names = BTreeSet::new();
    let mut receipts = Vec::with_capacity(routes.len());
    for route in routes {
        if !names.insert(route.route.clone()) {
            return Err(format!("duplicate release route={}", route.route));
        }
        receipts.push(verify_route(route)?);
    }
    Ok(receipts)
}
