//! The release Ready label, counted over the whole service catalogue.
//!
//! Gate 0808 made `Ready` need proof: in `services.rs` a service only earns the
//! label when it has real two-person protected messaging and a matching
//! [`DeliveryProof`]. That rule is keyed by `ServiceKind`, and `ServiceKind`
//! has five variants, so it only ever asked the question of the five services
//! `service_descriptors()` knows about. X, Instagram and Messenger - the three
//! put back by 4255-4257 and left honestly refusing by 4263 - were not asked at
//! all. A row that is never asked a question can never be given a wrong answer,
//! and a restored but empty service could ride into a release with nothing
//! saying it is not ready.
//!
//! This module counts the three the same way as everything else: one catalogue,
//! every service asked the same three questions in the same order.
//!
//! 1. Have its own build checks passed? The three still have outstanding ones
//!    (the same missing parts 4263 refuses by name), so they stop here and the
//!    reason names the missing part.
//! 2. Does it have real two-person protected messaging, either as a standing
//!    capability or through a matching delivery proof? (Gate 0808.)
//! 3. Is there a matching delivery proof? (Gate 0808.)
//!
//! A service is turned on by flipping its own-check row to `passed: true`, which
//! takes the gating task actually passing - the switch is the proof, not a flag
//! a release table can set. Nothing a label table says can make one of the three
//! Ready; hand-writing `Ready` beside one of them only makes the release check
//! exit 1.

/// The word a release prints beside a service that has earned the label.
pub const READY_LABEL: &str = "Ready";
/// The word a release prints beside a service that has not earned it.
pub const NOT_READY_LABEL: &str = "Not ready";

/// Gate 0808's refusals, spelled exactly as `services.rs` spells them, so the
/// catalogue-wide label and the `ServiceKind`-keyed one refuse in one language.
pub const READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY: &str =
    "ready_requires_real_two_person_protected_messaging_capability";
pub const READY_REQUIRES_MATCHING_DELIVERY_PROOF: &str = "ready_requires_matching_delivery_proof";
/// 4265's refusal: the service still has a build check of its own outstanding.
pub const READY_REQUIRES_OWN_CHECKS_PASSED: &str = "ready_requires_own_checks_passed";

/// One build check a service must pass before a release may label it at all.
///
/// `part_name` is the missing part in the same words 4263 refuses with, so a
/// person who reads "cannot send a message on X yet because the watched send
/// permission is missing" and then reads the release table sees one name for
/// one thing.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OwnCheck {
    pub id: &'static str,
    pub part_name: &'static str,
    pub gating_task: &'static str,
    pub passed: bool,
}

/// One row of the service catalogue a release is labelled against.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CatalogueService {
    pub service_id: &'static str,
    pub display_name: &'static str,
    /// Mirrors `SERVICE_CAPABILITY_FACTS` in `services.rs`: no service carries a
    /// standing real two-person protected messaging capability yet, so every
    /// one of them has to bring a delivery proof.
    pub real_two_person_protected_messaging: bool,
    /// Build checks this service still owes, or `&[]` when it owes none and the
    /// only thing left between it and the label is gate 0808's delivery proof.
    pub own_checks: &'static [OwnCheck],
}

/// The three's outstanding parts, read off the plan's own open tasks and kept
/// identical to 4263's table so the refusals agree word for word.
const X_OWN_CHECKS: [OwnCheck; 2] = [
    OwnCheck {
        id: "watched_send_permission",
        part_name: "the watched send permission",
        gating_task: "TASK 4264",
        passed: false,
    },
    OwnCheck {
        id: "web_reader_insides",
        part_name: "the web reader insides",
        gating_task: "TASK 4259",
        passed: false,
    },
];

const INSTAGRAM_OWN_CHECKS: [OwnCheck; 2] = [
    OwnCheck {
        id: "signed_page_control_table",
        part_name: "the signed page control table",
        gating_task: "TASK 4260",
        passed: false,
    },
    OwnCheck {
        id: "web_reader_insides",
        part_name: "the web reader insides",
        gating_task: "TASK 4259",
        passed: false,
    },
];

