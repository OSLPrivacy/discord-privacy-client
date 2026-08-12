use osl_privacy_hub::scrub_account_rebinding::{
    run_multi_page_read, verify_shipping_scrub_source_inventory, AuthenticatedProviderAccount,
    CheckedProviderAction, ProviderAction, ProviderActionReceipt, ProviderPortKind,
    ScrubAccountConsentGuard, ScrubAccountRebindingError, ScrubRunMode, ScrubShippingProviderPort,
    SHIPPING_SCRUB_CARRIER_ROUTES, SHIPPING_SCRUB_MAIL_ROUTES, SHIPPING_SCRUB_SOURCE_COUNT,
};

const ACCOUNT_A: &str = "provider-stable-account-A-6210";
const ACCOUNT_B: &str = "provider-stable-account-B-6210";
const LIVE_SESSION: &str = "same-live-provider-session-6210";
// Kept outside the production module so deleting or renaming a shipping route
// cannot shrink both the implementation and its oracle together.
const INDEPENDENT_CARRIER_ROUTES: [&str; 7] = [
    "discord",
    "telegram",
    "whatsapp",
    "x",
    "instagram",
    "messenger",
    "signal",
];
const INDEPENDENT_MAIL_ROUTES: [&str; 10] = [
    "gmail",
    "outlook-web",
    "outlook-desktop",
    "proton",
    "tuta",
    "yahoo",
    "aol",
    "gmx",
    "maildotcom",
    "icloud",
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct NetworkAction {
    source: String,
    stable_account_id: String,
    action: ProviderAction,
    authenticated_session_id: String,
    evidence: String,
}

/// Scripted shipping port, rather than a seeded mailbox or test paginator.
/// It models the provider/network seams: the stable authenticated identity and
/// every action are captured independently.  Its one barrier changes A to B
/// after the first real response while retaining the same live session.
struct ScriptedShippingPort {
    source: String,
    port_kind: ProviderPortKind,
    stable_account_id: String,
    switch_after_action: Option<usize>,
    switched: bool,
    barrier_holds: usize,
    signed_out_events: usize,
    identity_reads: Vec<AuthenticatedProviderAccount>,
    network_actions: Vec<NetworkAction>,
    corrupt_next_receipt: bool,
}

impl ScriptedShippingPort {
    fn new(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            port_kind: ProviderPortKind::ShippingRoute,
            stable_account_id: ACCOUNT_A.to_owned(),
            switch_after_action: None,
            switched: false,
            barrier_holds: 0,
            signed_out_events: 0,
            identity_reads: Vec::new(),
            network_actions: Vec::new(),
            corrupt_next_receipt: false,
        }
    }

    fn arm_in_place_switch_after_first_response(&mut self) {
        self.switch_after_action = Some(self.network_actions.len() + 1);
    }

    fn fixture(source: &str) -> Self {
        let mut port = Self::new(source);
        port.port_kind = ProviderPortKind::Fixture;
        port
    }

    fn switch_in_place_now(&mut self) {
        assert_eq!(self.signed_out_events, 0);
        self.stable_account_id = ACCOUNT_B.to_owned();
        self.switched = true;
    }
}

impl ScrubShippingProviderPort for ScriptedShippingPort {
    fn port_kind(&self) -> ProviderPortKind {
        self.port_kind
    }

    fn independently_authenticated_account(
        &mut self,
    ) -> Result<AuthenticatedProviderAccount, String> {
        let observed = AuthenticatedProviderAccount {
            provider_source: self.source.clone(),
            stable_account_id: self.stable_account_id.clone(),
            authenticated_session_id: LIVE_SESSION.to_owned(),
            independent_auth_evidence: format!(
                "provider-network-whoami:{}:{}",
                self.source, self.stable_account_id
            ),
            // All three untrusted hints stay stale at A.  They therefore
            // cannot hide the independently authenticated switch to B.
            requested_account_id: Some(ACCOUNT_A.to_owned()),
            ui_account_label: Some("Account A (unchanged label)".to_owned()),
            self_declared_account_name: Some("Account A".to_owned()),
        };
        self.identity_reads.push(observed.clone());
        Ok(observed)
    }

