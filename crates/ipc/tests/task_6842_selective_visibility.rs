//! Acceptance check for TASK 6842.  This is intentionally a four-member
//! black-box enclave exercise: no server ACL or identity-bearing store index
//! is present anywhere in the simulated store.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ed25519, ml_kem_768, x25519};
use ipc::selective_visibility::{
    membership_snapshot, opaque_slot_count, receive, seal, Audience, ClientSurfaces,
    SelectiveDelivery, SelectiveError, SelectiveRecipient, SelectiveRecipientSecret,
    SELECTIVE_PREFIX, SENT_ONLY_SELECTIVE_AUDIENCE_MARKER,
};

struct Member {
    public: SelectiveRecipient,
    secret: SelectiveRecipientSecret,
}

impl Member {
    fn generate() -> Self {
        let (x_secret, x_public) = x25519::generate_keypair();
        let (mlkem_secret, mlkem_public) = ml_kem_768::generate_keypair();
        Self {
            public: SelectiveRecipient {
                x25519_pub: x_public,
                mlkem_pub: mlkem_public,
            },
            secret: SelectiveRecipientSecret {
                x25519_secret: x_secret,
                mlkem_secret,
            },
        }
    }

    /// Models a sealed-key restart: key material is serialised then loaded into
    /// new secret objects before a replay is opened.
    fn restarted_secret(&self) -> SelectiveRecipientSecret {
        let x = *self.secret.x25519_secret.as_bytes();
        let mlkem = self.secret.mlkem_secret.to_bytes();
        SelectiveRecipientSecret {
            x25519_secret: x25519::SecretKey::from_bytes(x),
            mlkem_secret: ml_kem_768::DecapsulationKey::from_bytes(&mlkem),
        }
    }
}

/// Deliberately identity-blind: it stores exactly one opaque string and has
/// neither a recipient column nor an ACL decision method.
#[derive(Default)]
struct IdentityBlindStore(Vec<String>);

impl IdentityBlindStore {
    fn put(&mut self, object: String) {
        self.0.push(object);
    }

    fn replay(&self) -> &str {
        self.0.first().expect("one object uploaded")
    }
}

fn members() -> Vec<Member> {
    (0..4).map(|_| Member::generate()).collect()
}

fn public_members(members: &[Member]) -> Vec<SelectiveRecipient> {
    members.iter().map(|member| member.public.clone()).collect()
}

fn audience_indices(mask: u8) -> Vec<usize> {
    (0..4).filter(|index| mask & (1 << index) != 0).collect()
}

fn selected_by(mode: &Audience, member: usize) -> bool {
    match mode {
        Audience::OnlyThese(indices) => indices.contains(&member),
        Audience::HideFrom(indices) => !indices.contains(&member),
    }
}

fn raw(object: &str) -> Vec<u8> {
    STANDARD
        .decode(object.strip_prefix(SELECTIVE_PREFIX).unwrap())
        .unwrap()
}