const MESSENGER_OWN_CHECKS: [OwnCheck; 2] = [
    OwnCheck {
        id: "signed_page_control_table",
        part_name: "the signed page control table",
        gating_task: "TASK 4260",
        passed: false,
    },
    OwnCheck {
        id: "web_reader_insides",
        part_name: "the web reader insides",
        gating_task: "TASK 4259",
        passed: false,
    },
];

/// The eight services a release is labelled against: the five `services.rs`
/// already counted, plus the three it did not.
pub const RELEASE_READY_CATALOGUE: [CatalogueService; 8] = [
    CatalogueService {
        service_id: "discord",
        display_name: "Discord",
        real_two_person_protected_messaging: false,
        own_checks: &[],
    },
    CatalogueService {
        service_id: "telegram",
        display_name: "Telegram",
        real_two_person_protected_messaging: false,
        own_checks: &[],
    },
    CatalogueService {
        service_id: "whatsapp",
        display_name: "WhatsApp",
        real_two_person_protected_messaging: false,
        own_checks: &[],
    },
    CatalogueService {
        service_id: "email",
        display_name: "Email",
        real_two_person_protected_messaging: false,
        own_checks: &[],
    },
    CatalogueService {
        service_id: "signal",
        display_name: "Signal",
        real_two_person_protected_messaging: false,
        own_checks: &[],
    },
    CatalogueService {
        service_id: "x",
        display_name: "X",
        real_two_person_protected_messaging: false,
        own_checks: &X_OWN_CHECKS,
    },
    CatalogueService {
        service_id: "instagram",
        display_name: "Instagram",
        real_two_person_protected_messaging: false,
        own_checks: &INSTAGRAM_OWN_CHECKS,
    },
    CatalogueService {
        service_id: "messenger",
        display_name: "Messenger",
        real_two_person_protected_messaging: false,
        own_checks: &MESSENGER_OWN_CHECKS,
    },
];

/// The three restored by 4255-4257. Named here so a check can ask for them by
/// name rather than trusting the catalogue to still hold them.
pub const THE_THREE: [&str; 3] = ["x", "instagram", "messenger"];

/// Gate 0808's delivery proof, as a release label table carries it.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeliveryProof {
    pub service_id: String,
    pub protected_message_id: String,
    pub sender_person_id: String,
    pub recipient_person_id: String,
    pub protected_message_received: bool,
    pub received_by_real_other_person: bool,
}

/// What a release says about one service, and why.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadyDecision {
    pub service_id: &'static str,
    pub display_name: &'static str,
    pub label: &'static str,
    pub refusal: Option<&'static str>,
    pub reason: Option<String>,
    /// Every check of its own has passed: no outstanding build check, and gate
    /// 0808's delivery proof is in hand.
    pub own_checks_passed: bool,
}

impl ReadyDecision {
    pub fn is_ready(&self) -> bool {
        self.label == READY_LABEL
    }
}

/// Exact, lowercase id matching, the same rule `service_kind_from_id` uses, so
/// `"X"`, `"instagram "` and `"instagram.com"` are not catalogue services.
pub fn catalogue_service(service_id: &str) -> Option<&'static CatalogueService> {
    RELEASE_READY_CATALOGUE
        .iter()
        .find(|service| service.service_id == service_id)
}

/// The build checks this service still owes.
pub fn outstanding_own_checks(service: &CatalogueService) -> Vec<OwnCheck> {
    service
        .own_checks
        .iter()
        .copied()
        .filter(|check| !check.passed)
        .collect()
}

/// True when the service owes no build check of its own.
pub fn own_build_checks_passed(service: &CatalogueService) -> bool {
    outstanding_own_checks(service).is_empty()
}