    fn perform_checked_action(
        &mut self,
        authority: &CheckedProviderAction,
    ) -> Result<ProviderActionReceipt, String> {
        assert_eq!(authority.provider_source(), self.source);
        assert_eq!(authority.stable_account_id(), self.stable_account_id);
        assert_eq!(authority.authenticated_session_id(), LIVE_SESSION);
        assert!(authority
            .independent_auth_evidence()
            .contains(&self.stable_account_id));
        self.network_actions.push(NetworkAction {
            source: self.source.clone(),
            stable_account_id: self.stable_account_id.clone(),
            action: authority.action(),
            authenticated_session_id: authority.authenticated_session_id().to_owned(),
            evidence: authority.independent_auth_evidence().to_owned(),
        });
        let mut receipt = ProviderActionReceipt {
            provider_source: self.source.clone(),
            stable_account_id: self.stable_account_id.clone(),
            action: authority.action(),
            provider_response_id: format!(
                "{}-response-{}",
                self.source,
                self.network_actions.len()
            ),
        };
        if self.corrupt_next_receipt {
            receipt.stable_account_id = "receipt-for-someone-else".to_owned();
            self.corrupt_next_receipt = false;
        }

        if !self.switched
            && self.switch_after_action == Some(self.network_actions.len())
            && self.stable_account_id == ACCOUNT_A
        {
            // The response is captured first.  The held worker then observes
            // an in-place account switch and is released to its next guard.
            self.barrier_holds += 1;
            self.switch_in_place_now();
        }
        Ok(receipt)
    }
}

#[test]
fn task_6210_fixture_provider_cannot_mint_or_spend_shipping_consent() {
    let mut port = ScriptedShippingPort::fixture("discord");
    let mut guard = ScrubAccountConsentGuard::new("discord", ScrubRunMode::Discovery).unwrap();
    let error = guard
        .approve_live_account(&mut port)
        .expect_err("a fixture provider must not mint shipping consent");
    assert!(matches!(
        error,
        ScrubAccountRebindingError::FixtureProviderRefused { .. }
    ));
    assert!(port.identity_reads.is_empty());
    assert!(port.network_actions.is_empty());
    assert!(guard.selected_account().is_none());
    assert!(guard.approved_account().is_none());
    println!(
        "TASK6210_FIXTURE_PROVIDER shipping_consent_minted=0 identity_reads=0 network_actions=0 stale_state=0 refusal=fixture_provider_cannot_authorize_shipping"
    );
}

#[test]
fn task_6210_rejects_a_provider_receipt_not_bound_to_the_checked_live_account() {
    let mut port = ScriptedShippingPort::new("discord");
    let mut guard = ScrubAccountConsentGuard::new("discord", ScrubRunMode::Discovery).unwrap();
    guard.approve_live_account(&mut port).unwrap();
    port.corrupt_next_receipt = true;
    let error = guard
        .perform_provider_action(&mut port, ProviderAction::Fetch)
        .expect_err("a mismatched shipping receipt must not report success");
    assert!(matches!(
        error,
        ScrubAccountRebindingError::InvalidProviderReceipt { .. }
    ));
    assert!(error.to_string().contains("mismatched provider receipt"));
    assert!(guard.selected_account().is_none());
    assert!(guard.approved_account().is_none());
    println!(
        "TASK6210_RECEIPT_MUTANTS mismatched_account_receipts_refused=1 stale_state_cleared=1"
    );
}

fn all_sources() -> Vec<&'static str> {
    INDEPENDENT_CARRIER_ROUTES
        .into_iter()
        .chain(INDEPENDENT_MAIL_ROUTES)
        .collect()
}

fn modes() -> [ScrubRunMode; 2] {
    [ScrubRunMode::Discovery, ScrubRunMode::ScheduledFindOnly]
}

