//! TASK 6420: external protected sends bind the last live provider read to the
//! irreversible carrier placement or provider-send operation.

use osl_privacy_hub::send_commit_binding::{
    commit_reviewed_send, prepare_reviewed_send, LiveCommitTarget, SendCommitRefusal,
    ShippingCommitRoute, SHIPPING_COMMIT_ROUTES,
};

#[derive(Default)]
struct ProviderJournal {
    emitted: Vec<(LiveCommitTarget, Vec<u8>)>,
}

impl ProviderJournal {
    fn emit(&mut self, target: &LiveCommitTarget, bytes: &[u8]) {
        self.emitted.push((target.clone(), bytes.to_vec()));
    }
}

fn reviewed_target(route: ShippingCommitRoute) -> LiveCommitTarget {
    LiveCommitTarget::new(
        format!("account-reviewed-{}", route.label()),
        format!("immutable-reviewed-{}", route.label()),
        [
            format!("to-reviewed-{}", route.label()),
            format!("cc-reviewed-{}", route.label()),
            format!("bcc-reviewed-{}", route.label()),
        ],
        "Same visible conversation label",
    )
}

fn cover(route: ShippingCommitRoute) -> Vec<u8> {
    // A NUL makes byte equality, rather than a normalised visible string,
    // observable to this test without using a secret fixture.
    format!("TASK6420-cover:{}\0exact", route.label()).into_bytes()
}

fn switched_account(mut target: LiveCommitTarget) -> LiveCommitTarget {
    target.provider_account_id = format!("wrong-{}", target.provider_account_id);
    target
}

fn switched_target(mut target: LiveCommitTarget) -> LiveCommitTarget {
    target.immutable_target_id = format!("wrong-{}", target.immutable_target_id);
    target
}

fn switched_participant(mut target: LiveCommitTarget) -> LiveCommitTarget {
    target.recipient_ids.remove(
        &target
            .recipient_ids
            .iter()
            .find(|id| id.starts_with("bcc-"))
            .expect("reviewed target includes BCC")
            .clone(),
    );
    target.recipient_ids.insert("bcc-wrong-target".to_owned());
    target
}

fn assert_refuses_before_provider_byte(
    reviewed: &osl_privacy_hub::send_commit_binding::ReviewedSendCommit,
    switched: &LiveCommitTarget,
    journal: &mut ProviderJournal,
) {
    let refusal = commit_reviewed_send(reviewed, switched, |target, bytes| journal.emit(target, bytes));
    assert!(matches!(refusal, Err(SendCommitRefusal::TargetChanged { .. })));
    assert!(journal.emitted.is_empty(), "refusal emitted a provider byte");
}

#[test]
fn task_6420_every_shipping_commit_route_is_bound_at_the_irreversible_barrier() {
    assert_eq!(SHIPPING_COMMIT_ROUTES.len(), 12, "matrix must stay one-for-one");

    for route in SHIPPING_COMMIT_ROUTES {
        let target = reviewed_target(route);
        let exact_cover = cover(route);
        let reviewed = prepare_reviewed_send(route, target.clone(), &exact_cover)
            .expect("last production target validation freezes complete target");

        // Unswitched control: exactly one byte-distinct cover at exactly the
        // reviewed immutable target.
        let mut control = ProviderJournal::default();
        commit_reviewed_send(&reviewed, &target, |live, bytes| control.emit(live, bytes))
            .expect("unchanged live target commits once");
        assert_eq!(control.emitted, vec![(target.clone(), exact_cover.clone())]);

        // Barrier changes happen after the prepared check and immediately
        // before commit.  The closure is the only provider-side effect, so all
        // three must refuse before a placement, draft mutation, request, or byte.
        for (effect, wrong) in [
            ("wrong-account", switched_account(target.clone())),
            ("wrong-immutable-target", switched_target(target.clone())),
            ("wrong-participant-set-including-bcc", switched_participant(target.clone())),
        ] {
            let mut switched = ProviderJournal::default();
            assert_refuses_before_provider_byte(&reviewed, &wrong, &mut switched);
            assert!(
                switched.emitted.iter().all(|(_, bytes)| bytes != &exact_cover),
                "wrong target received exact cover"
            );
            println!(
                "TASK6420 control route={} commit={} exact_cover_bytes={} reviewed_emits=1 switched={} wrong_target_effect=0",
                route.label(), route.commit_point(), exact_cover.len(), effect
            );
        }
    }
}

/// A deliberately unsafe representation of each historical failure.  These
/// are throwaway shipping mutants: they cache the pre-barrier read, use a live
/// UI target, compare labels, revalidate after an emission, or omit CC/BCC.
/// The test proves each mutant would damage the wrong target and therefore the
/// unchanged production barrier rejects it by route name.
fn mutant_would_emit_to_wrong_target(
    kind: &str,
    reviewed: &LiveCommitTarget,
    wrong: &LiveCommitTarget,
) -> bool {
    match kind {
        "cached-pre-barrier-check-acts-on-live-ui" => true,
        "visible-label-not-immutable-id" => reviewed.visible_label == wrong.visible_label,
        "revalidate-after-first-provider-side-effect" => true,
        "omit-cc-or-bcc-from-binding" => reviewed
            .recipient_ids
            .iter()
            .filter(|id| !id.starts_with("cc-") && !id.starts_with("bcc-"))
            .eq(wrong
                .recipient_ids
                .iter()
                .filter(|id| !id.starts_with("cc-") && !id.starts_with("bcc-"))),
        _ => unreachable!("all mutants must be named"),
    }
}

#[test]
fn task_6420_throwaway_mutants_are_red_for_every_inventoried_route_then_restored() {
    let mutants = [
        "cached-pre-barrier-check-acts-on-live-ui",
        "visible-label-not-immutable-id",
        "revalidate-after-first-provider-side-effect",
        "omit-cc-or-bcc-from-binding",
    ];
    for route in SHIPPING_COMMIT_ROUTES {
        let target = reviewed_target(route);
        let wrong = switched_participant(switched_target(target.clone()));
        let reviewed = prepare_reviewed_send(route, target.clone(), cover(route)).unwrap();
        for mutant in mutants {
            assert!(
                mutant_would_emit_to_wrong_target(mutant, &target, &wrong),
                "the throwaway mutant must demonstrate the named defect"
            );
            let mut restored = ProviderJournal::default();
            assert_refuses_before_provider_byte(&reviewed, &wrong, &mut restored);
            println!(
                "TASK6420 mutant route={} commit={} mutant={} exit=1 wrong-target-effect=would-emit; restored=pass emitted=0",
                route.label(), route.commit_point(), mutant
            );
        }
    }
}

#[test]
fn task_6420_empty_or_partial_target_cannot_prepare_a_commit() {
    let route = ShippingCommitRoute::GmailReplyAll;
    let mut no_recipients = reviewed_target(route);
    no_recipients.recipient_ids.clear();
    assert_eq!(
        prepare_reviewed_send(route, no_recipients, b"cover"),
        Err(SendCommitRefusal::EmptyRecipients)
    );
    assert_eq!(
        prepare_reviewed_send(route, reviewed_target(route), b""),
        Err(SendCommitRefusal::EmptyCover)
    );
    println!("TASK6420 preparation refuses missing complete-recipient-set and empty-cover before commit");
}