/// Gate 0808's proof rule, mirrored from `delivery_proof_matches_ready_rule` in
/// `services.rs` with the service key as a string id so the three can be asked
/// the same question.
pub fn delivery_proof_matches_ready_rule(service_id: &str, proof: &DeliveryProof) -> bool {
    proof.service_id == service_id
        && proof.protected_message_received
        && proof.received_by_real_other_person
        && !proof.protected_message_id.trim().is_empty()
        && !proof.sender_person_id.trim().is_empty()
        && !proof.recipient_person_id.trim().is_empty()
        && proof.sender_person_id != proof.recipient_person_id
}

fn missing_part_reason(service: &CatalogueService, outstanding: &[OwnCheck]) -> String {
    let parts = outstanding
        .iter()
        .map(|check| {
            format!(
                "{} is missing ({} has not passed)",
                check.part_name, check.gating_task
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("{} is not ready: {parts}", service.display_name)
}

/// Label one service. The three questions, in order, for every service alike.
pub fn release_ready_decision(
    service: &CatalogueService,
    proofs: &[DeliveryProof],
) -> ReadyDecision {
    let matching_proof = proofs
        .iter()
        .find(|proof| delivery_proof_matches_ready_rule(service.service_id, proof));

    let outstanding = outstanding_own_checks(service);
    let own_checks_passed = outstanding.is_empty() && matching_proof.is_some();

    // 1. Its own build checks. A restored but empty service stops here, and the
    //    reason names the part that is missing.
    if !outstanding.is_empty() {
        return ReadyDecision {
            service_id: service.service_id,
            display_name: service.display_name,
            label: NOT_READY_LABEL,
            refusal: Some(READY_REQUIRES_OWN_CHECKS_PASSED),
            reason: Some(missing_part_reason(service, &outstanding)),
            own_checks_passed,
        };
    }

    // 2. Real two-person protected messaging (gate 0808). A matching proof is
    //    itself the capability, exactly as `ready_decisions_from_service_proof_records`
    //    treats it.
    if !service.real_two_person_protected_messaging && matching_proof.is_none() {
        return ReadyDecision {
            service_id: service.service_id,
            display_name: service.display_name,
            label: NOT_READY_LABEL,
            refusal: Some(READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY),
            reason: Some(format!(
                "{} is not ready: real two-person protected messaging is missing (TASK 0808 has not passed)",
                service.display_name
            )),
            own_checks_passed,
        };
    }

    // 3. The matching delivery proof (gate 0808).
    let Some(_proof) = matching_proof else {
        return ReadyDecision {
            service_id: service.service_id,
            display_name: service.display_name,
            label: NOT_READY_LABEL,
            refusal: Some(READY_REQUIRES_MATCHING_DELIVERY_PROOF),
            reason: Some(format!(
                "{} is not ready: a matching two-person delivery proof is missing (TASK 0808 has not passed)",
                service.display_name
            )),
            own_checks_passed,
        };
    };

    ReadyDecision {
        service_id: service.service_id,
        display_name: service.display_name,
        label: READY_LABEL,
        refusal: None,
        reason: None,
        own_checks_passed,
    }
}

/// Label the whole catalogue in one pass.
pub fn release_ready_decisions(proofs: &[DeliveryProof]) -> Vec<ReadyDecision> {
    RELEASE_READY_CATALOGUE
        .iter()
        .map(|service| release_ready_decision(service, proofs))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proof_for(service_id: &str) -> DeliveryProof {
        DeliveryProof {
            service_id: service_id.to_owned(),
            protected_message_id: "protected-4265-1".to_owned(),
            sender_person_id: "person-alice".to_owned(),
            recipient_person_id: "person-bob".to_owned(),
            protected_message_received: true,
            received_by_real_other_person: true,
        }
    }

    #[test]
    fn the_three_are_in_the_catalogue_with_a_named_missing_part() {
        for service_id in THE_THREE {
            let service = catalogue_service(service_id).expect("one of the three is catalogued");
            let outstanding = outstanding_own_checks(service);
            assert!(
                !outstanding.is_empty(),
                "{service_id} must still owe a build check"
            );
            for check in outstanding {
                assert!(
                    check.part_name.starts_with("the "),
                    "{service_id} names its missing part"
                );
                assert!(
                    check.gating_task.starts_with("TASK "),
                    "{service_id} names the gating task"
                );
            }
        }
    }

    #[test]
    fn all_three_read_not_ready_and_the_reason_names_the_missing_part() {
        let decisions = release_ready_decisions(&[]);
        let three: Vec<&ReadyDecision> = decisions
            .iter()
            .filter(|decision| THE_THREE.contains(&decision.service_id))
            .collect();
        assert_eq!(three.len(), 3);
        for decision in three {
            assert_eq!(decision.label, NOT_READY_LABEL);
            assert_eq!(decision.refusal, Some(READY_REQUIRES_OWN_CHECKS_PASSED));
            let reason = decision.reason.as_deref().expect("a reason");
            let service = catalogue_service(decision.service_id).expect("catalogued");
            for check in outstanding_own_checks(service) {
                assert!(
                    reason.contains(check.part_name),
                    "{} must name {}",
                    decision.service_id,
                    check.part_name
                );
            }
        }
    }

    #[test]
    fn a_hand_written_delivery_proof_cannot_make_one_of_the_three_ready() {
        for service_id in THE_THREE {
            let service = catalogue_service(service_id).expect("catalogued");
            let decision = release_ready_decision(service, &[proof_for(service_id)]);
            assert_eq!(
                decision.label, NOT_READY_LABEL,
                "{service_id} must stay out of the Ready label"
            );
            assert_eq!(decision.refusal, Some(READY_REQUIRES_OWN_CHECKS_PASSED));
            assert!(!decision.own_checks_passed);
        }
    }

    #[test]
    fn a_service_whose_own_checks_have_passed_still_reaches_ready() {
        let decisions = release_ready_decisions(&[proof_for("discord")]);
        let discord = decisions
            .iter()
            .find(|decision| decision.service_id == "discord")
            .expect("discord is catalogued");
        assert_eq!(discord.label, READY_LABEL);
        assert_eq!(discord.refusal, None);
        assert!(discord.own_checks_passed);
        // and it is the only one, in the same pass
        assert_eq!(
            decisions.iter().filter(|d| d.is_ready()).count(),
            1,
            "only the service with proof is Ready"
        );
    }

    #[test]
    fn gate_0808_still_refuses_a_service_with_no_proof_or_a_broken_one() {
        let service = catalogue_service("discord").expect("catalogued");
        assert_eq!(
            release_ready_decision(service, &[]).refusal,
            Some(READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY)
        );

        let mut same_person = proof_for("discord");
        same_person.recipient_person_id = same_person.sender_person_id.clone();
        assert_eq!(
            release_ready_decision(service, &[same_person]).label,
            NOT_READY_LABEL
        );

        let mut not_received = proof_for("discord");
        not_received.protected_message_received = false;
        assert_eq!(
            release_ready_decision(service, &[not_received]).label,
            NOT_READY_LABEL
        );

        let wrong_service = proof_for("telegram");
        assert_eq!(
            release_ready_decision(service, &[wrong_service]).label,
            NOT_READY_LABEL
        );
    }

    #[test]
    fn near_miss_ids_are_not_catalogue_services() {
        for near_miss in ["X", "instagram ", "instagram.com", "../x", ""] {
            assert!(catalogue_service(near_miss).is_none(), "{near_miss}");
        }
    }

    #[test]
    fn the_catalogue_holds_the_five_and_the_three_without_repeats() {
        assert_eq!(RELEASE_READY_CATALOGUE.len(), 8);
        for service in RELEASE_READY_CATALOGUE {
            let repeats = RELEASE_READY_CATALOGUE
                .iter()
                .filter(|other| other.service_id == service.service_id)
                .count();
            assert_eq!(repeats, 1, "{} is listed once", service.service_id);
        }
        for five in ["discord", "telegram", "whatsapp", "email", "signal"] {
            let service = catalogue_service(five).expect("one of the five is catalogued");
            assert!(
                own_build_checks_passed(service),
                "{five} owes no build check"
            );
        }
    }
}