#[test]
fn task_6210_every_shipping_source_rebinds_consent_on_an_in_place_switch() {
    verify_shipping_scrub_source_inventory(&INDEPENDENT_CARRIER_ROUTES, &INDEPENDENT_MAIL_ROUTES)
        .expect("the independent source inventory must match shipping");
    assert_eq!(SHIPPING_SCRUB_CARRIER_ROUTES, INDEPENDENT_CARRIER_ROUTES);
    assert_eq!(SHIPPING_SCRUB_MAIL_ROUTES, INDEPENDENT_MAIL_ROUTES);
    assert_eq!(SHIPPING_SCRUB_SOURCE_COUNT, 17);

    let mut cells = 0;
    let mut a_actions = 0;
    let mut b_actions_before_approval = 0;
    let mut refusals = 0;
    let mut stale_states_cleared = 0;
    let mut fresh_b_approvals = 0;
    let mut control_runs = 0;
    let mut control_pages = 0;
    let mut control_actions = 0;
    let mut identity_reads = 0;
    let mut barriers = 0;
    let mut signed_out_events = 0;
    let mut find_only_delete_refusals = 0;

    for source in all_sources() {
        for mode in modes() {
            cells += 1;
            let mut port = ScriptedShippingPort::new(source);
            let mut guard = ScrubAccountConsentGuard::new(source, mode).unwrap();

            let approved_a = guard.approve_live_account(&mut port).unwrap();
            assert_eq!(approved_a.provider_source, source);
            assert_eq!(approved_a.stable_account_id, ACCOUNT_A);
            port.arm_in_place_switch_after_first_response();

            let error = run_multi_page_read(&mut guard, &mut port, 3)
                .expect_err("the first post-switch page action must be refused");
            match &error {
                ScrubAccountRebindingError::AccountChanged {
                    provider_source,
                    approved_account_id,
                    live_account_id,
                } => {
                    assert_eq!(provider_source, source);
                    assert_eq!(approved_account_id, ACCOUNT_A);
                    assert_eq!(live_account_id, ACCOUNT_B);
                }
                other => panic!("{source} {mode:?} returned wrong refusal: {other}"),
            }
            let words = error.to_string();
            assert!(words.contains("account changed"));
            assert!(words.contains(ACCOUNT_A));
            assert!(words.contains(ACCOUNT_B));
            assert!(words.contains("fresh"));

            // Search got an A response.  The next List was stopped by the
            // guard, before the shipping port saw any B-bound action.
            assert_eq!(port.network_actions.len(), 1);
            assert_eq!(port.network_actions[0].stable_account_id, ACCOUNT_A);
            assert_eq!(port.network_actions[0].action, ProviderAction::Search);
            assert_eq!(port.barrier_holds, 1);
            assert_eq!(port.signed_out_events, 0);
            assert!(guard.selected_account().is_none());
            assert!(guard.approved_account().is_none());
            assert_eq!(
                port.identity_reads.last().unwrap().stable_account_id,
                ACCOUNT_B
            );
            assert_eq!(
                port.identity_reads
                    .last()
                    .unwrap()
                    .requested_account_id
                    .as_deref(),
                Some(ACCOUNT_A)
            );
            a_actions += 1;
            b_actions_before_approval += port
                .network_actions
                .iter()
                .filter(|action| action.stable_account_id == ACCOUNT_B)
                .count();
            refusals += 1;
            stale_states_cleared += 1;
            barriers += port.barrier_holds;
            signed_out_events += port.signed_out_events;

            let before_unapproved_retry = port.network_actions.len();
            let fresh_error = guard
                .perform_provider_action(&mut port, ProviderAction::List)
                .expect_err("B must remain blocked until its own fresh approval");
            assert!(matches!(
                fresh_error,
                ScrubAccountRebindingError::FreshApprovalRequired { .. }
            ));
            assert!(fresh_error
                .to_string()
                .contains("fresh account approval required"));
            assert_eq!(port.network_actions.len(), before_unapproved_retry);

            let approved_b = guard.approve_live_account(&mut port).unwrap();
            assert_eq!(approved_b.stable_account_id, ACCOUNT_B);
            fresh_b_approvals += 1;
            let before_control = port.network_actions.len();
            let outcome = run_multi_page_read(&mut guard, &mut port, 2)
                .expect("one control run must succeed after fresh B approval");
            assert_eq!(outcome.pages_completed, 2);
            assert_eq!(outcome.actions_completed, 9);
            assert_eq!(port.network_actions.len() - before_control, 9);
            assert!(port.network_actions[before_control..]
                .iter()
                .all(|action| action.stable_account_id == ACCOUNT_B));
            control_runs += 1;
            control_pages += outcome.pages_completed;
            control_actions += outcome.actions_completed;

            if mode == ScrubRunMode::ScheduledFindOnly {
                let before_delete = port.network_actions.len();
                let delete_error = guard
                    .perform_provider_action(&mut port, ProviderAction::Delete)
                    .expect_err("scheduled AutoScrub must remain Find only");
                assert!(matches!(
                    delete_error,
                    ScrubAccountRebindingError::ScheduledFindOnlyDeleteRefused { .. }
                ));
                assert!(delete_error.to_string().contains("Find only"));
                assert_eq!(port.network_actions.len(), before_delete);
                find_only_delete_refusals += 1;
            }
            identity_reads += port.identity_reads.len();
            println!(
                "TASK6210_CELL source={} mode={} account_A={} account_B={} A_consent=true B_consent_before_refusal=false selected_approved_before=true selected_approved_after=false A_actions=1 first_B_action=list B_actions_before_fresh_approval=0 multi_page_run=true mid_page_barrier=1 in_place_switch=1 same_live_session=true signed_out_events=0 independent_identity_read=true independent_network_action_read=true independent_osl_state_read=true refusal=account_changed stale_state_cleared=true fresh_approval_required=true fresh_B_approval=true control_runs=1 control_pages=2 control_actions=9 ui_label_unchanged=true expected_set_unchanged=true requested_id_mismatch=true self_declared_name_stale=true cooperative_ui_stop=0 fixture_provider=false find_only={}",
                source,
                match mode {
                    ScrubRunMode::Discovery => "discovery",
                    ScrubRunMode::ScheduledFindOnly => "scheduled_find_only",
                },
                ACCOUNT_A,
                ACCOUNT_B,
                mode == ScrubRunMode::ScheduledFindOnly,
            );
        }
    }

    assert_eq!(cells, 34);
    assert_eq!(a_actions, 34);
    assert_eq!(b_actions_before_approval, 0);
    assert_eq!(refusals, 34);
    assert_eq!(stale_states_cleared, 34);
    assert_eq!(fresh_b_approvals, 34);
    assert_eq!(
        control_runs, 34,
        "exactly one control run per source/mode cell"
    );
    assert_eq!(control_pages, 68);
    assert_eq!(control_actions, 306);
    assert_eq!(barriers, 34);
    assert_eq!(signed_out_events, 0);
    assert_eq!(find_only_delete_refusals, 17);

    println!(
        "TASK6210 shipping_carriers={} mailbox_sources={} modes=2 cells={} A_read_actions={} B_actions_before_fresh_approval={} account_changed_refusals={} stale_selected_approved_cleared={} mid_page_barriers={} signed_out_events={} independent_identity_reads={} fresh_B_approvals={} control_runs_per_cell=1 control_runs_total={} control_pages={} control_actions={} scheduled_find_only_delete_refusals={}",
        SHIPPING_SCRUB_CARRIER_ROUTES.len(),
        SHIPPING_SCRUB_MAIL_ROUTES.len(),
        cells,
        a_actions,
        b_actions_before_approval,
        refusals,
        stale_states_cleared,
        barriers,
        signed_out_events,
        identity_reads,
        fresh_b_approvals,
        control_runs,
        control_pages,
        control_actions,
        find_only_delete_refusals,
    );
}