#[test]
fn task_6842_every_only_these_and_hide_from_combination_is_recipient_key_delivery() {
    let members = members();
    let roster = public_members(&members);
    let (sender_x_secret, sender_x_public) = x25519::generate_keypair();
    let (sender_signing_secret, sender_signing_public) = ed25519::generate_keypair();
    let expected_snapshot = membership_snapshot(&roster);
    let mut cases = 0usize;
    let mut selected_opens = 0usize;
    let mut hidden_opens = 0usize;

    for mask in 0u8..16 {
        for mode in [
            Audience::OnlyThese(audience_indices(mask)),
            Audience::HideFrom(audience_indices(mask)),
        ] {
            let message = format!("selective case mode={:?} mask={mask:04b}", mode).into_bytes();
            let object = seal(
                &sender_x_secret,
                &sender_x_public,
                &sender_signing_secret,
                &roster,
                mode.clone(),
                &message,
            )
            .unwrap();
            let mut store = IdentityBlindStore::default();
            store.put(object.clone());
            assert_eq!(store.0.len(), 1, "one opaque object per selective send");
            assert_eq!(
                opaque_slot_count(&object).unwrap(),
                (0..4).filter(|member| selected_by(&mode, *member)).count()
            );

            // The store-visible slot area contains no recipient X25519 public
            // key or v3 recipient hash. A store can count slots, but cannot
            // map any wrap to one of the four identities.
            let object_raw = raw(&object);
            let slots = &object_raw[34..34 + opaque_slot_count(&object).unwrap() * 1182];
            for member in &members {
                assert!(!slots
                    .windows(32)
                    .any(|w| w == member.public.x25519_pub.as_bytes()));
            }

            for (member_index, member) in members.iter().enumerate() {
                // Open the original and then a restarted-client replay.  Both
                // execute the same local opaque-wrap scan; no store identity
                // check is available to influence the answer.
                for secret in [&member.secret, &member.restarted_secret()] {
                    let delivery =
                        receive(store.replay(), secret, &sender_signing_public, &roster).unwrap();
                    if selected_by(&mode, member_index) {
                        let SelectiveDelivery::Selected {
                            plaintext,
                            marker,
                            surfaces,
                            manifest,
                        } = delivery
                        else {
                            panic!("selected member {member_index} lost its usable key wrap")
                        };
                        assert_eq!(plaintext, message);
                        assert_eq!(marker, SENT_ONLY_SELECTIVE_AUDIENCE_MARKER);
                        assert_eq!(surfaces, ClientSurfaces::selected());
                        assert_eq!(manifest.membership_snapshot, expected_snapshot);
                        assert_eq!(manifest.mode, mode.tag_for_test());
                        selected_opens += 1;
                    } else {
                        let SelectiveDelivery::Hidden { surfaces } = delivery else {
                            panic!(
                                "hidden member {member_index} received a message-derived surface"
                            )
                        };
                        assert!(surfaces.is_zero());
                        hidden_opens += 1;
                    }
                }
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 32);
    assert_eq!(selected_opens + hidden_opens, 32 * 4 * 2);
    println!(
        "TASK6842_GREEN combinations={cases} objects={cases} selected_opens={selected_opens} hidden_zero_surface_opens={hidden_opens} restart_replays=128 marker={SENT_ONLY_SELECTIVE_AUDIENCE_MARKER}"
    );
}

#[test]
fn task_6842_fail_closed_fault_matrix_rejects_membership_key_wrap_signature_and_surface_bypasses() {
    let members = members();
    let roster = public_members(&members);
    let (sender_x_secret, sender_x_public) = x25519::generate_keypair();
    let (sender_signing_secret, sender_signing_public) = ed25519::generate_keypair();
    let object = seal(
        &sender_x_secret,
        &sender_x_public,
        &sender_signing_secret,
        &roster,
        Audience::OnlyThese(vec![0, 2]),
        b"bound decision",
    )
    .unwrap();

    // Membership race: the selected member cannot render under a changed
    // current roster, even though its key still opens a slot.
    let changed_roster = roster[..3].to_vec();
    assert_eq!(
        receive(
            &object,
            &members[0].secret,
            &sender_signing_public,
            &changed_roster
        ),
        Err(SelectiveError::MembershipRace)
    );

    // Signature starvation/wrong sender key cannot become a visible message.
    let (_, wrong_sender_signing_public) = ed25519::generate_keypair();
    assert_eq!(
        receive(
            &object,
            &members[0].secret,
            &wrong_sender_signing_public,
            &roster
        ),
        Err(SelectiveError::InvalidSignature)
    );

    // Break the first selected wrap. The selected member degrades to exactly
    // the hidden zero-surface result, never to a placeholder or object hint.
    let mut broken_wrap = raw(&object);
    broken_wrap[34 + 32 + 2 + ml_kem_768::CIPHERTEXT_SIZE + 3] ^= 0x80;
    let broken_wrap = format!("{SELECTIVE_PREFIX}{}", STANDARD.encode(broken_wrap));
    assert_eq!(
        receive(
            &broken_wrap,
            &members[0].secret,
            &sender_signing_public,
            &roster
        ),
        Ok(SelectiveDelivery::Hidden {
            surfaces: ClientSurfaces::hidden()
        })
    );

    // A body tamper after a usable key wrap is an authenticated rejection,
    // never a cosmetically hidden successful delivery.
    let mut body_tamper = raw(&object);
    *body_tamper.last_mut().unwrap() ^= 0x01;
    let body_tamper = format!("{SELECTIVE_PREFIX}{}", STANDARD.encode(body_tamper));
    assert_eq!(
        receive(
            &body_tamper,
            &members[0].secret,
            &sender_signing_public,
            &roster
        ),
        Err(SelectiveError::Crypto)
    );

    // Explicitly name every hidden surface: each is zero, and no optional
    // field can act as an OSL-controlled timing/reaction/reply oracle.
    let hidden = ClientSurfaces::hidden();
    assert_eq!(hidden.timeline_entries, 0);
    assert_eq!(hidden.message_keys, 0);
    assert_eq!(hidden.object_hints, 0);
    assert_eq!(hidden.placeholders, 0);
    assert_eq!(hidden.unread_increments, 0);
    assert_eq!(hidden.notifications, 0);
    assert_eq!(hidden.reaction_targets, 0);
    assert_eq!(hidden.reply_targets, 0);
    assert_eq!(hidden.osl_timing_metadata, 0);
    println!(
        "TASK6842_FAULTS membership_race=MembershipRace wrong_signer=InvalidSignature broken_wrap=Hidden body_tamper=Crypto hidden_surfaces=9"
    );
}

trait AudienceTestTag {
    fn tag_for_test(&self) -> u8;
}

impl AudienceTestTag for Audience {
    fn tag_for_test(&self) -> u8 {
        match self {
            Audience::OnlyThese(_) => 1,
            Audience::HideFrom(_) => 2,
        }
    }
}