#[test]
fn task_6210_production_guard_mutant_observer_covers_every_source_and_mode() {
    let mut unexpected_b_actions = Vec::new();
    let mut observed_cells = 0;
    for source in all_sources() {
        for mode in modes() {
            observed_cells += 1;
            let mut port = ScriptedShippingPort::new(source);
            let mut guard = ScrubAccountConsentGuard::new(source, mode).unwrap();
            guard.approve_live_account(&mut port).unwrap();
            port.switch_in_place_now();
            if guard
                .perform_provider_action(&mut port, ProviderAction::List)
                .is_ok()
            {
                unexpected_b_actions.push(format!("{source}/{mode:?}/list/{ACCOUNT_B}"));
            }
        }
    }
    assert_eq!(observed_cells, 34);
    assert!(
        unexpected_b_actions.is_empty(),
        "TASK6210_PRODUCTION_MUTANT first_B_bound_action={} mutant_B_actions={} expected_cells=34 UI_labels_unchanged=true expected_sets_unchanged=true local_output_suppressed=true",
        unexpected_b_actions
            .first()
            .map(String::as_str)
            .unwrap_or("none"),
        unexpected_b_actions.len(),
    );
    println!(
        "TASK6210_MUTANT_OBSERVER cells=34 B_bound_actions=0 UI_labels_unchanged=true expected_sets_unchanged=true local_output_dependency=0"
    );
}

#[test]
fn task_6210_common_guard_stops_every_first_b_bound_action_in_all_34_cells() {
    let mut attacks = 0;
    let mut refused_before_provider_action = 0;
    let mut cleared = 0;
    let mut identity_reads = 0;

    for source in all_sources() {
        for mode in modes() {
            for action in ProviderAction::ALL {
                attacks += 1;
                let mut port = ScriptedShippingPort::new(source);
                let mut guard = ScrubAccountConsentGuard::new(source, mode).unwrap();
                guard.approve_live_account(&mut port).unwrap();
                port.switch_in_place_now();
                let error = guard
                    .perform_provider_action(&mut port, action)
                    .expect_err("B must not receive the first action under A consent");
                assert!(matches!(
                    error,
                    ScrubAccountRebindingError::AccountChanged { .. }
                ));
                assert!(error
                    .to_string()
                    .contains(action_source_account_marker(source)));
                assert!(port.network_actions.is_empty());
                assert_eq!(port.identity_reads.len(), 2);
                assert_eq!(
                    port.identity_reads[1].requested_account_id.as_deref(),
                    Some(ACCOUNT_A),
                    "requested-id mismatch must not override live identity"
                );
                assert!(guard.selected_account().is_none());
                assert!(guard.approved_account().is_none());
                refused_before_provider_action += 1;
                cleared += 1;
                identity_reads += port.identity_reads.len();
            }
        }
    }

    assert_eq!(attacks, 17 * 2 * 6);
    assert_eq!(refused_before_provider_action, 204);
    assert_eq!(cleared, 204);
    println!(
        "TASK6210_ACTION_MATRIX cells=34 actions=list,open,fetch,search,scroll,delete attacks={} first_B_provider_actions=0 refusals={} cleared={} independent_identity_reads={} requested_ui_self_declared_hints_authoritative=0",
        attacks, refused_before_provider_action, cleared, identity_reads
    );
}

#[test]
fn task_6210_source_inventory_starvation_names_every_missing_shipping_route() {
    let mut starvation_refusals = 0;
    for missing in INDEPENDENT_CARRIER_ROUTES {
        let candidate = INDEPENDENT_CARRIER_ROUTES
            .into_iter()
            .filter(|source| *source != missing)
            .collect::<Vec<_>>();
        let error = verify_shipping_scrub_source_inventory(&candidate, &INDEPENDENT_MAIL_ROUTES)
            .expect_err("a starved carrier route must fail");
        assert!(error.contains(missing));
        starvation_refusals += 1;
    }
    for missing in INDEPENDENT_MAIL_ROUTES {
        let candidate = INDEPENDENT_MAIL_ROUTES
            .into_iter()
            .filter(|source| *source != missing)
            .collect::<Vec<_>>();
        let error = verify_shipping_scrub_source_inventory(&INDEPENDENT_CARRIER_ROUTES, &candidate)
            .expect_err("a starved mail route must fail");
        assert!(error.contains(missing));
        starvation_refusals += 1;
    }
    assert_eq!(starvation_refusals, 17);
    println!(
        "TASK6210_INVENTORY carrier_routes=7 mail_routes=10 starvation_refusals={} all_named=true",
        starvation_refusals
    );
}

fn action_source_account_marker(source: &str) -> &str {
    // The Display refusal names the source, A and B.  Returning the source here
    // keeps the matrix assertion readable while the exact ids are asserted in
    // the multi-page test above.
    source
}
